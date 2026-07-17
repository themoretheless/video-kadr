use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueLimits {
    pub dedupe_ttl: Duration,
    pub rate_window: Duration,
    pub max_new_jobs: u32,
}

impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            dedupe_ttl: Duration::from_secs(5 * 60),
            rate_window: Duration::from_secs(60),
            max_new_jobs: 60,
        }
    }
}

pub fn dedupe_key<T: Serialize>(namespace: &str, value: &T) -> anyhow::Result<String> {
    let canonical = serde_json::to_vec(value)?;
    let mut hash = Sha256::new();
    hash.update(b"video-editor-job-v1\0");
    hash.update(namespace.as_bytes());
    hash.update(b"\0");
    hash.update(canonical);
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn key_is_stable_namespaced_and_contains_no_source_data() {
        let source = json!({"url": "https://example.test/x?token=secret"});
        let a = dedupe_key("import", &source).unwrap();
        let b = dedupe_key("import", &source).unwrap();
        let c = dedupe_key("edit", &source).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(!a.contains("secret"));
    }
}
