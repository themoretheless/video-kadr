use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Security,
    Timeout,
    ToolUnavailable,
    ProcessExit,
    Storage,
    Interrupted,
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Security => "security",
            Self::Timeout => "timeout",
            Self::ToolUnavailable => "tool_unavailable",
            Self::ProcessExit => "process_exit",
            Self::Storage => "storage",
            Self::Interrupted => "interrupted",
            Self::Internal => "internal",
        }
    }

    pub fn from_token(token: &str) -> Self {
        match token {
            "validation" => Self::Validation,
            "security" => Self::Security,
            "timeout" => Self::Timeout,
            "tool_unavailable" => Self::ToolUnavailable,
            "process_exit" => Self::ProcessExit,
            "storage" => Self::Storage,
            "interrupted" => Self::Interrupted,
            _ => Self::Internal,
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::Timeout
                | Self::ToolUnavailable
                | Self::ProcessExit
                | Self::Storage
                | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
        }
    }
}

impl RetryPolicy {
    pub fn next_delay(self, completed_attempts: u32, kind: ErrorKind) -> Option<Duration> {
        if !kind.retryable() || completed_attempts >= self.max_attempts {
            return None;
        }
        let exponent = completed_attempts.saturating_sub(1).min(31);
        let multiplier = 1_u32 << exponent;
        Some(
            self.base_delay
                .saturating_mul(multiplier)
                .min(self.max_delay),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobAttempt {
    pub job_id: String,
    pub attempt: u32,
    pub status: String,
    pub error_kind: Option<ErrorKind>,
    pub error: Option<String>,
    pub next_retry_at: Option<i64>,
    pub tool_version: Option<String>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_and_security_never_retry() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.next_delay(1, ErrorKind::Validation), None);
        assert_eq!(policy.next_delay(1, ErrorKind::Security), None);
    }

    #[test]
    fn retry_backoff_is_bounded_and_capped() {
        let policy = RetryPolicy {
            max_attempts: 4,
            base_delay: Duration::from_secs(3),
            max_delay: Duration::from_secs(10),
        };
        assert_eq!(
            policy.next_delay(1, ErrorKind::Timeout),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            policy.next_delay(3, ErrorKind::ProcessExit),
            Some(Duration::from_secs(10))
        );
        assert_eq!(policy.next_delay(4, ErrorKind::Storage), None);
    }
}
