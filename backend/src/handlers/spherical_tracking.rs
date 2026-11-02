//! Model-free camera following from project-owned, cryptographically verified SAM regions.
use crate::{
    domain::{
        composition::{Composition, CompositionClipId, CompositionTrack},
        spherical_tracking::{frame_sequence, mask_centre, mask_fov, Centre, Framing},
        video_region::Attachment,
    },
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use anyhow::{ensure, Result};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    composition_id: Option<String>,
    clip_id: String,
    expected_revision: u64,
    region: Attachment,
    source_times_us: Vec<u64>,
    #[serde(default)]
    framing: Option<Framing>,
    #[serde(default)]
    reidentify: bool,
    #[serde(default)]
    spherical: Option<crate::domain::spherical_video::SphericalVideo>,
    #[serde(default)]
    parameter_start_us: Option<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    centres: Vec<Centre>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_indices: Option<Vec<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    states: Option<Vec<String>>,
}
pub async fn handler(
    State(state): State<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<Response>> {
    if request.source_times_us.len() < 2
        || request.source_times_us.len() > 300
        || request.source_times_us.windows(2).any(|t| t[0] >= t[1])
        || request.source_times_us.last().unwrap() - request.source_times_us[0] > 60_000_000
    {
        return Err(AppError::bad_request("invalid spherical mask sample times"));
    }
    if request.reidentify && request.source_times_us.len() > 60 {
        return Err(AppError::bad_request(
            "neural identity recovery supports at most60 sampled frames per request",
        ));
    }
    if let Some(framing) = request.framing {
        framing
            .validate()
            .map_err(|e| AppError::bad_request(e.to_string()))?;
    }
    if let Some(parameters) = &request.spherical {
        parameters
            .validate()
            .map_err(|e| AppError::bad_request(e.to_string()))?;
        if parameters.input == crate::domain::spherical_video::Input::DualFisheye
            && !request.reidentify
        {
            return Err(AppError::bad_request(
                "raw fisheye camera following requires neural panorama preparation",
            ));
        }
    }
    if request.parameter_start_us.is_some() && request.spherical.is_none() {
        return Err(AppError::bad_request(
            "parameterStartUs requires spherical parameters",
        ));
    }
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project, &actor).await?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let _guard = cancel.clone().drop_guard();
    let worker = tokio::spawn(async move {
        let result: Result<Response> = async {
            let _permit = super::concat_cutout_analyse::acquire_analysis(&state, &cancel).await?;
            let saved = state
                .db
                .get_composition_project_for(&project, &actor)
                .await?
                .ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
            ensure!(
                saved.revision == request.expected_revision,
                "project changed"
            );
            let content = if let Some(nodes) = saved.document.get("compositions") {
                &nodes[request
                    .composition_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("timeline required"))?]["content"]
            } else {
                ensure!(request.composition_id.is_none(), "unexpected timeline");
                &saved.document
            };
            let composition: Composition = serde_json::from_value(content.clone())?;
            composition.validate()?;
            let id = CompositionClipId::parse(request.clip_id)?;
            let clip = composition
                .tracks
                .iter()
                .find_map(|t| match t {
                    CompositionTrack::Video {
                        locked: false,
                        clips,
                        ..
                    } => clips.iter().find(|c| c.id == id && c.enabled),
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("unlocked video required"))?;
            let parameter_start = request
                .parameter_start_us
                .unwrap_or(clip.placement.source_in_tick);
            ensure!(
                parameter_start >= clip.placement.source_in_tick
                    && parameter_start <= request.source_times_us[0],
                "spherical parameter origin outside saved source interval"
            );
            if request.reidentify || request.spherical.is_some() {
                ensure!(
                    clip.playback_mode == crate::domain::composition::PlaybackMode::Forward
                        && clip.placement.speed == 1.
                        && clip.placement.speed_ramp.is_none()
                        && clip.placement.concat_retime.is_none()
                        && clip.effect_timing.is_none()
                        && clip.stabilization
                            == crate::domain::composition::StabilizationSpec::Disabled
                        && clip.placement.source_timeline_offset_tick.unwrap_or(0) == 0
                        && clip.placement.source_head_timeline_tick.unwrap_or(0) == 0,
                    "semantic camera proposal requires an ordinary forward source clock"
                );
                ensure!(
                    !composition.tracks.iter().any(|track| match track {
                        CompositionTrack::Video { transitions, .. } =>
                            transitions.iter().any(|t| t.concat_overlap.is_some()
                                && (t.from_clip_id.as_ref() == Some(&id)
                                    || t.to_clip_id.as_ref() == Some(&id))),
                        _ => false,
                    }),
                    "semantic camera proposal cannot use a source-overlap transition clock"
                );
            }
            let pair = composition
                .metadata
                .as_ref()
                .and_then(|m| m.media_versions.get(&id))
                .ok_or_else(|| anyhow::anyhow!("saved semantic region required"))?;
            let video = pair
                .video_cutout
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("saved video cutout required"))?;
            ensure!(
                clip.source_id == pair.original_source_id || clip.source_id == pair.copy_source_id,
                "clip does not use semantic original or processed source"
            );
            ensure!(
                video.tracked_regions.contains(&request.region)
                    && pair.original_source_id.as_str() == request.region.source_id,
                "semantic region does not belong to clip"
            );
            ensure!(
                request
                    .source_times_us
                    .iter()
                    .all(|t| *t >= clip.placement.source_in_tick
                        && *t < clip.placement.source_out_tick),
                "mask samples outside source clip range"
            );
            let lease = super::concat_video_region_cache::load(
                &state,
                &actor,
                &project,
                saved.revision,
                &request.region,
                &cancel,
            )
            .await?;
            let framing = request.framing.map(|mut f| {
                // The source raster is projected before the downstream canvas transform.
                // Fit both vertical extents conservatively when their aspects differ.
                let source_aspect =
                    if request.spherical.as_ref().is_some_and(|p| {
                        p.input == crate::domain::spherical_video::Input::DualFisheye
                    }) {
                        2.
                    } else {
                        f64::from(request.region.width) / f64::from(request.region.height)
                    };
                f.aspect_ratio = f.aspect_ratio.max(source_aspect);
                f
            });
            let recovery = if request.reidentify {
                Some(
                    super::spherical_reidentify::recover(
                        &state,
                        &actor,
                        &request.region,
                        &request.source_times_us,
                        &lease,
                        request.spherical.as_ref(),
                        parameter_start,
                        &cancel,
                    )
                    .await?,
                )
            } else {
                None
            };
            let mut measured_indices = Vec::new();
            let mut desired_fovs = Vec::new();
            let mut centres = Vec::with_capacity(request.source_times_us.len());
            for (sample_index, &time) in request.source_times_us.iter().enumerate() {
                let probabilities = if let Some(recovery) = &recovery {
                    let Some(mask) = &recovery.masks[sample_index] else {
                        continue;
                    };
                    mask.clone()
                } else {
                    let ordinal = lease.reader.ordinal(time)? as u64;
                    ensure!(
                        !request
                            .region
                            .forward_lost_at
                            .is_some_and(|at| ordinal >= at)
                            && !request
                                .region
                                .backward_lost_at
                                .is_some_and(|at| ordinal <= at),
                        "semantic target lost inside interval"
                    );
                    lease
                        .reader
                        .read(&request.region.source_hash, time, &cancel)?
                        .probabilities
                        .clone()
                };
                let mask = crate::domain::concat_cutout_clock::ResolvedMask {
                    width: 256,
                    height: 256,
                    probabilities,
                };
                measured_indices.push(sample_index);
                let centre = mask_centre(mask.width, mask.height, &mask.probabilities)?;
                if let Some(framing) = framing {
                    desired_fovs.push(mask_fov(
                        mask.width,
                        mask.height,
                        &mask.probabilities,
                        centre,
                        framing,
                    )?);
                }
                let centre = if let Some(parameters) = &request.spherical {
                    parameters
                        .map_tracking_centre(centre, (time - parameter_start) as f64 / 1_000_000.)?
                } else {
                    centre
                };
                centres.push(centre);
            }
            if let Some(framing) = framing {
                frame_sequence(
                    &mut centres,
                    &desired_fovs,
                    &measured_indices
                        .iter()
                        .map(|i| request.source_times_us[*i])
                        .collect::<Vec<_>>(),
                    framing.initial_fov,
                )?;
            }
            super::cutout_assets::authorize(&state, &project, &actor)
                .await
                .map_err(|_| anyhow::anyhow!("project access revoked"))?;
            let current = state
                .db
                .get_composition_project_for(&project, &actor)
                .await?
                .ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
            ensure!(
                current.revision == saved.revision
                    && current.document == saved.document
                    && !cancel.is_cancelled(),
                "project changed during semantic tracking"
            );
            ensure!(
                centres.len() >= 2,
                "target has fewer than two unambiguous identity measurements"
            );
            Ok(Response {
                centres,
                sample_indices: recovery.as_ref().map(|_| measured_indices),
                states: recovery.map(|r| r.states),
            })
        }
        .await;
        result
            .map(Json)
            .map_err(|e| AppError::conflict(e.to_string()))
    });
    worker
        .await
        .map_err(|e| AppError::internal("join semantic camera tracking", e))?
}
