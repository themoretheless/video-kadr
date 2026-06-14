use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::library::Library;
use crate::model::Job;

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
}

/// Shared application state. Jobs are kept in memory only — this is an MVP, so a
/// restart loses job history (the produced files on disk survive).
#[derive(Clone)]
pub struct AppState {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
    /// Per-job cancellation handles, removed when the job finishes.
    cancels: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Caps how many downloads/renders run at once; the rest wait as "queued".
    pub jobs_semaphore: Arc<Semaphore>,
    pub tools: Arc<ToolInfo>,
    pub library: Library,
    pub storage: PathBuf,
}

impl AppState {
    pub fn new(storage: PathBuf, max_concurrent: usize, tools: ToolInfo, library: Library) -> Self {
        AppState {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            cancels: Arc::new(Mutex::new(HashMap::new())),
            jobs_semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            tools: Arc::new(tools),
            library,
            storage,
        }
    }

    pub async fn set_job(&self, job: Job) {
        self.jobs.lock().await.insert(job.id.clone(), job);
    }

    pub async fn get_job(&self, id: &str) -> Option<Job> {
        self.jobs.lock().await.get(id).cloned()
    }

    /// Mutate a job in place if it exists.
    pub async fn update_job(&self, id: &str, f: impl FnOnce(&mut Job)) {
        if let Some(job) = self.jobs.lock().await.get_mut(id) {
            f(job);
        }
    }

    /// Register a cancellation token for a job and return a clone the worker can
    /// watch. Call `clear_cancel` when the job finishes.
    pub async fn register_cancel(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.cancels
            .lock()
            .await
            .insert(id.to_string(), token.clone());
        token
    }

    pub async fn clear_cancel(&self, id: &str) {
        self.cancels.lock().await.remove(id);
    }

    /// Signal cancellation for a job. Returns true if a live token was found.
    pub async fn cancel(&self, id: &str) -> bool {
        if let Some(token) = self.cancels.lock().await.remove(id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn sources_dir(&self) -> PathBuf {
        self.storage.join("sources")
    }

    pub fn outputs_dir(&self) -> PathBuf {
        self.storage.join("outputs")
    }
}
