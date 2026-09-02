//! Redaction helpers used before sensitive values reach logs or diagnostics.

pub mod redaction;
pub use redaction::RedactionTransform;

use std::fmt;

use serde_json::Value;

/// Display a URL without credentials, query values or fragments.
pub struct RedactedUrl<'a>(pub &'a str);

impl fmt::Display for RedactedUrl<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&redact_url(self.0))
    }
}

pub fn redact_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return "<invalid-url>".into();
    };

    if url.password().is_some() {
        let _ = url.set_password(None);
    }
    if !url.username().is_empty() {
        let _ = url.set_username("");
    }
    let had_query = url.query().is_some();
    url.set_query(None);
    url.set_fragment(None);

    let mut safe = url.to_string();
    if had_query {
        safe.push_str("?REDACTED");
    }
    safe
}

/// Redact URL credentials/query strings and absolute filesystem paths embedded
/// in a larger error message. URL recognition intentionally covers only HTTP(S),
/// the only remote schemes accepted by the import boundary.
pub fn redact_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = next_http_url(rest) {
        output.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let end = candidate
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | ']' | '}'))
            .unwrap_or(candidate.len());
        output.push_str(&redact_url(&candidate[..end]));
        rest = &candidate[end..];
    }
    output.push_str(rest);
    redact_absolute_paths(&output)
}

/// Redact sensitive values in JSON while preserving enough structure for local
/// recovery diagnostics. Invalid JSON is treated as plain text.
pub fn redact_json_text(input: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(input) else {
        return redact_text(input);
    };
    redact_json_value(&mut value);
    serde_json::to_string(&value).expect("redacted JSON serialization cannot fail")
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_text(text),
        Value::Array(values) => values.iter_mut().for_each(redact_json_value),
        Value::Object(values) => {
            for (key, value) in values {
                if is_sensitive_json_key(key) {
                    *value = Value::String("<redacted>".into());
                } else {
                    redact_json_value(value);
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
    [
        "token",
        "secret",
        "password",
        "authorization",
        "cookie",
        "apikey",
        "signature",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn next_http_url(input: &str) -> Option<usize> {
    [input.find("https://"), input.find("http://")]
        .into_iter()
        .flatten()
        .min()
}

fn redact_absolute_paths(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied_until = 0;
    let mut index = 0;

    while index < bytes.len() {
        let at_boundary = index == 0 || is_path_boundary(bytes[index - 1]);
        let unix_path = at_boundary && bytes[index] == b'/';
        let windows_path = at_boundary
            && index + 2 < bytes.len()
            && bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && matches!(bytes[index + 2], b'/' | b'\\');

        if !unix_path && !windows_path {
            index += 1;
            continue;
        }

        output.push_str(&input[copied_until..index]);
        output.push_str("<redacted-path>");
        index += if windows_path { 3 } else { 1 };
        while index < bytes.len() && !is_path_terminator(bytes[index]) {
            index += 1;
        }
        copied_until = index;
    }

    output.push_str(&input[copied_until..]);
    output
}

fn is_path_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'=' | b'(' | b'[' | b'{' | b'\'' | b'"')
}

fn is_path_terminator(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b';' | b':' | b')' | b']' | b'}' | b'\'' | b'"' | b'<' | b'>'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_redaction_removes_credentials_query_and_fragment() {
        let safe =
            redact_url("https://alice:secret@example.com/video.mp4?token=CANARY&part=2#private");
        assert_eq!(safe, "https://example.com/video.mp4?REDACTED");
        assert!(!safe.contains("CANARY"));
        assert!(!safe.contains("private"));
    }

    #[test]
    fn text_redaction_handles_multiple_urls() {
        let safe = redact_text(
            "download https://example.com/a?token=CANARY then http://host/b?signature=SECOND",
        );
        assert_eq!(
            safe,
            "download https://example.com/a?REDACTED then http://host/b?REDACTED"
        );
        assert!(!safe.contains("CANARY"));
        assert!(!safe.contains("SECOND"));
    }

    #[test]
    fn text_redaction_removes_unix_and_windows_paths_without_damaging_safe_urls() {
        let safe = redact_text(
            "probe /Users/alice/private/input.mp4 then C:\\Users\\alice\\secret.mov; url https://example.com/video",
        );

        assert_eq!(
            safe,
            "probe <redacted-path> then <redacted-path>; url https://example.com/video"
        );
        assert!(!safe.contains("alice"));
    }

    #[test]
    fn json_redaction_handles_nested_urls_and_named_secrets() {
        let safe = redact_json_text(
            r#"{"url":"https://example.test/video?token=CANARY","nested":{"api_key":"SECOND"},"items":["http://host/path?signature=THIRD"]}"#,
        );
        let value: Value = serde_json::from_str(&safe).unwrap();

        assert_eq!(value["url"], "https://example.test/video?REDACTED");
        assert_eq!(value["nested"]["api_key"], "<redacted>");
        assert_eq!(value["items"][0], "http://host/path?REDACTED");
        assert!(!safe.contains("CANARY"));
        assert!(!safe.contains("SECOND"));
        assert!(!safe.contains("THIRD"));
    }
}
