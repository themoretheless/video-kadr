//! Ownership boundary for application background tasks and shutdown.

use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Clone)]
pub struct TaskSupervisor {
    root: CancellationToken,
    tracker: TaskTracker,
}

impl Default for TaskSupervisor {
    fn default() -> Self {
        Self {
            root: CancellationToken::new(),
            tracker: TaskTracker::new(),
        }
    }
}

impl TaskSupervisor {
    pub fn child_token(&self) -> CancellationToken {
        self.root.child_token()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.tracker.spawn(future)
    }

    pub fn begin_shutdown(&self) {
        self.root.cancel();
        self.tracker.close();
    }

    pub fn is_shutting_down(&self) -> bool {
        self.root.is_cancelled()
    }

    pub async fn wait(&self, limit: Duration) -> bool {
        tokio::time::timeout(limit, self.tracker.wait())
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Notify;

    use super::*;

    #[tokio::test]
    async fn shutdown_cancels_children_and_waits_for_tracked_tasks() {
        let supervisor = TaskSupervisor::default();
        let child = supervisor.child_token();
        let finished = Arc::new(Notify::new());
        let task_finished = finished.clone();

        supervisor.spawn(async move {
            child.cancelled().await;
            task_finished.notify_one();
        });

        supervisor.begin_shutdown();
        assert!(supervisor.is_shutting_down());
        finished.notified().await;
        assert!(supervisor.wait(Duration::from_secs(1)).await);
    }

    #[tokio::test]
    async fn wait_is_bounded_when_a_task_ignores_cancellation() {
        let supervisor = TaskSupervisor::default();
        supervisor.spawn(std::future::pending::<()>());
        supervisor.begin_shutdown();
        assert!(!supervisor.wait(Duration::from_millis(10)).await);
    }
}
