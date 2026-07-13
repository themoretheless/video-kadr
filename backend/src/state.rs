use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{AcquireError, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::library::Library;
use crate::model::{Job, JobStatus};
use crate::runtime::TaskSupervisor;

const DEFAULT_RECOVER_JOBS_LIMIT: i64 = 200;

/// Availability and versions of the external tools we shell out to. Probed once
/// at startup and surfaced via `/api/health`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolInfo {
    pub ffmpeg: bool,
    pub ytdlp: bool,
    #[serde(rename = "ffmpegVersion")]
    pub ffmpeg_version: Option<String>,
    #[serde(rename = "ytdlpVersion")]
    pub ytdlp_version: Option<String>,
    #[serde(skip)]
    pub ffmpeg_encoders: Vec<String>,
    #[serde(skip)]
    pub ffmpeg_muxers: Vec<String>,
    #[serde(skip)]
    pub ffmpeg_filters: Vec<String>,
}

/// Shared application state. Jobs live in memory as the hot path (with live
/// progress) and are written through to SQLite on creation and at terminal
/// states, so a restart recovers them (in-flight jobs become `interrupted`).
#[derive(Clone)]
pub struct AppState {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
    /// Per-job cancellation handles, removed when the job finishes.
    cancels: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Per-render-cache-key locks. They serialize identical edit requests so
    /// only one worker renders while followers wait and then reuse the cache.
    render_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    /// Downloads/renders queue here; uploads use a separate pool so a slow
    /// client cannot consume every render slot.
    jobs_semaphore: Arc<Semaphore>,
    upload_semaphore: Arc<Semaphore>,
    supervisor: TaskSupervisor,
    pub tools: Arc<ToolInfo>,
    pub library: Library,
    pub db: Db,
    pub storage: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelJobOutcome {
    Cancelled,
    NotFound,
    AlreadyFinished,
}

impl AppState {
    pub fn new(
        storage: PathBuf,
        max_concurrent: usize,
        tools: ToolInfo,
        library: Library,
        db: Db,
    ) -> Self {
        let max_concurrent = max_concurrent.max(1);
        AppState {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            cancels: Arc::new(Mutex::new(HashMap::new())),
            render_locks: Arc::new(Mutex::new(HashMap::new())),
            jobs_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            upload_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            supervisor: TaskSupervisor::default(),
            tools: Arc::new(tools),
            library,
            db,
            storage,
        }
    }

    pub async fn set_job(&self, job: Job) {
        if let Err(e) = self.db.persist_job(&job).await {
            tracing::warn!("persist job {}: {e}", job.id);
        }
        self.jobs.lock().await.insert(job.id.clone(), job);
    }

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        self.jobs.lock().await.get(id).cloned()
    }

    /// Mutate a job in place if it exists. Memory-only (used for frequent progress
    /// ticks); call `persist_job` separately at status transitions.
    pub async fn update_job(&self, id: &str, f: impl FnOnce(&mut Job)) {
        if let Some(job) = self.jobs.lock().await.get_mut(id) {
            f(job);
        }
    }

    /// Mutate a job only if it has not reached a terminal state yet. Returns
    /// true when the update was applied. This keeps late worker completion from
    /// overwriting a user cancellation.
    pub async fn update_job_if_open(&self, id: &str, f: impl FnOnce(&mut Job)) -> bool {
        let mut guard = self.jobs.lock().await;
        let Some(job) = guard.get_mut(id) else {
            return false;
        };
        if job.status.is_terminal() {
            return false;
        }
        f(job);
        true
    }

    /// Write the current in-memory state of a job to the database (best-effort).
    pub async fn persist_job(&self, id: &str) {
        let job = self.jobs.lock().await.get(id).cloned();
        if let Some(job) = job {
            if let Err(e) = self.db.persist_job(&job).await {
                tracing::warn!("persist job {id}: {e}");
            }
        }
    }

    /// Load persisted jobs into memory at startup. Any job that was still in
    /// flight when the process stopped is marked `interrupted` so a polling
    /// client gets a clear terminal state instead of a 404 or an endless spinner.
    pub async fn recover_jobs(&self) {
        match self.db.load_recent_jobs(recover_jobs_limit()).await {
            Ok(jobs) => {
                let mut guard = self.jobs.lock().await;
                for mut job in jobs {
                    if !job.status.is_terminal() {
                        job.status = JobStatus::Interrupted;
                        job.stage = None;
                        if let Err(e) = self.db.persist_job(&job).await {
                            tracing::warn!("persist recovered job {}: {e}", job.id);
                        }
                    }
                    guard.insert(job.id.clone(), job);
                }
            }
            Err(e) => tracing::warn!("could not recover jobs: {e}"),
        }
    }

    /// Register a cancellation token for a job and return a clone the worker can
    /// watch. Call `clear_cancel` when the job finishes.
    pub async fn register_cancel(&self, id: &str) -> CancellationToken {
        let token = self.supervisor.child_token();
        self.cancels
            .lock()
            .await
            .insert(id.to_string(), token.clone());
        token
    }

    pub async fn clear_cancel(&self, id: &str) {
        self.cancels.lock().await.remove(id);
    }

    /// Atomically mark a non-terminal job as cancelled, then signal its worker.
    pub async fn cancel_open_job(&self, id: &str) -> CancelJobOutcome {
        {
            let mut guard = self.jobs.lock().await;
            let Some(job) = guard.get_mut(id) else {
                return CancelJobOutcome::NotFound;
            };
            if job.status.is_terminal() {
                return CancelJobOutcome::AlreadyFinished;
            }
            job.status = JobStatus::Cancelled;
            job.stage = None;
            job.progress = None;
        }

        if let Some(token) = self.cancels.lock().await.remove(id) {
            token.cancel();
        }
        self.persist_job(id).await;
        CancelJobOutcome::Cancelled
    }

    pub async fn render_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut guard = self.render_locks.lock().await;
        guard
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Wait for a download/render slot. Cancellation remains the caller's
    /// responsibility because job workers already own their cancellation token.
    pub async fn acquire_job_slot(&self) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.jobs_semaphore.clone().acquire_owned().await
    }

    /// Stop admitting queued jobs; this is the queue-level shutdown boundary.
    pub fn close_job_queue(&self) {
        self.jobs_semaphore.close();
    }

    pub fn spawn_task<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.supervisor.spawn(future)
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.supervisor.child_token()
    }

    pub fn begin_shutdown(&self) {
        self.jobs_semaphore.close();
        self.upload_semaphore.close();
        self.supervisor.begin_shutdown();
    }

    pub async fn wait_for_tasks(&self, limit: Duration) -> bool {
        self.supervisor.wait(limit).await
    }

    /// Uploads are synchronous HTTP requests, so they fail fast instead of
    /// occupying connections in an invisible queue.
    pub fn try_acquire_upload_slot(&self) -> Option<OwnedSemaphorePermit> {
        self.upload_semaphore.clone().try_acquire_owned().ok()
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.storage.join("sources")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.storage.join("staging")
    }

    pub fn outputs_dir(&self) -> PathBuf {
        self.storage.join("outputs")
    }
}

fn recover_jobs_limit() -> i64 {
    std::env::var("RECOVER_JOBS_LIMIT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_RECOVER_JOBS_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_jobs_limit_defaults_on_bad_values() {
        std::env::remove_var("RECOVER_JOBS_LIMIT");
        assert_eq!(recover_jobs_limit(), DEFAULT_RECOVER_JOBS_LIMIT);
        std::env::set_var("RECOVER_JOBS_LIMIT", "not-a-number");
        assert_eq!(recover_jobs_limit(), DEFAULT_RECOVER_JOBS_LIMIT);
        std::env::set_var("RECOVER_JOBS_LIMIT", "0");
        assert_eq!(recover_jobs_limit(), DEFAULT_RECOVER_JOBS_LIMIT);
        std::env::set_var("RECOVER_JOBS_LIMIT", "12");
        assert_eq!(recover_jobs_limit(), 12);
        std::env::remove_var("RECOVER_JOBS_LIMIT");
    }

    #[tokio::test]
    async fn render_lock_reuses_the_same_mutex_per_key() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);

        let a = st.render_lock("same").await;
        let b = st.render_lock("same").await;
        let c = st.render_lock("other").await;

        assert!(Arc::ptr_eq(&a, &b));
        assert!(!Arc::ptr_eq(&a, &c));
    }

    #[tokio::test]
    async fn job_and_upload_slots_are_independent_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);

        let _job_a = st.acquire_job_slot().await.unwrap();
        let _job_b = st.acquire_job_slot().await.unwrap();
        let upload_a = st.try_acquire_upload_slot().unwrap();
        let _upload_b = st.try_acquire_upload_slot().unwrap();
        assert!(st.try_acquire_upload_slot().is_none());

        drop(upload_a);
        assert!(st.try_acquire_upload_slot().is_some());
    }

    #[tokio::test]
    async fn cancel_open_job_is_atomic_for_non_terminal_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);

        st.set_job(Job::pending("running".into())).await;
        let token = st.register_cancel("running").await;
        st.update_job("running", |j| {
            j.status = JobStatus::Running;
            j.stage = Some("processing".into());
            j.progress = Some(12.0);
        })
        .await;

        assert_eq!(
            st.cancel_open_job("running").await,
            CancelJobOutcome::Cancelled
        );
        assert!(token.is_cancelled());
        let job = st.get_job("running").await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
        assert_eq!(
            st.db.load_jobs().await.unwrap()[0].status,
            JobStatus::Cancelled
        );

        assert_eq!(
            st.cancel_open_job("missing").await,
            CancelJobOutcome::NotFound
        );

        st.set_job(Job::pending("done".into())).await;
        st.update_job("done", |j| j.status = JobStatus::Done).await;
        assert_eq!(
            st.cancel_open_job("done").await,
            CancelJobOutcome::AlreadyFinished
        );
        assert_eq!(st.get_job("done").await.unwrap().status, JobStatus::Done);
    }
}
