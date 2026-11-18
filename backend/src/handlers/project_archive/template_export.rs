//! Saved native-project template download with an optional poster.

use super::*;
use crate::{
    domain::{composition::SourceId, nested_composition::NestedCompositionProject},
    services::filmcraft_video_preview::SourceVideoRevision,
    template_bundle,
};
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    name: Option<String>,
    #[serde(default)]
    use_saved_cover: bool,
    #[serde(default)]
    auto_poster: bool,
    poster_time_us: Option<u64>,
    poster_composition_id: Option<String>,
}
pub async fn export(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    axum::extract::Query(options): axum::extract::Query<Options>,
) -> AppResult<Response> {
    let actor = archive_actor(&state, &headers, true).await?;
    let (archive, name) =
        Box::pin(build_package(&state, &headers, &actor, &id, options, None)).await?;
    download(archive, &name).await
}

async fn download(archive: TemporaryFile, name: &str) -> AppResult<Response> {
    let file = tokio::fs::File::open(archive.path())
        .await
        .map_err(|e| AppError::internal("open template package", e))?;
    let length = file
        .metadata()
        .await
        .map_err(|e| AppError::internal("stat template package", e))?
        .len();
    let base = archive_download_filename(name);
    let filename = format!("{}.vktemplate", base.trim_end_matches(".veproj"));
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
        .map_err(|e| AppError::internal("template disposition", e))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.video-kadr.template")
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from_stream(ReaderStream::new(
            archive.into_reader(file),
        )))
        .map_err(|e| AppError::internal("template response", e))
}

pub async fn save_to_library(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    axum::extract::Query(options): axum::extract::Query<Options>,
) -> AppResult<(StatusCode, Json<crate::db::NativeTemplateEntry>)> {
    let actor = archive_actor(&state, &headers, false).await?;
    let space = archive_space_id(&headers)?;
    super::template_library::require_space_editor(&state, &actor, space.as_deref()).await?;
    let (archive, _) =
        Box::pin(build_package(&state, &headers, &actor, &id, options, None)).await?;
    publish(state, actor, space, archive).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoverOptions {
    name: Option<String>,
    #[serde(default)]
    download: bool,
}

pub async fn with_uploaded_cover(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<CoverOptions>,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<Response> {
    let actor = archive_actor(&state, &headers, query.download).await?;
    let space = if query.download { None } else { archive_space_id(&headers)? };
    super::template_library::require_space_editor(&state, &actor, space.as_deref()).await?;
    let cover =
        super::template_frame::receive(multipart, template_bundle::MAX_POSTER_BYTES).await?;
    let options = Options {
        name: query.name,
        ..Options::default()
    };
    let (archive, name) = Box::pin(build_package(
        &state,
        &headers,
        &actor,
        &id,
        options,
        Some(cover),
    ))
    .await?;
    if query.download {
        download(archive, &name).await
    } else {
        use axum::response::IntoResponse;
        Ok(publish(state, actor, space, archive).await?.into_response())
    }
}

async fn publish(
    state: AppState,
    actor: String,
    space: Option<String>,
    archive: TemporaryFile,
) -> AppResult<(StatusCode, Json<crate::db::NativeTemplateEntry>)> {
    let permit = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let entry =
        super::template_library::publish_package(state, actor, space, archive, Some(permit)).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

async fn build_package(
    state: &AppState,
    headers: &HeaderMap,
    actor: &str,
    id: &str,
    options: Options,
    uploaded_cover: Option<Vec<u8>>,
) -> AppResult<(TemporaryFile, String)> {
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let project = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|e| AppError::internal("load template project", e))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    let name = match options.name.as_deref() {
        Some(value) => {
            let value = value.trim();
            if value.is_empty()
                || value.len() > crate::db::MAX_COMPOSITION_PROJECT_NAME_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(AppError::bad_request("Некорректное название шаблона"));
            }
            value.to_owned()
        }
        None => project.name.clone(),
    };
    let native: NestedCompositionProject = serde_json::from_value(project.document.clone())
        .map_err(|_| AppError::bad_request("Шаблоны требуют нативный проект с таймлайнами"))?;
    if (options.use_saved_cover
        && (options.auto_poster
            || options.poster_time_us.is_some()
            || options.poster_composition_id.is_some()))
        || (options.auto_poster && options.poster_time_us.is_some())
    {
        return Err(AppError::bad_request("Выберите один источник обложки"));
    }
    let plan = template_bundle::plan(&native).map_err(|e| AppError::bad_request(e.to_string()))?;
    let mut inputs = Vec::new();
    let mut originals = Vec::new();
    let mut required: std::collections::BTreeSet<_> = plan.assets.keys().cloned().collect();
    let composition = options
        .poster_composition_id
        .as_deref()
        .unwrap_or(&native.root);
    if let Some(time) = options.poster_time_us {
        let ids = native
            .frame_image_sources(composition, time, &Default::default())
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        required.extend(ids.iter().map(|id| native.library_source_id(id).clone()));
    } else if !options.auto_poster && options.poster_composition_id.is_some() {
        return Err(AppError::bad_request("Укажите время кадра обложки"));
    }
    for source in &required {
        if !project.source_ids.iter().any(|id| id == source.as_str()) {
            return Err(AppError::conflict(
                "Ресурс шаблона отсутствует в сохранённом проекте",
            ));
        }
        crate::handlers::library::authorize_source_access(&state, source.as_str(), &headers)
            .await?;
        let entry = state
            .library
            .get(source.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::conflict("Ресурс шаблона отсутствует"))?;
        let path = validated_source_path(&state, &entry).await?;
        let revision = SourceVideoRevision::read(&path)
            .await
            .map_err(|_| AppError::conflict("Ресурс шаблона отсутствует"))?;
        if plan.assets.contains_key(source) {
            inputs.push(template_bundle::Input {
                source_id: SourceId::parse(source.as_str())
                    .map_err(|e| AppError::bad_request(e.to_string()))?,
                path: path.clone(),
            });
        }
        originals.push((source.clone(), entry, path, revision));
    }
    let poster = if let Some(cover) = uploaded_cover {
        Some(
            tokio::task::spawn_blocking(move || {
                super::template_frame::encode_cover(cover, image::ImageFormat::Png)
            })
            .await
            .map_err(|e| AppError::internal("join uploaded cover encoder", e))??,
        )
    } else if options.auto_poster {
        super::template_frame::automatic_concat_cover(
            state,
            headers,
            &project,
            &native,
            composition,
            image::ImageFormat::Png,
        )
        .await?
    } else if let Some(time) = options.poster_time_us {
        Some(
            Box::pin(super::template_frame::render(
                state.clone(),
                headers.clone(),
                native.clone(),
                crate::handlers::concat_video_bound::SavedProject {
                    project_id: id.to_owned(),
                    expected_revision: project.revision,
                },
                composition.to_owned(),
                time,
                image::ImageFormat::Png,
            ))
            .await?,
        )
    } else if options.use_saved_cover {
        let cover = native
            .cover_jpeg
            .clone()
            .ok_or_else(|| AppError::bad_request("У сохранённого проекта нет обложки"))?;
        Some(
            tokio::task::spawn_blocking(move || saved_cover_png(&cover))
                .await
                .map_err(|e| AppError::internal("join project cover encoder", e))??,
        )
    } else {
        None
    };
    let mut media_bytes = poster.as_ref().map_or(0, |bytes| bytes.len() as u64);
    for input in &inputs {
        let length = tokio::fs::metadata(&input.path)
            .await
            .map_err(|error| AppError::internal("stat template asset", error))?
            .len();
        media_bytes = media_bytes
            .checked_add(length)
            .ok_or_else(|| AppError::payload_too_large("Пакет шаблона слишком большой"))?;
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    let _cancel_on_drop = cancel.clone().drop_guard();
    let mut cache_project = project.clone();
    if !plan.slot_masks.is_empty() {
        let mut fixed = native.clone();
        for node in fixed.compositions.values_mut() {
            for track in &mut node.content.tracks {
                if let crate::domain::composition::CompositionTrack::Video { clips, .. } = track {
                    for clip in clips {
                        if fixed
                            .project_media
                            .get(&clip.source_id)
                            .is_some_and(|item| item.placeholder)
                        {
                            clip.concat_cutout = None;
                        }
                    }
                }
            }
        }
        cache_project.document =
            serde_json::to_value(fixed).map_err(|e| AppError::bad_request(e.to_string()))?;
    }
    let video = run_with_timeout(
        video_export::collect(state, actor, &cache_project, media_bytes, &cancel),
        ARCHIVE_IO_TIMEOUT,
        "Сбор видеомасок шаблона превысил лимит времени",
    )
    .await?
    .map_err(|error| AppError::conflict(error.to_string()))?;
    let authorities = video.authorities.clone();
    let output = state
        .staging_dir()
        .join(format!("{}.vktemplate-export", Uuid::new_v4()));
    let package_name = name.clone();
    // The worker owns the output guard and admission permit even after HTTP cancellation.
    let archive = tokio::task::spawn_blocking(move || -> anyhow::Result<TemporaryFile> {
        let _slot = slot;
        let archive = TemporaryFile::new(output);
        let file = std::fs::File::options()
            .write(true)
            .read(true)
            .create_new(true)
            .open(archive.path())?;
        template_bundle::write_with_resources(
            file,
            &package_name,
            &native,
            &inputs,
            poster.as_deref(),
            &video.entries,
            &video.inputs,
        )?
        .sync_all()?;
        Ok(archive)
    })
    .await
    .map_err(|e| AppError::internal("join template export", e))?
    .map_err(|e| AppError::conflict(e.to_string()))?;
    for (source, entry, path, revision) in originals {
        crate::handlers::library::authorize_source_access(&state, source.as_str(), &headers)
            .await?;
        let current = state
            .library
            .get(source.as_str())
            .await
            .ok_or_else(|| AppError::conflict("Ресурс шаблона изменился"))?;
        if serde_json::to_value(&current).ok() != serde_json::to_value(&entry).ok()
            || validated_source_path(&state, &current).await? != path
            || SourceVideoRevision::read(&path)
                .await
                .map_err(|_| AppError::conflict("Ресурс шаблона изменился"))?
                != revision
        {
            return Err(AppError::conflict("Ресурс шаблона изменился"));
        }
    }
    crate::handlers::concat_video_bound::recheck_authorities(state, actor, &authorities)
        .await
        .map_err(|error| AppError::conflict(error.to_string()))?;
    archive_actor(state, headers, true).await?;
    let current = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|e| AppError::internal("recheck template project", e))?
        .ok_or_else(|| AppError::conflict("Доступ к проекту отозван"))?;
    if current.revision != project.revision
        || current.document != project.document
        || current.source_ids != project.source_ids
    {
        return Err(AppError::conflict(
            "Проект изменился во время упаковки шаблона",
        ));
    }
    Ok((archive, name))
}

fn saved_cover_png(cover: &str) -> AppResult<Vec<u8>> {
    let invalid = || AppError::bad_request("Некорректная обложка проекта");
    let bytes = STANDARD.decode(cover).map_err(|_| invalid())?;
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(invalid());
    }
    super::template_frame::encode_cover(bytes, image::ImageFormat::Png)
}
