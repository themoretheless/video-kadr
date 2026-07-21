//! Recovery boundary for persisted job snapshots with unknown status tokens.

use anyhow::{Context, Result};
use sqlx::Row;
use uuid::Uuid;

use crate::db::Db;
use crate::jobs::{replay, ErrorKind, JobEvent, RecordedJobEvent};
use crate::library::now_secs;
use crate::privacy::{redact_json_text, redact_text};

use super::{
    append_event, complete_outbox, erase_request_payload, finish_latest_attempt, persist_snapshot,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS job_status_quarantine (
    quarantine_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    original_status TEXT NOT NULL,
    result_json TEXT,
    error TEXT,
    stage TEXT,
    progress REAL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    quarantined_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_status_quarantine_job
    ON job_status_quarantine(job_id, quarantined_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_status_quarantine_expiry
    ON job_status_quarantine(quarantined_at);
CREATE TABLE IF NOT EXISTS job_request_quarantine (
    quarantine_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    request_created_at INTEGER NOT NULL,
    outbox_available_at INTEGER,
    outbox_lease_until INTEGER,
    outbox_delivery_count INTEGER,
    outbox_completed_at INTEGER,
    outbox_last_error TEXT
);
CREATE TABLE IF NOT EXISTS job_event_quarantine (
    quarantine_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_json TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    PRIMARY KEY(quarantine_id, sequence)
);
CREATE TABLE IF NOT EXISTS job_attempt_quarantine (
    quarantine_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL,
    error_kind TEXT,
    error TEXT,
    next_retry_at INTEGER,
    tool_version TEXT,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    PRIMARY KEY(quarantine_id, attempt)
);
";

const QUARANTINE_RETENTION_SECS: i64 = 30 * 24 * 60 * 60;

pub(super) async fn migrate(db: &Db) -> Result<()> {
    let now = now_secs() as i64;
    let mut tx = db.pool().begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query(SCHEMA).execute(&mut *tx).await?;
    purge_expired(&mut tx, now - QUARANTINE_RETENTION_SECS).await?;
    let invalid_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM jobs \
         WHERE status NOT IN \
           ('pending', 'running', 'done', 'error', 'cancelled', 'interrupted')",
    )
    .fetch_all(&mut *tx)
    .await?;

    for job_id in &invalid_ids {
        let quarantine_id = Uuid::new_v4().to_string();
        archive_snapshot(&mut tx, &quarantine_id, job_id, now).await?;

        if let Some(job) = replay_history(&mut tx, job_id).await? {
            persist_snapshot(&mut tx, &job, now).await?;
        } else {
            quarantine_unrecoverable_job(&mut tx, &quarantine_id, job_id, now).await?;
        }
        redact_archived_values(&mut tx, &quarantine_id).await?;
    }

    tx.commit().await?;
    if !invalid_ids.is_empty() {
        tracing::error!(
            jobs.count = invalid_ids.len(),
            "quarantined invalid persisted job statuses"
        );
    }
    Ok(())
}

pub(super) async fn purge(db: &Db) -> Result<u64> {
    let cutoff = now_secs() as i64 - QUARANTINE_RETENTION_SECS;
    let mut tx = db.pool().begin_with("BEGIN IMMEDIATE").await?;
    let purged = purge_expired(&mut tx, cutoff).await?;
    tx.commit().await?;
    Ok(purged)
}

async fn purge_expired(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, cutoff: i64) -> Result<u64> {
    sqlx::query(
        "DELETE FROM job_request_quarantine WHERE quarantine_id IN ( \
           SELECT quarantine_id FROM job_status_quarantine WHERE quarantined_at < ?)",
    )
    .bind(cutoff)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM job_event_quarantine WHERE quarantine_id IN ( \
           SELECT quarantine_id FROM job_status_quarantine WHERE quarantined_at < ?)",
    )
    .bind(cutoff)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM job_attempt_quarantine WHERE quarantine_id IN ( \
           SELECT quarantine_id FROM job_status_quarantine WHERE quarantined_at < ?)",
    )
    .bind(cutoff)
    .execute(&mut **tx)
    .await?;
    let result = sqlx::query("DELETE FROM job_status_quarantine WHERE quarantined_at < ?")
        .bind(cutoff)
        .execute(&mut **tx)
        .await?;
    Ok(result.rows_affected())
}

async fn redact_archived_values(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    quarantine_id: &str,
) -> Result<()> {
    if let Some(row) = sqlx::query(
        "SELECT original_status, result_json, error, stage FROM job_status_quarantine \
         WHERE quarantine_id = ?",
    )
    .bind(quarantine_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let original_status = redact_text(&row.try_get::<String, _>("original_status")?);
        let result_json = row
            .try_get::<Option<String>, _>("result_json")?
            .map(|value| redact_json_text(&value));
        let error = row
            .try_get::<Option<String>, _>("error")?
            .map(|value| redact_text(&value));
        let stage = row
            .try_get::<Option<String>, _>("stage")?
            .map(|value| redact_text(&value));
        sqlx::query(
            "UPDATE job_status_quarantine SET original_status = ?, result_json = ?, \
             error = ?, stage = ? WHERE quarantine_id = ?",
        )
        .bind(original_status)
        .bind(result_json)
        .bind(error)
        .bind(stage)
        .bind(quarantine_id)
        .execute(&mut **tx)
        .await?;
    }

    if let Some(row) = sqlx::query(
        "SELECT kind, payload_json, dedupe_key, outbox_last_error \
         FROM job_request_quarantine WHERE quarantine_id = ?",
    )
    .bind(quarantine_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let kind = redact_text(&row.try_get::<String, _>("kind")?);
        let payload_json = redact_json_text(&row.try_get::<String, _>("payload_json")?);
        let dedupe_key = redact_text(&row.try_get::<String, _>("dedupe_key")?);
        let last_error = row
            .try_get::<Option<String>, _>("outbox_last_error")?
            .map(|value| redact_text(&value));
        sqlx::query(
            "UPDATE job_request_quarantine SET kind = ?, payload_json = ?, dedupe_key = ?, \
             outbox_last_error = ? WHERE quarantine_id = ?",
        )
        .bind(kind)
        .bind(payload_json)
        .bind(dedupe_key)
        .bind(last_error)
        .bind(quarantine_id)
        .execute(&mut **tx)
        .await?;
    }

    let events = sqlx::query(
        "SELECT sequence, idempotency_key, event_type, event_json \
         FROM job_event_quarantine WHERE quarantine_id = ?",
    )
    .bind(quarantine_id)
    .fetch_all(&mut **tx)
    .await?;
    for row in events {
        sqlx::query(
            "UPDATE job_event_quarantine SET idempotency_key = ?, event_type = ?, \
             event_json = ? WHERE quarantine_id = ? AND sequence = ?",
        )
        .bind(redact_text(&row.try_get::<String, _>("idempotency_key")?))
        .bind(redact_text(&row.try_get::<String, _>("event_type")?))
        .bind(redact_json_text(&row.try_get::<String, _>("event_json")?))
        .bind(quarantine_id)
        .bind(row.try_get::<i64, _>("sequence")?)
        .execute(&mut **tx)
        .await?;
    }

    let attempts = sqlx::query(
        "SELECT attempt, status, error_kind, error, tool_version \
         FROM job_attempt_quarantine WHERE quarantine_id = ?",
    )
    .bind(quarantine_id)
    .fetch_all(&mut **tx)
    .await?;
    for row in attempts {
        let error_kind = row
            .try_get::<Option<String>, _>("error_kind")?
            .map(|value| redact_text(&value));
        let error = row
            .try_get::<Option<String>, _>("error")?
            .map(|value| redact_text(&value));
        let tool_version = row
            .try_get::<Option<String>, _>("tool_version")?
            .map(|value| redact_text(&value));
        sqlx::query(
            "UPDATE job_attempt_quarantine SET status = ?, error_kind = ?, error = ?, \
             tool_version = ? WHERE quarantine_id = ? AND attempt = ?",
        )
        .bind(redact_text(&row.try_get::<String, _>("status")?))
        .bind(error_kind)
        .bind(error)
        .bind(tool_version)
        .bind(quarantine_id)
        .bind(row.try_get::<i64, _>("attempt")?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn archive_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    quarantine_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO job_status_quarantine \
           (quarantine_id, job_id, original_status, result_json, error, stage, progress, \
            created_at, updated_at, quarantined_at) \
         SELECT ?, id, status, result_json, error, stage, progress, \
                created_at, updated_at, ? FROM jobs WHERE id = ?",
    )
    .bind(quarantine_id)
    .bind(now)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn replay_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: &str,
) -> Result<Option<crate::model::Job>> {
    let rows = sqlx::query(
        "SELECT sequence, idempotency_key, event_json, occurred_at \
         FROM job_events WHERE job_id = ? ORDER BY sequence",
    )
    .bind(job_id)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }

    let history = rows
        .into_iter()
        .map(|row| {
            Ok(RecordedJobEvent {
                job_id: job_id.to_owned(),
                sequence: u32::try_from(row.try_get::<i64, _>("sequence")?)
                    .context("event sequence overflow")?,
                idempotency_key: row.try_get("idempotency_key")?,
                event: serde_json::from_str(&row.try_get::<String, _>("event_json")?)?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>>>();

    match history {
        Ok(history) => match replay(job_id, &history) {
            Ok(job) => Ok(Some(job)),
            Err(error) => {
                tracing::warn!(job.id = job_id, %error, "quarantined job history cannot be replayed");
                Ok(None)
            }
        },
        Err(error) => {
            tracing::warn!(job.id = job_id, %error, "quarantined job history is unreadable");
            Ok(None)
        }
    }
}

async fn quarantine_unrecoverable_job(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    quarantine_id: &str,
    job_id: &str,
    now: i64,
) -> Result<()> {
    let reason = "invalid persisted job status was quarantined during startup";
    sqlx::query(
        "INSERT INTO job_request_quarantine \
           (quarantine_id, job_id, kind, payload_json, dedupe_key, request_created_at, \
            outbox_available_at, outbox_lease_until, outbox_delivery_count, \
            outbox_completed_at, outbox_last_error) \
         SELECT ?, r.job_id, r.kind, r.payload_json, r.dedupe_key, r.created_at, \
                o.available_at, o.lease_until, o.delivery_count, \
                o.completed_at, o.last_error \
         FROM job_requests r LEFT JOIN job_outbox o ON o.job_id = r.job_id \
         WHERE r.job_id = ?",
    )
    .bind(quarantine_id)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO job_event_quarantine \
           (quarantine_id, job_id, sequence, idempotency_key, event_type, \
            event_json, occurred_at) \
         SELECT ?, job_id, sequence, idempotency_key, event_type, event_json, occurred_at \
         FROM job_events WHERE job_id = ?",
    )
    .bind(quarantine_id)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM job_events WHERE job_id = ?")
        .bind(job_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO job_attempt_quarantine \
           (quarantine_id, job_id, attempt, status, error_kind, error, \
            next_retry_at, tool_version, started_at, finished_at) \
         SELECT ?, job_id, attempt, status, error_kind, error, \
                next_retry_at, tool_version, started_at, finished_at \
         FROM job_attempts WHERE job_id = ?",
    )
    .bind(quarantine_id)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    append_event(tx, job_id, &JobEvent::Created, "quarantine-created-v1", now).await?;
    append_event(
        tx,
        job_id,
        &JobEvent::Interrupted {
            reason: reason.into(),
        },
        "quarantine-interrupted-v1",
        now,
    )
    .await?;
    sqlx::query(
        "UPDATE jobs SET status = 'interrupted', result_json = NULL, error = ?, \
         stage = NULL, progress = NULL, updated_at = ? WHERE id = ?",
    )
    .bind(reason)
    .bind(now)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    finish_latest_attempt(
        tx,
        job_id,
        "interrupted",
        Some(ErrorKind::Internal),
        Some(reason),
        None,
        now,
    )
    .await?;
    complete_outbox(tx, job_id, Some(reason), now).await?;
    erase_request_payload(tx, job_id).await?;
    Ok(())
}
