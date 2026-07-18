//! Bounded executor for CPU-heavy work.
//!
//! Tokio owns orchestration and I/O. Hashing, waveform generation, thumbnails,
//! and similar closures enter this dedicated Rayon pool through one queue gate.

use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use rayon::ThreadPool;
use tokio::sync::{oneshot, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuPoolConfig {
    pub threads: usize,
    /// Work waiting behind active workers. Total admitted work is
    /// `threads + queue_capacity`.
    pub queue_capacity: usize,
}

impl CpuPoolConfig {
    pub fn validate(self) -> Result<Self, CpuPoolBuildError> {
        if self.threads == 0 {
            return Err(CpuPoolBuildError::ZeroThreads);
        }
        self.threads
            .checked_add(self.queue_capacity)
            .ok_or(CpuPoolBuildError::CapacityOverflow)?;
        Ok(self)
    }
}

#[derive(Clone)]
pub struct CpuPool {
    inner: Arc<CpuPoolInner>,
}

struct CpuPoolInner {
    pool: ThreadPool,
    admission: Arc<Semaphore>,
    metrics: CpuPoolMetrics,
}

impl CpuPool {
    pub fn new(config: CpuPoolConfig) -> Result<Self, CpuPoolBuildError> {
        let config = config.validate()?;
        let admitted = config.threads + config.queue_capacity;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(config.threads)
            .thread_name(|index| format!("video-cpu-{index}"))
            .panic_handler(|payload| {
                tracing::error!(?payload, "unhandled Rayon worker panic");
            })
            .build()
            .map_err(CpuPoolBuildError::Rayon)?;
        Ok(Self {
            inner: Arc::new(CpuPoolInner {
                pool,
                admission: Arc::new(Semaphore::new(admitted)),
                metrics: CpuPoolMetrics::default(),
            }),
        })
    }

    /// Fail fast when the bounded queue is full. Cancellation is cooperative:
    /// callers may return immediately while an already-running closure observes
    /// the supplied token and releases its permit when it exits.
    pub async fn execute<T, F>(
        &self,
        cancellation: CancellationToken,
        work: F,
    ) -> Result<T, CpuTaskError>
    where
        T: Send + 'static,
        F: FnOnce(&CancellationToken) -> anyhow::Result<T> + Send + 'static,
    {
        if cancellation.is_cancelled() {
            self.inner.metrics.cancelled.fetch_add(1, Ordering::Relaxed);
            return Err(CpuTaskError::Cancelled);
        }
        let permit = match self.inner.admission.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                self.inner.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(CpuTaskError::Saturated);
            }
        };

        self.inner.metrics.submitted.fetch_add(1, Ordering::Relaxed);
        self.inner.metrics.queued.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        let inner = self.inner.clone();
        let worker_cancellation = cancellation.clone();
        self.inner.pool.spawn_fifo(move || {
            inner.metrics.queued.fetch_sub(1, Ordering::Relaxed);
            inner.metrics.running.fetch_add(1, Ordering::Relaxed);
            let result = if worker_cancellation.is_cancelled() {
                Err(CpuTaskError::Cancelled)
            } else {
                let outcome = catch_unwind(AssertUnwindSafe(|| work(&worker_cancellation)));
                if worker_cancellation.is_cancelled() {
                    Err(CpuTaskError::Cancelled)
                } else {
                    match outcome {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(error)) => Err(CpuTaskError::Work(error)),
                        Err(_) => Err(CpuTaskError::Panicked),
                    }
                }
            };
            inner.metrics.running.fetch_sub(1, Ordering::Relaxed);
            match &result {
                Ok(_) => {
                    inner.metrics.completed.fetch_add(1, Ordering::Relaxed);
                }
                Err(CpuTaskError::Cancelled) => {
                    inner.metrics.cancelled.fetch_add(1, Ordering::Relaxed);
                }
                Err(CpuTaskError::Panicked) => {
                    inner.metrics.panicked.fetch_add(1, Ordering::Relaxed);
                }
                Err(CpuTaskError::Work(_)) | Err(CpuTaskError::Saturated) => {
                    inner.metrics.failed.fetch_add(1, Ordering::Relaxed);
                }
                Err(CpuTaskError::WorkerStopped) => {}
            }
            drop(permit);
            let _ = sender.send(result);
        });

        tokio::select! {
            result = receiver => result.unwrap_or(Err(CpuTaskError::WorkerStopped)),
            _ = cancellation.cancelled() => Err(CpuTaskError::Cancelled),
        }
    }

    pub fn snapshot(&self) -> CpuPoolSnapshot {
        self.inner.metrics.snapshot()
    }
}

#[derive(Default)]
struct CpuPoolMetrics {
    submitted: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    cancelled: AtomicU64,
    rejected: AtomicU64,
    panicked: AtomicU64,
    queued: AtomicUsize,
    running: AtomicUsize,
}

impl CpuPoolMetrics {
    fn snapshot(&self) -> CpuPoolSnapshot {
        CpuPoolSnapshot {
            submitted: self.submitted.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            cancelled: self.cancelled.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            panicked: self.panicked.load(Ordering::Relaxed),
            queued: self.queued.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuPoolSnapshot {
    pub submitted: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub rejected: u64,
    pub panicked: u64,
    pub queued: usize,
    pub running: usize,
}

#[derive(Debug)]
pub enum CpuTaskError {
    Saturated,
    Cancelled,
    Panicked,
    WorkerStopped,
    Work(anyhow::Error),
}

impl fmt::Display for CpuTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Saturated => formatter.write_str("CPU work queue is saturated"),
            Self::Cancelled => formatter.write_str("CPU task was cancelled"),
            Self::Panicked => formatter.write_str("CPU task panicked"),
            Self::WorkerStopped => formatter.write_str("CPU worker stopped before replying"),
            Self::Work(error) => write!(formatter, "CPU task failed: {error}"),
        }
    }
}

impl std::error::Error for CpuTaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Work(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum CpuPoolBuildError {
    ZeroThreads,
    CapacityOverflow,
    Rayon(rayon::ThreadPoolBuildError),
}

impl fmt::Display for CpuPoolBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid CPU pool configuration: {self:?}")
    }
}

impl std::error::Error for CpuPoolBuildError {}

#[cfg(test)]
mod tests {
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closure_runs_on_named_cpu_worker() {
        let pool = CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 0,
        })
        .unwrap();
        let name = pool
            .execute(CancellationToken::new(), |_| {
                Ok(std::thread::current().name().unwrap_or_default().to_owned())
            })
            .await
            .unwrap();
        assert!(name.starts_with("video-cpu-"), "{name}");
        assert_eq!(pool.snapshot().completed, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queue_is_bounded_and_reports_saturation() {
        let pool = CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 1,
        })
        .unwrap();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let first_gate = gate.clone();
        let first_pool = pool.clone();
        let first = tokio::spawn(async move {
            first_pool
                .execute(CancellationToken::new(), move |_| {
                    let (lock, ready) = &*first_gate;
                    let mut released = lock.lock().unwrap();
                    while !*released {
                        released = ready.wait(released).unwrap();
                    }
                    Ok(1)
                })
                .await
        });
        while pool.snapshot().running != 1 {
            tokio::task::yield_now().await;
        }

        let second_pool = pool.clone();
        let second = tokio::spawn(async move {
            second_pool
                .execute(CancellationToken::new(), |_| Ok(2))
                .await
        });
        while pool.snapshot().queued != 1 {
            tokio::task::yield_now().await;
        }
        assert!(matches!(
            pool.execute(CancellationToken::new(), |_| Ok(3)).await,
            Err(CpuTaskError::Saturated)
        ));

        let (lock, ready) = &*gate;
        *lock.lock().unwrap() = true;
        ready.notify_all();
        assert_eq!(first.await.unwrap().unwrap(), 1);
        assert_eq!(second.await.unwrap().unwrap(), 2);
        assert_eq!(pool.snapshot().rejected, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_is_visible_inside_running_work() {
        let pool = CpuPool::new(CpuPoolConfig {
            threads: 1,
            queue_capacity: 0,
        })
        .unwrap();
        let token = CancellationToken::new();
        let cancel = token.clone();
        let task_pool = pool.clone();
        let task = tokio::spawn(async move {
            task_pool
                .execute(token, |token| {
                    while !token.is_cancelled() {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Ok(())
                })
                .await
        });
        while pool.snapshot().running != 1 {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
        assert!(matches!(task.await.unwrap(), Err(CpuTaskError::Cancelled)));
        tokio::time::timeout(Duration::from_secs(1), async {
            while pool.snapshot().running != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
