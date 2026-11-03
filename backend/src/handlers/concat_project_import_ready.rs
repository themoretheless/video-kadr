//! Import still cutouts by saved-project references, never client mask assertions.
use crate::{
    domain::{
        composition::{CompositionClipId, SourceId},
        concat_project_import::{ImportReport, ImportRequest, ReadyStillBinding, StillBindings},
    },
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use anyhow::{ensure, Context, Result};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedStillReference {
    timeline_id: String,
    clip_id: String,
    project_id: String,
    composition_id: Option<String>,
    source_clip_id: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    request: ImportRequest,
    prepared_stills: Vec<SavedStillReference>,
}
fn settings(
    document: &Value,
    timeline: &str,
    clip: &str,
) -> Result<crate::domain::concat_cutout_import::Plan> {
    let timeline = if let Some(timelines) = document
        .get("timelines")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
    {
        timelines
            .iter()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(timeline))
            .context("destination timeline missing")?
    } else {
        ensure!(timeline == "TL1", "destination timeline missing");
        document
    };
    let clip = timeline
        .get("clips")
        .and_then(Value::as_array)
        .and_then(|clips| {
            clips
                .iter()
                .find(|c| c.get("id").and_then(Value::as_str) == Some(clip))
        })
        .context("destination clip missing")?;
    ensure!(
        clip.get("kind").and_then(Value::as_str) == Some("image"),
        "prepared still requires an image destination"
    );
    crate::domain::concat_cutout_import::plan(
        clip.get("cutout")
            .context("destination cutout settings missing")?,
    )
}
pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<Request>,
) -> AppResult<([(&'static str, &'static str); 1], Json<ImportReport>)> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    if input.prepared_stills.is_empty() || input.prepared_stills.len() > 16 * 512 {
        return Err(AppError::bad_request(
            "Expected 1..8192 saved still references",
        ));
    }
    // Preserve explicit authorization errors before admitting expensive decoding.
    for reference in &input.prepared_stills {
        super::cutout_assets::authorize(&state, &reference.project_id, &actor).await?;
    }
    for source in input.request.media_bindings.values() {
        super::library::authorize_source_access(&state, source.id.as_str(), &headers).await?;
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    let _guard = cancel.clone().drop_guard();
    let worker = tokio::spawn(async move {
        let result: Result<ImportReport> = async {
            let _permit = super::concat_cutout_analyse::acquire_analysis(&state, &cancel).await?;
            let mut media_paths = BTreeMap::new();
            for source in input.request.media_bindings.values() {
                media_paths.insert(
                    source.id.clone(),
                    super::cutout_jobs::source(&state, source.id.as_str(), &actor).await?,
                );
            }
            let mut ready = StillBindings::new();
            let mut checks = vec![];
            for reference in input.prepared_stills {
                ensure!(!cancel.is_cancelled(), "ready import cancelled");
                ensure!(
                    !ready
                        .get(&reference.timeline_id)
                        .is_some_and(|clips| clips.contains_key(&reference.clip_id)),
                    "duplicate prepared destination"
                );
                let plan = settings(
                    &input.request.document,
                    &reference.timeline_id,
                    &reference.clip_id,
                )?;
                let clip = CompositionClipId::parse(&reference.source_clip_id)?;
                let saved = state
                    .db
                    .get_composition_project_for(&reference.project_id, &actor)
                    .await?
                    .context("saved source project unavailable")?;
                ensure!(
                    saved.revision == reference.expected_revision,
                    "stale saved cutout revision"
                );
                let descriptors = crate::domain::concat_cutout_binding::saved_still(
                    &saved.document,
                    reference.composition_id.as_deref(),
                    &clip,
                )?;
                for source in [&descriptors.original, &descriptors.copy, &descriptors.mask]
                    .into_iter()
                    .chain(descriptors.regions.iter())
                {
                    let physical = super::concat_cutout_access::library_id(
                        &saved.document,
                        source.id.as_str(),
                    )?;
                    super::library::authorize_source_access(&state, &physical, &headers)
                        .await
                        .map_err(|_| anyhow::anyhow!("saved cutout file access denied"))?;
                }
                let candidate = super::concat_cutout_access::verify_saved_still(
                    &state,
                    &actor,
                    &reference.project_id,
                    reference.composition_id.as_deref(),
                    &clip,
                    reference.expected_revision,
                    &plan,
                    &cancel,
                )
                .await?;
                let mut assets = candidate.assets.clone();
                let physical =
                    |id: &str| super::concat_cutout_access::library_id(&saved.document, id);
                let original = physical(assets.original.id.as_str())?;
                assets.snapshot.source_id = original.clone();
                assets.snapshot.mask_source_id = physical(&assets.snapshot.mask_source_id)?;
                for stroke in &mut assets.snapshot.correction.strokes {
                    if let Some(region) = &mut stroke.region {
                        region.source_id = original.clone();
                        region.mask_source_id = physical(&region.mask_source_id)?;
                    }
                }
                for source in [&mut assets.original, &mut assets.copy, &mut assets.mask]
                    .into_iter()
                    .chain(assets.regions.iter_mut())
                {
                    source.id = SourceId::parse(physical(source.id.as_str())?)?;
                }
                let snapshot = &assets.snapshot;
                let binding = ReadyStillBinding {
                    source_settings: plan.clone(),
                    source_hash: snapshot.source_hash.clone(),
                    model_hash: snapshot.model_hash.clone(),
                    runtime_hash: snapshot.runtime_hash.clone(),
                    assets,
                };
                ready
                    .entry(reference.timeline_id.clone())
                    .or_insert_with(BTreeMap::new)
                    .insert(reference.clip_id.clone(), binding);
                checks.push((reference, clip, plan, candidate));
            }
            let report = crate::domain::concat_project_import::import_with_still_bindings(
                input.request,
                &ready,
            )?;
            // Every referenced project/file is checked again after conversion; no
            // library records, assets or saved documents are created by this route.
            for (reference, clip, plan, before) in checks {
                let current = super::concat_cutout_access::verify_saved_still(
                    &state,
                    &actor,
                    &reference.project_id,
                    reference.composition_id.as_deref(),
                    &clip,
                    reference.expected_revision,
                    &plan,
                    &cancel,
                )
                .await?;
                ensure!(
                    current.paths == before.paths
                        && current.assets.snapshot == before.assets.snapshot
                        && current.assets.original == before.assets.original
                        && current.assets.copy == before.assets.copy
                        && current.assets.mask == before.assets.mask
                        && current.assets.regions == before.assets.regions,
                    "saved cutout changed during import"
                );
            }
            for (id, path) in media_paths {
                super::library::authorize_source_access(&state, id.as_str(), &headers)
                    .await
                    .map_err(|_| anyhow::anyhow!("import media access revoked"))?;
                ensure!(
                    super::cutout_jobs::source(&state, id.as_str(), &actor).await? == path,
                    "import media path changed"
                );
            }
            ensure!(!cancel.is_cancelled(), "ready import cancelled");
            Ok(report)
        }
        .await;
        result
    });
    let report = worker
        .await
        .map_err(|e| AppError::internal("join ready cutout import", e))?
        .map_err(|e| AppError::conflict(e.to_string()))?;
    Ok(([("cache-control", "private, no-store")], Json(report)))
}
