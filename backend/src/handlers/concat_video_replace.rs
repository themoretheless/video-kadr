//! Reanalyse a saved masked video before committing its replacement.
use anyhow::{ensure, Context, Result};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path as FilePath, time::Duration};
use tokio_util::sync::CancellationToken;

use crate::{
    db::CompositionProject,
    domain::{
        composition::{
            CompositionClipId, CompositionSource, CompositionTrack, PlaybackMode, SourceId, SourceKind,
        },
        concat_video_binding::{Recipe, RegionTrack},
        nested_composition::NestedCompositionProject,
        video_clock::presented_source_bounds,
    },
    error::{ApiJson, AppError, AppResult},
    services::{
        concat_video_analysis::RunCache,
        concat_video_selection::model_from_hash,
        local_enhance_video::{resolve_tools, Tools},
    },
    state::AppState,
};

pub(crate) mod regions;

pub(crate) const CACHE_BUDGET: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    expected_revision: u64,
    media_id: SourceId,
    source_id: SourceId,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<CompositionProject>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project_id, &actor).await?;
    super::library::authorize_source_access(&state, request.source_id.as_str(), &headers).await?;
    let cancel = state.shutdown_token().child_token();
    let _cancel_on_drop = cancel.clone().drop_guard();
    tokio::spawn(
        async move { Box::pin(replace(&state, &actor, &project_id, &request, &cancel)).await },
    )
    .await
    .map_err(|e| AppError::internal("join video replacement", e))?
    .map(Json)
    .map_err(|e| AppError::conflict(format!("{e:#}")))
}

async fn replace(
    state: &AppState,
    actor: &str,
    project_id: &str,
    request: &Request,
    cancel: &CancellationToken,
) -> Result<CompositionProject> {
    let saved = state
        .db
        .get_composition_project_for(project_id, actor)
        .await?
        .context("Проект недоступен")?;
    ensure!(
        saved.revision == request.expected_revision,
        "Проект изменился"
    );
    let mut draft: NestedCompositionProject = serde_json::from_value(saved.document)?;
    let mut bindings = BTreeMap::new();
    let mut retired_regions = std::collections::BTreeSet::new();
    for (timeline, node) in &mut draft.compositions {
        for track in &mut node.content.tracks {
            let CompositionTrack::Video { clips, transitions, .. } = track else {
                continue;
            };
            for clip in clips
                .iter_mut()
                .filter(|clip| clip.source_id == request.media_id)
            {
                let Some(binding) = clip.concat_cutout.take() else {
                    continue;
                };
                for track in &binding.region_tracks {
                    retired_regions.insert(SourceId::parse(&track.snapshot.mask_source_id)?);
                }
                let previous_in = if clip.placement.concat_retime.is_some() {
                    let source = node.content.sources.get(&clip.source_id).context("Исходник маски недоступен")?;
                    presented_source_bounds(&clip.placement, clip.playback_mode, source.duration_ticks)?.0
                } else if clip.playback_mode == PlaybackMode::Forward
                    && !transitions.iter().any(|transition| {
                        transition.from_clip_id.as_ref() == Some(&clip.id)
                            || transition.to_clip_id.as_ref() == Some(&clip.id)
                    })
                {
                    crate::domain::project_media_fill::visible_constant_speed_window(&clip.placement)
                        .map_or(clip.placement.source_in_tick, |window| window.start)
                } else {
                    clip.placement.source_in_tick
                };
                bindings.insert(
                    (timeline.clone(), clip.id.clone()),
                    (
                        Recipe::from_binding(binding)?,
                        previous_in,
                    ),
                );
            }
        }
    }
    let path = super::cutout_jobs::source(state, request.source_id.as_str(), actor).await?;
    let before = crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await?;
    let probe = crate::tools::probe_video(&state.process_runtime, &path).await?;
    let name = state
        .library
        .get(request.source_id.as_str())
        .await
        .context("Файл замены недоступен")?
        .filename;
    let kind = super::upload::media_type(&probe);
    ensure!(kind == Some("video"), "Для видеомаски требуется видео");
    let (item, source) = crate::domain::concat_registered_media::replacement(
        request.source_id.clone(),
        &name,
        SourceKind::Video,
        &probe,
    )?;
    let (draft, _) =
        crate::domain::project_media::set_placeholder(&draft, &request.media_id, true)?;
    let mut draft =
        crate::domain::project_media_fill::fill(&draft, &request.media_id, &item, &source)?;

    let _permit = super::concat_cutout_analyse::acquire_analysis(state, cancel).await?;
    let _models = tokio::select! {
        guard = state.cutout_model_ops.lock() => guard,
        _ = cancel.cancelled() => anyhow::bail!("Анализ отменён"),
    };
    let needs_sam = bindings
        .values()
        .any(|(binding, _)| binding.tracked_strokes().next().is_some());
    let _sam_operation = if needs_sam {
        Some(tokio::select! {
            guard = state.sam_model_ops.lock() => guard,
            _ = cancel.cancelled() => anyhow::bail!("Анализ отменён"),
        })
    } else {
        None
    };
    let (mut cache, mut regions) = Box::pin(rebuild(
        state, actor, &mut draft, bindings, &source, &path, &before.1, cancel,
    ))
    .await?;
    if !retired_regions.is_empty() {
        crate::domain::project_media::prune_retired_regions(&mut draft, &retired_regions)?;
    }
    draft.dependency_order()?;
    let document = serde_json::to_value(draft)?;
    let sources = crate::db::extract_document_source_ids(&document)?;

    let root = state.storage.join("concat-video-caches");
    tokio::fs::create_dir_all(&root).await?;
    let receipt = cache.publish(&root, CACHE_BUDGET, cancel).await?;
    let result = async {
        let registration = super::concat_video_cache::prepare_archive(
            state,
            actor,
            request.media_id.as_str(),
            &document,
            &path,
            &receipt,
            &cache.portable_identity()?,
            regions.files(),
            cancel,
        )
        .await?;
        let current_path =
            super::cutout_jobs::source(state, request.source_id.as_str(), actor).await?;
        ensure!(
            current_path == path
                && crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await?
                    == before
                && !cancel.is_cancelled(),
            "Файл замены изменился"
        );
        regions.publish(state, saved.space_id.as_deref()).await?;
        state
            .db
            .update_composition_project_adopting_video(
                project_id,
                actor,
                &saved.name,
                &document,
                &sources,
                saved.revision,
                &[registration],
            )
            .await?
            .context("Проект изменился до сохранения замены")
    }
    .await;
    if result.is_err() {
        regions.rollback(state).await;
        let _ = tokio::fs::remove_dir_all(&receipt.directory).await;
    }
    result
}

/// Rebuild bindings and stage their region movies for a video already bound in
/// the draft. The caller holds the analysis permit and model operation locks
/// through cache publication and the project transaction.
pub(crate) async fn rebuild(
    state: &AppState,
    actor: &str,
    draft: &mut NestedCompositionProject,
    mut bindings: BTreeMap<(String, CompositionClipId), (Recipe, u64)>,
    source: &CompositionSource,
    path: &FilePath,
    source_hash: &str,
    cancel: &CancellationToken,
) -> Result<(RunCache, regions::Regions)> {
    let first = bindings
        .values()
        .next()
        .context("У источника нет автоматической видеомаски")?;
    let model = model_from_hash(&first.0.model_hash)?;
    ensure!(
        bindings
            .values()
            .all(|(binding, _)| binding.model_hash == first.0.model_hash),
        "У клипов выбраны разные модели видеомаски"
    );

    for (recipe, _) in bindings.values() {
        recipe.validate()?;
    }

    let engine = super::concat_video_run::load(state, actor, model).await?;
    let identity = engine.sequence().identity().await?;
    let mut cache = RunCache::new_for_model(
        source_hash.to_owned(),
        identity.runtime_hash,
        first.0.model_hash.clone(),
    )?;
    let tools = resolve_tools().await?;
    let mut regions = regions::Regions::new(state)?;
    let sam = if bindings
        .values()
        .any(|(binding, _)| binding.tracked_strokes().next().is_some())
    {
        Some(
            super::sam_models::config_for_owner(state, actor)
                .await
                .map_err(|error| anyhow::anyhow!("{error:?}"))?
                .0,
        )
    } else {
        None
    };
    let mut ranges = Vec::new();
    for (timeline, node) in &mut draft.compositions {
        for track in &mut node.content.tracks {
            let CompositionTrack::Video { clips, .. } = track else {
                continue;
            };
            for clip in clips {
                let Some((mut recipe, previous_in_tick)) =
                    bindings.remove(&(timeline.clone(), clip.id.clone()))
                else {
                    continue;
                };
                let placement = &clip.placement;
                let (first, last) = presented_source_bounds(placement, clip.playback_mode, source.duration_ticks)?;
                ranges.push((first.min(last) as f64 / 1e6, last.max(first) as f64 / 1e6));
                if let Some(correction) = &mut recipe.correction {
                    let previous_in = previous_in_tick as f64 / 1e6;
                    let last_frame_time = source.duration_ticks.saturating_sub(1) as f64 / 1e6;
                    for at in correction
                        .strokes
                        .iter_mut()
                        .filter_map(|stroke| stroke.at.as_mut())
                    {
                        *at = (*at - previous_in).clamp(0.0, last_frame_time);
                    }
                }
                let mut tracks = Vec::new();
                let tracked_strokes: Vec<_> = recipe.tracked_strokes().collect();
                if !tracked_strokes.is_empty() {
                    let config = sam.as_ref().context("Модель SAM недоступна")?;
                    let correction = recipe
                        .correction
                        .as_mut()
                        .context("Нет штрихов областей SAM")?;
                    let mut logical_source = source.clone();
                    logical_source.id = clip.source_id.clone();
                    for stroke in tracked_strokes {
                        let (descriptor, snapshot) = regions
                            .track(
                                state,
                                config,
                                &tools,
                                &logical_source,
                                path,
                                cache.video_hash(),
                                &mut correction.strokes[stroke],
                                cancel,
                            )
                            .await?;
                        tracks.push(RegionTrack { stroke, snapshot });
                        node.content
                            .sources
                            .insert(descriptor.id.clone(), descriptor);
                    }
                }
                clip.concat_cutout = Some(recipe.bind(
                    cache.video_hash().into(),
                    cache.runtime_hash().into(),
                    tracks,
                )?);
            }
        }
    }
    ensure!(
        bindings.is_empty(),
        "Клип видеомаски отсутствует после замены"
    );
    let staging = state.storage.join("concat-video-analysis");
    tokio::fs::create_dir_all(&staging).await?;
    let (progress, _) = tokio::sync::mpsc::unbounded_channel();
    cache
        .fill_missing_with_engine(
            &state.process_runtime,
            Tools {
                ffmpeg: &tools.ffmpeg,
                ffprobe: &tools.ffprobe,
            },
            engine.sequence(),
            path,
            &ranges,
            &staging,
            CACHE_BUDGET,
            cancel,
            Duration::from_secs(1800),
            &progress,
        )
        .await?;
    ensure!(!cache.is_empty(), "Модель не построила видеомаску");
    Ok((cache, regions))
}
