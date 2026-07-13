//! Stable, low-cardinality tracing schema for HTTP requests and background work.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use uuid::Uuid;

pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

pub async fn request_context(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_request_id(value))
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    request
        .headers_mut()
        .insert(REQUEST_ID_HEADER.clone(), header_value(&request_id));

    let span = tracing::info_span!(
        "request",
        request.id = %request_id,
        http.request.method = %method,
        url.path = %path,
    );
    let mut response = next.run(request).instrument(span.clone()).await;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER.clone(), header_value(&request_id));
    tracing::info!(parent: &span, http.response.status_code = response.status().as_u16(), "request complete");
    response
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn header_value(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).expect("validated request id is a header value")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CaptureWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = CaptureWriter;

        fn make_writer(&'a self) -> Self::Writer {
            CaptureWriter(self.0.clone())
        }
    }

    #[test]
    fn request_ids_are_bounded_and_header_safe() {
        assert!(valid_request_id("client-123.trace"));
        assert!(!valid_request_id(""));
        assert!(!valid_request_id("has spaces"));
        assert!(!valid_request_id(&"x".repeat(129)));
    }

    #[test]
    fn json_span_contract_redacts_url_canaries() {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_current_span(true)
            .with_span_list(true)
            .with_writer(capture.clone())
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let request = tracing::info_span!("request", request.id = "req-1");
            let _request = request.enter();
            let job = tracing::info_span!("job", job.id = "job-1");
            let _job = job.enter();
            let process = tracing::info_span!("process", process.tool = "yt-dlp");
            let _process = process.enter();
            tracing::warn!(
                url = %crate::privacy::RedactedUrl("https://user:CANARY@example.com/video?token=CANARY#CANARY"),
                error = %crate::privacy::redact_text("failed https://example.com/a?secret=CANARY"),
                "process failed"
            );
        });

        let output = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert!(!output.contains("CANARY"), "secret leaked in {output}");
        assert!(output.contains("request"));
        assert!(output.contains("job"));
        assert!(output.contains("process"));
        assert!(output.contains("https://example.com/video"));
    }
}
