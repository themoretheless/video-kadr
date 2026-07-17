use serde::Serialize;

use super::ErrorKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedJob {
    pub job_id: String,
    pub attempt: u32,
    pub error_kind: ErrorKind,
    pub reason: String,
    pub next_retry_at: Option<i64>,
    pub tool_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorAction {
    Retry,
    Discard,
}

impl OperatorAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Discard => "discard",
        }
    }
}
