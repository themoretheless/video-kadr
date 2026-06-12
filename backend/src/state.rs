use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::model::Job;

/// Shared application state. Jobs are kept in memory only — this is an MVP, so a
/// restart loses job history (the produced files on disk survive).
#[derive(Clone)]
pub struct AppState {
    jobs: Arc<Mutex<HashMap<String, Job>>>,
    pub storage: PathBuf,
}

impl AppState {
    pub fn new(storage: PathBuf) -> Self {
        AppState {
            jobs: Arc::new(Mutex::new(HashMap::new())),
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

    pub fn sources_dir(&self) -> PathBuf {
        self.storage.join("sources")
    }

    pub fn outputs_dir(&self) -> PathBuf {
        self.storage.join("outputs")
    }
}
