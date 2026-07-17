use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Edit,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Edit => "edit",
        }
    }

    pub fn from_token(token: &str) -> anyhow::Result<Self> {
        match token {
            "import" => Ok(Self::Import),
            "edit" => Ok(Self::Edit),
            _ => anyhow::bail!("unknown job kind {token}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobEnvelope {
    pub job_id: String,
    pub kind: JobKind,
    pub payload: Value,
    pub attempt: u32,
}
