use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Edit,
    Composition,
    Proxy,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Edit => "edit",
            Self::Composition => "composition",
            Self::Proxy => "proxy",
        }
    }

    pub fn from_token(token: &str) -> anyhow::Result<Self> {
        match token {
            "import" => Ok(Self::Import),
            "edit" => Ok(Self::Edit),
            "composition" => Ok(Self::Composition),
            "proxy" => Ok(Self::Proxy),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_job_kind_has_a_stable_durable_token() {
        assert_eq!(JobKind::Proxy.as_str(), "proxy");
        assert_eq!(JobKind::from_token("proxy").unwrap(), JobKind::Proxy);
    }
}
