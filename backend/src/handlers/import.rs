use std::path::Path;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::resource_classes::ResourceClass;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{
    dedupe_key, AcquirePlan, EnqueueOutcome, ErrorKind, JobEvent, JobKind, JobService,
};
use crate::model::ImportRequest;
use crate::state::AppState;
use crate::tools::{self, Done};

use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use super::{apply_job_event, mark_queued};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImportWork {
    pub(crate) schema_version: u32,
    pub(crate) request: ImportRequest,
    pub(crate) video_id: String,
    #[serde(default)]
    pub(crate) trace_context: crate::telemetry::context::TraceContext,
}
/// `POST /api/import` — accept a video URL, kick off a download in the background,
/// and immediately return a job id to poll.
pub async fn import_handler(
    State(state): State<AppState>,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(req): ApiJson<ImportRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let job_id = Uuid::new_v4().to_string();
    let work = ImportWork {
        schema_version: 1,
        request: req,
        video_id: Uuid::new_v4().to_string(),
        trace_context: trace.map(|Extension(value)| value).unwrap_or_default(),
    };
    let key = dedupe_key("import", &work.request)
        .map_err(|error| AppError::internal("build import dedupe key", error))?;
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize import job", error))?;
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Import, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue import job", error))?
    {
        EnqueueOutcome::Created(_) => job_id,
        EnqueueOutcome::Existing(existing) => existing,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests(crate::messages::TOO_MANY_JOBS));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobId": resolved_id }))))
}

pub(crate) fn spawn_import_job(
    state: AppState,
    job_id: String,
    work: ImportWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let trace_context = work.trace_context.clone();
    let req = work.request;
    let vid = work.video_id;
    let span = trace_context.job_span(&job_id, "import");
    state.spawn_task(
        async move {
            let _lease = lease;
            // Reject bad/unsafe URLs before doing any work.
            if let Err(e) = tools::validate_url(&req.url).await {
                apply_job_event(
                    &st,
                    &jid,
                    JobEvent::Failed {
                        kind: ErrorKind::Security,
                        message: crate::privacy::redact_text(&e.to_string()),
                    },
                )
                .await;
                st.clear_cancel(&jid).await;
                return;
            }

            if !mark_queued(&st, &jid).await {
                st.clear_cancel(&jid).await;
                return;
            }
            let Some(guards) = JobService::acquire_execution(
                &st,
                &jid,
                &token,
                None,
                AcquirePlan {
                    resource_class: ResourceClass::Ingest,
                    acquire_render_permit: false,
                },
            )
            .await
            else {
                return;
            };

            let st_work = st.clone();
            JobService::run_acquired(
                &st,
                &jid,
                &token,
                attempt,
                "downloading",
                "source",
                guards,
                |ctx| async move {
                    let st = st_work;
                    let sources = st.sources_dir();
                    let outcome = async {
                        let done = tools::download_video(
                            &st.process_runtime,
                            &req.url,
                            &sources,
                            &vid,
                            req.start,
                            req.end,
                            &ctx.progress,
                            &ctx.cancel,
                            st.max_download_height(),
                            st.job_timeout(),
                        )
                        .instrument(tracing::info_span!("process", process.tool = "yt-dlp"))
                        .await?;
                        if matches!(done, Done::Cancelled) {
                            cleanup_files_with_prefix(&sources, &vid).await;
                            return Ok::<Option<Value>, anyhow::Error>(None);
                        }
                        let path = tools::find_source(&sources, &vid).await?;
                        let info = tools::probe_video(&st.process_runtime, &path).await?;
                        let title = tools::read_title(&sources, &vid).await;
                        let size = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| format!("{vid}.mp4"));
                        Ok(Some(json!({
                            "id": vid,
                            "url": format!("/files/sources/{filename}"),
                            "filename": filename,
                            "duration": info.duration,
                            "width": info.width,
                            "height": info.height,
                            "title": title,
                            "fps": info.fps,
                            "vcodec": info.vcodec,
                            "acodec": info.acodec,
                            "sizeBytes": size,
                        })))
                    }
                    .await;

                    if outcome.is_err() {
                        cleanup_files_with_prefix(&sources, &vid).await;
                    }
                    outcome
                },
            )
            .await;
        }
        .instrument(span),
    );
}
async fn cleanup_files_with_prefix(dir: &Path, prefix: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let matches_prefix = name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'));
        if name == ".gitkeep" || !matches_prefix {
            continue;
        }
        let _ = tokio::fs::remove_file(entry.path()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::state::ToolInfo;

    #[tokio::test]
    async fn cleanup_files_with_prefix_removes_partial_imports() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        let sources = st.sources_dir();
        tokio::fs::write(sources.join("abc.mp4.part"), b"x")
            .await
            .unwrap();
        tokio::fs::write(sources.join("abc.info.json"), b"x")
            .await
            .unwrap();
        tokio::fs::write(sources.join("abcd.mp4"), b"x")
            .await
            .unwrap();
        tokio::fs::write(sources.join("abc"), b"x").await.unwrap();
        tokio::fs::write(sources.join(".gitkeep"), b"")
            .await
            .unwrap();

        cleanup_files_with_prefix(&sources, "abc").await;

        assert!(!sources.join("abc.mp4.part").exists());
        assert!(!sources.join("abc.info.json").exists());
        assert!(!sources.join("abc").exists());
        assert!(sources.join("abcd.mp4").exists());
        assert!(sources.join(".gitkeep").exists());
    }
}
