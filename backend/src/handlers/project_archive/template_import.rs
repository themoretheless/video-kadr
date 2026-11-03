//! Verified template upload followed by rollback-safe standalone project creation.
use super::*;
use crate::{
    domain::composition::SourceId, services::filmcraft_video_preview::SourceVideoRevision,
    template_bundle,
};
use axum::extract::Query;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub(super) fills: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Reference {
    media_id: SourceId,
    source_id: SourceId,
}
pub(super) fn parse_references(raw: &str) -> AppResult<Vec<Reference>> {
    if raw.len() > 65536 {
        return Err(AppError::bad_request("Слишком много замен шаблона"));
    }
    let refs: Vec<Reference> = serde_json::from_str(&raw)
        .map_err(|_| AppError::bad_request("Некорректные замены шаблона"))?;
    if refs.len() > crate::domain::composition::MAX_SOURCES
        || refs
            .iter()
            .map(|r| &r.media_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != refs.len()
    {
        return Err(AppError::bad_request(
            "Повторяющиеся или лишние замены шаблона",
        ));
    }
    Ok(refs)
}
pub async fn import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(options): Query<Options>,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<(StatusCode, Json<ProjectArchiveImportResponse>)> {
    let actor = archive_actor(&state, &headers, false).await?;
    let space = archive_space_id(&headers)?;
    let refs = parse_references(&options.fills)?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let archive = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.template-upload", Uuid::new_v4())),
    );
    run_with_timeout(
        receive_archive(multipart, archive.path()),
        ARCHIVE_IO_TIMEOUT,
        "Загрузка шаблона превысила лимит времени",
    )
    .await??;
    // Once admitted, this task owns staging and reaches commit or compensating rollback.
    let result = tokio::spawn(async move {
        let _slot = slot;
        Box::pin(create(state, archive, actor, space, headers, refs)).await
    })
    .await
    .map_err(|e| AppError::internal("join template import", e))??;
    Ok((StatusCode::CREATED, Json(result)))
}
pub(super) async fn create(
    state: AppState,
    archive: TemporaryFile,
    actor: String,
    space: Option<String>,
    headers: HeaderMap,
    refs: Vec<Reference>,
) -> AppResult<ProjectArchiveImportResponse> {
    if let Some(space) = &space {
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id=? AND actor=? AND role IN ('owner','editor'))").bind(space).bind(&actor).fetch_one(state.db.pool()).await.map_err(|e|AppError::internal("authorize template destination",e))?;
        if !allowed {
            return Err(AppError::forbidden(
                "Недостаточно прав для создания проекта",
            ));
        }
    }
    let stage = state.staging_dir();
    let parsed = tokio::task::spawn_blocking(move || {
        let _archive = archive;
        template_bundle::read(std::fs::File::open(_archive.path())?, &stage)
    })
    .await
    .map_err(|e| AppError::internal("join template reader", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    let slots: std::collections::BTreeSet<_> = parsed
        .manifest
        .project
        .project_media
        .iter()
        .filter(|(_, item)| item.placeholder)
        .map(|(id, _)| id)
        .collect();
    if slots != refs.iter().map(|r| &r.media_id).collect() {
        return Err(AppError::bad_request(
            "Выберите замену для каждого слота шаблона",
        ));
    }
    if (!parsed.assets.is_empty() || !refs.is_empty()) && !state.tools.ffmpeg {
        return Err(AppError::service_unavailable("Требуется локальный ffprobe"));
    }
    let mut prepared = Vec::new();
    let mut design = BTreeMap::new();
    let mut reserved = HashSet::new();
    for (asset, staged) in parsed.manifest.assets.iter().zip(parsed.assets) {
        let probe = tools::probe_video(&state.process_runtime, staged.path())
            .await
            .map_err(import_probe_error)?;
        let kind = media_type(&probe)
            .ok_or_else(|| AppError::unsupported_media_type("Неизвестный ресурс шаблона"))?;
        let extension = safe_upload_extension(&probe)
            .ok_or_else(|| AppError::unsupported_media_type("Неизвестный формат ресурса"))?;
        let new_id = allocate_source_id(&state, &mut reserved, extension).await?;
        let filename = format!("{new_id}.{extension}");
        for item in parsed
            .manifest
            .project
            .project_media
            .values()
            .filter(|item| item.library_source_id.as_ref() == Some(&asset.key))
        {
            if serde_json::to_value(item.source.kind).ok() != Some(json!(kind)) {
                return Err(AppError::bad_request(
                    "Тип встроенного ресурса не совпадает с шаблоном",
                ));
            }
        }
        let archived = ArchivedSource {
            source_id: asset.key.as_str().into(),
            filename: filename.clone(),
            media_type: kind.into(),
            title: None,
            original_filename: None,
            duration: Some(probe.duration),
            width: (probe.width > 0).then_some(probe.width),
            height: (probe.height > 0).then_some(probe.height),
            favorite: false,
            tags: vec![],
            byte_length: asset.byte_length,
            sha256: asset.sha256.clone(),
        };
        let entry = MediaEntry {
            id: new_id.clone(),
            kind: "source".into(),
            filename: filename.clone(),
            storage_key: None,
            url: format!("/files/sources/{filename}"),
            title: None,
            original_filename: None,
            media_type: Some(kind.into()),
            duration: Some(probe.duration),
            width: archived.width,
            height: archived.height,
            fps: probe.fps,
            vcodec: probe.vcodec,
            acodec: probe.acodec,
            size_bytes: Some(asset.byte_length),
            created_at: now_secs(),
        };
        design.insert(
            asset.key.clone(),
            SourceId::parse(&new_id).map_err(|e| AppError::bad_request(e.to_string()))?,
        );
        prepared.push(PreparedSource {
            archived,
            staged: staged.into_archive(filename.clone()),
            new_id,
            destination_filename: filename,
            entry,
        });
    }
    let mut fills = BTreeMap::new();
    let mut originals = Vec::new();
    for reference in refs {
        crate::handlers::library::authorize_source_access(
            &state,
            reference.source_id.as_str(),
            &headers,
        )
        .await?;
        let entry = state
            .library
            .get(reference.source_id.as_str())
            .await
            .filter(|e| e.kind == "source")
            .ok_or_else(|| AppError::not_found("Замена слота не найдена"))?;
        let path = validated_source_path(&state, &entry).await?;
        let revision = SourceVideoRevision::read(&path)
            .await
            .map_err(|_| AppError::conflict("Источник изменился"))?;
        let metadata = state
            .db
            .get_library_metadata(&entry.id)
            .await
            .map_err(|e| AppError::internal("read template replacement", e))?;
        let probe = tools::probe_video(&state.process_runtime, &path)
            .await
            .map_err(import_probe_error)?;
        let kind = media_type(&probe)
            .ok_or_else(|| AppError::unsupported_media_type("Неизвестная замена слота"))?;
        let name = metadata
            .as_ref()
            .and_then(|m| m.title.as_deref())
            .or(entry.title.as_deref())
            .or(entry.original_filename.as_deref())
            .unwrap_or(&entry.filename);
        let (replacement, source) = crate::domain::concat_registered_media::replacement(
            reference.source_id.clone(),
            name,
            serde_json::from_value(json!(kind))
                .map_err(|e| AppError::bad_request(e.to_string()))?,
            &probe,
        )
        .map_err(|e| AppError::bad_request(e.to_string()))?;
        fills.insert(
            reference.media_id,
            template_bundle::SlotFill {
                replacement,
                source,
            },
        );
        originals.push((reference.source_id, entry, path, revision, metadata));
    }
    let name = parsed.manifest.name.clone();
    let mapping = design
        .iter()
        .map(|(a, b)| (a.as_str().into(), b.as_str().into()))
        .collect();
    let project = tokio::task::spawn_blocking(move || {
        template_bundle::instantiate(&parsed.manifest, &design, &fills)
    })
    .await
    .map_err(|e| AppError::internal("join template assembly", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    let document =
        serde_json::to_value(project).map_err(|e| AppError::bad_request(e.to_string()))?;
    for (id, entry, path, revision, metadata) in originals {
        crate::handlers::library::authorize_source_access(&state, id.as_str(), &headers).await?;
        let current = state
            .library
            .get(id.as_str())
            .await
            .ok_or_else(|| AppError::conflict("Замена слота исчезла"))?;
        if serde_json::to_value(&entry).ok() != serde_json::to_value(&current).ok()
            || validated_source_path(&state, &current).await? != path
            || SourceVideoRevision::read(&path)
                .await
                .map_err(|_| AppError::conflict("Замена слота изменилась"))?
                != revision
            || state
                .db
                .get_library_metadata(id.as_str())
                .await
                .map_err(|e| AppError::internal("recheck template replacement", e))?
                != metadata
        {
            return Err(AppError::conflict("Замена слота изменилась"));
        }
    }
    let source_ids = crate::db::extract_document_source_ids(&document)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let saved = publish_with_rollback(
        &state,
        &prepared,
        &name,
        &document,
        &source_ids,
        &actor,
        space.as_deref(),
        &[],
    )
    .await?;
    for source in &prepared {
        if let Some(entry) = state.library.get(&source.new_id).await {
            state.index_media(&entry).await;
        }
    }
    Ok(ProjectArchiveImportResponse {
        project: saved,
        source_mapping: mapping,
    })
}

/// Verify a package and expose only gallery/slot metadata, without publication.
pub async fn inspect(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<Response> {
    archive_actor(&state, &headers, false).await?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let archive = TemporaryFile::new(
        state
            .staging_dir()
            .join(format!("{}.template-inspect", Uuid::new_v4())),
    );
    run_with_timeout(
        receive_archive(multipart, archive.path()),
        ARCHIVE_IO_TIMEOUT,
        "Проверка шаблона превысила лимит времени",
    )
    .await??;
    let stage = state.staging_dir();
    let info = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        let _slot = slot;
        let _archive = archive;
        let parsed = template_bundle::read(std::fs::File::open(_archive.path())?, &stage)?;
        let info = crate::domain::project_template::inspect(&parsed.manifest.project, None)?;
        Ok(json!({"schemaVersion":1,"name":parsed.manifest.name,"template":info}))
    })
    .await
    .map_err(|e| AppError::internal("join template inspection", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(info)).into_response())
}
