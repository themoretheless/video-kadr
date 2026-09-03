//! HTTP/application orchestration for portable `.veproj` composition archives.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{multipart::MultipartError, Multipart, Path as AxPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::time::timeout;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::db::{
    normalize_library_tags, normalize_library_title, CompositionProject, LibraryMetadata,
};
use crate::error::{ApiMultipart, AppError, AppResult};
use crate::library::{now_secs, MediaEntry};
use crate::project_archive::{
    fingerprint_file, read_project_archive, safe_archive_filename, write_project_archive,
    ArchiveError, ArchiveMediaInput, ArchivedProject, ArchivedSource, ParsedProjectArchive,
    ProjectArchiveManifest, StagedArchiveMedia, ARCHIVE_EXTENSION, ARCHIVE_MEDIA_TYPE,
    MAX_ARCHIVE_BYTES,
};
use crate::state::AppState;
use crate::tools;

use super::upload::{media_type, safe_upload_extension};

const ARCHIVE_IO_TIMEOUT: Duration = Duration::from_secs(30 * 60);

struct TemporaryFile {
    path: Option<PathBuf>,
}

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn path(&self) -> &Path {
        self.path.as_deref().expect("temporary path is present")
    }

    fn into_reader(mut self, file: tokio::fs::File) -> DeleteOnDropReader {
        DeleteOnDropReader {
            file,
            path: self.path.take().expect("temporary path is present"),
        }
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct DeleteOnDropReader {
    file: tokio::fs::File,
    path: PathBuf,
}

impl AsyncRead for DeleteOnDropReader {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_read(context, buffer)
    }
}

impl Drop for DeleteOnDropReader {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectArchiveImportResponse {
    project: CompositionProject,
    source_mapping: BTreeMap<String, String>,
}

#[derive(Debug)]
struct PreparedSource {
    archived: ArchivedSource,
    staged: StagedArchiveMedia,
    new_id: String,
    destination_filename: String,
    entry: MediaEntry,
}

#[derive(Debug)]
struct PublishedSource {
    id: String,
    path: PathBuf,
}

/// `GET /api/composition-projects/:id/archive` — materialize a portable archive
/// privately, then stream it to the client and unlink it when the body drops.
pub async fn composition_project_archive_export_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let actor = archive_actor(&state, &headers, true).await?;
    let _slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let project = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|error| AppError::internal("load project for archive", error))?
        .ok_or_else(|| AppError::not_found("Композиционный проект не найден"))?;

    let mut sources = Vec::with_capacity(project.source_ids.len());
    let mut inputs = Vec::with_capacity(project.source_ids.len());
    let mut snapshots = Vec::with_capacity(project.source_ids.len());
    for source_id in &project.source_ids {
        let entry = state
            .library
            .get(source_id)
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::conflict("Проект ссылается на отсутствующий источник"))?;
        let source_path = validated_source_path(&state, &entry).await?;
        let snapshot = TemporaryFile::new(
            state
                .staging_dir()
                .join(format!("{}.veproj-export-source", Uuid::new_v4())),
        );
        tokio::fs::hard_link(&source_path, snapshot.path())
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    AppError::conflict("Источник исчез во время экспорта")
                } else {
                    AppError::internal("snapshot project source", error)
                }
            })?;
        let snapshot_metadata = tokio::fs::symlink_metadata(snapshot.path())
            .await
            .map_err(|error| AppError::internal("inspect source snapshot", error))?;
        if !snapshot_metadata.file_type().is_file() || snapshot_metadata.file_type().is_symlink() {
            return Err(AppError::conflict(
                "Не удалось создать безопасный snapshot источника",
            ));
        }
        let (byte_length, sha256) = fingerprint_file(snapshot.path())
            .await
            .map_err(export_archive_error)?;
        let metadata = state
            .db
            .get_library_metadata(source_id)
            .await
            .map_err(|error| AppError::internal("load source metadata for archive", error))?;
        let (title, favorite, tags) = archive_library_metadata(&entry, metadata)?;
        let media_type = archive_media_type(&entry)
            .ok_or_else(|| AppError::conflict("Не удалось определить тип источника"))?;
        sources.push(ArchivedSource {
            source_id: source_id.clone(),
            filename: entry.filename.clone(),
            media_type,
            title,
            duration: entry.duration,
            width: entry.width,
            height: entry.height,
            favorite,
            tags,
            byte_length,
            sha256,
        });
        inputs.push(ArchiveMediaInput {
            source_id: source_id.clone(),
            filename: entry.filename,
            path: snapshot.path().to_owned(),
        });
        snapshots.push(snapshot);
    }

    let manifest = ProjectArchiveManifest::new(
        ArchivedProject {
            schema_version: project.schema_version,
            mode: project.mode,
            name: project.name.clone(),
            document: project.document,
        },
        sources,
    );
    let archive = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.veproj-export", Uuid::new_v4())),
    );
    let writer = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive.path())
        .await
        .map_err(|error| AppError::internal("create project archive", error))?;
    run_with_timeout(
        write_project_archive(writer, &manifest, &inputs),
        ARCHIVE_IO_TIMEOUT,
        "экспорт проекта превысил лимит времени",
    )
    .await?
    .map_err(export_archive_error)?;
    drop(snapshots);

    let length = tokio::fs::metadata(archive.path())
        .await
        .map_err(|error| AppError::internal("stat project archive", error))?
        .len();
    let file = tokio::fs::File::open(archive.path())
        .await
        .map_err(|error| AppError::internal("open project archive", error))?;
    let filename = archive_download_filename(&project.name);
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|error| AppError::internal("build archive response header", error))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ARCHIVE_MEDIA_TYPE)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(ReaderStream::new(
            archive.into_reader(file),
        )))
        .map_err(|error| AppError::internal("build project archive response", error))
}

/// `POST /api/composition-projects/import` — stage one multipart `.veproj`,
/// validate and probe everything, then publish through a rollback-safe task.
pub async fn composition_project_archive_import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<(StatusCode, Json<ProjectArchiveImportResponse>)> {
    let owner = archive_actor(&state, &headers, false).await?;
    let space_id = archive_space_id(&headers)?;
    let _slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let archive = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.veproj-upload", Uuid::new_v4())),
    );
    run_with_timeout(
        receive_archive(multipart, archive.path()),
        ARCHIVE_IO_TIMEOUT,
        "загрузка архива превысила лимит времени",
    )
    .await??;
    let reader = tokio::fs::File::open(archive.path())
        .await
        .map_err(|error| AppError::internal("open staged project archive", error))?;
    let parsed = run_with_timeout(
        read_project_archive(reader, &state.staging_dir()),
        ARCHIVE_IO_TIMEOUT,
        "проверка архива превысила лимит времени",
    )
    .await?
    .map_err(import_archive_error)?;
    drop(archive);
    if !parsed.media.is_empty() && !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Импорт архива с медиа требует локальный ffprobe",
        ));
    }

    // The detached task owns every extracted staging guard. If the HTTP client
    // disconnects, publication still reaches success or compensating rollback.
    let task_state = state.clone();
    let imported =
        tokio::spawn(
            async move { import_parsed_archive(task_state, parsed, owner, space_id).await },
        )
        .await
        .map_err(|error| AppError::internal("join project archive import", error))??;
    Ok((StatusCode::CREATED, Json(imported)))
}

async fn import_parsed_archive(
    state: AppState,
    parsed: ParsedProjectArchive,
    owner: String,
    space_id: Option<String>,
) -> AppResult<ProjectArchiveImportResponse> {
    let ParsedProjectArchive { manifest, media } = parsed;
    let ProjectArchiveManifest {
        project, sources, ..
    } = manifest;
    let mut reserved_ids = HashSet::with_capacity(sources.len());
    let mut source_mapping = BTreeMap::new();
    let mut prepared = Vec::with_capacity(sources.len());

    for (archived, staged) in sources.into_iter().zip(media) {
        if archived.source_id != staged.source_id() || archived.filename != staged.filename() {
            return Err(AppError::bad_request(
                "Идентификатор распакованного источника не совпадает с manifest",
            ));
        }
        let probe = tools::probe_video(&state.process_runtime, staged.path())
            .await
            .map_err(import_probe_error)?;
        let extension = safe_upload_extension(&probe).ok_or_else(|| {
            AppError::unsupported_media_type("Архив содержит неподдерживаемый формат медиа")
        })?;
        let probed_media_type = media_type(&probe).ok_or_else(|| {
            AppError::unsupported_media_type("Архив не содержит поддерживаемое медиа")
        })?;
        if probed_media_type != archived.media_type {
            return Err(AppError::bad_request(
                "Тип медиа в manifest не совпадает с содержимым",
            ));
        }
        let new_id = allocate_source_id(&state, &mut reserved_ids, extension).await?;
        let destination_filename = format!("{new_id}.{extension}");
        let entry = MediaEntry {
            id: new_id.clone(),
            kind: "source".into(),
            filename: destination_filename.clone(),
            storage_key: None,
            url: format!("/files/sources/{destination_filename}"),
            media_type: Some(probed_media_type.into()),
            title: archived.title.clone(),
            duration: Some(probe.duration),
            width: (probe.width > 0).then_some(probe.width),
            height: (probe.height > 0).then_some(probe.height),
            fps: probe.fps,
            vcodec: probe.vcodec.clone(),
            acodec: probe.acodec.clone(),
            size_bytes: Some(archived.byte_length),
            created_at: now_secs(),
        };
        source_mapping.insert(archived.source_id.clone(), new_id.clone());
        prepared.push(PreparedSource {
            archived,
            staged,
            new_id,
            destination_filename,
            entry,
        });
    }

    let document = rewrite_document_source_ids(project.document, &source_mapping)?;
    let source_ids = document
        .get("sources")
        .and_then(Value::as_object)
        .expect("archive validation and rewrite preserve sources")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let project = publish_with_rollback(
        &state,
        &prepared,
        &project.name,
        &document,
        &source_ids,
        &owner,
        space_id.as_deref(),
    )
    .await?;
    for source in &prepared {
        state.index_media(&source.entry).await;
    }
    Ok(ProjectArchiveImportResponse {
        project,
        source_mapping,
    })
}

async fn publish_with_rollback(
    state: &AppState,
    prepared: &[PreparedSource],
    project_name: &str,
    document: &Value,
    source_ids: &[String],
    owner: &str,
    space_id: Option<&str>,
) -> AppResult<CompositionProject> {
    let mut published = Vec::with_capacity(prepared.len());
    let target = ImportTarget { owner, space_id };
    match publish_sources_and_project(
        state,
        prepared,
        project_name,
        document,
        source_ids,
        target,
        &mut published,
    )
    .await
    {
        Ok(project) => Ok(project),
        Err(error) => {
            rollback_published_sources(state, &published).await;
            Err(error)
        }
    }
}

#[derive(Clone, Copy)]
struct ImportTarget<'a> {
    owner: &'a str,
    space_id: Option<&'a str>,
}

async fn publish_sources_and_project(
    state: &AppState,
    prepared: &[PreparedSource],
    project_name: &str,
    document: &Value,
    source_ids: &[String],
    target: ImportTarget<'_>,
    published: &mut Vec<PublishedSource>,
) -> AppResult<CompositionProject> {
    for source in prepared {
        let destination = state.sources_dir().join(&source.destination_filename);
        tokio::fs::hard_link(source.staged.path(), &destination)
            .await
            .map_err(|error| AppError::internal("publish archived source", error))?;
        published.push(PublishedSource {
            id: source.new_id.clone(),
            path: destination,
        });
        if !state.library.add(source.entry.clone()).await {
            return Err(AppError::internal(
                "persist imported library source",
                anyhow::anyhow!("library rejected imported source"),
            ));
        }
        state
            .db
            .replace_library_metadata(
                &source.new_id,
                source.archived.title.clone(),
                source.archived.favorite,
                source.archived.tags.clone(),
            )
            .await
            .map_err(|error| AppError::internal("persist imported source metadata", error))?;
    }
    let result = if let Some(space_id) = target.space_id {
        state
            .db
            .create_space_composition_project(
                project_name,
                document,
                source_ids,
                target.owner,
                space_id,
            )
            .await
    } else {
        state
            .db
            .create_owned_composition_project(project_name, document, source_ids, target.owner)
            .await
    };
    let project = result.map_err(|error| {
        if error.to_string().contains("space role cannot")
            || error
                .to_string()
                .contains("belongs to another collaboration space")
        {
            AppError::forbidden("Недостаточно прав для импорта в пространство")
        } else {
            AppError::internal("persist imported composition project", error)
        }
    })?;
    if let Some(space_id) = project.space_id.as_deref() {
        for source_id in &project.source_ids {
            state
                .library
                .relocate_source_to_space(source_id, space_id)
                .await
                .map_err(|error| AppError::internal("isolate imported Space source", error))?;
        }
    }
    Ok(project)
}

fn archive_space_id(headers: &HeaderMap) -> AppResult<Option<String>> {
    headers
        .get("x-space-id")
        .map(|value| {
            let value = value
                .to_str()
                .map_err(|_| AppError::bad_request("Некорректный Space ID"))?;
            if value.is_empty() || value.len() > 64 || uuid::Uuid::parse_str(value).is_err() {
                return Err(AppError::bad_request("Некорректный Space ID"));
            }
            Ok(value.to_owned())
        })
        .transpose()
}

async fn archive_actor(
    state: &AppState,
    headers: &HeaderMap,
    allow_cookie: bool,
) -> AppResult<String> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            allow_cookie
                .then(|| {
                    headers
                        .get(header::COOKIE)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|cookies| {
                            cookies
                                .split(';')
                                .map(str::trim)
                                .find_map(|cookie| cookie.strip_prefix("video_kadr_session="))
                        })
                })
                .flatten()
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthorized("Требуется Bearer-сессия"))?;
    state
        .db
        .resolve_auth_session(token)
        .await
        .map_err(|error| AppError::internal("authenticate project archive request", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

async fn rollback_published_sources(state: &AppState, published: &[PublishedSource]) {
    for source in published.iter().rev() {
        if let Err(error) = state.db.delete_library_metadata(&source.id).await {
            tracing::error!(media.id = %source.id, %error, "rollback source metadata failed");
        }
        let removed = state.library.remove(&source.id).await;
        if !removed {
            let _ = tokio::fs::remove_file(&source.path).await;
        }
        if let Err(error) = state.media_index.remove(&source.id).await {
            tracing::warn!(media.id = %source.id, %error, "rollback media index failed");
        }
    }
}

fn rewrite_document_source_ids(
    mut document: Value,
    source_mapping: &BTreeMap<String, String>,
) -> AppResult<Value> {
    let root = document
        .as_object_mut()
        .ok_or_else(|| AppError::bad_request("Документ композиции не является объектом"))?;
    let old_sources = root
        .remove("sources")
        .and_then(|sources| sources.as_object().cloned())
        .ok_or_else(|| AppError::bad_request("В документе отсутствует sources"))?;
    if old_sources.len() != source_mapping.len() {
        return Err(AppError::bad_request(
            "Количество sources не совпадает с source mapping",
        ));
    }
    let mut old_sources = old_sources;
    let mut new_sources = Map::new();
    for (old_id, new_id) in source_mapping {
        let mut source = old_sources
            .remove(old_id)
            .ok_or_else(|| AppError::bad_request("Source mapping неполон"))?;
        source
            .as_object_mut()
            .ok_or_else(|| AppError::bad_request("Описание source не является объектом"))?
            .insert("id".into(), Value::String(new_id.clone()));
        if new_sources.insert(new_id.clone(), source).is_some() {
            return Err(AppError::bad_request(
                "Source mapping содержит повторяющийся новый id",
            ));
        }
    }
    if !old_sources.is_empty() {
        return Err(AppError::bad_request(
            "Source mapping содержит не все sources",
        ));
    }
    root.insert("sources".into(), Value::Object(new_sources));
    rewrite_source_id_fields(&mut document, source_mapping)?;
    Ok(document)
}

fn rewrite_source_id_fields(
    value: &mut Value,
    source_mapping: &BTreeMap<String, String>,
) -> AppResult<()> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if key == "sourceId" {
                    let old_id = child
                        .as_str()
                        .ok_or_else(|| AppError::bad_request("sourceId не является строкой"))?;
                    let new_id = source_mapping
                        .get(old_id)
                        .ok_or_else(|| AppError::bad_request("sourceId отсутствует в mapping"))?;
                    *child = Value::String(new_id.clone());
                } else {
                    rewrite_source_id_fields(child, source_mapping)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_source_id_fields(item, source_mapping)?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn allocate_source_id(
    state: &AppState,
    reserved: &mut HashSet<String>,
    extension: &str,
) -> AppResult<String> {
    for _ in 0..64 {
        let id = Uuid::new_v4().to_string();
        let file_exists =
            tokio::fs::try_exists(state.sources_dir().join(format!("{id}.{extension}")))
                .await
                .map_err(|error| AppError::internal("check imported source collision", error))?;
        if reserved.contains(&id) || state.library.get(&id).await.is_some() || file_exists {
            continue;
        }
        reserved.insert(id.clone());
        return Ok(id);
    }
    Err(AppError::internal(
        "allocate imported source id",
        anyhow::anyhow!("could not allocate a collision-free source id"),
    ))
}

async fn validated_source_path(state: &AppState, entry: &MediaEntry) -> AppResult<PathBuf> {
    if !safe_archive_filename(&entry.filename) {
        return Err(AppError::conflict("Источник имеет небезопасное имя файла"));
    }
    state
        .library
        .resolve_media_path(entry)
        .await
        .map_err(|_| AppError::conflict("Файл источника отсутствует"))
}

fn archive_library_metadata(
    entry: &MediaEntry,
    metadata: Option<LibraryMetadata>,
) -> AppResult<(Option<String>, bool, Vec<String>)> {
    let (metadata_title, favorite, tags) = metadata
        .map(|metadata| (metadata.title, metadata.favorite, metadata.tags))
        .unwrap_or_else(|| (None, false, Vec::new()));
    let title = normalize_library_title(metadata_title.or_else(|| entry.title.clone()))
        .map_err(|_| AppError::conflict("Название источника нельзя безопасно архивировать"))?;
    let normalized_tags = normalize_library_tags(tags.clone())
        .map_err(|_| AppError::conflict("Теги источника нельзя безопасно архивировать"))?;
    if normalized_tags != tags {
        return Err(AppError::conflict(
            "Теги источника имеют неканонический формат",
        ));
    }
    Ok((title, favorite, tags))
}

fn archive_media_type(entry: &MediaEntry) -> Option<String> {
    if matches!(
        entry.media_type.as_deref(),
        Some("video" | "audio" | "image")
    ) {
        return entry.media_type.clone();
    }
    let extension = Path::new(&entry.filename)
        .extension()
        .and_then(|extension| extension.to_str())?
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
        Some("image".into())
    } else if matches!(
        extension.as_str(),
        "aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav"
    ) {
        Some("audio".into())
    } else if matches!(
        extension.as_str(),
        "avi" | "flv" | "m4v" | "mkv" | "mov" | "mp4" | "mpg" | "ogv" | "ts" | "webm"
    ) || (entry.width.unwrap_or(0) > 0 && entry.height.unwrap_or(0) > 0)
    {
        Some("video".into())
    } else {
        None
    }
}

async fn receive_archive(mut multipart: Multipart, path: &Path) -> AppResult<u64> {
    let mut found = false;
    let mut total = 0_u64;
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some("file") || field.file_name().is_none() || found {
            return Err(AppError::bad_request(
                "Ожидается ровно одно multipart-поле file",
            ));
        }
        found = true;
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
            .map_err(|error| AppError::internal("create staged project archive", error))?;
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            total = total
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| AppError::payload_too_large("Архив превышает допустимый размер"))?;
            if total > MAX_ARCHIVE_BYTES {
                return Err(AppError::payload_too_large(
                    "Архив превышает допустимый размер",
                ));
            }
            output
                .write_all(&chunk)
                .await
                .map_err(|error| AppError::internal("write staged project archive", error))?;
        }
        output
            .flush()
            .await
            .map_err(|error| AppError::internal("flush staged project archive", error))?;
    }
    if !found || total == 0 {
        return Err(AppError::bad_request("Файл архива не найден или пуст"));
    }
    Ok(total)
}

fn multipart_error(error: MultipartError) -> AppError {
    match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => {
            AppError::payload_too_large("Архив превышает допустимый размер")
        }
        StatusCode::BAD_REQUEST => AppError::invalid_multipart(),
        _ => AppError::internal("read project archive multipart", error),
    }
}

fn import_archive_error(error: ArchiveError) -> AppError {
    match error {
        ArchiveError::Invalid(message) => {
            AppError::bad_request(format!("Некорректный .veproj: {message}"))
        }
        ArchiveError::Limit(message) => {
            AppError::payload_too_large(format!(".veproj превышает лимит: {message}"))
        }
        ArchiveError::Io(error) => AppError::internal("read project archive", error),
    }
}

fn export_archive_error(error: ArchiveError) -> AppError {
    match error {
        ArchiveError::Invalid(message) => {
            AppError::conflict(format!("Проект нельзя экспортировать: {message}"))
        }
        ArchiveError::Limit(message) => {
            AppError::payload_too_large(format!("Проект превышает лимит архива: {message}"))
        }
        ArchiveError::Io(error) => AppError::internal("write project archive", error),
    }
}

fn import_probe_error(error: anyhow::Error) -> AppError {
    if tools::is_tool_timeout(&error) {
        AppError::gateway_timeout("Проверка медиа из архива превысила лимит времени")
    } else {
        AppError::unsupported_media_type("Не удалось распознать медиа из архива")
    }
}

async fn run_with_timeout<F, T>(
    operation: F,
    limit: Duration,
    message: &'static str,
) -> AppResult<T>
where
    F: Future<Output = T>,
{
    timeout(limit, operation)
        .await
        .map_err(|_| AppError::request_timeout(message))
}

fn archive_download_filename(project_name: &str) -> String {
    let mut stem = String::with_capacity(64);
    let mut separator = false;
    for character in project_name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            stem.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !stem.is_empty() {
            stem.push('-');
            separator = true;
        }
        if stem.len() >= 64 {
            break;
        }
    }
    while stem.ends_with('-') {
        stem.pop();
    }
    if stem.is_empty() {
        stem.push_str("composition-project");
    }
    format!("{stem}.{ARCHIVE_EXTENSION}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::state::ToolInfo;

    #[test]
    fn relink_rewrites_source_keys_embedded_ids_and_every_clip_reference() {
        let document = json!({
            "schemaVersion": 1,
            "sources": {
                "old-a": {"id": "old-a", "kind": "video"},
                "old-b": {"id": "old-b", "kind": "audio"}
            },
            "tracks": [
                {"clips": [{"sourceId": "old-a"}]},
                {"clips": [{"sourceId": "old-b"}, {"sourceId": "old-a"}]}
            ],
            "unrelated": {"id": "old-a"}
        });
        let mapping = BTreeMap::from([
            ("old-a".into(), "new-a".into()),
            ("old-b".into(), "new-b".into()),
        ]);
        let rewritten = rewrite_document_source_ids(document, &mapping).unwrap();

        assert_eq!(
            rewritten["sources"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            vec!["new-a", "new-b"]
        );
        assert_eq!(rewritten["sources"]["new-a"]["id"], "new-a");
        assert_eq!(rewritten["tracks"][0]["clips"][0]["sourceId"], "new-a");
        assert_eq!(rewritten["tracks"][1]["clips"][0]["sourceId"], "new-b");
        assert_eq!(rewritten["tracks"][1]["clips"][1]["sourceId"], "new-a");
        assert_eq!(rewritten["unrelated"]["id"], "old-a");
    }

    #[test]
    fn download_filename_is_ascii_header_safe_and_bounded() {
        assert_eq!(
            archive_download_filename("  My Portable Cut!  "),
            "my-portable-cut.veproj"
        );
        assert_eq!(
            archive_download_filename("Монтаж"),
            "composition-project.veproj"
        );
        assert!(archive_download_filename(&"x".repeat(500)).len() <= 64 + 7);
    }

    #[tokio::test]
    async fn project_failure_rolls_back_published_file_library_and_metadata() {
        let directory = tempfile::tempdir().unwrap();
        for child in ["sources", "outputs", "staging", "luts"] {
            tokio::fs::create_dir_all(directory.path().join(child))
                .await
                .unwrap();
        }
        let library = Library::load(directory.path().to_owned()).await;
        let db = Db::open(directory.path()).await.unwrap();
        let state = AppState::new(
            directory.path().to_owned(),
            1,
            ToolInfo::default(),
            library,
            db,
        );
        sqlx::query(
            "CREATE TRIGGER fail_archive_project BEFORE INSERT ON composition_projects \
             BEGIN SELECT RAISE(ABORT, 'injected project failure'); END",
        )
        .execute(state.db.pool())
        .await
        .unwrap();

        let staged_path = state.staging_dir().join("rollback-source.wav");
        tokio::fs::write(&staged_path, b"rollback-media")
            .await
            .unwrap();
        let new_id = "rollback-source";
        let destination_filename = format!("{new_id}.wav");
        let prepared = vec![PreparedSource {
            archived: ArchivedSource {
                source_id: "old-source".into(),
                filename: "old-source.wav".into(),
                media_type: "audio".into(),
                title: Some("Rollback".into()),
                duration: Some(1.0),
                width: None,
                height: None,
                favorite: true,
                tags: vec!["test".into()],
                byte_length: 14,
                sha256: "0".repeat(64),
            },
            staged: StagedArchiveMedia::from_test_file("old-source", "old-source.wav", staged_path),
            new_id: new_id.into(),
            destination_filename: destination_filename.clone(),
            entry: MediaEntry {
                id: new_id.into(),
                kind: "source".into(),
                filename: destination_filename.clone(),
                storage_key: None,
                url: format!("/files/sources/{destination_filename}"),
                media_type: Some("audio".into()),
                title: Some("Rollback".into()),
                duration: Some(1.0),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: Some("pcm_s16le".into()),
                size_bytes: Some(14),
                created_at: 1,
            },
        }];
        let document = json!({
            "schemaVersion": 1,
            "sources": {(new_id): {"id": new_id}}
        });

        assert!(publish_with_rollback(
            &state,
            &prepared,
            "Rollback project",
            &document,
            &[new_id.into()],
            "owner",
            None,
        )
        .await
        .is_err());
        assert!(state.library.get(new_id).await.is_none());
        assert!(state
            .db
            .get_library_metadata(new_id)
            .await
            .unwrap()
            .is_none());
        assert!(
            tokio::fs::metadata(state.sources_dir().join(destination_filename))
                .await
                .is_err()
        );
        assert!(state
            .db
            .list_composition_projects()
            .await
            .unwrap()
            .is_empty());
    }
}
