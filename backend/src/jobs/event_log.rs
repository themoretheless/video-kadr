use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Job, JobStatus};

use super::ErrorKind;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Created,
    Queued,
    Started { stage: String, attempt: u32 },
    Succeeded { result: Value },
    Failed { kind: ErrorKind, message: String },
    Cancelled,
    Discarded { reason: Option<String> },
    Interrupted { reason: String },
    RetryScheduled { attempt: u32, available_at: i64 },
}

impl JobEvent {
    pub fn token(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Queued => "queued",
            Self::Started { .. } => "started",
            Self::Succeeded { .. } => "succeeded",
            Self::Failed { .. } => "failed",
            Self::Cancelled => "cancelled",
            Self::Discarded { .. } => "discarded",
            Self::Interrupted { .. } => "interrupted",
            Self::RetryScheduled { .. } => "retry_scheduled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedJobEvent {
    pub job_id: String,
    pub sequence: u32,
    pub idempotency_key: String,
    pub event: JobEvent,
    pub occurred_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionError(&'static str);

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for TransitionError {}

pub fn apply(job: &mut Job, event: &JobEvent) -> Result<(), TransitionError> {
    if job.status.is_terminal()
        && !matches!(
            event,
            JobEvent::RetryScheduled { .. } | JobEvent::Discarded { .. }
        )
    {
        return Err(TransitionError("terminal job cannot accept another event"));
    }

    match event {
        JobEvent::Created => {
            if job.status != JobStatus::Pending
                || job.result.is_some()
                || job.error.is_some()
                || job.stage.is_some()
            {
                return Err(TransitionError("created must be the first event"));
            }
        }
        JobEvent::Queued => {
            if job.status != JobStatus::Pending {
                return Err(TransitionError("only a pending job can be queued"));
            }
            job.stage = Some("queued".into());
        }
        JobEvent::Started { stage, .. } => {
            if job.status != JobStatus::Pending {
                return Err(TransitionError("only a pending job can be started"));
            }
            if stage.is_empty() {
                return Err(TransitionError("started stage cannot be empty"));
            }
            job.status = JobStatus::Running;
            job.stage = Some(stage.clone());
            job.progress = Some(0.0);
            job.error = None;
        }
        JobEvent::Succeeded { result } => {
            if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                return Err(TransitionError("only an open job can succeed"));
            }
            job.status = JobStatus::Done;
            job.result = Some(result.clone());
            job.error = None;
            job.stage = None;
            job.progress = Some(100.0);
        }
        JobEvent::Failed { message, .. } => {
            if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                return Err(TransitionError("only an open job can fail"));
            }
            job.status = JobStatus::Error;
            job.error = Some(message.clone());
            job.stage = None;
            job.progress = None;
        }
        JobEvent::Cancelled => {
            if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                return Err(TransitionError("only an open job can be cancelled"));
            }
            job.status = JobStatus::Cancelled;
            job.stage = None;
            job.progress = None;
        }
        JobEvent::Discarded { .. } => {
            if !matches!(job.status, JobStatus::Error | JobStatus::Interrupted) {
                return Err(TransitionError("only failed jobs can be discarded"));
            }
            job.status = JobStatus::Cancelled;
            job.error = None;
            job.stage = None;
            job.progress = None;
        }
        JobEvent::Interrupted { reason } => {
            if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
                return Err(TransitionError("only an open job can be interrupted"));
            }
            job.status = JobStatus::Interrupted;
            job.error = Some(reason.clone());
            job.stage = None;
            job.progress = None;
        }
        JobEvent::RetryScheduled { .. } => {
            if !matches!(job.status, JobStatus::Error | JobStatus::Interrupted) {
                return Err(TransitionError("only failed jobs can be retried"));
            }
            job.status = JobStatus::Pending;
            job.result = None;
            job.error = None;
            job.stage = Some("deferred".into());
            job.progress = None;
        }
    }
    Ok(())
}

pub fn replay(job_id: &str, events: &[RecordedJobEvent]) -> Result<Job, TransitionError> {
    let mut job = Job::pending(job_id.to_string());
    for (expected, recorded) in (1_u32..).zip(events) {
        if recorded.job_id != job_id || recorded.sequence != expected {
            return Err(TransitionError("event history is not contiguous"));
        }
        if expected == 1 && !matches!(recorded.event, JobEvent::Created) {
            return Err(TransitionError("event history must start with created"));
        }
        if expected > 1 && matches!(recorded.event, JobEvent::Created) {
            return Err(TransitionError("created event may only appear first"));
        }
        apply(&mut job, &recorded.event)?;
    }
    Ok(job)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn record(sequence: u32, event: JobEvent) -> RecordedJobEvent {
        RecordedJobEvent {
            job_id: "job-1".into(),
            sequence,
            idempotency_key: format!("event-{sequence}"),
            event,
            occurred_at: sequence as i64,
        }
    }

    #[test]
    fn replay_reconstructs_the_same_terminal_snapshot() {
        let history = vec![
            record(1, JobEvent::Created),
            record(2, JobEvent::Queued),
            record(
                3,
                JobEvent::Started {
                    stage: "processing".into(),
                    attempt: 1,
                },
            ),
            record(
                4,
                JobEvent::Succeeded {
                    result: json!({"filename": "done.mp4"}),
                },
            ),
        ];
        let job = replay("job-1", &history).unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.progress, Some(100.0));
        assert_eq!(job.result.unwrap()["filename"], "done.mp4");
    }

    #[test]
    fn replay_rejects_gaps_and_terminal_overwrites() {
        assert!(replay(
            "job-1",
            &[record(1, JobEvent::Created), record(3, JobEvent::Cancelled)]
        )
        .is_err());

        let mut job = Job::pending("job-1".into());
        apply(&mut job, &JobEvent::Cancelled).unwrap();
        assert!(apply(&mut job, &JobEvent::Succeeded { result: json!({}) }).is_err());
    }

    #[test]
    fn replay_accepts_idempotent_queue_markers_after_recovery() {
        let job = replay(
            "job-1",
            &[
                record(1, JobEvent::Created),
                record(2, JobEvent::Queued),
                record(3, JobEvent::Queued),
                record(
                    4,
                    JobEvent::Started {
                        stage: "processing".into(),
                        attempt: 1,
                    },
                ),
            ],
        )
        .unwrap();

        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.stage.as_deref(), Some("processing"));
    }

    #[test]
    fn transition_matrix_rejects_invalid_open_state_regressions() {
        let mut running = Job::pending("running".into());
        apply(
            &mut running,
            &JobEvent::Started {
                stage: "processing".into(),
                attempt: 1,
            },
        )
        .unwrap();
        assert!(apply(&mut running, &JobEvent::Queued).is_err());
        assert!(apply(
            &mut running,
            &JobEvent::Started {
                stage: "processing".into(),
                attempt: 2,
            }
        )
        .is_err());

        let mut pending = Job::pending("pending".into());
        apply(&mut pending, &JobEvent::Queued).unwrap();
        apply(&mut pending, &JobEvent::Queued).unwrap();
        assert_eq!(pending.stage.as_deref(), Some("queued"));

        assert!(replay(
            "job-1",
            &[record(1, JobEvent::Created), record(2, JobEvent::Created)]
        )
        .is_err());
    }

    #[test]
    fn transition_matrix_keeps_supported_shortcuts() {
        let mut cache_hit = Job::pending("cache-hit".into());
        apply(&mut cache_hit, &JobEvent::Queued).unwrap();
        apply(
            &mut cache_hit,
            &JobEvent::Succeeded {
                result: json!({"cached": true}),
            },
        )
        .unwrap();
        assert_eq!(cache_hit.status, JobStatus::Done);

        let mut validation_failure = Job::pending("validation".into());
        apply(
            &mut validation_failure,
            &JobEvent::Failed {
                kind: ErrorKind::Timeout,
                message: "timed out".into(),
            },
        )
        .unwrap();
        apply(
            &mut validation_failure,
            &JobEvent::RetryScheduled {
                attempt: 2,
                available_at: 42,
            },
        )
        .unwrap();
        assert_eq!(validation_failure.status, JobStatus::Pending);
        assert_eq!(validation_failure.stage.as_deref(), Some("deferred"));
    }
}
