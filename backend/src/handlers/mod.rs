use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::domain::artifact_graph::Fingerprint;
use crate::domain::output::OutputFormat;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, EnqueueOutcome, ErrorKind, JobEvent, JobKind, JobPermit};
use crate::library::MediaEntry;
use crate::model::{EditRequest, ImportRequest};
use crate::ports::{ExportCommandCompiler, ExportCompileRequest};
use crate::services::render::{
    EditPlan, ExportExecutionProfile, RenderExecution, SourceMediaMetadata,
};
use crate::state::AppState;
use crate::tools::{self, Done};

mod jobs;
mod library;
mod upload;

pub use jobs::{
    cancel_handler, discard_job_handler, failed_jobs_handler, job_registry_handler,
    job_status_handler, resume_pending_jobs, retry_job_handler, start_job_dispatcher,
};
use jobs::{dispatch_job, JobLeaseHeartbeat};
pub use library::{library_delete_handler, library_list_handler, library_search_handler};
pub use upload::upload_handler;

/// Per-job wall-clock limit (download or render), overridable via env.
fn job_timeout() -> Duration {
    let secs = std::env::var("JOB_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1800);
    Duration::from_secs(secs)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportWork {
    schema_version: u32,
    request: ImportRequest,
    video_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditWork {
    schema_version: u32,
    request: EditRequest,
    output_id: String,
    cache_key: String,
}

/// `POST /api/import` — accept a video URL, kick off a download in the background,
/// and immediately return a job id to poll.
pub async fn import_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ImportRequest>,
) -> AppResult<Json<Value>> {
    let job_id = Uuid::new_v4().to_string();
    let work = ImportWork {
        schema_version: 1,
        request: req,
        video_id: Uuid::new_v4().to_string(),
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
            return Err(AppError::too_many_requests(
                "Слишком много новых задач; повторите позже",
            ));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok(Json(json!({ "jobId": resolved_id })))
}

fn spawn_import_job(
    state: AppState,
    job_id: String,
    work: ImportWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let req = work.request;
    let vid = work.video_id;
    let span = tracing::info_span!("job", job.id = %job_id, job.kind = "import");
    state.spawn_task(
        async move {
            let _lease = lease;
            // Reject bad/unsafe URLs before doing any work.
            if let Err(e) = tools::validate_url(&req.url).await {
                st.transition_job(
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

            // Wait for a queue slot.
            if !mark_queued(&st, &jid).await {
                st.clear_cancel(&jid).await;
                return;
            }
            let _permit = match acquire_job_permit_or_cancelled(&st, &jid, &token).await {
                Some(p) => p,
                None => return,
            };
            if token.is_cancelled() {
                mark_cancelled(&st, &jid).await;
                return;
            }

            if !mark_running(&st, &jid, "downloading", attempt).await {
                st.clear_cancel(&jid).await;
                return;
            }

            let (tx, rx) = mpsc::unbounded_channel::<f64>();
            let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

            let sources = st.sources_dir();
            let outcome = async {
                let done = tools::download_video(
                    &st.process_runtime,
                    &req.url,
                    &sources,
                    &vid,
                    req.start,
                    req.end,
                    &tx,
                    &token,
                    job_timeout(),
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

            drop(tx);
            let _ = drain.await;
            finish_job(&st, &jid, outcome, "source").await;
        }
        .instrument(span),
    );
}

/// Content key for the render cache: a hash of the canonical (source + edit)
/// request. Re-serializing the deserialized `EditRequest` normalises omitted
/// defaults, so two equivalent requests map to the same key.
pub fn render_cache_key(req: &EditRequest) -> String {
    let canonical = serde_json::to_string(req).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `POST /api/edit` — apply trim/crop/scale/mute/speed to a previously imported
/// video and return a job id to poll for the rendered result.
pub async fn edit_handler(
    State(state): State<AppState>,
    ApiJson(req): ApiJson<EditRequest>,
) -> AppResult<Json<Value>> {
    let job_id = Uuid::new_v4().to_string();
    let cache_key = render_cache_key(&req);
    let key = dedupe_key("edit", &req)
        .map_err(|error| AppError::internal("build edit dedupe key", error))?;
    let work = EditWork {
        schema_version: 1,
        request: req,
        output_id: Uuid::new_v4().to_string(),
        cache_key,
    };
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize edit job", error))?;
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Edit, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue edit job", error))?
    {
        EnqueueOutcome::Created(_) => job_id,
        EnqueueOutcome::Existing(existing) => existing,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests(
                "Слишком много новых задач; повторите позже",
            ));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok(Json(json!({ "jobId": resolved_id })))
}

fn spawn_edit_job(
    state: AppState,
    job_id: String,
    work: EditWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let EditWork {
        schema_version: _,
        request: req,
        output_id: out_id,
        cache_key,
    } = work;
    let st = state.clone();
    let jid = job_id.clone();
    let span = tracing::info_span!("job", job.id = %job_id, job.kind = "edit");
    let task = async move {
        let _lease = lease;
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let render_lock = st.render_lock(&cache_key).await;
        let _render_guard =
            match acquire_render_lock_or_cancelled(&st, &jid, &token, render_lock).await {
                Some(g) => g,
                None => return,
            };
        if finish_from_render_cache(&st, &jid, &cache_key).await {
            return;
        }

        let _render_permit = match acquire_render_permit_or_cancelled(&st, &jid, &token).await {
            Some(p) => p,
            None => return,
        };
        let _permit = match acquire_job_permit_or_cancelled(&st, &jid, &token).await {
            Some(p) => p,
            None => return,
        };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }

        if !mark_running(&st, &jid, "processing", attempt).await {
            st.clear_cancel(&jid).await;
            return;
        }

        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

        let sources = st.sources_dir();
        let outputs = st.outputs_dir();
        let req = req;
        let requested_format =
            OutputFormat::parse(req.format.as_deref()).unwrap_or(OutputFormat::Mp4);
        let ext = tools::output_ext(requested_format);
        let filename = format!("{out_id}.{ext}");
        let output_path = outputs.join(&filename);

        let outcome = async {
            let input = tools::find_source(&sources, &req.video_id).await?;
            let probe = tools::probe_video(&st.process_runtime, &input).await?;
            let source_fingerprint = Fingerprint::digest(req.video_id.as_bytes());
            let source = SourceMediaMetadata::new(probe.width, probe.height, probe.duration)?;
            let plan = Arc::new(EditPlan::compile(source_fingerprint, req, source)?);
            let execution = RenderExecution::new(
                plan,
                ExportExecutionProfile {
                    encode_budget: st.encode_budget.clone(),
                    verify_checksums: true,
                },
            );
            let command = tools::FfmpegExportCompiler.compile(ExportCompileRequest {
                input: &input,
                destination: &output_path,
                parallel_jobs: st.render_parallelism(),
                execution: &execution,
            })?;
            tracing::info!(output.format = %execution.output().format, "starting render");
            let done = tools::run_ffmpeg(
                &st.process_runtime,
                &command.arguments,
                command.expected_duration_seconds,
                &tx,
                &token,
                job_timeout(),
            )
            .instrument(tracing::info_span!("process", process.tool = "ffmpeg"))
            .await?;
            if matches!(done, Done::Cancelled) {
                let _ = tokio::fs::remove_file(&output_path).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let size = tokio::fs::metadata(&output_path)
                .await
                .map(|m| m.len())
                .ok();
            Ok(Some(json!({
                "id": out_id,
                "url": format!("/files/outputs/{filename}"),
                "filename": filename,
                "sizeBytes": size,
            })))
        }
        .await;

        if outcome.is_err() {
            let _ = tokio::fs::remove_file(&output_path).await;
        }

        let cache_info = match &outcome {
            Ok(Some(info)) => Some(info.clone()),
            _ => None,
        };

        drop(tx);
        let _ = drain.await;
        let updated = finish_job(&st, &jid, outcome, "output").await;
        if updated {
            if let Some(info) = &cache_info {
                if let Err(error) = st.db.cache_put(&cache_key, info, &filename).await {
                    tracing::warn!("cache successful render {cache_key}: {error}");
                }
            }
        }
    }
    .instrument(span);
    state.spawn_task(task);
}

pub async fn api_not_found_handler() -> AppError {
    AppError::not_found("API-маршрут не найден")
}

pub async fn method_not_allowed_handler() -> AppError {
    AppError::method_not_allowed("Метод не поддерживается")
}

/// Drain numeric progress updates from a worker into the job record.
fn spawn_progress_drain(
    st: AppState,
    jid: String,
    mut rx: mpsc::UnboundedReceiver<f64>,
) -> tokio::task::JoinHandle<()> {
    let owner = st.clone();
    owner.spawn_task(async move {
        let mut last_saved = -1.0_f64;
        while let Some(p) = rx.recv().await {
            if p >= 100.0 || p - last_saved >= 1.0 {
                last_saved = p;
                st.update_job_if_open(&jid, |j| j.progress = Some(p)).await;
            }
        }
    })
}

/// Atomically start an open job. Cancellation may win immediately before this
/// transition; in that case the terminal state must never be overwritten.
async fn mark_running(st: &AppState, jid: &str, stage: &str, attempt: u32) -> bool {
    st.transition_job(
        jid,
        JobEvent::Started {
            stage: stage.into(),
            attempt,
        },
    )
    .await
}

async fn mark_queued(st: &AppState, jid: &str) -> bool {
    st.transition_job(jid, JobEvent::Queued).await
}

async fn finish_from_render_cache(st: &AppState, jid: &str, cache_key: &str) -> bool {
    let Ok(Some((output, filename))) = st.db.cache_get(cache_key).await else {
        return false;
    };
    if !is_plain_filename(&filename) {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    if tokio::fs::metadata(st.outputs_dir().join(&filename))
        .await
        .is_err()
    {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    let updated = st
        .transition_job(jid, JobEvent::Succeeded { result: output })
        .await;
    st.clear_cancel(jid).await;
    updated
}

fn is_plain_filename(filename: &str) -> bool {
    !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && filename != "."
        && filename != ".."
        && Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == filename)
}

async fn acquire_render_lock_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    lock: Arc<Mutex<()>>,
) -> Option<OwnedMutexGuard<()>> {
    tokio::select! {
        guard = lock.lock_owned() => Some(guard),
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn acquire_job_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
) -> Option<JobPermit> {
    tokio::select! {
        permit = st.acquire_job_slot() => match permit {
            Ok(p) => Some(p),
            Err(_) => {
                mark_queue_closed(st, jid).await;
                None
            }
        },
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn acquire_render_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
) -> Option<JobPermit> {
    tokio::select! {
        permit = st.acquire_render_slot() => match permit {
            Ok(p) => Some(p),
            Err(_) => {
                mark_queue_closed(st, jid).await;
                None
            }
        },
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn mark_cancelled(st: &AppState, jid: &str) {
    // Shutdown cancellation is recoverable: keep the durable pending/running
    // snapshot for startup reconciliation. Only a user cancellation is terminal.
    if st.is_shutting_down() {
        st.clear_cancel(jid).await;
        return;
    }
    st.transition_job(jid, JobEvent::Cancelled).await;
    st.clear_cancel(jid).await;
}

async fn mark_queue_closed(st: &AppState, jid: &str) {
    if st.is_shutting_down() {
        st.clear_cancel(jid).await;
        return;
    }
    st.transition_job(
        jid,
        JobEvent::Failed {
            kind: ErrorKind::Internal,
            message: "очередь задач закрыта".into(),
        },
    )
    .await;
    st.clear_cancel(jid).await;
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

/// Apply the terminal outcome of a worker to the job and clear its cancel token.
/// `Ok(Some)` -> done (also recorded in the media library under `kind`),
/// `Ok(None)` -> cancelled, `Err` -> error.
async fn finish_job(
    st: &AppState,
    jid: &str,
    outcome: anyhow::Result<Option<Value>>,
    kind: &str,
) -> bool {
    let updated = match outcome {
        Ok(Some(info)) => {
            let updated = st
                .transition_job(
                    jid,
                    JobEvent::Succeeded {
                        result: info.clone(),
                    },
                )
                .await;
            if updated {
                let entry = MediaEntry::from_result(kind, &info);
                if st.library.add(entry.clone()).await {
                    st.index_media(&entry).await;
                }
            }
            updated
        }
        Ok(None) if st.is_shutting_down() => false,
        Ok(None) => st.transition_job(jid, JobEvent::Cancelled).await,
        Err(e) => {
            let kind = classify_job_error(&e);
            let message = crate::privacy::redact_text(&e.to_string());
            tracing::error!(job.id = jid, error.kind = kind.as_str(), error = %message, "job failed");
            let updated = st
                .transition_job(jid, JobEvent::Failed { kind, message })
                .await;
            if updated && kind.retryable() {
                match st.job_store.schedule_retry(jid).await {
                    Ok(Some((job, delay))) => {
                        st.replace_job(job).await;
                        tracing::info!(
                            job.id = jid,
                            retry.delay_ms = delay.as_millis(),
                            "job retry scheduled"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::error!(job.id = jid, %error, "schedule job retry");
                    }
                }
            }
            updated
        }
    };
    st.clear_cancel(jid).await;
    updated
}

fn classify_job_error(error: &anyhow::Error) -> ErrorKind {
    let message = error.to_string().to_lowercase();
    if message.contains("timeout") || message.contains("deadline") || message.contains("таймаут")
    {
        ErrorKind::Timeout
    } else if message.contains("недопуст")
        || message.contains("invalid")
        || message.contains("outside duration")
        || (message.contains("source") && message.contains("not found"))
    {
        ErrorKind::Validation
    } else if message.contains("not found")
        || message.contains("no such file")
        || message.contains("metadata")
    {
        ErrorKind::Storage
    } else if message.contains("ffmpeg")
        || message.contains("yt-dlp")
        || message.contains("exit status")
    {
        ErrorKind::ProcessExit
    } else {
        ErrorKind::Internal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::model::{Job, JobStatus};
    use crate::state::{CancelJobOutcome, ToolInfo};
    use serde_json::json;

    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(storage.join("outputs"))
            .await
            .unwrap();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        (AppState::new(storage, 2, ToolInfo::default(), lib, db), dir)
    }

    #[tokio::test]
    async fn finish_job_does_not_overwrite_terminal_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("j1".into())).await;
        st.update_job("j1", |j| j.status = JobStatus::Cancelled)
            .await;
        st.persist_job("j1").await;

        let updated = finish_job(
            &st,
            "j1",
            Ok(Some(json!({
                "id": "out",
                "filename": "out.mp4",
                "url": "/files/outputs/out.mp4"
            }))),
            "output",
        )
        .await;

        assert!(!updated);
        let job = st.get_job("j1").await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.result.is_none());
        assert!(st.library.list().await.is_empty());
    }

    #[tokio::test]
    async fn shutdown_cancellation_leaves_durable_work_recoverable() {
        let (st, _dir) = state().await;
        let outcome = st
            .enqueue_job(
                "restartable".into(),
                JobKind::Import,
                &json!({"schemaVersion": 1}),
                "restartable-key",
            )
            .await
            .unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Created(_)));

        st.begin_shutdown();
        mark_cancelled(&st, "restartable").await;
        assert!(!finish_job(&st, "restartable", Ok(None), "source").await);
        assert_eq!(
            st.get_job("restartable").await.unwrap().status,
            JobStatus::Pending
        );
        assert_eq!(
            st.job_store.deliverable_ids().await.unwrap(),
            vec!["restartable"]
        );
    }

    #[tokio::test]
    async fn mark_running_does_not_revive_a_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("cancelled-before-start".into()))
            .await;
        assert_eq!(
            st.cancel_open_job("cancelled-before-start").await,
            CancelJobOutcome::Cancelled
        );

        assert!(!mark_running(&st, "cancelled-before-start", "processing", 1).await);
        assert!(!mark_queued(&st, "cancelled-before-start").await);
        let job = st.get_job("cancelled-before-start").await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
    }

    #[test]
    fn cache_filenames_must_be_plain_output_names() {
        assert!(is_plain_filename("output.mp4"));
        assert!(is_plain_filename("b5b2b5b2-clip.webm"));

        for filename in ["", ".", "..", "../x.mp4", "dir/x.mp4", "dir\\x.mp4"] {
            assert!(
                !is_plain_filename(filename),
                "{filename:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn finish_from_render_cache_validates_filename_and_file() {
        let (st, _dir) = state().await;

        st.set_job(Job::pending("valid".into())).await;
        let output = json!({
            "id": "out",
            "filename": "out.mp4",
            "url": "/files/outputs/out.mp4"
        });
        tokio::fs::write(st.outputs_dir().join("out.mp4"), b"video")
            .await
            .unwrap();
        st.db
            .cache_put("valid-key", &output, "out.mp4")
            .await
            .unwrap();

        assert!(finish_from_render_cache(&st, "valid", "valid-key").await);
        let job = st.get_job("valid").await.unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.progress, Some(100.0));
        assert_eq!(job.result.unwrap()["filename"], "out.mp4");

        st.set_job(Job::pending("unsafe".into())).await;
        st.db
            .cache_put(
                "unsafe-key",
                &json!({ "filename": "../leak.mp4" }),
                "../leak.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "unsafe", "unsafe-key").await);
        assert!(st.db.cache_get("unsafe-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("unsafe").await.unwrap().status,
            JobStatus::Pending
        );

        st.set_job(Job::pending("missing".into())).await;
        st.db
            .cache_put(
                "missing-key",
                &json!({ "filename": "missing.mp4" }),
                "missing.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "missing", "missing-key").await);
        assert!(st.db.cache_get("missing-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("missing").await.unwrap().status,
            JobStatus::Pending
        );
    }

    #[tokio::test]
    async fn cleanup_files_with_prefix_removes_partial_imports() {
        let (st, _dir) = state().await;
        let dir = st.sources_dir();
        tokio::fs::write(dir.join("abc.mp4.part"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abc.info.json"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abcd.mp4"), b"x").await.unwrap();
        tokio::fs::write(dir.join("abc"), b"x").await.unwrap();
        tokio::fs::write(dir.join(".gitkeep"), b"").await.unwrap();

        cleanup_files_with_prefix(&dir, "abc").await;

        assert!(!dir.join("abc.mp4.part").exists());
        assert!(!dir.join("abc.info.json").exists());
        assert!(!dir.join("abc").exists());
        assert!(dir.join("abcd.mp4").exists());
        assert!(dir.join(".gitkeep").exists());
    }
}
