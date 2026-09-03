//! End-to-end durable proxy API, isolated media serving, and deletion coverage.

mod support;

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use video_kadr_backend::library::MediaEntry;
use video_kadr_backend::model::JobStatus;
use video_kadr_backend::tools;

use support::{assert_api_error, make_state, router};

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn tools_and_h264_available(state: &video_kadr_backend::state::AppState) -> bool {
    if !tools::check_tool(&state.process_runtime, "ffmpeg", "-version")
        .await
        .0
        || !tools::check_tool(&state.process_runtime, "ffprobe", "-version")
            .await
            .0
    {
        return false;
    }
    tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .await
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("libx264")
        })
}

async fn generate_tiny_video(path: &std::path::Path) -> bool {
    tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x180:rate=12",
            "-t",
            "0.5",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-an",
            "-y",
        ])
        .arg(path)
        .status()
        .await
        .is_ok_and(|status| status.success())
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn poll_terminal(app: &axum::Router, job_id: &str) -> Value {
    for _ in 0..200 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/jobs/{job_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let job = response_json(response).await;
        if matches!(
            job["status"].as_str(),
            Some("done" | "error" | "cancelled" | "interrupted")
        ) {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("proxy job did not become terminal");
}

#[tokio::test]
async fn proxy_profile_is_strict_and_only_source_videos_are_accepted() {
    let (state, _directory) = make_state(false, false).await;
    let audio_id = "audio-source";
    let audio_filename = "audio-source.wav";
    tokio::fs::write(state.sources_dir().join(audio_filename), b"audio")
        .await
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: audio_id.into(),
                kind: "source".into(),
                filename: audio_filename.into(),
                storage_key: None,
                url: format!("/files/sources/{audio_filename}"),
                media_type: Some("audio".into()),
                title: None,
                duration: Some(1.0),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: Some("pcm_s16le".into()),
                size_bytes: Some(5),
                created_at: 1,
            })
            .await
    );
    let app = router(state);
    let invalid = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/library/audio-source/proxies",
            json!({
                "maxWidth": 960,
                "codec": "h264",
                "quality": 28,
                "includeAudio": false,
                "unexpected": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert_api_error(&response_json(invalid).await, "invalid_json");

    let audio = app
        .oneshot(json_request(
            "POST",
            "/api/library/audio-source/proxies",
            json!({
                "maxWidth": 960,
                "codec": "h264",
                "quality": 28,
                "includeAudio": false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::BAD_REQUEST);
    assert_api_error(&response_json(audio).await, "bad_request");
}

#[tokio::test]
async fn durable_proxy_round_trip_serves_only_media_with_range_and_security_headers() {
    let (state, _directory) = make_state(true, false).await;
    if !tools_and_h264_available(&state).await {
        eprintln!("skipping real proxy API test: ffmpeg/ffprobe/libx264 unavailable");
        return;
    }
    let source_id = "proxy-video-source";
    let source_filename = "proxy-video-source.mp4";
    let source_path = state.sources_dir().join(source_filename);
    if !generate_tiny_video(&source_path).await {
        eprintln!("skipping real proxy API test: failed to generate fixture");
        return;
    }
    let original_bytes = tokio::fs::read(&source_path).await.unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: source_id.into(),
                kind: "source".into(),
                filename: source_filename.into(),
                storage_key: None,
                url: format!("/files/sources/{source_filename}"),
                media_type: Some("video".into()),
                title: Some("Proxy source".into()),
                duration: Some(0.5),
                width: Some(320),
                height: Some(180),
                fps: Some(25.0),
                vcodec: Some("h264".into()),
                acodec: None,
                size_bytes: Some(original_bytes.len() as u64),
                created_at: 1,
            })
            .await
    );
    let app = router(state.clone());
    let create = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/library/{source_id}/proxies"),
            json!({
                "maxWidth": 160,
                "codec": "h264",
                "quality": 30,
                "includeAudio": false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::ACCEPTED);
    let create = response_json(create).await;
    let job_id = create["jobId"].as_str().unwrap();
    let key = create["key"].as_str().unwrap();
    assert_eq!(key.len(), 64);
    let terminal = poll_terminal(&app, job_id).await;
    assert_eq!(terminal["status"], "done", "{terminal}");
    assert_eq!(terminal["progress"], 100.0);
    assert_eq!(terminal["result"]["key"], key);
    assert!(terminal["result"].get("path").is_none());

    let listing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/library/{source_id}/proxies"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let listing = response_json(listing).await;
    assert_eq!(listing["status"], "ready");
    assert_eq!(listing["jobs"], json!([]));
    assert_eq!(listing["proxies"].as_array().unwrap().len(), 1);
    let proxy_url = listing["proxies"][0]["url"].as_str().unwrap();
    assert_eq!(listing["proxies"][0]["key"], key);
    assert!(!proxy_url.contains("manifest"));

    let media = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(proxy_url)
                .header(header::RANGE, "bytes=0-31")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(media.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(media.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        media.headers()["content-security-policy"],
        "sandbox; default-src 'none'"
    );
    assert!(media.headers().contains_key(header::CONTENT_RANGE));
    assert_eq!(to_bytes(media.into_body(), 64).await.unwrap().len(), 32);

    let private_manifest = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/files/proxies/{key}.json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(private_manifest.status(), StatusCode::NOT_FOUND);

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/library/{source_id}/proxies/{key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(tokio::fs::read(&source_path).await.unwrap(), original_bytes);
    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(proxy_url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let recreate = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/library/{source_id}/proxies"),
            json!({
                "maxWidth": 160,
                "codec": "h264",
                "quality": 30,
                "includeAudio": false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(recreate.status(), StatusCode::ACCEPTED);
    let recreated = response_json(recreate).await;
    assert_ne!(recreated["jobId"], job_id);
    let recreated_job = recreated["jobId"].as_str().unwrap();
    assert_eq!(poll_terminal(&app, recreated_job).await["status"], "done");

    let source_deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/library/{source_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(source_deleted.status(), StatusCode::NO_CONTENT);
    assert!(!source_path.exists());
    assert!(
        !state.storage.join("proxies").join("media").exists()
            || tokio::fs::read_dir(state.storage.join("proxies").join("media"))
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none()
    );
}

#[tokio::test]
async fn proxy_delete_response_is_linearizable_against_an_active_publisher() {
    let (state, _directory) = make_state(true, false).await;
    if !tools_and_h264_available(&state).await {
        eprintln!("skipping proxy DELETE race test: ffmpeg/ffprobe/libx264 unavailable");
        return;
    }
    let source_id = "proxy-delete-race";
    let source_filename = "proxy-delete-race.mp4";
    let source_path = state.sources_dir().join(source_filename);
    if !generate_tiny_video(&source_path).await {
        eprintln!("skipping proxy DELETE race test: failed to generate fixture");
        return;
    }
    let size = tokio::fs::metadata(&source_path).await.unwrap().len();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: source_id.into(),
                kind: "source".into(),
                filename: source_filename.into(),
                storage_key: None,
                url: format!("/files/sources/{source_filename}"),
                media_type: Some("video".into()),
                title: None,
                duration: Some(0.5),
                width: Some(320),
                height: Some(180),
                fps: Some(25.0),
                vcodec: Some("h264".into()),
                acodec: None,
                size_bytes: Some(size),
                created_at: 1,
            })
            .await
    );
    let app = router(state.clone());
    let create = app
        .clone()
        .oneshot(json_request(
            "POST",
            &format!("/api/library/{source_id}/proxies"),
            json!({
                "maxWidth": 160,
                "codec": "h264",
                "quality": 30,
                "includeAudio": false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::ACCEPTED);
    let create = response_json(create).await;
    let key = create["key"].as_str().unwrap();
    let job_id = create["jobId"].as_str().unwrap();

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/library/{source_id}/proxies/{key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(matches!(
        poll_terminal(&app, job_id).await["status"].as_str(),
        Some("done" | "cancelled")
    ));
    let listing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/library/{source_id}/proxies"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listing.status(), StatusCode::OK);
    let listing = response_json(listing).await;
    assert_eq!(listing["status"], "none", "{listing}");
    assert_eq!(listing["proxies"], json!([]));
    let media_dir = state.storage.join("proxies").join("media");
    assert!(
        !media_dir.exists()
            || tokio::fs::read_dir(media_dir)
                .await
                .unwrap()
                .next_entry()
                .await
                .unwrap()
                .is_none()
    );
}

#[tokio::test]
async fn proxy_routes_are_cors_preflight_enabled() {
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    for (uri, method) in [
        ("/api/library/source/proxies", "GET"),
        ("/api/library/source/proxies", "POST"),
        ("/api/library/source/proxies/key", "DELETE"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(uri)
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .split(',')
            .map(str::trim)
            .any(|allowed| allowed == method));
    }
}

#[test]
fn terminal_status_contract_still_includes_cancelled() {
    assert!(JobStatus::Cancelled.is_terminal());
}
