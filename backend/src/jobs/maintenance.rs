//! Periodic retention enforcement for durable job support data.

use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::SqliteJobStore;

const QUARANTINE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub async fn run_quarantine_cleanup(store: SqliteJobStore, shutdown: CancellationToken) {
    let first_run = Instant::now() + QUARANTINE_CLEANUP_INTERVAL;
    let mut interval = tokio::time::interval_at(first_run, QUARANTINE_CLEANUP_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {
                match store.purge_expired_quarantine().await {
                    Ok(0) => {}
                    Ok(count) => tracing::info!(jobs.count = count, "purged expired job quarantine"),
                    Err(error) => tracing::warn!(%error, "purge expired job quarantine"),
                }
            }
        }
    }
}
