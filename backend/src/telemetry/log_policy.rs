use std::collections::BTreeMap;

use serde_json::Value;

pub const LOKI_LABEL_KEYS: [&str; 4] = ["service", "env", "level", "error_kind"];

pub fn loki_labels(fields: &serde_json::Map<String, Value>) -> BTreeMap<&'static str, String> {
    let mut labels = BTreeMap::new();
    for key in LOKI_LABEL_KEYS {
        let Some(value) = fields.get(key).and_then(Value::as_str) else {
            continue;
        };
        if value.len() <= 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            labels.insert(key, value.to_owned());
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_fixed_low_cardinality_fields_can_become_loki_labels() {
        let fields = json!({
            "service": "video-kadr",
            "env": "staging",
            "level": "warn",
            "error_kind": "timeout",
            "job_id": "high-cardinality",
            "url": "https://example.test/private",
            "filename": "private.mp4"
        });
        let labels = loki_labels(fields.as_object().unwrap());
        assert_eq!(labels.len(), 4);
        assert!(!labels.contains_key("job_id"));
        assert!(!labels.contains_key("url"));
        assert!(!labels.contains_key("filename"));
    }
}
