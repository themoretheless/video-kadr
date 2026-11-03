//! Prefer an already adopted namespace for historical origin-bearing snapshots.
use super::concat_video_bound::SavedProject;
use crate::{domain::concat_video_binding::Binding, state::AppState};
use anyhow::{ensure, Result};

/// Registry authority selects the namespace; the provider still verifies every
/// cache/region file. Missing authority may use the explicit external origin,
/// while corruption in an owned store must never silently switch providers.
pub(crate) fn use_owned<'a>(
    state: &'a AppState,
    actor: &'a str,
    context: Option<&'a SavedProject>,
    source: &'a str,
    physical: &'a str,
    masks: &'a std::collections::BTreeMap<String, String>,
    binding: &'a Binding,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + Send + 'a>> {
    Box::pin(async move {
        let Some(context) = context else {
            return Ok(false);
        };
        super::cutout_assets::authorize(state, &context.project_id, actor)
            .await
            .map_err(|_| anyhow::anyhow!("saved project editor access required"))?;
        let saved = state
            .db
            .get_composition_project_for(&context.project_id, actor)
            .await?
            .ok_or_else(|| anyhow::anyhow!("saved project unavailable"))?;
        ensure!(
            saved.revision == context.expected_revision,
            "stale saved video project"
        );
        if !super::concat_video_source_access::allowed(
            state,
            actor,
            &context.project_id,
            context.expected_revision,
            source,
            &binding.source_hash,
            physical,
        )
        .await?
        {
            return Ok(false);
        }
        let registered:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concat_video_project_caches WHERE project_id=? AND source_id=? AND video_hash=? AND model_hash=? AND runtime_hash=?)")
        .bind(&context.project_id).bind(source).bind(&binding.source_hash).bind(&binding.model_hash).bind(&binding.runtime_hash).fetch_one(state.db.pool()).await?;
        if !registered {
            return Ok(false);
        }
        for track in &binding.region_tracks {
            let snapshot = &track.snapshot;
            ensure!(
                snapshot.source_id == source,
                "historical region source mismatch"
            );
            let physical = masks
                .get(&snapshot.mask_source_id)
                .ok_or_else(|| anyhow::anyhow!("historical region physical binding missing"))?;
            let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_project_sources WHERE project_id=? AND source_id=?) OR EXISTS(SELECT 1 FROM person_video_project_resources WHERE project_id=? AND role='sam-video-regions' AND source_id=? AND file_hash=?) OR EXISTS(SELECT 1 FROM concat_video_project_region_refs WHERE project_id=? AND source_id=? AND video_hash=? AND mask_source_id=? AND library_source_id=? AND file_hash=?)")
            .bind(&context.project_id).bind(physical).bind(&context.project_id).bind(physical).bind(&snapshot.mask_file_hash).bind(&context.project_id).bind(source).bind(&binding.source_hash).bind(&snapshot.mask_source_id).bind(physical).bind(&snapshot.mask_file_hash).fetch_one(state.db.pool()).await?;
            if !allowed {
                return Ok(false);
            }
        }
        Ok(true)
    })
}
