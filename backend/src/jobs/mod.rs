//! Durable background-job domain and SQLite adapter.

pub mod attempt;
pub mod dedupe;
pub mod event_log;
pub mod failed_registry;
pub mod job_cell;
mod maintenance;
pub mod outbox;
pub mod persistence_profile;
pub mod registry;
mod store;

pub use attempt::{ErrorKind, JobAttempt, RetryPolicy};
pub use dedupe::{dedupe_key, QueueLimits};
pub use event_log::{replay, JobEvent, RecordedJobEvent};
pub use failed_registry::{FailedJob, OperatorAction};
pub use job_cell::{JobCell, JobPermit};
pub use maintenance::run_quarantine_cleanup;
pub use outbox::{JobEnvelope, JobKind};
pub use registry::{JobLifecycle, LifecycleCounts, ReconciliationReport};
pub use store::{ActiveJobRequest, EnqueueOutcome, SqliteJobStore};
