//! Saved-video analysis command. Source paths and ranges are derived server-side.
use anyhow::{ensure, Result};
use axum::{extract::{Path, State}, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use crate::{state::AppState, error::{ApiJson, AppError, AppResult}, domain::composition::{Composition, CompositionTrack, CompositionClipId}};
fn person_subject()->crate::domain::concat_cutout_import::Subject{crate::domain::concat_cutout_import::Subject::Person}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request { operation_id:Option<String>,#[serde(default="person_subject")] subject:crate::domain::concat_cutout_import::Subject, composition_id: Option<String>, clip_id: String, expected_revision: u64 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysed { subject:crate::domain::concat_cutout_import::Subject,model_hash:Option<String>,source_id: String, analysed_runs: usize, analysed_frames: u64, empty_runs: usize, cache_reused: bool }
pub async fn handler(State(state): State<AppState>, Path(project): Path<String>, headers: HeaderMap, ApiJson(request): ApiJson<Request>) -> AppResult<Json<Analysed>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project, &actor).await?;
    let saved = state.db.get_composition_project_for(&project, &actor).await.map_err(|e| AppError::internal("read video target", e))?.ok_or_else(|| AppError::not_found("Проект не найден"))?;
    if saved.revision != request.expected_revision { return Err(AppError::conflict("Проект изменился")); }
    let content = if let Some(nodes) = saved.document.get("compositions") {
        &nodes[request.composition_id.as_deref().ok_or_else(|| AppError::bad_request("Нужен таймлайн"))?]["content"]
    } else {
        if request.composition_id.is_some() { return Err(AppError::bad_request("Лишний таймлайн")); }
        &saved.document
    };
    let composition: Composition = serde_json::from_value(content.clone()).map_err(|_| AppError::bad_request("Неверная композиция"))?;
    composition.validate().map_err(|e| AppError::bad_request(e.to_string()))?;
    let id = CompositionClipId::parse(request.clip_id).map_err(|e| AppError::bad_request(e.to_string()))?;
    let clip = composition.tracks.iter().find_map(|track| match track {
        CompositionTrack::Video { locked: false, clips, .. } => clips.iter().find(|clip| clip.id == id && clip.enabled), _ => None,
    }).ok_or_else(|| AppError::bad_request("Нужно незаблокированное видео"))?;
    if composition.metadata.as_ref().is_some_and(|m| m.media_versions.contains_key(&id)) { return Err(AppError::conflict("Клип содержит обработанную версию")); }
    let placement = &clip.placement;
    let end = placement.timeline_end_tick().map_err(|e| AppError::bad_request(e.to_string()))?;
    let clock = |at| crate::domain::video_clock::source_tick(placement, clip.playback_mode, at).ok().flatten().ok_or_else(|| AppError::bad_request("Неверный диапазон видео"));
    let first = clock(placement.timeline_start_tick)?;
    let last = clock(end.checked_sub(1).ok_or_else(|| AppError::bad_request("Пустое видео"))?)?;
    let ranges = [(first.min(last) as f64 / 1_000_000., first.max(last) as f64 / 1_000_000.)];
    let source_id = clip.source_id.as_str().to_owned();
    let path = super::cutout_jobs::source(&state, &source_id, &actor).await.map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let lease=if let Some(id)=&request.operation_id{Some(state.concat_video_cancellations.begin(&actor,&project,id,&state.shutdown_token()).map_err(|error|AppError::conflict(error.to_string()))?)}else{None};
    let token=lease.as_ref().map(|lease|lease.token.clone()).unwrap_or_else(||state.shutdown_token().child_token());if token.is_cancelled(){return Err(AppError::conflict("Анализ отменён"))}let _cancel=token.clone().drop_guard();
    let work = tokio::spawn(async move { let _lease=lease;let result: Result<Analysed> = async {
        let _permit = super::concat_cutout_analyse::acquire_analysis(&state, &token).await?;
        let _models = tokio::select! { biased; _=token.cancelled()=>anyhow::bail!("video analysis cancelled"), guard=state.cutout_model_ops.lock()=>guard };
        let report=super::concat_video_run::analyse(&state,&actor,&project,&source_id,request.expected_revision,&path,&ranges,request.subject,&token).await?;
        super::cutout_assets::authorize(&state, &project, &actor).await.map_err(|_| anyhow::anyhow!("project editor access revoked"))?;
        let current = state.db.get_composition_project_for(&project, &actor).await?.ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
        ensure!(current.revision == request.expected_revision && current.document == saved.document && !token.is_cancelled(), "video target changed during analysis");
        Ok(Analysed { subject:request.subject,model_hash:report.model_hash,source_id, analysed_runs: report.fill.analysed_runs, analysed_frames: report.fill.analysed_frames, empty_runs: report.fill.empty_runs, cache_reused:report.cache_reused })
    }.await; result });
    work.await.map_err(|e| AppError::internal("join video analysis", e))?.map(Json).map_err(|e| AppError::conflict(e.to_string()))
}
