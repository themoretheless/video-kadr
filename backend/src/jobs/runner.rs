//! Shared job orchestration: queue → acquire → run → terminal transition.
//!
//! Handlers build kind-specific work; this module owns the DRY lifecycle so
//! import/edit/composition/proxy/publish do not re-implement cancel races.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

use crate::config::resource_classes::ResourceClass;
use crate::jobs::{ErrorKind, JobEvent, JobPermit};
use crate::library::MediaEntry;
use crate::ports::telemetry::TelemetryEvent;
use crate::state::AppState;

/// Progress + cancellation handle passed into kind-specific work.
pub struct JobContext {
    pub progress: mpsc::UnboundedSender<f64>,
    pub cancel: CancellationToken,
}

/// How a standard worker acquires slots before `mark_running`.
#[derive(Clone, Copy)]
pub struct AcquirePlan {
    pub resource_class: ResourceClass,
    pub acquire_render_permit: bool,
}

/// Outcome of the shared acquire → run → finish pipeline.
pub struct ExecutionGuards {
    pub _render_lock: Option<OwnedMutexGuard<()>>,
    pub _render_permit: Option<JobPermit>,
    pub _job_permit: JobPermit,
}

/// Job lifecycle service (P2-9 / P2-12).
pub struct JobService;

impl JobService {
    pub async fn transition(st: &AppState, jid: &str, event: JobEvent) -> bool {
        match st.transition_job(jid, event).await {
            Ok(applied) => applied,
            Err(error) => {
                tracing::error!(job.id = jid, %error, "persist job transition");
                false
            }
        }
    }

    pub async fn mark_queued(st: &AppState, jid: &str) -> bool {
        Self::transition(st, jid, JobEvent::Queued).await
    }

    pub async fn mark_running(st: &AppState, jid: &str, stage: &str, attempt: u32) -> bool {
        Self::transition(
            st,
            jid,
            JobEvent::Started {
                stage: stage.into(),
                attempt,
            },
        )
        .await
    }

    pub async fn mark_cancelled(st: &AppState, jid: &str) {
        if st.is_shutting_down() {
            st.clear_cancel(jid).await;
            return;
        }
        Self::transition(st, jid, JobEvent::Cancelled).await;
        st.clear_cancel(jid).await;
    }

    pub async fn mark_queue_closed(st: &AppState, jid: &str) {
        if st.is_shutting_down() {
            st.clear_cancel(jid).await;
            return;
        }
        Self::transition(
            st,
            jid,
            JobEvent::Failed {
                kind: ErrorKind::Internal,
                message: crate::messages::QUEUE_CLOSED.into(),
            },
        )
        .await;
        st.clear_cancel(jid).await;
    }

    pub fn spawn_progress_drain(
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

    pub async fn acquire_render_lock_or_cancelled(
        st: &AppState,
        jid: &str,
        token: &CancellationToken,
        lock: Arc<Mutex<()>>,
    ) -> Option<OwnedMutexGuard<()>> {
        tokio::select! {
            guard = lock.lock_owned() => Some(guard),
            _ = token.cancelled() => {
                Self::mark_cancelled(st, jid).await;
                None
            }
        }
    }

    pub async fn acquire_job_permit_or_cancelled(
        st: &AppState,
        jid: &str,
        token: &CancellationToken,
        class: ResourceClass,
    ) -> Option<JobPermit> {
        tokio::select! {
            permit = st.acquire_job_slot(class) => match permit {
                Ok(p) => Some(p),
                Err(_) => {
                    Self::mark_queue_closed(st, jid).await;
                    None
                }
            },
            _ = token.cancelled() => {
                Self::mark_cancelled(st, jid).await;
                None
            }
        }
    }

    pub async fn acquire_render_permit_or_cancelled(
        st: &AppState,
        jid: &str,
        token: &CancellationToken,
    ) -> Option<JobPermit> {
        tokio::select! {
            permit = st.acquire_render_slot() => match permit {
                Ok(p) => Some(p),
                Err(_) => {
                    Self::mark_queue_closed(st, jid).await;
                    None
                }
            },
            _ = token.cancelled() => {
                Self::mark_cancelled(st, jid).await;
                None
            }
        }
    }

    /// Acquire render-lock (optional), render permit (optional), and job permit.
    pub async fn acquire_execution(
        st: &AppState,
        jid: &str,
        token: &CancellationToken,
        render_lock: Option<Arc<Mutex<()>>>,
        plan: AcquirePlan,
    ) -> Option<ExecutionGuards> {
        let render_lock_guard = if let Some(lock) = render_lock {
            match Self::acquire_render_lock_or_cancelled(st, jid, token, lock).await {
                Some(guard) => Some(guard),
                None => return None,
            }
        } else {
            None
        };

        let render_permit = if plan.acquire_render_permit {
            match Self::acquire_render_permit_or_cancelled(st, jid, token).await {
                Some(permit) => Some(permit),
                None => return None,
            }
        } else {
            None
        };

        let job_permit = match Self::acquire_job_permit_or_cancelled(
            st,
            jid,
            token,
            plan.resource_class,
        )
        .await
        {
            Some(permit) => permit,
            None => return None,
        };

        if token.is_cancelled() {
            Self::mark_cancelled(st, jid).await;
            return None;
        }

        Some(ExecutionGuards {
            _render_lock: render_lock_guard,
            _render_permit: render_permit,
            _job_permit: job_permit,
        })
    }

    pub async fn finish_from_render_cache(
        st: &AppState,
        jid: &str,
        cache_key: &str,
        actor: Option<&str>,
    ) -> bool {
        use crate::ports::RenderCache;
        let cache = &*st.db; // Unwrap Arc to get Db for RenderCache trait
        let Ok(Some((output, filename))) = cache.cache_get(cache_key).await else {
            st.telemetry
                .record(TelemetryEvent::CacheLookup { result: "miss" });
            return false;
        };
        if !is_plain_filename(&filename) {
            let _ = cache.cache_delete(cache_key).await;
            return false;
        }
        if tokio::fs::metadata(st.outputs_dir().join(&filename))
            .await
            .is_err()
        {
            let _ = cache.cache_delete(cache_key).await;
            return false;
        }
        let output_id = output["id"].as_str().map(str::to_owned);
        let updated = Self::transition(st, jid, JobEvent::Succeeded { result: output }).await;
        if updated {
            if let (Some(actor), Some(output_id)) = (actor, output_id.as_deref()) {
                if let Err(error) = st.db.grant_output_access(output_id, actor).await {
                    tracing::error!(%error, %output_id, "grant cached output access");
                }
            }
        }
        st.telemetry
            .record(TelemetryEvent::CacheLookup { result: "hit" });
        st.clear_cancel(jid).await;
        updated
    }

    /// Apply terminal outcome and always clear the cancel token (P2-12).
    pub async fn finish(
        st: &AppState,
        jid: &str,
        outcome: anyhow::Result<Option<Value>>,
        kind: &str,
    ) -> bool {
        let updated = match outcome {
            Ok(Some(info)) => {
                let updated = Self::transition(
                    st,
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
            Ok(None) => Self::transition(st, jid, JobEvent::Cancelled).await,
            Err(e) => {
                let kind = classify_job_error(&e);
                let message = crate::privacy::redact_text(&e.to_string());
                tracing::error!(job.id = jid, error.kind = kind.as_str(), error = %message, "job failed");
                let updated = Self::transition(st, jid, JobEvent::Failed { kind, message }).await;
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

    /// After queueing: acquire slots, mark running, run work with progress, finish.
    /// Returns whether the terminal transition applied.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_acquired<F, Fut>(
        st: &AppState,
        jid: &str,
        token: &CancellationToken,
        attempt: u32,
        stage: &str,
        finish_kind: &str,
        guards: ExecutionGuards,
        work: F,
    ) -> bool
    where
        F: FnOnce(JobContext) -> Fut,
        Fut: Future<Output = anyhow::Result<Option<Value>>>,
    {
        let _guards = guards;
        if !Self::mark_running(st, jid, stage, attempt).await {
            st.clear_cancel(jid).await;
            return false;
        }
        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = Self::spawn_progress_drain(st.clone(), jid.to_owned(), rx);
        let ctx = JobContext {
            progress: tx.clone(),
            cancel: token.clone(),
        };
        let outcome = work(ctx).await;
        drop(tx);
        let _ = drain.await;
        Self::finish(st, jid, outcome, finish_kind).await
    }
}

pub fn classify_job_error(error: &anyhow::Error) -> ErrorKind {
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

pub fn is_plain_filename(filename: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::model::{Job, JobStatus};
    use crate::state::ToolInfo;
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
    async fn finish_clears_cancel_token_on_success() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("done".into())).await;
        let _ = st.register_cancel("done").await;
        let updated = JobService::finish(
            &st,
            "done",
            Ok(Some(json!({
                "id": "out",
                "filename": "out.mp4",
                "url": "/files/outputs/out.mp4"
            }))),
            "output",
        )
        .await;
        assert!(updated);
        // Token must be gone so a late cancel cannot race a finished job.
        assert_eq!(
            st.cancel_open_job("done").await.unwrap(),
            crate::state::CancelJobOutcome::AlreadyFinished
        );
    }

    #[tokio::test]
    async fn mark_running_does_not_revive_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("cancelled-before-start".into()))
            .await;
        st.update_job("cancelled-before-start", |j| {
            j.status = JobStatus::Cancelled
        })
        .await;
        assert!(!JobService::mark_running(&st, "cancelled-before-start", "processing", 1).await);
        assert!(!JobService::mark_queued(&st, "cancelled-before-start").await);
    }

    #[tokio::test]
    async fn cancel_during_queued_acquire_marks_cancelled() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("queued-cancel".into())).await;
        let token = st.register_cancel("queued-cancel").await;
        token.cancel();
        let plan = AcquirePlan {
            resource_class: ResourceClass::Export,
            acquire_render_permit: false,
        };
        assert!(
            JobService::acquire_execution(&st, "queued-cancel", &token, None, plan)
                .await
                .is_none()
        );
        let job = st.get_job("queued-cancel").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn transition_persists_terminal_failed() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("fail".into())).await;
        st.update_job("fail", |j| j.status = JobStatus::Running)
            .await;
        assert!(
            JobService::transition(
                &st,
                "fail",
                JobEvent::Failed {
                    kind: ErrorKind::Validation,
                    message: "bad".into(),
                },
            )
            .await
        );
        let job = st.get_job("fail").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Error);
    }
}
