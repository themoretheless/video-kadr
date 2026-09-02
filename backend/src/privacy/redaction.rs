use serde_json::{Map, Value};

const ALLOWED_FIELDS: [&str; 14] = [
    "service",
    "env",
    "level",
    "error_kind",
    "event",
    "stage",
    "message",
    "tool",
    "status",
    "timestamp",
    "request_id",
    "job_id",
    "trace_id",
    "redaction_version",
];

#[derive(Debug, Clone, Copy, Default)]
pub struct RedactionTransform;

impl RedactionTransform {
    pub fn sanitize_log(self, fields: &Value) -> Value {
        self.sanitize(fields)
    }

    pub fn sanitize_diagnostic_bundle(self, fields: &Value) -> Value {
        self.sanitize(fields)
    }

    fn sanitize(self, fields: &Value) -> Value {
        let Some(fields) = fields.as_object() else {
            return Value::String(super::redact_text(&fields.to_string()));
        };
        let mut safe = Map::new();
        safe.insert("redaction_version".into(), Value::String("v1".into()));
        for key in ALLOWED_FIELDS {
            let Some(value) = fields.get(key) else {
                continue;
            };
            safe.insert(key.to_owned(), sanitize_value(value));
        }
        Value::Object(safe)
    }
}

fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::String(value) => Value::String(super::redact_text(value)),
        Value::Array(values) => Value::Array(values.iter().map(sanitize_value).collect()),
        Value::Object(_) => Value::String(super::redact_json_text(&value.to_string())),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn one_transform_protects_logs_and_diagnostic_bundles() {
        let canary = json!({
            "service": "video-kadr",
            "level": "error",
            "message": "failed https://user:CANARY@example.test/a?token=CANARY at /Users/CANARY/input.mp4",
            "token": "CANARY",
            "url": "https://example.test/?secret=CANARY",
            "filename": "/Users/CANARY/input.mp4"
        });
        let transform = RedactionTransform;
        let log = transform.sanitize_log(&canary).to_string();
        let diagnostic = transform.sanitize_diagnostic_bundle(&canary).to_string();
        for output in [log, diagnostic] {
            assert!(!output.contains("CANARY"), "canary leaked: {output}");
            assert!(!output.contains("token"));
            assert!(!output.contains("filename"));
            assert!(output.contains("redacted-path"));
        }
    }
}
