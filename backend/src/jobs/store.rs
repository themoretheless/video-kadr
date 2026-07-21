use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

use crate::db::Db;
use crate::library::now_secs;
use crate::model::{Job, JobStatus};

use super::attempt::{ErrorKind, JobAttempt, RetryPolicy};
use super::event_log::{apply, JobEvent, RecordedJobEvent};
use super::failed_registry::{FailedJob, OperatorAction};
use super::outbox::{JobEnvelope, JobKind};
use super::registry::{JobLifecycle, LifecycleCounts, ReconciliationReport};
use super::QueueLimits;

mod quarantine;

const JOB_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS job_requests (
    job_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_requests_kind_created
    ON job_requests(kind, created_at DESC);
CREATE TABLE IF NOT EXISTS job_dedupe (
    dedupe_key TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_dedupe_expires ON job_dedupe(expires_at);
CREATE TABLE IF NOT EXISTS job_events (
    job_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    idempotency_key TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_json TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    PRIMARY KEY(job_id, sequence),
    UNIQUE(job_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_job_events_occurred ON job_events(occurred_at);
CREATE TABLE IF NOT EXISTS job_attempts (
    job_id TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL,
    error_kind TEXT,
    error TEXT,
    next_retry_at INTEGER,
    tool_version TEXT,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    PRIMARY KEY(job_id, attempt)
);
CREATE INDEX IF NOT EXISTS idx_job_attempts_status ON job_attempts(status, finished_at);
CREATE TABLE IF NOT EXISTS job_outbox (
    job_id TEXT PRIMARY KEY,
    available_at INTEGER NOT NULL,
    lease_until INTEGER,
    delivery_count INTEGER NOT NULL DEFAULT 0,
    completed_at INTEGER,
    last_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_job_outbox_dispatch
    ON job_outbox(completed_at, available_at, lease_until);
CREATE TABLE IF NOT EXISTS job_operator_actions (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    action TEXT NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_job_operator_actions_job
    ON job_operator_actions(job_id, created_at DESC);
";

#[derive(Debug, Clone, PartialEq)]
pub enum EnqueueOutcome {
    Created(Job),
    Existing(String),
    RateLimited,
}

#[derive(Clone)]
pub struct SqliteJobStore {
    db: Db,
}

impl SqliteJobStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::query(JOB_SCHEMA).execute(self.db.pool()).await?;
        quarantine::migrate(&self.db).await?;
        self.backfill_legacy_events().await?;
        Ok(())
    }

    pub async fn purge_expired_quarantine(&self) -> Result<u64> {
        quarantine::purge(&self.db).await
    }

    async fn backfill_legacy_events(&self) -> Result<()> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let rows = sqlx::query(
            "SELECT j.id, j.status, j.result_json, j.error, j.stage, j.progress \
             FROM jobs j WHERE NOT EXISTS \
               (SELECT 1 FROM job_events e WHERE e.job_id = j.id)",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in rows {
            let job = row_to_job(&row)?;
            append_event(
                &mut tx,
                &job.id,
                &JobEvent::Created,
                "migration-created-v1",
                now,
            )
            .await?;
            let snapshot_event = match job.status {
                JobStatus::Pending if job.stage.as_deref() == Some("queued") => {
                    Some(JobEvent::Queued)
                }
                JobStatus::Pending => None,
                JobStatus::Running => Some(JobEvent::Started {
                    stage: job.stage.clone().unwrap_or_else(|| "processing".into()),
                    attempt: 0,
                }),
                JobStatus::Done => Some(JobEvent::Succeeded {
                    result: job.result.clone().unwrap_or(Value::Null),
                }),
                JobStatus::Error => Some(JobEvent::Failed {
                    kind: ErrorKind::Internal,
                    message: job.error.clone().unwrap_or_else(|| "legacy failure".into()),
                }),
                JobStatus::Cancelled => Some(JobEvent::Cancelled),
                JobStatus::Interrupted => Some(JobEvent::Interrupted {
                    reason: job
                        .error
                        .clone()
                        .unwrap_or_else(|| "legacy interruption".into()),
                }),
            };
            if let Some(event) = snapshot_event {
                append_event(&mut tx, &job.id, &event, "migration-snapshot-v1", now).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn enqueue(
        &self,
        job_id: String,
        kind: JobKind,
        payload: &Value,
        dedupe_key: &str,
        limits: QueueLimits,
    ) -> Result<EnqueueOutcome> {
        self.enqueue_at(job_id, kind, payload, dedupe_key, limits, now_secs() as i64)
            .await
    }

    async fn enqueue_at(
        &self,
        job_id: String,
        kind: JobKind,
        payload: &Value,
        dedupe_key: &str,
        limits: QueueLimits,
        now: i64,
    ) -> Result<EnqueueOutcome> {
        self.enqueue_at_with_fault(job_id, kind, payload, dedupe_key, limits, now, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn enqueue_at_with_fault(
        &self,
        job_id: String,
        kind: JobKind,
        payload: &Value,
        dedupe_key: &str,
        limits: QueueLimits,
        now: i64,
        fail_after: Option<u8>,
    ) -> Result<EnqueueOutcome> {
        let mut tx = self.db.pool().begin().await?;
        sqlx::query("DELETE FROM job_dedupe WHERE expires_at <= ?")
            .bind(now)
            .execute(&mut *tx)
            .await?;

        if let Some(existing) = sqlx::query_scalar::<_, String>(
            "SELECT job_id FROM job_dedupe WHERE dedupe_key = ? AND expires_at > ?",
        )
        .bind(dedupe_key)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(EnqueueOutcome::Existing(existing));
        }

        let window_start = now.saturating_sub(duration_secs(limits.rate_window));
        let recent: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_requests WHERE kind = ? AND created_at >= ?",
        )
        .bind(kind.as_str())
        .bind(window_start)
        .fetch_one(&mut *tx)
        .await?;
        if recent >= i64::from(limits.max_new_jobs) {
            tx.commit().await?;
            return Ok(EnqueueOutcome::RateLimited);
        }

        let job = Job::pending(job_id.clone());
        persist_snapshot(&mut tx, &job, now).await?;
        inject_enqueue_fault(fail_after, 1)?;
        sqlx::query(
            "INSERT INTO job_requests (job_id, kind, payload_json, dedupe_key, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&job_id)
        .bind(kind.as_str())
        .bind(serde_json::to_string(payload)?)
        .bind(dedupe_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        inject_enqueue_fault(fail_after, 2)?;
        append_event(
            &mut tx,
            &job_id,
            &JobEvent::Created,
            &format!("enqueue:{dedupe_key}"),
            now,
        )
        .await?;
        inject_enqueue_fault(fail_after, 3)?;
        sqlx::query(
            "INSERT INTO job_outbox \
             (job_id, available_at, lease_until, delivery_count, completed_at, last_error) \
             VALUES (?, ?, NULL, 0, NULL, NULL)",
        )
        .bind(&job_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        inject_enqueue_fault(fail_after, 4)?;

        let dedupe_insert = sqlx::query(
            "INSERT INTO job_dedupe (dedupe_key, job_id, expires_at) VALUES (?, ?, ?) \
             ON CONFLICT(dedupe_key) DO NOTHING",
        )
        .bind(dedupe_key)
        .bind(&job_id)
        .bind(now.saturating_add(duration_secs(limits.dedupe_ttl)))
        .execute(&mut *tx)
        .await?;
        if dedupe_insert.rows_affected() == 0 {
            tx.rollback().await?;
            let existing = sqlx::query_scalar::<_, String>(
                "SELECT job_id FROM job_dedupe WHERE dedupe_key = ?",
            )
            .bind(dedupe_key)
            .fetch_one(self.db.pool())
            .await?;
            return Ok(EnqueueOutcome::Existing(existing));
        }
        inject_enqueue_fault(fail_after, 5)?;

        tx.commit().await?;
        Ok(EnqueueOutcome::Created(job))
    }

    pub async fn claim(&self, job_id: &str, lease_seconds: i64) -> Result<Option<JobEnvelope>> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let claimed = sqlx::query(
            "UPDATE job_outbox SET lease_until = ?, delivery_count = delivery_count + 1 \
             WHERE job_id = ? AND completed_at IS NULL AND available_at <= ? \
               AND (lease_until IS NULL OR lease_until <= ?) \
               AND EXISTS (SELECT 1 FROM jobs j \
                           WHERE j.id = job_outbox.job_id AND j.status = 'pending') \
               AND EXISTS (SELECT 1 FROM job_requests r \
                           WHERE r.job_id = job_outbox.job_id AND r.payload_json <> 'null') \
             RETURNING delivery_count",
        )
        .bind(now.saturating_add(lease_seconds.max(1)))
        .bind(job_id)
        .bind(now)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = claimed else {
            tx.commit().await?;
            return Ok(None);
        };
        let attempt = u32::try_from(row.try_get::<i64, _>("delivery_count")?)
            .context("outbox delivery count overflow")?;
        let request = sqlx::query("SELECT kind, payload_json FROM job_requests WHERE job_id = ?")
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO job_attempts \
             (job_id, attempt, status, error_kind, error, next_retry_at, tool_version, started_at, finished_at) \
             VALUES (?, ?, 'claimed', NULL, NULL, NULL, NULL, ?, NULL) \
             ON CONFLICT(job_id, attempt) DO UPDATE SET \
               status = 'claimed', error_kind = NULL, error = NULL, next_retry_at = NULL, \
               started_at = excluded.started_at, finished_at = NULL",
        )
        .bind(job_id)
        .bind(i64::from(attempt))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(Some(JobEnvelope {
            job_id: job_id.to_string(),
            kind: JobKind::from_token(&request.try_get::<String, _>("kind")?)?,
            payload: serde_json::from_str(&request.try_get::<String, _>("payload_json")?)?,
            attempt,
        }))
    }

    pub async fn renew_lease(
        &self,
        job_id: &str,
        attempt: u32,
        lease_seconds: i64,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        let result = sqlx::query(
            "UPDATE job_outbox SET lease_until = ? \
             WHERE job_id = ? AND delivery_count = ? AND completed_at IS NULL",
        )
        .bind(now.saturating_add(lease_seconds.max(1)))
        .bind(job_id)
        .bind(i64::from(attempt))
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn deliverable_ids(&self) -> Result<Vec<String>> {
        let now = now_secs() as i64;
        let rows = sqlx::query(
            "SELECT o.job_id FROM job_outbox o \
             JOIN jobs j ON j.id = o.job_id \
             JOIN job_requests r ON r.job_id = o.job_id AND r.payload_json <> 'null' \
             WHERE j.status = 'pending' AND o.completed_at IS NULL AND o.available_at <= ? \
               AND (o.lease_until IS NULL OR o.lease_until <= ?) \
             ORDER BY o.available_at, o.job_id",
        )
        .bind(now)
        .bind(now)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| row.try_get("job_id").map_err(Into::into))
            .collect()
    }

    pub async fn record_transition(
        &self,
        snapshot: &Job,
        event: &JobEvent,
        idempotency_key: &str,
        tool_version: Option<&str>,
    ) -> Result<bool> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        if !append_event(&mut tx, &snapshot.id, event, idempotency_key, now).await? {
            tx.commit().await?;
            return Ok(false);
        }
        persist_snapshot(&mut tx, snapshot, now).await?;

        match event {
            JobEvent::Started { attempt, .. } => {
                sqlx::query(
                    "UPDATE job_attempts SET status = 'running', tool_version = ? \
                     WHERE job_id = ? AND attempt = ?",
                )
                .bind(tool_version)
                .bind(&snapshot.id)
                .bind(i64::from(*attempt))
                .execute(&mut *tx)
                .await?;
            }
            JobEvent::Succeeded { .. } => {
                finish_latest_attempt(&mut tx, &snapshot.id, "succeeded", None, None, None, now)
                    .await?;
                complete_outbox(&mut tx, &snapshot.id, None, now).await?;
                erase_request_payload(&mut tx, &snapshot.id).await?;
            }
            JobEvent::Failed { kind, message } => {
                let attempt = latest_attempt_number(&mut tx, &snapshot.id).await?;
                let next_retry_at = RetryPolicy::default()
                    .next_delay(attempt, *kind)
                    .map(|delay| now.saturating_add(duration_secs(delay)));
                finish_latest_attempt(
                    &mut tx,
                    &snapshot.id,
                    "failed",
                    Some(*kind),
                    Some(message),
                    next_retry_at,
                    now,
                )
                .await?;
                complete_outbox(&mut tx, &snapshot.id, Some(message), now).await?;
                if next_retry_at.is_none() {
                    erase_request_payload(&mut tx, &snapshot.id).await?;
                }
            }
            JobEvent::Cancelled => {
                finish_latest_attempt(&mut tx, &snapshot.id, "cancelled", None, None, None, now)
                    .await?;
                complete_outbox(&mut tx, &snapshot.id, None, now).await?;
                erase_request_payload(&mut tx, &snapshot.id).await?;
            }
            JobEvent::Discarded { .. } => {
                finish_latest_attempt(&mut tx, &snapshot.id, "discarded", None, None, None, now)
                    .await?;
                complete_outbox(&mut tx, &snapshot.id, None, now).await?;
                erase_request_payload(&mut tx, &snapshot.id).await?;
            }
            JobEvent::Interrupted { reason } => {
                finish_latest_attempt(
                    &mut tx,
                    &snapshot.id,
                    "interrupted",
                    Some(ErrorKind::Interrupted),
                    Some(reason),
                    None,
                    now,
                )
                .await?;
            }
            JobEvent::RetryScheduled { available_at, .. } => {
                sqlx::query(
                    "UPDATE job_outbox SET available_at = ?, lease_until = NULL, \
                     completed_at = NULL, last_error = NULL WHERE job_id = ?",
                )
                .bind(*available_at)
                .bind(&snapshot.id)
                .execute(&mut *tx)
                .await?;
            }
            JobEvent::Created | JobEvent::Queued => {}
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn event_history(&self, job_id: &str) -> Result<Vec<RecordedJobEvent>> {
        let rows = sqlx::query(
            "SELECT sequence, idempotency_key, event_json, occurred_at \
             FROM job_events WHERE job_id = ? ORDER BY sequence",
        )
        .bind(job_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(RecordedJobEvent {
                    job_id: job_id.to_string(),
                    sequence: u32::try_from(row.try_get::<i64, _>("sequence")?)
                        .context("event sequence overflow")?,
                    idempotency_key: row.try_get("idempotency_key")?,
                    event: serde_json::from_str(&row.try_get::<String, _>("event_json")?)?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect()
    }

    pub async fn list_failed(&self) -> Result<Vec<FailedJob>> {
        let rows = sqlx::query(
            "SELECT j.id, a.attempt, a.error_kind, COALESCE(a.error, j.error, 'unknown error') AS reason, \
                    a.next_retry_at, a.tool_version \
             FROM jobs j JOIN job_attempts a ON a.job_id = j.id \
             WHERE j.status IN ('error', 'interrupted') \
               AND a.status IN ('failed', 'interrupted') \
               AND a.attempt = (SELECT MAX(a2.attempt) FROM job_attempts a2 WHERE a2.job_id = j.id) \
             ORDER BY a.finished_at DESC, j.id",
        )
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                let attempt =
                    u32::try_from(row.try_get::<i64, _>("attempt")?).context("attempt overflow")?;
                let kind = row
                    .try_get::<Option<String>, _>("error_kind")?
                    .map(|token| ErrorKind::from_token(&token))
                    .unwrap_or(ErrorKind::Internal);
                Ok(FailedJob {
                    job_id: row.try_get("id")?,
                    attempt,
                    error_kind: kind,
                    reason: row.try_get("reason")?,
                    next_retry_at: row.try_get("next_retry_at")?,
                    tool_version: row.try_get("tool_version")?,
                })
            })
            .collect()
    }

    pub async fn retry_failed(&self, job_id: &str, actor: &str) -> Result<Job> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let row = sqlx::query(
            "SELECT j.id, j.status, j.result_json, j.error, j.stage, j.progress, \
                    a.attempt, a.error_kind \
             FROM jobs j JOIN job_attempts a ON a.job_id = j.id \
             JOIN job_requests r ON r.job_id = j.id AND r.payload_json <> 'null' \
             WHERE j.id = ? AND j.status IN ('error', 'interrupted') \
             ORDER BY a.attempt DESC LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow!("failed job not found"))?;
        let attempts =
            u32::try_from(row.try_get::<i64, _>("attempt")?).context("attempt overflow")?;
        let kind = row
            .try_get::<Option<String>, _>("error_kind")?
            .map(|token| ErrorKind::from_token(&token))
            .unwrap_or(ErrorKind::Internal);
        if RetryPolicy::default().next_delay(attempts, kind).is_none() {
            return Err(anyhow!(
                "retry policy rejected {kind:?} after {attempts} attempts"
            ));
        }

        let mut job = row_to_job(&row)?;
        let event = JobEvent::RetryScheduled {
            attempt: attempts.saturating_add(1),
            available_at: now,
        };
        apply(&mut job, &event)?;
        append_event(
            &mut tx,
            job_id,
            &event,
            &format!("operator-retry:{}", Uuid::new_v4()),
            now,
        )
        .await?;
        persist_snapshot(&mut tx, &job, now).await?;
        sqlx::query(
            "UPDATE job_outbox SET available_at = ?, lease_until = NULL, completed_at = NULL, \
             last_error = NULL WHERE job_id = ?",
        )
        .bind(now)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        insert_operator_action(&mut tx, job_id, OperatorAction::Retry, actor, None, now).await?;
        tx.commit().await?;
        Ok(job)
    }

    pub async fn schedule_retry(&self, job_id: &str) -> Result<Option<(Job, std::time::Duration)>> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let Some(row) = sqlx::query(
            "SELECT j.id, j.status, j.result_json, j.error, j.stage, j.progress, \
                    a.attempt, a.error_kind \
             FROM jobs j JOIN job_attempts a ON a.job_id = j.id \
             JOIN job_requests r ON r.job_id = j.id AND r.payload_json <> 'null' \
             WHERE j.id = ? AND j.status IN ('error', 'interrupted') \
             ORDER BY a.attempt DESC LIMIT 1",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.commit().await?;
            return Ok(None);
        };
        let attempts =
            u32::try_from(row.try_get::<i64, _>("attempt")?).context("attempt overflow")?;
        let kind = row
            .try_get::<Option<String>, _>("error_kind")?
            .map(|token| ErrorKind::from_token(&token))
            .unwrap_or(ErrorKind::Internal);
        let Some(delay) = RetryPolicy::default().next_delay(attempts, kind) else {
            tx.commit().await?;
            return Ok(None);
        };
        let available_at = now.saturating_add(duration_secs(delay));
        let mut job = row_to_job(&row)?;
        let event = JobEvent::RetryScheduled {
            attempt: attempts.saturating_add(1),
            available_at,
        };
        apply(&mut job, &event)?;
        append_event(
            &mut tx,
            job_id,
            &event,
            &format!("retry-policy:{}", Uuid::new_v4()),
            now,
        )
        .await?;
        persist_snapshot(&mut tx, &job, now).await?;
        sqlx::query(
            "UPDATE job_outbox SET available_at = ?, lease_until = NULL, completed_at = NULL, \
             last_error = NULL \
             WHERE job_id = ?",
        )
        .bind(available_at)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some((job, delay)))
    }

    /// Recover the crash window after a failed attempt was committed but before
    /// its retry event was scheduled. Only attempts whose persisted deadline is
    /// due are moved back to pending.
    pub async fn schedule_due_retries(&self) -> Result<Vec<Job>> {
        self.schedule_due_retries_at(now_secs() as i64).await
    }

    async fn schedule_due_retries_at(&self, now: i64) -> Result<Vec<Job>> {
        let mut tx = self.db.pool().begin().await?;
        let rows = sqlx::query(
            "SELECT j.id, j.status, j.result_json, j.error, j.stage, j.progress, \
                    a.attempt, a.error_kind \
             FROM jobs j JOIN job_attempts a ON a.job_id = j.id \
             JOIN job_requests r ON r.job_id = j.id AND r.payload_json <> 'null' \
             JOIN job_outbox o ON o.job_id = j.id \
             WHERE j.status IN ('error', 'interrupted') \
               AND a.status IN ('failed', 'interrupted') \
               AND a.next_retry_at IS NOT NULL AND a.next_retry_at <= ? \
               AND o.completed_at IS NOT NULL \
               AND a.attempt = (SELECT MAX(a2.attempt) FROM job_attempts a2 WHERE a2.job_id = j.id) \
             ORDER BY a.next_retry_at, j.id",
        )
        .bind(now)
        .fetch_all(&mut *tx)
        .await?;
        let mut scheduled = Vec::with_capacity(rows.len());
        for row in rows {
            let attempts =
                u32::try_from(row.try_get::<i64, _>("attempt")?).context("attempt overflow")?;
            let kind = row
                .try_get::<Option<String>, _>("error_kind")?
                .map(|token| ErrorKind::from_token(&token))
                .unwrap_or(ErrorKind::Internal);
            if RetryPolicy::default().next_delay(attempts, kind).is_none() {
                continue;
            }
            let mut job = row_to_job(&row)?;
            let event = JobEvent::RetryScheduled {
                attempt: attempts.saturating_add(1),
                available_at: now,
            };
            apply(&mut job, &event)?;
            append_event(
                &mut tx,
                &job.id,
                &event,
                &format!("retry-recovery:{attempts}"),
                now,
            )
            .await?;
            persist_snapshot(&mut tx, &job, now).await?;
            sqlx::query(
                "UPDATE job_outbox SET available_at = ?, lease_until = NULL, \
                 completed_at = NULL, last_error = NULL WHERE job_id = ?",
            )
            .bind(now)
            .bind(&job.id)
            .execute(&mut *tx)
            .await?;
            scheduled.push(job);
        }
        tx.commit().await?;
        Ok(scheduled)
    }

    pub async fn discard_failed(
        &self,
        job_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Option<Job>> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let Some(row) = sqlx::query(
            "SELECT id, status, result_json, error, stage, progress FROM jobs \
             WHERE id = ? AND status IN ('error', 'interrupted')",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.commit().await?;
            return Ok(None);
        };
        let mut job = row_to_job(&row)?;
        let event = JobEvent::Discarded {
            reason: reason.map(str::to_owned),
        };
        apply(&mut job, &event)?;
        append_event(
            &mut tx,
            job_id,
            &event,
            &format!("operator-discard:{}", Uuid::new_v4()),
            now,
        )
        .await?;
        persist_snapshot(&mut tx, &job, now).await?;
        sqlx::query("UPDATE job_outbox SET completed_at = ?, lease_until = NULL WHERE job_id = ?")
            .bind(now)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE job_attempts SET status = 'discarded' WHERE job_id = ? \
             AND attempt = (SELECT MAX(a.attempt) FROM job_attempts a WHERE a.job_id = ?)",
        )
        .bind(job_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        erase_request_payload(&mut tx, job_id).await?;
        insert_operator_action(&mut tx, job_id, OperatorAction::Discard, actor, reason, now)
            .await?;
        tx.commit().await?;
        Ok(Some(job))
    }

    pub async fn operator_action_count(&self, job_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM job_operator_actions WHERE job_id = ?")
                .bind(job_id)
                .fetch_one(self.db.pool())
                .await?,
        )
    }

    pub async fn lifecycle_counts(&self) -> Result<LifecycleCounts> {
        let now = now_secs() as i64;
        let rows = sqlx::query(
            "SELECT j.status, o.available_at, o.completed_at \
             FROM jobs j LEFT JOIN job_outbox o ON o.job_id = j.id",
        )
        .fetch_all(self.db.pool())
        .await?;
        let mut counts = LifecycleCounts::default();
        for row in rows {
            let status: String = row.try_get("status")?;
            let available_at: Option<i64> = row.try_get("available_at")?;
            let completed_at: Option<i64> = row.try_get("completed_at")?;
            let lifecycle = match status.as_str() {
                "running" => JobLifecycle::Started,
                "error" | "interrupted" => JobLifecycle::Failed,
                "done" | "cancelled" => JobLifecycle::Finished,
                _ if completed_at.is_none() && available_at.is_some_and(|at| at > now) => {
                    JobLifecycle::Deferred
                }
                _ => JobLifecycle::Queued,
            };
            counts.increment(lifecycle);
        }
        Ok(counts)
    }

    pub async fn reconcile_started(&self) -> Result<ReconciliationReport> {
        let now = now_secs() as i64;
        let mut tx = self.db.pool().begin().await?;
        let rows = sqlx::query(
            "SELECT j.id, j.status, j.result_json, j.error, j.stage, j.progress, \
                    o.delivery_count, o.job_id AS outbox_job_id, \
                    CASE WHEN r.payload_json <> 'null' THEN r.job_id END AS request_job_id \
             FROM jobs j LEFT JOIN job_outbox o ON o.job_id = j.id \
             LEFT JOIN job_requests r ON r.job_id = j.id \
             WHERE j.status IN ('pending', 'running')",
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut report = ReconciliationReport::default();
        for row in rows {
            let mut job = row_to_job(&row)?;
            let has_outbox = row.try_get::<Option<String>, _>("outbox_job_id")?.is_some();
            let has_request = row
                .try_get::<Option<String>, _>("request_job_id")?
                .is_some();
            let can_dispatch = has_outbox && has_request;
            let attempts = u32::try_from(
                row.try_get::<Option<i64>, _>("delivery_count")?
                    .unwrap_or(0),
            )
            .context("delivery count overflow")?;

            if job.status == JobStatus::Pending && can_dispatch {
                sqlx::query(
                    "UPDATE job_outbox SET lease_until = NULL, completed_at = NULL, last_error = NULL \
                     WHERE job_id = ?",
                )
                .bind(&job.id)
                .execute(&mut *tx)
                .await?;
                report.requeued += 1;
                continue;
            }

            let interrupted = JobEvent::Interrupted {
                reason: "process restarted while the job was active".into(),
            };
            apply(&mut job, &interrupted)?;
            append_event(
                &mut tx,
                &job.id,
                &interrupted,
                &format!("reconcile-interrupted:{now}"),
                now,
            )
            .await?;
            finish_latest_attempt(
                &mut tx,
                &job.id,
                "interrupted",
                Some(ErrorKind::Interrupted),
                job.error.as_deref(),
                None,
                now,
            )
            .await?;

            if can_dispatch
                && RetryPolicy::default()
                    .next_delay(attempts, ErrorKind::Interrupted)
                    .is_some()
            {
                let retry = JobEvent::RetryScheduled {
                    attempt: attempts.saturating_add(1),
                    available_at: now,
                };
                apply(&mut job, &retry)?;
                append_event(
                    &mut tx,
                    &job.id,
                    &retry,
                    &format!("reconcile-retry:{now}"),
                    now,
                )
                .await?;
                sqlx::query(
                    "UPDATE job_outbox SET available_at = ?, lease_until = NULL, \
                     completed_at = NULL, last_error = NULL WHERE job_id = ?",
                )
                .bind(now)
                .bind(&job.id)
                .execute(&mut *tx)
                .await?;
                report.requeued += 1;
            } else {
                if has_outbox {
                    complete_outbox(&mut tx, &job.id, job.error.as_deref(), now).await?;
                }
                erase_request_payload(&mut tx, &job.id).await?;
                report.interrupted += 1;
            }
            persist_snapshot(&mut tx, &job, now).await?;
        }
        tx.commit().await?;
        Ok(report)
    }

    pub async fn attempt_history(&self, job_id: &str) -> Result<Vec<JobAttempt>> {
        let rows = sqlx::query(
            "SELECT attempt, status, error_kind, error, next_retry_at, tool_version, \
                    started_at, finished_at FROM job_attempts WHERE job_id = ? ORDER BY attempt",
        )
        .bind(job_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(JobAttempt {
                    job_id: job_id.to_string(),
                    attempt: u32::try_from(row.try_get::<i64, _>("attempt")?)
                        .context("attempt overflow")?,
                    status: row.try_get("status")?,
                    error_kind: row
                        .try_get::<Option<String>, _>("error_kind")?
                        .map(|token| ErrorKind::from_token(&token)),
                    error: row.try_get("error")?,
                    next_retry_at: row.try_get("next_retry_at")?,
                    tool_version: row.try_get("tool_version")?,
                    started_at: row.try_get("started_at")?,
                    finished_at: row.try_get("finished_at")?,
                })
            })
            .collect()
    }
}

fn inject_enqueue_fault(configured: Option<u8>, boundary: u8) -> Result<()> {
    if configured == Some(boundary) {
        return Err(anyhow!("injected enqueue fault after boundary {boundary}"));
    }
    Ok(())
}

async fn append_event(
    tx: &mut Transaction<'_, Sqlite>,
    job_id: &str,
    event: &JobEvent,
    idempotency_key: &str,
    now: i64,
) -> Result<bool> {
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM job_events WHERE job_id = ?",
    )
    .bind(job_id)
    .fetch_one(&mut **tx)
    .await?;
    let result = sqlx::query(
        "INSERT INTO job_events \
         (job_id, sequence, idempotency_key, event_type, event_json, occurred_at) \
         VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(job_id, idempotency_key) DO NOTHING",
    )
    .bind(job_id)
    .bind(sequence)
    .bind(idempotency_key)
    .bind(event.token())
    .bind(serde_json::to_string(event)?)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

async fn persist_snapshot(tx: &mut Transaction<'_, Sqlite>, job: &Job, now: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO jobs \
         (id, status, result_json, error, stage, progress, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET status = excluded.status, result_json = excluded.result_json, \
           error = excluded.error, stage = excluded.stage, progress = excluded.progress, \
           updated_at = excluded.updated_at",
    )
    .bind(&job.id)
    .bind(job.status.as_str())
    .bind(job.result.as_ref().map(serde_json::to_string).transpose()?)
    .bind(&job.error)
    .bind(&job.stage)
    .bind(job.progress)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn latest_attempt_number(tx: &mut Transaction<'_, Sqlite>, job_id: &str) -> Result<u32> {
    let value: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(attempt), 0) FROM job_attempts WHERE job_id = ?")
            .bind(job_id)
            .fetch_one(&mut **tx)
            .await?;
    u32::try_from(value).context("attempt overflow")
}

#[allow(clippy::too_many_arguments)]
async fn finish_latest_attempt(
    tx: &mut Transaction<'_, Sqlite>,
    job_id: &str,
    status: &str,
    error_kind: Option<ErrorKind>,
    error: Option<&str>,
    next_retry_at: Option<i64>,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_attempts SET status = ?, error_kind = ?, error = ?, next_retry_at = ?, \
         finished_at = ? WHERE job_id = ? \
           AND attempt = (SELECT MAX(a.attempt) FROM job_attempts a WHERE a.job_id = ?)",
    )
    .bind(status)
    .bind(error_kind.map(ErrorKind::as_str))
    .bind(error)
    .bind(next_retry_at)
    .bind(now)
    .bind(job_id)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn complete_outbox(
    tx: &mut Transaction<'_, Sqlite>,
    job_id: &str,
    error: Option<&str>,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_outbox SET completed_at = ?, lease_until = NULL, last_error = ? WHERE job_id = ?",
    )
    .bind(now)
    .bind(error)
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn erase_request_payload(tx: &mut Transaction<'_, Sqlite>, job_id: &str) -> Result<()> {
    // Keep the timestamp/kind/hash row for rate accounting while removing URL
    // credentials and other execution inputs that terminal work no longer needs.
    sqlx::query("UPDATE job_requests SET payload_json = 'null' WHERE job_id = ?")
        .bind(job_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_operator_action(
    tx: &mut Transaction<'_, Sqlite>,
    job_id: &str,
    action: OperatorAction,
    actor: &str,
    reason: Option<&str>,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO job_operator_actions (id, job_id, action, actor, reason, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(job_id)
    .bind(action.as_str())
    .bind(actor)
    .bind(reason)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> Result<Job> {
    Ok(Job {
        id: row.try_get("id")?,
        status: JobStatus::from_token(&row.try_get::<String, _>("status")?)?,
        result: row
            .try_get::<Option<String>, _>("result_json")?
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        error: row.try_get("error")?,
        stage: row.try_get("stage")?,
        progress: row.try_get("progress")?,
    })
}

fn duration_secs(duration: std::time::Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::jobs::replay;

    async fn store() -> (SqliteJobStore, Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).await.unwrap();
        (SqliteJobStore::new(db.clone()), db, dir)
    }

    #[tokio::test]
    async fn enqueue_is_transactional_deduplicated_and_rate_limited() {
        let (store, db, _dir) = store().await;
        let limits = QueueLimits {
            max_new_jobs: 1,
            ..QueueLimits::default()
        };
        let first = store
            .enqueue_at(
                "one".into(),
                JobKind::Import,
                &json!({"x": 1}),
                "same",
                limits,
                10,
            )
            .await
            .unwrap();
        assert!(matches!(first, EnqueueOutcome::Created(_)));
        assert_eq!(
            store
                .enqueue_at(
                    "two".into(),
                    JobKind::Import,
                    &json!({"x": 1}),
                    "same",
                    limits,
                    10
                )
                .await
                .unwrap(),
            EnqueueOutcome::Existing("one".into())
        );
        assert_eq!(
            store
                .enqueue_at(
                    "three".into(),
                    JobKind::Import,
                    &json!({"x": 2}),
                    "other",
                    limits,
                    10
                )
                .await
                .unwrap(),
            EnqueueOutcome::RateLimited
        );
        assert_eq!(db.load_jobs().await.unwrap().len(), 1);
        assert_eq!(store.event_history("one").await.unwrap().len(), 1);
        assert_eq!(store.deliverable_ids().await.unwrap(), vec!["one"]);
    }

    #[tokio::test]
    async fn migration_backfills_legacy_snapshots_once() {
        let (store, db, _dir) = store().await;
        let mut legacy = Job::pending("legacy".into());
        legacy.status = JobStatus::Done;
        legacy.result = Some(json!({"filename": "legacy.mp4"}));
        legacy.progress = Some(100.0);
        db.persist_job(&legacy).await.unwrap();

        store.migrate().await.unwrap();
        store.migrate().await.unwrap();
        let history = store.event_history("legacy").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(replay("legacy", &history).unwrap(), legacy);
    }

    #[tokio::test]
    async fn enqueue_faults_never_leave_partial_job_or_outbox_rows() {
        for boundary in 1..=5 {
            let (store, db, _dir) = store().await;
            let error = store
                .enqueue_at_with_fault(
                    format!("job-{boundary}"),
                    JobKind::Import,
                    &json!({"boundary": boundary}),
                    &format!("dedupe-{boundary}"),
                    QueueLimits::default(),
                    10,
                    Some(boundary),
                )
                .await
                .unwrap_err();
            assert!(error.to_string().contains("injected enqueue fault"));
            assert!(db.load_jobs().await.unwrap().is_empty());
            assert!(store.deliverable_ids().await.unwrap().is_empty());
            assert!(store
                .event_history(&format!("job-{boundary}"))
                .await
                .unwrap()
                .is_empty());
        }
    }

    #[tokio::test]
    async fn idempotent_events_replay_to_the_snapshot() {
        let (store, db, _dir) = store().await;
        store
            .enqueue(
                "job".into(),
                JobKind::Edit,
                &json!({}),
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        let envelope = store.claim("job", 60).await.unwrap().unwrap();
        assert!(store
            .renew_lease("job", envelope.attempt, 60)
            .await
            .unwrap());
        assert!(!store
            .renew_lease("job", envelope.attempt.saturating_add(1), 60)
            .await
            .unwrap());
        let mut job = Job::pending("job".into());
        let started = JobEvent::Started {
            stage: "processing".into(),
            attempt: envelope.attempt,
        };
        apply(&mut job, &started).unwrap();
        assert!(store
            .record_transition(&job, &started, "started-1", Some("ffmpeg test"))
            .await
            .unwrap());
        assert!(!store
            .record_transition(&job, &started, "started-1", Some("ffmpeg test"))
            .await
            .unwrap());
        let done = JobEvent::Succeeded {
            result: json!({"filename": "done.mp4"}),
        };
        apply(&mut job, &done).unwrap();
        store
            .record_transition(&job, &done, "done-1", Some("ffmpeg test"))
            .await
            .unwrap();

        let replayed = replay("job", &store.event_history("job").await.unwrap()).unwrap();
        let persisted = db.load_jobs().await.unwrap().pop().unwrap();
        assert_eq!(replayed.status, persisted.status);
        assert_eq!(replayed.result, persisted.result);
        assert_eq!(
            store.attempt_history("job").await.unwrap()[0].status,
            "succeeded"
        );
        let erased_payload: String =
            sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(erased_payload, "null", "terminal payload must be erased");
    }

    #[tokio::test]
    async fn quarantine_restores_a_corrupt_snapshot_from_valid_events() {
        let (store, db, _dir) = store().await;
        store
            .enqueue(
                "job".into(),
                JobKind::Edit,
                &json!({}),
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        let envelope = store.claim("job", 60).await.unwrap().unwrap();
        let mut job = Job::pending("job".into());
        let started = JobEvent::Started {
            stage: "processing".into(),
            attempt: envelope.attempt,
        };
        apply(&mut job, &started).unwrap();
        store
            .record_transition(&job, &started, "started", Some("ffmpeg test"))
            .await
            .unwrap();
        let done = JobEvent::Succeeded {
            result: json!({"filename": "done.mp4"}),
        };
        apply(&mut job, &done).unwrap();
        store
            .record_transition(&job, &done, "done", Some("ffmpeg test"))
            .await
            .unwrap();

        sqlx::query(
            "UPDATE jobs SET status = 'future-status', result_json = NULL WHERE id = 'job'",
        )
        .execute(db.pool())
        .await
        .unwrap();
        quarantine::migrate(&db).await.unwrap();

        let restored = db.load_job("job").await.unwrap().unwrap();
        assert_eq!(restored.status, JobStatus::Done);
        assert_eq!(restored.result.unwrap()["filename"], "done.mp4");
        let original_status: String = sqlx::query_scalar(
            "SELECT original_status FROM job_status_quarantine WHERE job_id = 'job'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(original_status, "future-status");
    }

    #[tokio::test]
    async fn quarantine_archives_unreadable_history_and_private_request() {
        let (store, db, _dir) = store().await;
        let payload = json!({"url": "https://example.test/video?token=secret"});
        store
            .enqueue(
                "job".into(),
                JobKind::Import,
                &payload,
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        store.claim("job", 60).await.unwrap().unwrap();
        sqlx::query(
            "UPDATE job_events SET event_json = \
             'not-json https://events.test/path?token=EVENT_CANARY' WHERE job_id = 'job'",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "UPDATE job_attempts SET error = \
             'failed https://attempt.test/path?token=ATTEMPT_CANARY' WHERE job_id = 'job'",
        )
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE jobs SET status = 'future-status' WHERE id = 'job'")
            .execute(db.pool())
            .await
            .unwrap();

        quarantine::migrate(&db).await.unwrap();

        let restored = db.load_job("job").await.unwrap().unwrap();
        assert_eq!(restored.status, JobStatus::Interrupted);
        let history = store.event_history("job").await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            replay("job", &history).unwrap().status,
            JobStatus::Interrupted
        );

        let live_payload: String =
            sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(live_payload, "null");
        let archived_payload: String = sqlx::query_scalar(
            "SELECT payload_json FROM job_request_quarantine WHERE job_id = 'job'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        let archived_payload: Value = serde_json::from_str(&archived_payload).unwrap();
        assert_eq!(
            archived_payload["url"],
            "https://example.test/video?REDACTED"
        );
        assert!(!archived_payload.to_string().contains("secret"));
        let archived_event: String =
            sqlx::query_scalar("SELECT event_json FROM job_event_quarantine WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(archived_event, "not-json https://events.test/path?REDACTED");
        assert!(!archived_event.contains("EVENT_CANARY"));
        let archived_attempt: String =
            sqlx::query_scalar("SELECT status FROM job_attempt_quarantine WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(archived_attempt, "claimed");
        let archived_attempt_error: String =
            sqlx::query_scalar("SELECT error FROM job_attempt_quarantine WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            archived_attempt_error,
            "failed https://attempt.test/path?REDACTED"
        );
        assert!(!archived_attempt_error.contains("ATTEMPT_CANARY"));
        let completed_at: Option<i64> =
            sqlx::query_scalar("SELECT completed_at FROM job_outbox WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(completed_at.is_some());

        sqlx::query("UPDATE job_status_quarantine SET quarantined_at = 0 WHERE job_id = 'job'")
            .execute(db.pool())
            .await
            .unwrap();
        let purged = store.purge_expired_quarantine().await.unwrap();
        assert_eq!(purged, 1);
        let expiry_index: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_index_list('job_status_quarantine') \
             WHERE name = 'idx_job_status_quarantine_expiry'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(expiry_index, 1);
        for table in [
            "job_status_quarantine",
            "job_request_quarantine",
            "job_event_quarantine",
            "job_attempt_quarantine",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(db.pool())
                .await
                .unwrap();
            assert_eq!(count, 0, "expired rows remain in {table}");
        }
    }

    #[tokio::test]
    async fn failed_registry_retry_and_discard_are_audited() {
        let (store, _db, _dir) = store().await;
        store
            .enqueue(
                "job".into(),
                JobKind::Edit,
                &json!({}),
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        let envelope = store.claim("job", 60).await.unwrap().unwrap();
        let mut job = Job::pending("job".into());
        let started = JobEvent::Started {
            stage: "processing".into(),
            attempt: envelope.attempt,
        };
        apply(&mut job, &started).unwrap();
        store
            .record_transition(&job, &started, "start", None)
            .await
            .unwrap();
        let failed = JobEvent::Failed {
            kind: ErrorKind::Timeout,
            message: "deadline".into(),
        };
        apply(&mut job, &failed).unwrap();
        store
            .record_transition(&job, &failed, "fail", None)
            .await
            .unwrap();
        assert_eq!(store.list_failed().await.unwrap().len(), 1);

        let retried = store.retry_failed("job", "test-operator").await.unwrap();
        assert_eq!(retried.status, JobStatus::Pending);
        assert_eq!(store.operator_action_count("job").await.unwrap(), 1);
        assert!(store.claim("job", 60).await.unwrap().is_some());

        let failed_again = JobEvent::Failed {
            kind: ErrorKind::Timeout,
            message: "deadline again".into(),
        };
        let mut retried = retried;
        apply(&mut retried, &failed_again).unwrap();
        store
            .record_transition(&retried, &failed_again, "fail-again", None)
            .await
            .unwrap();
        let discarded = store
            .discard_failed("job", "test-operator", Some("manual discard"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(discarded.status, JobStatus::Cancelled);
        assert_eq!(store.operator_action_count("job").await.unwrap(), 2);
        assert!(store.list_failed().await.unwrap().is_empty());
        assert_eq!(
            replay("job", &store.event_history("job").await.unwrap())
                .unwrap()
                .status,
            JobStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn startup_reconciliation_requeues_claimed_work() {
        let (store, db, _dir) = store().await;
        store
            .enqueue(
                "job".into(),
                JobKind::Import,
                &json!({}),
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        let envelope = store.claim("job", 60).await.unwrap().unwrap();
        let mut job = Job::pending("job".into());
        let started = JobEvent::Started {
            stage: "downloading".into(),
            attempt: envelope.attempt,
        };
        apply(&mut job, &started).unwrap();
        store
            .record_transition(&job, &started, "started", None)
            .await
            .unwrap();

        let report = store.reconcile_started().await.unwrap();
        assert_eq!(report.requeued, 1);
        assert_eq!(db.load_jobs().await.unwrap()[0].status, JobStatus::Pending);
        assert_eq!(store.deliverable_ids().await.unwrap(), vec!["job"]);
        assert_eq!(store.lifecycle_counts().await.unwrap().queued, 1);
    }

    #[tokio::test]
    async fn retry_scheduler_uses_error_taxonomy_and_defers_delivery() {
        let (store, db, _dir) = store().await;
        for (job_id, kind) in [
            ("retryable", ErrorKind::Timeout),
            ("blocked", ErrorKind::Security),
        ] {
            store
                .enqueue(
                    job_id.into(),
                    JobKind::Import,
                    &json!({}),
                    &format!("dedupe-{job_id}"),
                    QueueLimits::default(),
                )
                .await
                .unwrap();
            let envelope = store.claim(job_id, 60).await.unwrap().unwrap();
            let mut job = Job::pending(job_id.into());
            let started = JobEvent::Started {
                stage: "downloading".into(),
                attempt: envelope.attempt,
            };
            apply(&mut job, &started).unwrap();
            store
                .record_transition(&job, &started, &format!("start-{job_id}"), None)
                .await
                .unwrap();
            let failed = JobEvent::Failed {
                kind,
                message: "failure".into(),
            };
            apply(&mut job, &failed).unwrap();
            store
                .record_transition(&job, &failed, &format!("fail-{job_id}"), None)
                .await
                .unwrap();
        }

        let (job, delay) = store.schedule_retry("retryable").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(delay, std::time::Duration::from_secs(2));
        assert!(store.deliverable_ids().await.unwrap().is_empty());
        assert!(store.schedule_retry("blocked").await.unwrap().is_none());
        let blocked_payload: String =
            sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id = 'blocked'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(
            blocked_payload, "null",
            "non-retryable payload must be erased"
        );
        let limited = store
            .enqueue(
                "cannot-bypass-rate-limit".into(),
                JobKind::Import,
                &json!({}),
                "third-dedupe",
                QueueLimits {
                    max_new_jobs: 2,
                    ..QueueLimits::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(limited, EnqueueOutcome::RateLimited);
        let counts = store.lifecycle_counts().await.unwrap();
        assert_eq!(counts.deferred, 1);
        assert_eq!(counts.failed, 1);
    }

    #[tokio::test]
    async fn due_retry_recovers_a_crash_between_failure_and_scheduling() {
        let (store, db, _dir) = store().await;
        store
            .enqueue(
                "job".into(),
                JobKind::Import,
                &json!({"url": "https://example.test/?token=CANARY"}),
                "dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        let envelope = store.claim("job", 60).await.unwrap().unwrap();
        let mut job = Job::pending("job".into());
        let started = JobEvent::Started {
            stage: "downloading".into(),
            attempt: envelope.attempt,
        };
        apply(&mut job, &started).unwrap();
        store
            .record_transition(&job, &started, "start", None)
            .await
            .unwrap();
        let failed = JobEvent::Failed {
            kind: ErrorKind::Timeout,
            message: "deadline".into(),
        };
        apply(&mut job, &failed).unwrap();
        store
            .record_transition(&job, &failed, "failure", None)
            .await
            .unwrap();

        let due: i64 = sqlx::query_scalar(
            "SELECT next_retry_at FROM job_attempts WHERE job_id = 'job' AND attempt = 1",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        let recovered = store.schedule_due_retries_at(due).await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, JobStatus::Pending);
        let completed_at: Option<i64> =
            sqlx::query_scalar("SELECT completed_at FROM job_outbox WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(completed_at.is_none());
        let payload: String =
            sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id = 'job'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(payload.contains("CANARY"), "retry still needs its request");
    }
}
