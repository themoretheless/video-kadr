//! Saved native-project template download with an optional rendered poster.

use super::*;
use crate::{
    domain::{composition::SourceId, nested_composition::NestedCompositionProject},
    services::filmcraft_video_preview::SourceVideoRevision,
    template_bundle,
};

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
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
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let project = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|e| AppError::internal("load template project", e))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    let native: NestedCompositionProject = serde_json::from_value(project.document.clone())
        .map_err(|_| AppError::bad_request("Шаблоны требуют нативный проект с таймлайнами"))?;
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
    } else if options.poster_composition_id.is_some() {
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
    let poster = if let Some(time) = options.poster_time_us {
        Some(
            Box::pin(super::template_frame::render(
                state.clone(),
                headers.clone(),
                native.clone(),
                crate::handlers::concat_video_bound::SavedProject {
                    project_id: id.clone(),
                    expected_revision: project.revision,
                },
                composition.to_owned(),
                time,
            ))
            .await?,
        )
    } else {
        None
    };
    let output = state
        .staging_dir()
        .join(format!("{}.vktemplate-export", Uuid::new_v4()));
    let name = project.name.clone();
    // The worker owns the output guard and admission permit even after HTTP cancellation.
    let archive = tokio::task::spawn_blocking(move || -> anyhow::Result<TemporaryFile> {
        let _slot = slot;
        let archive = TemporaryFile::new(output);
        let file = std::fs::File::options()
            .write(true)
            .read(true)
            .create_new(true)
            .open(archive.path())?;
        template_bundle::write_with_poster(file, &name, &native, &inputs, poster.as_deref())?
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
    archive_actor(&state, &headers, true).await?;
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
    let file = tokio::fs::File::open(archive.path())
        .await
        .map_err(|e| AppError::internal("open template package", e))?;
    let length = file
        .metadata()
        .await
        .map_err(|e| AppError::internal("stat template package", e))?
        .len();
    let base = archive_download_filename(&project.name);
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
