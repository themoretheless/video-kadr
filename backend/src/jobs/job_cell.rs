use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use tokio::sync::{Mutex, OwnedSemaphorePermit};

use crate::model::{Job, JobStatus};

use super::event_log::{apply, JobEvent};

const OPEN: u8 = 0;
const DONE: u8 = 1;
const ERROR: u8 = 2;
const CANCELLED: u8 = 3;
const INTERRUPTED: u8 = 4;

macro_rules! claim_once {
    ($atomic:expr, $value:expr) => {
        $atomic.compare_exchange(OPEN, $value, Ordering::AcqRel, Ordering::Acquire)
    };
}

macro_rules! release_once {
    ($atomic:expr) => {
        $atomic.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
    };
}

/// Ownership wrapper used by the real queue path. Rust ownership drops the
/// permit once; the gate makes that invariant explicit and model-checkable.
pub struct JobPermit {
    permit: Option<OwnedSemaphorePermit>,
    released: AtomicBool,
}

impl JobPermit {
    pub fn new(permit: OwnedSemaphorePermit) -> Self {
        Self {
            permit: Some(permit),
            released: AtomicBool::new(false),
        }
    }
}

impl Drop for JobPermit {
    fn drop(&mut self) {
        let first_release = release_once!(self.released).is_ok();
        debug_assert!(first_release, "job permit released more than once");
        if first_release {
            self.permit.take();
        }
    }
}

/// Per-job synchronization boundary. The outer map lock is used only to locate
/// a cell; transitions for unrelated jobs never block each other.
pub struct JobCell {
    job: Mutex<Job>,
    terminal: AtomicU8,
    cancel_requested: AtomicBool,
}

impl JobCell {
    pub fn new(job: Job) -> Self {
        Self {
            terminal: AtomicU8::new(terminal_code(job.status)),
            cancel_requested: AtomicBool::new(job.status == JobStatus::Cancelled),
            job: Mutex::new(job),
        }
    }

    pub async fn snapshot(&self) -> Job {
        self.job.lock().await.clone()
    }

    pub async fn update(&self, update: impl FnOnce(&mut Job)) {
        let mut job = self.job.lock().await;
        update(&mut job);
        self.synchronize_flags(job.status);
    }

    pub async fn update_if_open(&self, update: impl FnOnce(&mut Job)) -> bool {
        let mut job = self.job.lock().await;
        if job.status.is_terminal() || self.terminal.load(Ordering::Acquire) != OPEN {
            return false;
        }
        update(&mut job);
        self.synchronize_flags(job.status);
        true
    }

    pub async fn apply(&self, event: &JobEvent) -> Option<Job> {
        self.apply_durable(event, |_| async { Ok(()) })
            .await
            .ok()
            .flatten()
    }

    pub async fn apply_durable<F, Fut>(
        &self,
        event: &JobEvent,
        persist: F,
    ) -> anyhow::Result<Option<Job>>
    where
        F: FnOnce(Job) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        let mut job = self.job.lock().await;
        if event_terminal_code(event).is_some() {
            if self.terminal.load(Ordering::Acquire) != OPEN
                && !matches!(event, JobEvent::Discarded { .. })
            {
                return Ok(None);
            }
        } else if matches!(event, JobEvent::RetryScheduled { .. }) {
            if !job.status.is_terminal() {
                return Ok(None);
            }
        } else if self.terminal.load(Ordering::Acquire) != OPEN {
            return Ok(None);
        }

        let mut candidate = job.clone();
        if apply(&mut candidate, event).is_err() {
            return Ok(None);
        }
        persist(candidate.clone()).await?;
        *job = candidate.clone();
        self.terminal
            .store(terminal_code(candidate.status), Ordering::Release);
        self.cancel_requested
            .store(candidate.status == JobStatus::Cancelled, Ordering::Release);
        Ok(Some(candidate))
    }

    pub async fn replace(&self, replacement: Job) {
        let status = replacement.status;
        *self.job.lock().await = replacement;
        self.terminal
            .store(terminal_code(status), Ordering::Release);
        self.cancel_requested
            .store(status == JobStatus::Cancelled, Ordering::Release);
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Acquire)
    }

    fn synchronize_flags(&self, status: JobStatus) {
        let code = terminal_code(status);
        if code == OPEN {
            self.terminal.store(OPEN, Ordering::Release);
        } else {
            let _ = claim_once!(self.terminal, code);
        }
        if status == JobStatus::Cancelled {
            self.cancel_requested.store(true, Ordering::Release);
        }
    }
}

fn terminal_code(status: JobStatus) -> u8 {
    match status {
        JobStatus::Pending | JobStatus::Running => OPEN,
        JobStatus::Done => DONE,
        JobStatus::Error => ERROR,
        JobStatus::Cancelled => CANCELLED,
        JobStatus::Interrupted => INTERRUPTED,
    }
}

fn event_terminal_code(event: &JobEvent) -> Option<u8> {
    match event {
        JobEvent::Succeeded { .. } => Some(DONE),
        JobEvent::Failed { .. } => Some(ERROR),
        JobEvent::Cancelled | JobEvent::Discarded { .. } => Some(CANCELLED),
        JobEvent::Interrupted { .. } => Some(INTERRUPTED),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use loom::sync::atomic::{AtomicBool as LoomBool, AtomicU8 as LoomU8};
    use loom::thread;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn one_terminal_transition_wins_and_cancel_is_not_lost() {
        let cell = Arc::new(JobCell::new(Job::pending("job".into())));
        let cancel = cell.clone();
        let finish = cell.clone();
        let (cancelled, finished) = tokio::join!(
            async move { cancel.apply(&JobEvent::Cancelled).await.is_some() },
            async move {
                finish
                    .apply(&JobEvent::Succeeded { result: json!({}) })
                    .await
                    .is_some()
            }
        );
        assert_ne!(cancelled, finished);
        assert!(cell.snapshot().await.status.is_terminal());
        if cancelled {
            assert!(cell.cancel_requested());
        }
    }

    #[tokio::test]
    async fn failed_persistence_does_not_publish_the_transition_in_memory() {
        let cell = JobCell::new(Job::pending("job".into()));
        let result = cell
            .apply_durable(&JobEvent::Succeeded { result: json!({}) }, |_| async {
                anyhow::bail!("disk unavailable")
            })
            .await;
        assert!(result.is_err());
        assert_eq!(cell.snapshot().await.status, JobStatus::Pending);
    }

    #[test]
    fn loom_checks_terminal_and_permit_release_claims() {
        loom::model(|| {
            let terminal = Arc::new(LoomU8::new(OPEN));
            let left = terminal.clone();
            let right = terminal.clone();
            let a = thread::spawn(move || claim_once!(left, DONE).is_ok());
            let b = thread::spawn(move || claim_once!(right, CANCELLED).is_ok());
            assert_ne!(a.join().unwrap(), b.join().unwrap());
            assert_ne!(terminal.load(Ordering::Acquire), OPEN);
        });

        loom::model(|| {
            let released = Arc::new(LoomBool::new(false));
            let left = released.clone();
            let right = released.clone();
            let a = thread::spawn(move || release_once!(left).is_ok());
            let b = thread::spawn(move || release_once!(right).is_ok());
            assert_ne!(a.join().unwrap(), b.join().unwrap());
            assert!(released.load(Ordering::Acquire));
        });
    }
}
