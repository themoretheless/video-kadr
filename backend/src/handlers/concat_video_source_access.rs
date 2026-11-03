//! Exact project-owned original-file authority for current and historical slots.
use crate::state::AppState;
use anyhow::{ensure, Result};
pub(crate) fn allowed<'a>(
    state: &'a AppState,
    actor: &'a str,
    project: &'a str,
    revision: u64,
    source: &'a str,
    hash: &'a str,
    physical: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool>> + Send + 'a>> {
    Box::pin(async move {
        super::cutout_assets::authorize(state, project, actor)
            .await
            .map_err(|_| anyhow::anyhow!("video source editor access required"))?;
        let saved = state
            .db
            .get_composition_project_for(project, actor)
            .await?
            .ok_or_else(|| anyhow::anyhow!("video source project unavailable"))?;
        ensure!(
            saved.revision == revision,
            "stale saved video source revision"
        );
        let live = super::concat_cutout_access::library_id(&saved.document, source)?;
        if live == physical && saved.source_ids.iter().any(|id| id == physical) {
            return Ok(true);
        }
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concat_video_project_source_refs WHERE project_id=? AND source_id=? AND video_hash=? AND library_source_id=?)").bind(project).bind(source).bind(hash).bind(physical).fetch_one(state.db.pool()).await?)
    })
}
