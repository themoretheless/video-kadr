//! End-to-end controls for the local upload trust boundary.

mod support;

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use uuid::Uuid;

use support::{assert_api_error, get, make_state, router, send};
use video_kadr_backend::build_router;

fn post_multipart_file(filename: &str, bytes: &[u8]) -> Request<Body> {
    post_multipart_file_with_type(filename, "application/octet-stream", bytes)
}

fn post_multipart_file_with_type(
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> Request<Body> {
    let boundary = "BOUNDARY_UPLOAD_SECURITY";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri("/api/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn directory_is_empty(path: &std::path::Path) -> bool {
    let mut entries = tokio::fs::read_dir(path).await.unwrap();
    entries.next_entry().await.unwrap().is_none()
}

#[tokio::test]
async fn spoofed_extension_and_declared_mime_cannot_publish_non_media() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);
    let canary = b"<html><script>UPLOAD_CANARY_TOKEN</script></html>";

    let (status, body, text) = send(
        &app,
        post_multipart_file_with_type("payload.mp4", "video/mp4", canary),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(!text.contains("UPLOAD_CANARY_TOKEN"));
    assert!(directory_is_empty(&storage.path().join("staging")).await);
    assert!(directory_is_empty(&storage.path().join("sources")).await);
}

#[tokio::test]
async fn published_name_and_content_type_come_from_probe_not_client_name() {
    let fixture_dir = tempfile::tempdir().unwrap();
    let fixture = fixture_dir.path().join("fixture.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1",
            "-frames:v",
            "1",
            "-c:v",
            "mpeg4",
            "-y",
        ])
        .arg(&fixture)
        .status()
        .await;
    let status = match generated {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("could not generate upload fixture: {error}"),
    };
    assert!(status.success(), "ffmpeg could not generate upload fixture");

    let bytes = tokio::fs::read(&fixture).await.unwrap();
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let response = app
        .clone()
        .oneshot(post_multipart_file("payload.html", &bytes))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let uploaded: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let filename = uploaded["filename"].as_str().unwrap();
    let generated_id = filename.strip_suffix(".mp4").unwrap();
    assert!(Uuid::parse_str(generated_id).is_ok(), "got {filename}");

    let static_response = app
        .oneshot(get(uploaded["url"].as_str().unwrap()))
        .await
        .unwrap();
    assert_eq!(static_response.status(), StatusCode::OK);
    assert_eq!(
        static_response.headers()["x-content-type-options"],
        "nosniff"
    );
    assert_eq!(
        static_response.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert_eq!(static_response.headers()["content-type"], "video/mp4");
}

#[tokio::test]
async fn disconnected_upload_is_removed_from_private_staging() {
    let (state, storage) = make_state(true, true).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let boundary = "SLOW_DISCONNECT_BOUNDARY";
    let prefix = format!(
        "POST /api/upload HTTP/1.1\r\nHost: {address}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\nContent-Type: video/mp4\r\n\r\npartial"
    );
    stream.write_all(prefix.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let staging = storage.path().join("staging");
    let mut upload_started = false;
    for _ in 0..100 {
        if !directory_is_empty(&staging).await {
            upload_started = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(upload_started, "server never staged the upload");

    stream.shutdown().await.unwrap();
    for _ in 0..100 {
        if directory_is_empty(&staging).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(directory_is_empty(&staging).await);

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn staging_is_not_served_and_limits_fail_closed() {
    let (state, storage) = make_state(true, true).await;
    tokio::fs::write(storage.path().join("staging/private.upload"), b"secret")
        .await
        .unwrap();
    let (status, _, text) = send(&router(state), get("/files/staging/private.upload")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!text.contains("secret"));

    let (state, _storage) = make_state(true, true).await;
    let app = build_router(state, 64);
    let (status, body, _) = send(&app, post_multipart_file("clip.mp4", &[b'x'; 128])).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_api_error(&body, "payload_too_large");
}

#[tokio::test]
async fn upload_slots_limit_concurrency_and_recover() {
    let (state, _storage) = make_state(true, true).await;
    let slot_a = state.try_acquire_upload_slot().unwrap();
    let _slot_b = state.try_acquire_upload_slot().unwrap();
    let app = router(state);

    let (status, body, _) = send(&app, post_multipart_file("clip.mp4", b"data")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_api_error(&body, "too_many_requests");

    drop(slot_a);
    let (status, body, _) = send(&app, post_multipart_file("clip.mp4", b"")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}

#[tokio::test]
async fn empty_or_missing_file_fields_are_rejected() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let boundary = "BOUNDARY_EMPTY";
    let empty = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\n\r\n\r\n--{boundary}--\r\n"
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(empty))
        .unwrap();
    let (status, body, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");

    let boundary = "BOUNDARY_MISSING";
    let missing = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\ntext\r\n--{boundary}--\r\n"
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(missing))
        .unwrap();
    let (status, body, _) = send(&app, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}
