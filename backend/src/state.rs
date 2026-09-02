use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::{AcquireError, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::analysis::proxy::ProxyService;
use crate::analysis::thumbnail::{infer_thumbnail_kind, ThumbnailService, ThumbnailSource};
use crate::config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use crate::config::resource_classes::{ResourceClass, ResourceClassLimits};
use crate::config::WorkloadConfig;
use crate::db::Db;
use crate::jobs::{EnqueueOutcome, JobCell, JobEvent, JobKind, JobPermit, SqliteJobStore};
use crate::library::Library;
use crate::model::Job;
use crate::ports::telemetry::{JobOutcome, ResourceObservation, TelemetryEvent, TelemetryPort};
use crate::ports::{MediaIndexWriter, MediaSearchQuery, SqliteMediaSearch};
use crate::process_control::ProcessRuntime;
use crate::runtime::cpu_pool::{CpuPool, CpuPoolConfig};
use crate::runtime::TaskSupervisor;
use crate::services::media_indexer::MediaIndexer;
use crate::tools::proxy::FfmpegProxyEncoder;
use crate::tools::thumbnail::FfmpegThumbnailEncoder;

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
    job_kinds: Arc<Mutex<HashMap<String, JobKind>>>,
    /// Per-job cancellation handles, removed when the job finishes.
    cancels: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Per-render-cache-key locks. They serialize identical edit requests so
    /// only one worker renders while followers wait and then reuse the cache.
    render_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
    /// Independent admission pools prevent ingest saturation from consuming
    /// analysis or export capacity.
    ingest_semaphore: Arc<Semaphore>,
    analysis_semaphore: Arc<Semaphore>,
    export_semaphore: Arc<Semaphore>,
    resource_limits: ResourceClassLimits,
    render_semaphore: Arc<Semaphore>,
    upload_semaphore: Arc<Semaphore>,
    max_concurrent_renders: usize,
    workload: Arc<WorkloadConfig>,
    supervisor: TaskSupervisor,
    pub cpu_pool: CpuPool,
    pub encode_budget: EncodeBudget,
    pub process_runtime: ProcessRuntime,
    pub proxy_service: ProxyService<FfmpegProxyEncoder>,
    pub thumbnail_service: ThumbnailService<FfmpegThumbnailEncoder>,
    pub tools: Arc<ToolInfo>,
    pub library: Library,
    pub db: Db,
    pub job_store: SqliteJobStore,
    pub media_search: Arc<dyn MediaSearchQuery>,
    pub media_index: Arc<dyn MediaIndexWriter>,
    pub media_indexer: MediaIndexer,
    pub telemetry: Arc<dyn TelemetryPort>,
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
        Self::new_with_process_runtime(
            storage,
            max_concurrent,
            tools,
            library,
            db,
            encode_budget,
            cpu_queue_capacity,
            ProcessRuntime::local_default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_process_runtime(
        storage: PathBuf,
        max_concurrent: usize,
        tools: ToolInfo,
        library: Library,
        db: Db,
        encode_budget: EncodeBudget,
        cpu_queue_capacity: usize,
        process_runtime: ProcessRuntime,
    ) -> anyhow::Result<Self> {
        let max_concurrent = max_concurrent.max(1);
        Self::new_with_resource_limits(
            storage,
            max_concurrent,
            tools,
            library,
            db,
            encode_budget,
            cpu_queue_capacity,
            process_runtime,
            ResourceClassLimits::balanced(max_concurrent),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_resource_limits(
        storage: PathBuf,
        max_concurrent: usize,
        tools: ToolInfo,
        library: Library,
        db: Db,
        encode_budget: EncodeBudget,
        cpu_queue_capacity: usize,
        process_runtime: ProcessRuntime,
        resource_limits: ResourceClassLimits,
    ) -> anyhow::Result<Self> {
        let max_concurrent = max_concurrent.max(1);
        anyhow::ensure!(
            ResourceClass::ALL
                .into_iter()
                .all(|class| resource_limits.get(class) > 0),
            "resource class limits must be greater than zero"
        );
        let max_concurrent_renders = max_concurrent.min(encode_budget.threads).max(1);
        let cpu_pool = CpuPool::new(CpuPoolConfig {
            threads: encode_budget.threads,
            queue_capacity: cpu_queue_capacity,
        })?;
        let job_store = SqliteJobStore::new(db.clone());
        let media_adapter = Arc::new(SqliteMediaSearch::new(db.clone()));
        let media_search: Arc<dyn MediaSearchQuery> = media_adapter.clone();
        let media_index: Arc<dyn MediaIndexWriter> = media_adapter;
        let media_indexer = MediaIndexer::new(db.clone(), media_index.clone());
        let proxy_service = ProxyService::new(
            storage.clone(),
            cpu_pool.clone(),
            Arc::new(FfmpegProxyEncoder::new(process_runtime.clone())),
        );
        let render_semaphore = Arc::new(Semaphore::new(max_concurrent_renders));
        let thumbnail_service = ThumbnailService::new(
            storage.join("thumbnails"),
            Arc::new(FfmpegThumbnailEncoder::new(process_runtime.clone())),
            render_semaphore.clone(),
        );
        Ok(AppState {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            job_kinds: Arc::new(Mutex::new(HashMap::new())),
            cancels: Arc::new(Mutex::new(HashMap::new())),
            render_locks: Arc::new(Mutex::new(HashMap::new())),
            ingest_semaphore: Arc::new(Semaphore::new(resource_limits.ingest)),
            analysis_semaphore: Arc::new(Semaphore::new(resource_limits.analysis)),
            export_semaphore: Arc::new(Semaphore::new(resource_limits.export)),
            resource_limits,
            render_semaphore,
            upload_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent_renders,
            workload: Arc::new(WorkloadConfig::default()),
            supervisor: TaskSupervisor::default(),
            cpu_pool,
            encode_budget,
            process_runtime,
            proxy_service,
            thumbnail_service,
            tools: Arc::new(tools),
            library,
            db,
            job_store,
            media_search,
            media_index,
            media_indexer,
            telemetry: Arc::new(crate::telemetry::metrics::PrometheusTelemetry::default()),
            storage,
        })
    }

    pub fn with_workload_config(mut self, workload: WorkloadConfig) -> Self {
        self.workload = Arc::new(workload);
        self
    }

    pub fn with_telemetry(mut self, telemetry: Arc<dyn TelemetryPort>) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Reconcile private thumbnail derivatives against the current library
    /// before the HTTP server starts serving cache keys.
    pub async fn cleanup_thumbnail_cache(&self) {
        let mut sources = Vec::new();
        for entry in self.library.list().await {
            let Some(kind) = infer_thumbnail_kind(&entry) else {
                continue;
            };
            let Ok(path) = self.library.resolve_media_path(&entry).await else {
                continue;
            };
            sources.push(ThumbnailSource {
                id: entry.id,
                path,
                kind,
                duration_seconds: entry.duration,
            });
        }
        if let Err(error) = self.thumbnail_service.cleanup_stale(&sources).await {
            tracing::warn!(%error, "failed to reconcile thumbnail cache");
        }
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
            .enqueue(id, kind, payload, dedupe_key, self.workload.queue_limits)
            .await?;
        match &outcome {
            EnqueueOutcome::Created(job) => {
                self.job_kinds.lock().await.insert(job.id.clone(), kind);
                self.remember_job(job.clone()).await;
            }
            EnqueueOutcome::Existing(id) => {
                self.job_kinds.lock().await.insert(id.clone(), kind);
            }
            EnqueueOutcome::RateLimited => {}
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

    pub async fn get_job(&self, id: &str) -> anyhow::Result<Option<Job>> {
        if let Some(cell) = self.job_cell(id).await {
            return Ok(Some(cell.snapshot().await));
        }
        let Some(persisted) = self.db.load_job(id).await? else {
            return Ok(None);
        };
        let cell = {
            let mut jobs = self.jobs.lock().await;
            jobs.entry(id.to_string())
                .or_insert_with(|| Arc::new(JobCell::new(persisted)))
                .clone()
        };
        Ok(Some(cell.snapshot().await))
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

    pub async fn transition_job(&self, id: &str, event: JobEvent) -> anyhow::Result<bool> {
        self.transition_job_with_key(id, event, &Uuid::new_v4().to_string())
            .await
    }

    pub async fn transition_job_with_key(
        &self,
        id: &str,
        event: JobEvent,
        idempotency_key: &str,
    ) -> anyhow::Result<bool> {
        let Some(cell) = self.job_cell(id).await else {
            return Ok(false);
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
        let applied = match cell
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
            Err(error) => return Err(error),
        };
        if applied {
            let outcome = match event {
                JobEvent::Succeeded { .. } => Some(JobOutcome::Succeeded),
                JobEvent::Failed { .. } | JobEvent::Interrupted { .. } => Some(JobOutcome::Failed),
                JobEvent::Cancelled | JobEvent::Discarded { .. } => Some(JobOutcome::Cancelled),
                _ => None,
            };
            if let Some(outcome) = outcome {
                if let Some(kind) = self.job_kinds.lock().await.remove(id) {
                    self.telemetry.record(TelemetryEvent::JobTerminal {
                        kind: kind.as_str(),
                        outcome,
                    });
                }
            }
        }
        Ok(applied)
    }

    /// Write the current in-memory state of a job to the database (best-effort).
    pub async fn persist_job(&self, id: &str) {
        match self.get_job(id).await {
            Ok(Some(job)) => {
                if let Err(error) = self.db.persist_job(&job).await {
                    tracing::warn!(job.id = id, %error, "persist job");
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(job.id = id, %error, "load job before persist"),
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
        match self
            .db
            .load_recent_jobs(self.workload.recover_jobs_limit)
            .await
        {
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
    pub async fn cancel_open_job(&self, id: &str) -> anyhow::Result<CancelJobOutcome> {
        let Some(job) = self.get_job(id).await? else {
            return Ok(CancelJobOutcome::NotFound);
        };
        if job.status.is_terminal() {
            return Ok(CancelJobOutcome::AlreadyFinished);
        }
        if !self.transition_job(id, JobEvent::Cancelled).await? {
            return Ok(CancelJobOutcome::AlreadyFinished);
        }

        if let Some(token) = self.cancels.lock().await.remove(id) {
            token.cancel();
        }
        Ok(CancelJobOutcome::Cancelled)
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
    pub async fn acquire_job_slot(&self, class: ResourceClass) -> Result<JobPermit, AcquireError> {
        let started = std::time::Instant::now();
        let permit = self
            .resource_semaphore(class)
            .clone()
            .acquire_owned()
            .await
            .map(JobPermit::new);
        self.telemetry.record(TelemetryEvent::QueueWait {
            class,
            duration: started.elapsed(),
        });
        permit
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
        self.ingest_semaphore.close();
        self.analysis_semaphore.close();
        self.export_semaphore.close();
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
        self.ingest_semaphore.close();
        self.analysis_semaphore.close();
        self.export_semaphore.close();
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
    pub fn try_acquire_upload_slot(&self) -> Option<UploadPermit> {
        let ingest = self.ingest_semaphore.clone().try_acquire_owned().ok()?;
        let upload = self.upload_semaphore.clone().try_acquire_owned().ok()?;
        Some(UploadPermit {
            _ingest: ingest,
            _upload: upload,
        })
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

    pub fn luts_dir(&self) -> PathBuf {
        self.storage.join("luts")
    }

    pub fn render_parallelism(&self) -> usize {
        self.max_concurrent_renders
    }

    pub fn resource_limit(&self, class: ResourceClass) -> usize {
        self.resource_limits.get(class)
    }

    pub fn resource_available(&self, class: ResourceClass) -> usize {
        self.resource_semaphore(class).available_permits()
    }

    pub fn prometheus_metrics(&self) -> Option<String> {
        let resources: Vec<_> = ResourceClass::ALL
            .into_iter()
            .map(|class| ResourceObservation {
                class,
                available: self.resource_available(class),
                limit: self.resource_limit(class),
            })
            .collect();
        self.telemetry.render_prometheus(&resources)
    }

    fn resource_semaphore(&self, class: ResourceClass) -> &Arc<Semaphore> {
        match class {
            ResourceClass::Ingest => &self.ingest_semaphore,
            ResourceClass::Analysis => &self.analysis_semaphore,
            ResourceClass::Export => &self.export_semaphore,
        }
    }

    pub fn job_timeout(&self) -> Duration {
        self.workload.job_timeout
    }

    pub fn max_download_height(&self) -> u32 {
        self.workload.max_download_height
    }

    pub async fn rebuild_media_search(&self) {
        let entries = self.library.list().await;
        match self.media_indexer.rebuild(&entries).await {
            Ok(report) if !report.bad_entries.is_empty() => tracing::warn!(
                media_index.bad_entries = report.bad_entries.len(),
                "rebuilt media index with isolated bad entries"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "rebuild media search index"),
        }
    }

    pub async fn sync_media_search(&self) {
        let entries = self.library.list().await;
        match self.media_indexer.sync_incremental(&entries).await {
            Ok(report) => {
                if !report.bad_entries.is_empty() || report.deferred_error.is_some() {
                    tracing::warn!(
                        media_index.bad_entries = report.bad_entries.len(),
                        media_index.deferred = report.deferred_error.is_some(),
                        "incremental media index completed with isolated entries"
                    );
                }
            }
            Err(error) => tracing::warn!(%error, "sync media search index"),
        }
    }

    pub async fn index_media(&self, entry: &crate::library::MediaEntry) {
        if let Err(error) = self.media_indexer.index_entry(entry).await {
            tracing::warn!(media.id = %entry.id, %error, "index media");
        }
    }

    async fn job_cell(&self, id: &str) -> Option<Arc<JobCell>> {
        self.jobs.lock().await.get(id).cloned()
    }
}

pub struct UploadPermit {
    _ingest: OwnedSemaphorePermit,
    _upload: OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::QueueLimits;
    use crate::model::JobStatus;
    use serde_json::json;

    #[tokio::test]
    async fn configured_workload_drives_job_and_import_limits() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let workload = WorkloadConfig {
            job_timeout: Duration::from_secs(45),
            recover_jobs_limit: 10,
            queue_limits: QueueLimits {
                max_new_jobs: 1,
                ..QueueLimits::default()
            },
            max_download_height: 480,
        };
        let st =
            AppState::new(storage, 2, ToolInfo::default(), lib, db).with_workload_config(workload);

        assert_eq!(st.job_timeout(), Duration::from_secs(45));
        assert_eq!(st.max_download_height(), 480);
        assert!(matches!(
            st.enqueue_job("one".into(), JobKind::Import, &json!({}), "first")
                .await
                .unwrap(),
            EnqueueOutcome::Created(_)
        ));
        assert_eq!(
            st.enqueue_job("two".into(), JobKind::Import, &json!({}), "second")
                .await
                .unwrap(),
            EnqueueOutcome::RateLimited
        );
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

        let _job_a = st.acquire_job_slot(ResourceClass::Export).await.unwrap();
        let _job_b = st.acquire_job_slot(ResourceClass::Export).await.unwrap();
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
    async fn saturated_ingest_does_not_delay_export_admission_p95() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        let _ingest_a = st.acquire_job_slot(ResourceClass::Ingest).await.unwrap();
        let _ingest_b = st.acquire_job_slot(ResourceClass::Ingest).await.unwrap();

        let mut samples = Vec::new();
        for _ in 0..100 {
            let started = std::time::Instant::now();
            let export = tokio::time::timeout(
                Duration::from_millis(25),
                st.acquire_job_slot(ResourceClass::Export),
            )
            .await
            .expect("ingest saturation must not block export")
            .unwrap();
            samples.push(started.elapsed());
            drop(export);
        }
        samples.sort_unstable();
        assert!(samples[94] < Duration::from_millis(25));
        assert_eq!(st.resource_available(ResourceClass::Ingest), 0);
        assert_eq!(st.resource_available(ResourceClass::Export), 2);
    }

    #[tokio::test]
    async fn terminal_job_records_bounded_kind_and_outcome_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let telemetry = Arc::new(crate::ports::telemetry::TestTelemetry::default());
        let st = AppState::new(storage, 1, ToolInfo::default(), lib, db)
            .with_telemetry(telemetry.clone());
        st.enqueue_job(
            "metric-job".into(),
            JobKind::Import,
            &json!({}),
            "metric-dedupe",
        )
        .await
        .unwrap();
        st.transition_job("metric-job", JobEvent::Queued)
            .await
            .unwrap();
        st.transition_job("metric-job", JobEvent::Succeeded { result: json!({}) })
            .await
            .unwrap();
        assert!(telemetry.events().contains(&TelemetryEvent::JobTerminal {
            kind: "import",
            outcome: JobOutcome::Succeeded,
        }));
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
            st.cancel_open_job("running").await.unwrap(),
            CancelJobOutcome::Cancelled
        );
        assert!(token.is_cancelled());
        let job = st.get_job("running").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
        assert_eq!(
            st.db.load_jobs().await.unwrap()[0].status,
            JobStatus::Cancelled
        );

        assert_eq!(
            st.cancel_open_job("missing").await.unwrap(),
            CancelJobOutcome::NotFound
        );

        st.set_job(Job::pending("done".into())).await;
        st.update_job("done", |j| j.status = JobStatus::Done).await;
        assert_eq!(
            st.cancel_open_job("done").await.unwrap(),
            CancelJobOutcome::AlreadyFinished
        );
        assert_eq!(
            st.get_job("done").await.unwrap().unwrap().status,
            JobStatus::Done
        );
    }

    #[tokio::test]
    async fn durable_job_is_loaded_when_the_hot_set_misses() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        db.persist_job(&Job::pending("cold".into())).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);

        assert_eq!(st.get_job("cold").await.unwrap().unwrap().id, "cold");
        assert!(st.job_cell("cold").await.is_some());
    }

    #[tokio::test]
    async fn job_storage_failure_is_not_reported_as_a_cache_miss() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        st.db.pool().close().await;

        assert!(st.get_job("cold").await.is_err());
        assert!(st.cancel_open_job("cold").await.is_err());
    }

    #[tokio::test]
    async fn cancellation_persistence_failure_is_not_reported_as_finished() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        st.set_job(Job::pending("running".into())).await;
        let token = st.register_cancel("running").await;
        st.db.pool().close().await;

        assert!(st.cancel_open_job("running").await.is_err());
        assert!(!token.is_cancelled());
        assert_eq!(
            st.get_job("running").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }

    #[tokio::test]
    async fn reused_event_key_cannot_diverge_memory_from_the_durable_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        let st = AppState::new(storage, 2, ToolInfo::default(), lib, db);
        st.set_job(Job::pending("keyed".into())).await;

        assert!(st
            .transition_job_with_key("keyed", JobEvent::Queued, "same-key")
            .await
            .unwrap());
        assert!(st
            .transition_job_with_key(
                "keyed",
                JobEvent::Started {
                    stage: "processing".into(),
                    attempt: 1,
                },
                "same-key",
            )
            .await
            .is_err());
        assert_eq!(
            st.get_job("keyed").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
        assert_eq!(
            st.db.load_job("keyed").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }
}
