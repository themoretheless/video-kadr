use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{AcquireError, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use crate::db::Db;
use crate::jobs::{
    EnqueueOutcome, JobCell, JobEvent, JobKind, JobPermit, QueueLimits, SqliteJobStore,
};
use crate::library::Library;
use crate::model::Job;
use crate::ports::{MediaDocument, MediaSearch, SqliteMediaSearch};
use crate::runtime::cpu_pool::{CpuPool, CpuPoolConfig};
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
/// progress). SQLite owns durable requests/events/outbox rows, while one
/// `JobCell` per id keeps unrelated transitions from sharing a global lock.
#[derive(Clone)]
pub struct AppState {
    jobs: Arc<Mutex<HashMap<String, Arc<JobCell>>>>,
    /// Per-job cancellation handles, removed when the job finishes.
    cancels: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Per-render-cache-key locks. They serialize identical edit requests so
    /// only one worker renders while followers wait and then reuse the cache.
    render_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
    /// Downloads/renders queue here; uploads use a separate pool so a slow
    /// client cannot consume every render slot.
    jobs_semaphore: Arc<Semaphore>,
    render_semaphore: Arc<Semaphore>,
    upload_semaphore: Arc<Semaphore>,
    max_concurrent_renders: usize,
    supervisor: TaskSupervisor,
    pub cpu_pool: CpuPool,
    pub encode_budget: EncodeBudget,
    pub tools: Arc<ToolInfo>,
    pub library: Library,
    pub db: Db,
    pub job_store: SqliteJobStore,
    pub media_search: Arc<dyn MediaSearch>,
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
        let limits = RuntimeLimits::detect();
        let encode_budget = EncodeBudget::for_profile(EncodeProfile::Balanced, limits)
            .expect("detected runtime limits must produce a valid encode budget");
        Self::new_with_runtime(
            storage,
            max_concurrent,
            tools,
            library,
            db,
            encode_budget,
            limits.logical_cpus.saturating_mul(2).max(2),
        )
        .expect("detected runtime limits must produce a CPU pool")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_runtime(
        storage: PathBuf,
        max_concurrent: usize,
        tools: ToolInfo,
        library: Library,
        db: Db,
        encode_budget: EncodeBudget,
        cpu_queue_capacity: usize,
    ) -> anyhow::Result<Self> {
        let max_concurrent = max_concurrent.max(1);
        let max_concurrent_renders = max_concurrent.min(encode_budget.threads).max(1);
        let cpu_pool = CpuPool::new(CpuPoolConfig {
            threads: encode_budget.threads,
            queue_capacity: cpu_queue_capacity,
        })?;
        let job_store = SqliteJobStore::new(db.clone());
        let media_search = Arc::new(SqliteMediaSearch::new(db.clone()));
        Ok(AppState {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            cancels: Arc::new(Mutex::new(HashMap::new())),
            render_locks: Arc::new(Mutex::new(HashMap::new())),
            jobs_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            render_semaphore: Arc::new(Semaphore::new(max_concurrent_renders)),
            upload_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent_renders,
            supervisor: TaskSupervisor::default(),
            cpu_pool,
            encode_budget,
            tools: Arc::new(tools),
            library,
            db,
            job_store,
            media_search,
            storage,
        })
    }

    pub async fn set_job(&self, job: Job) {
        if let Err(e) = self.db.persist_job(&job).await {
            tracing::warn!("persist job {}: {e}", job.id);
        }
        if let Err(error) = self
            .job_store
            .record_transition(&job, &JobEvent::Created, "legacy-created", None)
            .await
        {
            tracing::warn!(job.id = %job.id, %error, "persist legacy job event");
        }
        self.remember_job(job).await;
    }

    pub async fn enqueue_job(
        &self,
        id: String,
        kind: JobKind,
        payload: &Value,
        dedupe_key: &str,
    ) -> anyhow::Result<EnqueueOutcome> {
        let outcome = self
            .job_store
            .enqueue(id, kind, payload, dedupe_key, queue_limits())
            .await?;
        if let EnqueueOutcome::Created(job) = &outcome {
            self.remember_job(job.clone()).await;
        }
        Ok(outcome)
    }

    pub async fn remember_job(&self, job: Job) {
        self.jobs
            .lock()
            .await
            .insert(job.id.clone(), Arc::new(JobCell::new(job)));
    }

    pub async fn replace_job(&self, job: Job) {
        if let Some(cell) = self.job_cell(&job.id).await {
            cell.replace(job).await;
        } else {
            self.remember_job(job).await;
        }
    }

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        if let Some(cell) = self.job_cell(id).await {
            return Some(cell.snapshot().await);
        }
        let persisted = match self.db.load_job(id).await {
            Ok(Some(job)) => job,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(job.id = id, %error, "load job on cache miss");
                return None;
            }
        };
        let cell = {
            let mut jobs = self.jobs.lock().await;
            jobs.entry(id.to_string())
                .or_insert_with(|| Arc::new(JobCell::new(persisted)))
                .clone()
        };
        Some(cell.snapshot().await)
    }

    /// Mutate a job in place if it exists. Memory-only (used for frequent progress
    /// ticks); call `persist_job` separately at status transitions.
    pub async fn update_job(&self, id: &str, f: impl FnOnce(&mut Job)) {
        if let Some(cell) = self.job_cell(id).await {
            cell.update(f).await;
        }
    }

    /// Mutate a job only if it has not reached a terminal state yet. Returns
    /// true when the update was applied. This keeps late worker completion from
    /// overwriting a user cancellation.
    pub async fn update_job_if_open(&self, id: &str, f: impl FnOnce(&mut Job)) -> bool {
        let Some(cell) = self.job_cell(id).await else {
            return false;
        };
        cell.update_if_open(f).await
    }

    pub async fn transition_job(&self, id: &str, event: JobEvent) -> bool {
        self.transition_job_with_key(id, event, &Uuid::new_v4().to_string())
            .await
    }

    pub async fn transition_job_with_key(
        &self,
        id: &str,
        event: JobEvent,
        idempotency_key: &str,
    ) -> bool {
        let Some(cell) = self.job_cell(id).await else {
            return false;
        };
        let tool_version = match &event {
            JobEvent::Started { stage, .. } if stage == "downloading" => {
                self.tools.ytdlp_version.as_deref()
            }
            JobEvent::Started { .. } => self.tools.ffmpeg_version.as_deref(),
            _ => None,
        }
        .map(str::to_owned);
        let store = self.job_store.clone();
        let persisted_event = event.clone();
        let persisted_key = idempotency_key.to_string();
        match cell
            .apply_durable(&event, move |snapshot| async move {
                let inserted = store
                    .record_transition(
                        &snapshot,
                        &persisted_event,
                        &persisted_key,
                        tool_version.as_deref(),
                    )
                    .await?;
                anyhow::ensure!(inserted, "job transition idempotency key already used");
                Ok(())
            })
            .await
        {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(error) => {
                tracing::error!(job.id = id, %error, "persist job transition");
                false
            }
        }
    }

    /// Write the current in-memory state of a job to the database (best-effort).
    pub async fn persist_job(&self, id: &str) {
        if let Some(job) = self.get_job(id).await {
            if let Err(e) = self.db.persist_job(&job).await {
                tracing::warn!("persist job {id}: {e}");
            }
        }
    }

    /// Reconcile abandoned attempts/outbox leases, then restore the bounded hot
    /// set. Durable requests that can be retried remain pending for dispatch.
    pub async fn recover_jobs(&self) {
        match self.job_store.reconcile_started().await {
            Ok(report) if report.requeued > 0 || report.interrupted > 0 => tracing::info!(
                requeued = report.requeued,
                interrupted = report.interrupted,
                "reconciled durable jobs"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "could not reconcile jobs"),
        }
        match self.db.load_recent_jobs(recover_jobs_limit()).await {
            Ok(jobs) => {
                let mut guard = self.jobs.lock().await;
                for job in jobs {
                    guard.insert(job.id.clone(), Arc::new(JobCell::new(job)));
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
        let Some(job) = self.get_job(id).await else {
            return CancelJobOutcome::NotFound;
        };
        if job.status.is_terminal() {
            return CancelJobOutcome::AlreadyFinished;
        }
        if !self.transition_job(id, JobEvent::Cancelled).await {
            return CancelJobOutcome::AlreadyFinished;
        }

        if let Some(token) = self.cancels.lock().await.remove(id) {
            token.cancel();
        }
        CancelJobOutcome::Cancelled
    }

    pub async fn render_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut guard = self.render_locks.lock().await;
        guard.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = guard.get(key).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(Mutex::new(()));
        guard.insert(key.to_string(), Arc::downgrade(&lock));
        lock
    }

    /// Wait for a download/render slot. Cancellation remains the caller's
    /// responsibility because job workers already own their cancellation token.
    pub async fn acquire_job_slot(&self) -> Result<JobPermit, AcquireError> {
        self.jobs_semaphore
            .clone()
            .acquire_owned()
            .await
            .map(JobPermit::new)
    }

    /// Render admission is additionally capped by the process-wide encoder
    /// thread budget. Every admitted FFmpeg process can therefore receive at
    /// least one thread without oversubscribing the declared total.
    pub async fn acquire_render_slot(&self) -> Result<JobPermit, AcquireError> {
        self.render_semaphore
            .clone()
            .acquire_owned()
            .await
            .map(JobPermit::new)
    }

    /// Stop admitting queued jobs; this is the queue-level shutdown boundary.
    pub fn close_job_queue(&self) {
        self.jobs_semaphore.close();
        self.render_semaphore.close();
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
        self.render_semaphore.close();
        self.upload_semaphore.close();
        self.supervisor.begin_shutdown();
    }

    pub fn is_shutting_down(&self) -> bool {
        self.supervisor.is_shutting_down()
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

    pub fn render_parallelism(&self) -> usize {
        self.max_concurrent_renders
    }

    pub async fn rebuild_media_search(&self) {
        let documents: Vec<_> = self
            .library
            .list()
            .await
            .iter()
            .map(MediaDocument::from)
            .collect();
        if let Err(error) = self.media_search.rebuild(&documents).await {
            tracing::warn!(%error, "rebuild media search index");
        }
    }

    pub async fn index_media(&self, entry: &crate::library::MediaEntry) {
        if let Err(error) = self.media_search.index(&MediaDocument::from(entry)).await {
            tracing::warn!(media.id = %entry.id, %error, "index media");
        }
    }

    async fn job_cell(&self, id: &str) -> Option<Arc<JobCell>> {
        self.jobs.lock().await.get(id).cloned()
    }
}

fn recover_jobs_limit() -> i64 {
    std::env::var("RECOVER_JOBS_LIMIT")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_RECOVER_JOBS_LIMIT)
}

fn queue_limits() -> QueueLimits {
    let mut limits = QueueLimits::default();
    if let Some(value) = env_u64("JOB_DEDUPE_TTL_SECS") {
        limits.dedupe_ttl = Duration::from_secs(value);
    }
    if let Some(value) = env_u64("JOB_RATE_WINDOW_SECS") {
        limits.rate_window = Duration::from_secs(value);
    }
    if let Some(value) = env_u64("JOB_RATE_LIMIT") {
        limits.max_new_jobs = u32::try_from(value).unwrap_or(u32::MAX);
    }
    limits
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::JobStatus;

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
        drop((a, b, c));
        let fresh = st.render_lock("fresh").await;
        assert_eq!(st.render_locks.lock().await.len(), 1);
        drop(fresh);
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
    async fn render_slots_are_capped_by_encode_threads() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let budget = EncodeBudget {
            threads: 1,
            tiles: crate::config::encode_budget::TileLayout {
                columns: 1,
                rows: 1,
            },
            speed: 6,
            memory_mib: 512,
        };
        let st = AppState::new_with_runtime(storage, 4, ToolInfo::default(), lib, db, budget, 2)
            .unwrap();

        let _render = st.acquire_render_slot().await.unwrap();
        assert!(st.render_semaphore.try_acquire().is_err());
        assert_eq!(st.render_parallelism(), 1);
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

    #[tokio::test]
    async fn durable_job_is_loaded_when_the_hot_set_misses() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        db.persist_job(&Job::pending("cold".into())).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);

        assert_eq!(st.get_job("cold").await.unwrap().id, "cold");
        assert!(st.job_cell("cold").await.is_some());
    }

    #[tokio::test]
    async fn reused_event_key_cannot_diverge_memory_from_the_durable_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        st.set_job(Job::pending("keyed".into())).await;

        assert!(
            st.transition_job_with_key("keyed", JobEvent::Queued, "same-key")
                .await
        );
        assert!(
            !st.transition_job_with_key(
                "keyed",
                JobEvent::Started {
                    stage: "processing".into(),
                    attempt: 1,
                },
                "same-key",
            )
            .await
        );
        assert_eq!(
            st.get_job("keyed").await.unwrap().status,
            JobStatus::Pending
        );
        assert_eq!(
            st.db.load_job("keyed").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }
}
