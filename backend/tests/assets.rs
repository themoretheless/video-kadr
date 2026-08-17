//! HTTP contract tests for private media assets.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};

use support::{assert_api_error, get, make_state, router, send};

const BOUNDARY: &str = "BOUNDARY_ASSET_UPLOAD";

fn png_bytes() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0_u8; 32]);
    bytes
}

fn otf_bytes() -> Vec<u8> {
    let mut bytes = b"OTTO".to_vec();
    bytes.extend_from_slice(&[0_u8; 64]);
    bytes
}

fn vtt_bytes() -> Vec<u8> {
    b"WEBVTT\n\n00:00.000 --> 00:02.000\n\xd0\x9f\xd1\x80\xd0\xb8\xd0\xb2\xd0\xb5\xd1\x82\n"
        .to_vec()
}

/// Minimal 8-bit mono PCM WAVE file, valid enough for ffprobe.
fn wav_bytes() -> Vec<u8> {
    let samples = vec![128_u8; 8_000];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&8_000_u32.to_le_bytes()); // sample rate
    bytes.extend_from_slice(&8_000_u32.to_le_bytes()); // byte rate
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&8_u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&samples);
    bytes
}

fn upload(kind: &str, filename: &str, bytes: &[u8]) -> Request<Body> {
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n\
         --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/assets")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

/// A rejected upload must publish nothing, whether or not the directory exists.
async fn is_empty_dir(path: &std::path::Path) -> bool {
    match tokio::fs::read_dir(path).await {
        Ok(mut entries) => entries
            .next_entry()
            .await
            .is_ok_and(|entry| entry.is_none()),
        Err(_) => true,
    }
}

async fn ffprobe_available() -> bool {
    tokio::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

#[tokio::test]
async fn upload_stores_each_kind_under_a_server_chosen_name() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state.clone());

    for (kind, client_name, bytes, extension, mime) in [
        ("image", "../../evil.svg", png_bytes(), "png", "image/png"),
        ("font", "Title.bin", otf_bytes(), "otf", "font/otf"),
        ("subtitle", "cues.txt", vtt_bytes(), "vtt", "text/vtt"),
    ] {
        let (status, body, _) = send(&app, upload(kind, client_name, &bytes)).await;
        assert_eq!(status, StatusCode::CREATED, "{kind}: {body}");
        let id = body["id"].as_str().unwrap();
        assert!(id.starts_with("ast_"), "{id}");
        assert_eq!(body["kind"], kind);
        assert_eq!(body["mime"], mime);
        assert_eq!(body["sizeBytes"], bytes.len() as u64);
        assert_eq!(body["filename"], format!("{id}.{extension}"));
        assert!(body["sha256"].as_str().is_some_and(|hash| hash.len() == 64));

        let stored = storage
            .path()
            .join("assets")
            .join(format!("{id}.{extension}"));
        assert_eq!(tokio::fs::read(&stored).await.unwrap(), bytes);

        let (get_status, fetched, _) = send(&app, get(&format!("/api/assets/{id}"))).await;
        assert_eq!(get_status, StatusCode::OK);
        assert_eq!(fetched["sha256"], body["sha256"]);
    }

    let (list_status, listed, _) = send(&app, get("/api/assets")).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(listed["assets"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn audio_uploads_are_confirmed_by_a_bounded_probe() {
    if !ffprobe_available().await {
        return;
    }
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);

    let (status, body, _) = send(&app, upload("audio", "bed.bin", &wav_bytes())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["kind"], "audio");
    assert_eq!(body["mime"], "audio/wav");
    assert!(body["duration"].as_f64().is_some_and(|value| value > 0.0));

    // The same bytes carry no video stream, so a video upload must fail closed.
    let (status, body, _) = send(&app, upload("video", "bed.mp4", &wav_bytes())).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_api_error(&body, "unsupported_media_type");
}

#[tokio::test]
async fn content_is_sniffed_instead_of_trusted_from_the_client() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);

    // HTML bytes named .png are not an image.
    let (status, body, _) = send(
        &app,
        upload("image", "logo.png", b"<html><body>hi</body></html>"),
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_api_error(&body, "unsupported_media_type");

    // A real PNG cannot be registered as a font either.
    let (status, body, _) = send(&app, upload("font", "logo.otf", &png_bytes())).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_api_error(&body, "unsupported_media_type");

    let (status, body, _) = send(&app, upload("nonsense", "x.png", &png_bytes())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");

    assert!(is_empty_dir(&storage.path().join("assets")).await);
    let mut staged = tokio::fs::read_dir(storage.path().join("staging"))
        .await
        .unwrap();
    assert!(staged.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn oversized_bodies_are_rejected_per_kind() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);

    let mut oversized = otf_bytes();
    oversized.resize(4 * 1024 * 1024 + 1, 0);
    let (status, body, _) = send(&app, upload("font", "huge.otf", &oversized)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_api_error(&body, "payload_too_large");

    assert!(is_empty_dir(&storage.path().join("assets")).await);
}

#[tokio::test]
async fn traversal_and_unknown_ids_are_not_found() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);

    for id in [
        "..%2f..%2fassets.json",
        "%2e%2e%2flibrary.json",
        "not-an-asset-id",
        "ast_0000000000000000",
    ] {
        let (status, body, _) = send(&app, get(&format!("/api/assets/{id}"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
        assert_api_error(&body, "not_found");

        let (status, _, _) = send(&app, delete(&format!("/api/assets/{id}"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
    }
}

#[tokio::test]
async fn delete_removes_the_record_and_the_file() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);
    let (_, uploaded, _) = send(&app, upload("image", "logo.png", &png_bytes())).await;
    let id = uploaded["id"].as_str().unwrap().to_owned();
    let filename = uploaded["filename"].as_str().unwrap().to_owned();

    let (status, _, _) = send(&app, delete(&format!("/api/assets/{id}"))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, delete(&format!("/api/assets/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        tokio::fs::metadata(storage.path().join("assets").join(&filename))
            .await
            .is_err()
    );
    let (_, listed, _) = send(&app, get("/api/assets")).await;
    assert!(listed["assets"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn static_serving_is_confined_to_the_assets_directory() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);
    let (_, uploaded, _) = send(&app, upload("image", "logo.png", &png_bytes())).await;
    let filename = uploaded["filename"].as_str().unwrap().to_owned();
    tokio::fs::write(storage.path().join("secret.txt"), b"private")
        .await
        .unwrap();

    let (status, _, body) = send(&app, get(&format!("/files/assets/{filename}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.starts_with('\u{fffd}') || !body.is_empty());

    for escape in [
        "/files/assets/../secret.txt",
        "/files/assets/..%2fsecret.txt",
        "/files/assets/%2e%2e%2fsecret.txt",
        "/files/assets/..%252fsecret.txt",
        "/files/assets/../assets.json",
    ] {
        let (status, _, text) = send(&app, get(escape)).await;
        assert_ne!(status, StatusCode::OK, "{escape} was served");
        assert!(!text.contains("private"), "{escape} leaked a private file");
    }
}
