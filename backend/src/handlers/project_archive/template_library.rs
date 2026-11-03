//! Owner-scoped package library; a hard-link snapshot survives concurrent deletion.
use super::*;
use crate::{db::NativeTemplateEntry, template_bundle};
use axum::{extract::Query, response::IntoResponse};
fn directory(state: &AppState) -> PathBuf {
    state.storage.join("template-library")
}
fn package(state: &AppState, id: &str) -> AppResult<PathBuf> {
    if Uuid::parse_str(id).is_err() {
        return Err(AppError::not_found("Шаблон не найден"));
    }
    Ok(directory(state).join(format!("{id}.vktemplate")))
}
async fn owned(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    cookie: bool,
) -> AppResult<(String, NativeTemplateEntry)> {
    let actor = archive_actor(state, headers, cookie).await?;
    let entry = state
        .db
        .native_template(&actor, id)
        .await
        .map_err(|e| AppError::internal("read template library", e))?
        .ok_or_else(|| AppError::not_found("Шаблон не найден"))?;
    Ok((actor, entry))
}
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let actor = archive_actor(&state, &headers, false).await?;
    let entries = state
        .db
        .native_templates(&actor)
        .await
        .map_err(|e| AppError::internal("list template library", e))?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(entries),
    )
        .into_response())
}
pub async fn install(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<(StatusCode, Json<NativeTemplateEntry>)> {
    let actor = archive_actor(&state, &headers, false).await?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let upload = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.template-library-upload", Uuid::new_v4())),
    );
    run_with_timeout(
        receive_archive(multipart, upload.path()),
        ARCHIVE_IO_TIMEOUT,
        "Загрузка шаблона превысила лимит времени",
    )
    .await??;
    let result = tokio::spawn(async move {
        let _slot = slot;
        let stage = state.staging_dir();
        let (upload, info, name, poster) =
            tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                let parsed = template_bundle::read(std::fs::File::open(upload.path())?, &stage)?;
                let info =
                    crate::domain::project_template::inspect(&parsed.manifest.project, None)?;
                let name = parsed.manifest.name.clone();
                Ok((upload, serde_json::to_value(info)?, name, parsed.poster_png))
            })
            .await
            .map_err(|e| AppError::internal("join library verification", e))?
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        let (bytes, sha256) = fingerprint_file(upload.path())
            .await
            .map_err(export_archive_error)?;
        let entry = NativeTemplateEntry {
            has_poster: poster.is_some(),
            id: Uuid::new_v4().to_string(),
            name,
            info,
            bytes,
            sha256,
            created_at: now_secs(),
        };
        tokio::fs::create_dir_all(directory(&state))
            .await
            .map_err(|e| AppError::internal("create template library", e))?;
        let path = package(&state, &entry.id)?;
        let mut installed = TemporaryFile::new(path.clone());
        tokio::fs::hard_link(upload.path(), &path)
            .await
            .map_err(|e| AppError::internal("publish template package", e))?;
        state
            .db
            .insert_native_template(&actor, &entry, poster.as_deref())
            .await
            .map_err(|e| {
                if e.to_string().contains("name already exists") {
                    AppError::conflict("Шаблон с таким именем уже есть")
                } else if e.to_string().contains("quota") {
                    AppError::conflict("Библиотека шаблонов заполнена")
                } else {
                    AppError::internal("save template library entry", e)
                }
            })?;
        installed.path.take();
        Ok::<_, AppError>(entry)
    })
    .await
    .map_err(|e| AppError::internal("join template install", e))??;
    Ok((StatusCode::CREATED, Json(result)))
}
pub async fn download(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (_actor, entry) = owned(&state, &headers, &id, true).await?;
    let path = package(&state, &id)?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| AppError::conflict("Файл шаблона отсутствует"))?;
    let (length, hash) = fingerprint_file(&path)
        .await
        .map_err(export_archive_error)?;
    if length != entry.bytes || hash != entry.sha256 {
        return Err(AppError::conflict("Файл шаблона изменился"));
    }
    let filename = format!(
        "{}.vktemplate",
        archive_download_filename(&entry.name).trim_end_matches(".veproj")
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "application/vnd.video-kadr.template")
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(header::CONTENT_LENGTH, length)
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                .map_err(|e| AppError::internal("template filename", e))?,
        )
        .body(Body::from_stream(ReaderStream::new(file)))
        .map_err(|e| AppError::internal("template download", e))
}
pub async fn remove(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let (actor, _) = owned(&state, &headers, &id, false).await?;
    state
        .db
        .delete_native_template(&actor, &id)
        .await
        .map_err(|e| AppError::internal("delete template entry", e))?;
    if let Err(error) = tokio::fs::remove_file(package(&state, &id)?).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(template.id=%id,%error,"orphan template package needs cleanup");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn instantiate(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    Query(options): Query<template_import::Options>,
) -> AppResult<(StatusCode, Json<ProjectArchiveImportResponse>)> {
    let (actor, entry) = owned(&state, &headers, &id, false).await?;
    let space = archive_space_id(&headers)?;
    let refs = template_import::parse_references(&options.fills)?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let snapshot = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.template-library-instance", Uuid::new_v4())),
    );
    tokio::fs::hard_link(package(&state, &id)?, snapshot.path())
        .await
        .map_err(|_| AppError::conflict("Файл шаблона отсутствует"))?;
    let (bytes, hash) = fingerprint_file(snapshot.path())
        .await
        .map_err(export_archive_error)?;
    if bytes != entry.bytes || hash != entry.sha256 {
        return Err(AppError::conflict("Файл шаблона изменился"));
    }
    let result = tokio::spawn(async move {
        let _slot = slot;
        Box::pin(template_import::create(
            state, snapshot, actor, space, headers, refs,
        ))
        .await
    })
    .await
    .map_err(|e| AppError::internal("join saved template creation", e))??;
    Ok((StatusCode::CREATED, Json(result)))
}

pub async fn poster(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (actor, _) = owned(&state, &headers, &id, false).await?;
    let bytes = state
        .db
        .native_template_poster(&actor, &id)
        .await
        .map_err(|e| AppError::internal("read template poster", e))?
        .ok_or_else(|| AppError::not_found("Обложка отсутствует"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
