use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TRACE_SAMPLE_PER_MILLE: u16 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceContext {
    pub trace_id: String,
    pub parent_id: String,
    pub sampled: bool,
}

impl TraceContext {
    pub fn from_request_id(request_id: &str) -> Self {
        let digest = Sha256::digest(request_id.as_bytes());
        let bucket = u16::from_be_bytes([digest[0], digest[1]]) % 1000;
        Self {
            trace_id: hex_prefix(&digest, 16),
            parent_id: request_id.to_owned(),
            sampled: bucket < TRACE_SAMPLE_PER_MILLE,
        }
    }

    pub fn job_span(&self, job_id: &str, kind: &'static str) -> tracing::Span {
        tracing::info_span!(
            "job",
            job.id = job_id,
            job.kind = kind,
            trace.id = %self.trace_id,
            trace.parent_id = %self.parent_id,
            trace.sampled = self.sampled,
        )
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self {
            trace_id: "legacy".into(),
            parent_id: "legacy".into(),
            sampled: false,
        }
    }
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_deterministic_bounded_and_budgeted() {
        let first = TraceContext::from_request_id("request-1");
        assert_eq!(first, TraceContext::from_request_id("request-1"));
        assert_eq!(first.trace_id.len(), 32);
        let sampled = (0..10_000)
            .filter(|index| TraceContext::from_request_id(&format!("request-{index}")).sampled)
            .count();
        assert!((800..=1_200).contains(&sampled));
    }

    #[test]
    fn durable_round_trip_preserves_queue_link() {
        let context = TraceContext::from_request_id("request-42");
        let json = serde_json::to_string(&context).unwrap();
        assert_eq!(
            serde_json::from_str::<TraceContext>(&json).unwrap(),
            context
        );
    }
}
