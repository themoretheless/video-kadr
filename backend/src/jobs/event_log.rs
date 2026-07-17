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
            job.stage = Some("queued".into());
        }
        JobEvent::Started { stage, .. } => {
            if stage.is_empty() {
                return Err(TransitionError("started stage cannot be empty"));
            }
            job.status = JobStatus::Running;
            job.stage = Some(stage.clone());
            job.progress = Some(0.0);
            job.error = None;
        }
        JobEvent::Succeeded { result } => {
            job.status = JobStatus::Done;
            job.result = Some(result.clone());
            job.error = None;
            job.stage = None;
            job.progress = Some(100.0);
        }
        JobEvent::Failed { message, .. } => {
            job.status = JobStatus::Error;
            job.error = Some(message.clone());
            job.stage = None;
            job.progress = None;
        }
        JobEvent::Cancelled => {
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
}
