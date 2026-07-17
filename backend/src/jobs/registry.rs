use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobLifecycle {
    Queued,
    Started,
    Deferred,
    Failed,
    Finished,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleCounts {
    pub queued: u64,
    pub started: u64,
    pub deferred: u64,
    pub failed: u64,
    pub finished: u64,
}

impl LifecycleCounts {
    pub fn increment(&mut self, lifecycle: JobLifecycle) {
        match lifecycle {
            JobLifecycle::Queued => self.queued += 1,
            JobLifecycle::Started => self.started += 1,
            JobLifecycle::Deferred => self.deferred += 1,
            JobLifecycle::Failed => self.failed += 1,
            JobLifecycle::Finished => self.finished += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub requeued: u64,
    pub interrupted: u64,
}
