//! Reanalyse authored Concat video cutouts when saving an imported project.
use super::*;
use crate::{
    domain::{
        composition::{
            CompositionClipId, CompositionSource, CompositionTrack, SourceId, SourceKind,
        },
        concat_cutout_import::Subject,
        concat_project_import::{
            preflight_generated_stills, prepare_video_import, ImportRequest, PendingStillCutout,
        },
        concat_video_binding::Recipe,
        nested_composition::NestedCompositionProject,
        video_clock::presented_source_bounds,
    },
    error::ApiJson,
    handlers::{concat_project_import_ready, concat_video_replace as masks, concat_video_run},
    services::{
        concat_video_sample::Input,
        concat_video_selection,
        filmcraft_video_preview::SourceVideoRevision,
        local_enhance_video::{resolve_tools, Tools},
    },
};
use anyhow::{Context as _, Result};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    request: ImportRequest,
    name: Option<String>,
    #[serde(default)]
    prepared_stills: Vec<concat_project_import_ready::SavedReference>,
    #[serde(default)]
    rebuild_stills: bool,
    cover_jpeg: Option<String>,
    #[serde(default)]
    subtitle_documents: Vec<crate::domain::metadata_burnin::SubtitleDocument>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    #[serde(flatten)]
    saved: ProjectArchiveImportResponse,
    timeline_ids: BTreeMap<String, String>,
}

pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Created>)> {
    let actor = archive_actor(&state, &headers, false).await?;
    let space = archive_space_id(&headers)?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let saved = tokio::spawn(async move {
        let _slot = slot;
        Box::pin(build(state, headers, request, actor, space)).await
    })
    .await
    .map_err(|e| AppError::internal("join Concat mask import", e))??;
    Ok((StatusCode::CREATED, Json(saved)))
}

struct Source {
    descriptor: CompositionSource,
    path: PathBuf,
    revision: SourceVideoRevision,
}

async fn build(
    state: AppState,
    headers: HeaderMap,
    mut input: Request,
    actor: String,
    space: Option<String>,
) -> AppResult<Created> {
    if let Some(space) = &space {
        let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collaboration_space_members WHERE space_id=? AND actor=? AND role IN ('owner','editor'))")
            .bind(space).bind(&actor).fetch_one(state.db.pool()).await
            .map_err(|e| AppError::internal("authorize Concat destination", e))?;
        if !allowed {
            return Err(AppError::forbidden(
                "Недостаточно прав для создания проекта",
            ));
        }
    }
    let name = template_import::project_name(input.name)?.unwrap_or_else(|| "Проект Concat".into());
    if !crate::domain::metadata_burnin::validate_subtitle_documents(&input.subtitle_documents) {
        return Err(AppError::bad_request("Некорректный пул субтитров шаблона"));
    }
    if input
        .cover_jpeg
        .as_ref()
        .is_some_and(|cover| cover.len() > 512 * 1024 || !cover.starts_with("/9j/"))
    {
        return Err(AppError::bad_request("Некорректная обложка шаблона"));
    }
    let cancel = state.shutdown_token().child_token();
    let _cancel_on_drop = cancel.clone().drop_guard();
    if input.request.media_bindings.len() > 32 {
        return Err(AppError::bad_request("Слишком много источников Concat"));
    }
    let mut sources: BTreeMap<SourceId, Source> = BTreeMap::new();
    for supplied in input.request.media_bindings.values_mut() {
        if let Some(source) = sources.get(&supplied.id) {
            *supplied = source.descriptor.clone();
            continue;
        }
        crate::handlers::library::authorize_source_access(&state, supplied.id.as_str(), &headers)
            .await?;
        let entry = state
            .library
            .get(supplied.id.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::not_found("Источник Concat не найден"))?;
        let path = validated_source_path(&state, &entry).await?;
        let revision = SourceVideoRevision::read(&path).await.map_err(mask_error)?;
        let probe = tools::probe_video(&state.process_runtime, &path)
            .await
            .map_err(import_probe_error)?;
        let kind = match media_type(&probe) {
            Some("video") => SourceKind::Video,
            Some("image") => SourceKind::Image,
            Some("audio") => SourceKind::Audio,
            _ => {
                return Err(AppError::unsupported_media_type(
                    "Тип источника Concat не поддерживается",
                ))
            }
        };
        let (_, descriptor) = crate::domain::concat_registered_media::replacement(
            supplied.id.clone(),
            &entry.filename,
            kind,
            &probe,
        )
        .map_err(mask_error)?;
        *supplied = descriptor.clone();
        sources.insert(
            descriptor.id.clone(),
            Source {
                descriptor,
                path,
                revision,
            },
        );
    }
    if input.prepared_stills.len() > 16 * 512 {
        return Err(AppError::bad_request(
            "Слишком много масок изображений Concat",
        ));
    }
    for reference in &input.prepared_stills {
        crate::handlers::cutout_assets::authorize(&state, &reference.project_id, &actor).await?;
    }
    let _permit = crate::handlers::concat_cutout_analyse::acquire_analysis(&state, &cancel)
        .await
        .map_err(mask_error)?;
    let (stills, still_checks) = concat_project_import_ready::verify_stills(
        &state,
        &actor,
        &headers,
        &input.request.document,
        input.prepared_stills,
        &cancel,
    )
    .await
    .map_err(mask_error)?;
    let still_jobs = if input.rebuild_stills {
        preflight_generated_stills(&input.request, &stills)
            .map_err(|error| AppError::bad_request(format!("{error:#}")))?
    } else {
        Vec::new()
    };
    let _models = tokio::select! {
        guard = state.cutout_model_ops.lock() => guard,
        _ = cancel.cancelled() => return Err(AppError::conflict("Анализ отменён")),
    };
    let mut prepared = Vec::new();
    let mut generated_stills = Vec::new();
    let region_stage = tempfile::tempdir_in(state.staging_dir()).map_err(mask_error)?;
    let mut stills = stills;
    let mut sam = None;
    if still_jobs
        .iter()
        .any(|job| !job.settings.region_source_seconds.is_empty())
    {
        sam = Some(tokio::select! {
            guard = state.sam_model_ops.lock() => guard,
            _ = cancel.cancelled() => return Err(AppError::conflict("Анализ отменён")),
        });
    }
    for PendingStillCutout {
        timeline_id,
        clip_id,
        media_id,
        settings,
    } in still_jobs
    {
        let descriptor = input
            .request
            .media_bindings
            .get(&media_id)
            .ok_or_else(|| AppError::bad_request("Не задан источник маски изображения"))?;
        if descriptor.kind != SourceKind::Image {
            return Err(AppError::bad_request("Для этой маски выберите изображение"));
        }
        let source = sources
            .get(&descriptor.id)
            .ok_or_else(|| AppError::bad_request("Источник маски изображения не найден"))?;
        let generated = Box::pin(generated_still::prepare(
            &state,
            &actor,
            descriptor,
            &source.path,
            settings,
            region_stage.path(),
            &mut prepared,
            &cancel,
        ))
        .await
        .map_err(mask_error)?;
        stills
            .entry(timeline_id)
            .or_default()
            .insert(clip_id, generated.binding.clone());
        generated_stills.push(generated);
    }
    let (report, pending) = prepare_video_import(input.request, &stills)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let mut project = report
        .project
        .ok_or_else(|| AppError::bad_request(report.diagnostics.join("\n")))?;
    project.cover_jpeg = input.cover_jpeg;
    if !input.subtitle_documents.is_empty() {
        let root = project
            .compositions
            .get_mut(&project.root)
            .ok_or_else(|| AppError::bad_request("Корневой таймлайн Concat отсутствует"))?;
        root.content
            .metadata
            .get_or_insert_with(Default::default)
            .subtitle_documents = input.subtitle_documents;
    }
    project
        .dependency_order()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if pending.is_empty() && generated_stills.is_empty() {
        return Err(AppError::bad_request(
            "В проекте Concat нет масок для восстановления",
        ));
    }
    if project
        .project_media
        .values()
        .any(|media| media.placeholder)
    {
        return Err(AppError::bad_request(
            "Выберите файлы для всех слотов Concat",
        ));
    }
    if sam.is_none()
        && pending
            .iter()
            .any(|mask| !mask.settings.region_source_seconds.is_empty())
    {
        sam = Some(tokio::select! {
            guard = state.sam_model_ops.lock() => guard,
            _ = cancel.cancelled() => return Err(AppError::conflict("Анализ отменён")),
        });
    }
    let mut groups = BTreeMap::new();
    for mask in pending {
        let subject = match mask.settings.subject {
            Subject::Auto => "auto",
            Subject::Person => "person",
            Subject::Object => "object",
        };
        groups
            .entry((mask.source_id.clone(), subject))
            .or_insert_with(Vec::new)
            .push(mask);
    }
    let mut decided = BTreeMap::new();
    for ((media_id, subject), group) in groups {
        let source = sources
            .get(project.library_source_id(&media_id))
            .ok_or_else(|| {
                AppError::bad_request("Не найден зарегистрированный файл для видеомаски")
            })?;
        let (_, hash) =
            crate::project_archive::fingerprint_file_with_limit(&source.path, masks::CACHE_BUDGET)
                .await
                .map_err(mask_error)?;
        let start = group
            .iter()
            .map(|mask| {
                clip_start(
                    &project,
                    &mask.timeline_id,
                    &mask.clip_id,
                    &source.descriptor,
                )
            })
            .collect::<Result<Vec<_>>>()
            .map_err(mask_error)?
            .into_iter()
            .min()
            .unwrap_or_default();
        let model_hash = select_model(
            &state,
            &actor,
            group[0].settings.subject,
            source,
            &hash,
            start,
            &cancel,
        )
        .await
        .map_err(mask_error)?;
        let bindings = group.into_iter().map(|mask| {
            let mut correction = mask.settings.correction;
            if !correction.custom {
                correction.strokes.clear();
            }
            (
                (mask.timeline_id, mask.clip_id),
                (
                    Recipe {
                        model_hash: model_hash.clone(),
                        subject: subject.into(),
                        feather: Some(correction.feather),
                        correction: Some(correction),
                    },
                    0,
                ),
            )
        });
        let (_, current_hash, combined) = decided
            .entry((media_id, model_hash.clone()))
            .or_insert_with(|| (source, hash.clone(), BTreeMap::new()));
        if *current_hash != hash {
            return Err(AppError::conflict("Источник Concat изменился"));
        }
        combined.extend(bindings);
    }
    let mut generated = Vec::with_capacity(decided.len());
    for ((media_id, _), (source, hash, bindings)) in decided {
        let (cache, regions) = Box::pin(masks::rebuild(
            &state,
            &actor,
            &mut project,
            bindings,
            &source.descriptor,
            &source.path,
            &hash,
            &cancel,
        ))
        .await
        .map_err(mask_error)?;
        generated.push(generated_video::Generated {
            media_id,
            original: source.path.clone(),
            cache,
            regions,
        });
    }
    for (id, source) in &sources {
        crate::handlers::library::authorize_source_access(&state, id.as_str(), &headers).await?;
        let current = state
            .library
            .get(id.as_str())
            .await
            .ok_or_else(|| AppError::conflict("Источник Concat исчез"))?;
        if validated_source_path(&state, &current).await? != source.path
            || SourceVideoRevision::read(&source.path)
                .await
                .map_err(mask_error)?
                != source.revision
        {
            return Err(AppError::conflict("Источник Concat изменился"));
        }
    }
    let document = serde_json::to_value(project).map_err(mask_error)?;
    let source_ids = crate::db::extract_document_source_ids(&document).map_err(mask_error)?;
    let mut video = video_import::Imported::new();
    generated_video::prepare(
        &state,
        &actor,
        &document,
        &mut generated,
        &mut prepared,
        &mut video,
        &cancel,
    )
    .await
    .map_err(mask_error)?;
    concat_project_import_ready::recheck_stills(&state, &actor, still_checks, &cancel)
        .await
        .map_err(mask_error)?;
    let project = publish_with_rollback(
        &state,
        &prepared,
        &name,
        &document,
        &source_ids,
        &actor,
        space.as_deref(),
        &video.registrations,
    )
    .await?;
    video.commit();
    drop(sam);
    drop(generated_stills);
    Ok(Created {
        saved: ProjectArchiveImportResponse {
            project,
            source_mapping: BTreeMap::new(),
        },
        timeline_ids: report.timeline_ids,
    })
}

fn clip_start(
    project: &NestedCompositionProject,
    timeline: &str,
    clip_id: &CompositionClipId,
    source: &CompositionSource,
) -> Result<u64> {
    let clip = project.compositions[timeline]
        .content
        .tracks
        .iter()
        .filter_map(|track| match track {
            CompositionTrack::Video { clips, .. } => Some(clips),
            _ => None,
        })
        .flatten()
        .find(|clip| &clip.id == clip_id)
        .context("Не найден клип видеомаски")?;
    Ok(presented_source_bounds(&clip.placement, clip.playback_mode, source.duration_ticks)?.0)
}

async fn select_model(
    state: &AppState,
    actor: &str,
    subject: Subject,
    source: &Source,
    hash: &str,
    start: u64,
    cancel: &CancellationToken,
) -> Result<String> {
    let limit = Duration::from_secs(1800);
    if subject != Subject::Auto {
        let (_, engine) = concat_video_selection::acquire_decided(
            subject,
            None,
            |model| concat_video_run::load(state, actor, model),
            cancel,
            limit,
        )
        .await?;
        return Ok(engine.sequence().model_hash().into());
    }
    let tools = resolve_tools().await?;
    let root = state.storage.join("concat-video-analysis");
    tokio::fs::create_dir_all(&root).await?;
    let (progress, _) = tokio::sync::mpsc::unbounded_channel();
    let selected = concat_video_selection::analyse(
        &state.process_runtime,
        Tools {
            ffmpeg: &tools.ffmpeg,
            ffprobe: &tools.ffprobe,
        },
        Input {
            source: &source.path,
            source_hash: hash,
            staging_bytes: masks::CACHE_BUDGET,
            run: crate::domain::concat_cutout_clock::AnalysisRun {
                start_millis: (start / 100_000) * 100,
                frames: 1,
            },
        },
        subject,
        None,
        &root,
        |model| concat_video_run::load(state, actor, model),
        cancel,
        limit,
        &progress,
    )
    .await?
    .context("Модель не выбрана для первого кадра Concat")?;
    Ok(selected.run.metadata.model_hash)
}

fn mask_error(error: impl std::fmt::Display) -> AppError {
    AppError::conflict(format!(
        "Не удалось восстановить видеомаску Concat: {error:#}"
    ))
}
