//! HTTP-level integration tests. They drive the real router via
//! `tower::ServiceExt::oneshot` (no socket bound) against an isolated temp
//! storage dir. Upload-specific threat regressions live in
//! `upload_security.rs` and share the same test harness.

mod support;

use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use video_editor_backend::analysis::proxy::{
    proxy_key, ProxyProfile, SourceIdentity, FFMPEG_PROXY_COMPATIBILITY,
};
use video_editor_backend::db::Db;
use video_editor_backend::domain::artifact_graph::Fingerprint;
use video_editor_backend::handlers::resume_pending_jobs;
use video_editor_backend::jobs::{EnqueueOutcome, JobKind, QueueLimits};
use video_editor_backend::library::{Library, MediaEntry};
use video_editor_backend::model::{EditRequest, Job, JobStatus};
use video_editor_backend::state::{AppState, ToolInfo};

use support::{assert_api_error, get, make_state, router, send};

fn post_empty(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn decode_jpeg_pixel(path: &std::path::Path, jpeg: &[u8]) -> [u8; 3] {
    tokio::fs::write(path, jpeg).await.unwrap();
    let decoded = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(decoded.status.success());
    decoded.stdout[..3].try_into().unwrap()
}

fn post_identity_lut() -> Request<Body> {
    const BOUNDARY: &str = "API_EDIT_LUT_BRIDGE";
    const IDENTITY_CUBE: &str = "LUT_3D_SIZE 2\n\
0 0 0\n\
1 0 0\n\
0 1 0\n\
1 1 0\n\
0 0 1\n\
1 0 1\n\
0 1 1\n\
1 1 1\n";
    let body = format!(
        "--{BOUNDARY}\r\n\
Content-Disposition: form-data; name=\"file\"; filename=\"identity.cube\"\r\n\
Content-Type: application/octet-stream\r\n\r\n\
{IDENTITY_CUBE}\r\n\
--{BOUNDARY}--\r\n"
    );
    Request::builder()
        .method("POST")
        .uri("/api/luts")
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

fn patch_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Poll a job until it reaches a terminal state. The background worker runs on
/// the test runtime; the sleeps give it slots to make progress.
async fn poll_terminal(app: &Router, id: &str) -> Value {
    for _ in 0..400 {
        let (status, body, _) = send(app, get(&format!("/api/jobs/{id}"))).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "job {id} should exist while polling"
        );
        match body["status"].as_str().unwrap_or("") {
            "done" | "error" | "cancelled" => return body,
            _ => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
    panic!("job {id} never reached a terminal state");
}

#[tokio::test]
async fn health_reflects_tool_availability() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["ffmpeg"], true);
    assert_eq!(body["ytdlp"], true);

    let (state2, _d2) = make_state(false, false).await;
    let app2 = router(state2);
    let (_s, body2, _) = send(&app2, get("/api/health")).await;
    assert_eq!(body2["status"], "degraded");
    assert_eq!(body2["ffmpeg"], false);
}

#[tokio::test]
async fn derived_graph_api_persists_dag_priority_and_cancel() {
    let (state, _dir) = make_state(false, false).await;
    let app = router(state.clone());
    let source_fingerprint = Fingerprint::digest(b"derived-source");
    let proxy_profile = ProxyProfile::default();
    let proxy_artifact_key = proxy_key(
        &SourceIdentity {
            id: "source".into(),
            original_path: state.storage.join("unused.mp4"),
            duration_seconds: 1.0,
            fingerprint: source_fingerprint.clone(),
        },
        &proxy_profile,
        FFMPEG_PROXY_COMPATIBILITY,
    )
    .to_string();
    let graph = json!({
        "projectId": "project-a",
        "tasks": [
            { "key": "probe", "artifactKey": "probe-content-v1", "kind": "probe", "payload": {"sourceId":"source"}, "dependencies": [], "priority": 0 },
            { "key": "proxy", "artifactKey": proxy_artifact_key, "kind": "proxy", "payload": {"sourceId":"source", "sourceFingerprint": source_fingerprint, "profile": proxy_profile}, "dependencies": ["probe"], "priority": -10 }
        ]
    });
    let (status, body, _) = send(&app, post_json("/api/derived-graphs", graph)).await;
    assert_eq!(status, StatusCode::OK);
    let graph_id = body["graphId"].as_str().unwrap();
    let probe = body["tasks"]["probe"].as_str().unwrap();
    let proxy = body["tasks"]["proxy"].as_str().unwrap();

    let (status, tasks, _) = send(&app, get(&format!("/api/derived-graphs/{graph_id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks.as_array().unwrap().len(), 2);
    let (status, _, _) = send(
        &app,
        patch_json(
            &format!("/api/derived-jobs/{probe}/priority"),
            json!({"priority": 10, "expectedRevision": 0}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, conflict, _) = send(
        &app,
        patch_json(
            &format!("/api/derived-jobs/{probe}/priority"),
            json!({"priority": 20, "expectedRevision": 0}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&conflict, "conflict");
    let (status, _, _) = send(
        &app,
        post_empty(&format!("/api/derived-jobs/{probe}/cancel")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, tasks, _) = send(&app, get("/api/derived-jobs?projectId=project-a")).await;
    let proxy_state = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["taskId"] == proxy)
        .unwrap()["state"]
        .as_str();
    assert_eq!(proxy_state, Some("blocked"));
}

#[tokio::test]
async fn derived_shared_task_cancel_detaches_each_project_through_api() {
    let (state, _dir) = make_state(false, false).await;
    let app = router(state);
    let graph = |project: &str| {
        json!({
            "projectId": project,
            "tasks": [{ "key": "probe", "artifactKey": "shared-probe-v1", "kind": "probe",
                "payload": {"sourceId":"source"}, "dependencies": [], "priority": 0 }]
        })
    };
    let (_, first, _) = send(&app, post_json("/api/derived-graphs", graph("project-a"))).await;
    let (_, second, _) = send(&app, post_json("/api/derived-graphs", graph("project-b"))).await;
    let task_id = first["tasks"]["probe"].as_str().unwrap();
    assert_eq!(second["tasks"]["probe"], task_id);

    let (status, body, _) = send(
        &app,
        post_empty(&format!(
            "/api/derived-jobs/{task_id}/cancel?projectId=project-a"
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancelled"], 0);
    let (_, tasks, _) = send(&app, get("/api/derived-jobs")).await;
    assert_eq!(tasks[0]["consumerProjectIds"], json!(["project-b"]));

    let (status, _, _) = send(
        &app,
        post_empty(&format!(
            "/api/derived-jobs/{task_id}/cancel?projectId=project-b"
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, tasks, _) = send(&app, get("/api/derived-jobs")).await;
    assert_eq!(tasks[0]["state"], "cancelled");
    assert_eq!(tasks[0]["consumerProjectIds"], json!([]));
}

#[tokio::test]
async fn proxy_catalog_rejects_untrusted_keys_before_filesystem_resolution() {
    let (state, _dir) = make_state(false, false).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/proxies/../../app.db/status")).await;
    assert!(matches!(
        status,
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
    ));
    assert!(body.get("path").is_none());

    let (status, body, _) = send(&app, get("/api/proxies/not-a-fingerprint/status")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}

#[tokio::test]
async fn proxy_catalog_serves_only_a_checksum_verified_ready_artifact_with_range() {
    let (state, _dir) = make_state(false, false).await;
    let bytes = b"verified proxy bytes";
    let source_fingerprint = Fingerprint::digest(b"source").to_string();
    tokio::fs::create_dir_all(state.storage.join("sources"))
        .await
        .unwrap();
    tokio::fs::write(state.storage.join("sources/source.mp4"), b"source")
        .await
        .unwrap();
    assert!(state
        .library
        .add(video_editor_backend::library::MediaEntry::from_result(
            "source",
            &json!({"id":"source","filename":"source.mp4","url":"/media/source.mp4","fingerprint":source_fingerprint.clone()}),
        ))
        .await);
    let key = proxy_key(
        &SourceIdentity {
            id: "source".into(),
            original_path: state.storage.join("unused-source.mp4"),
            duration_seconds: 10.0,
            fingerprint: Fingerprint::parse(&source_fingerprint).unwrap(),
        },
        &ProxyProfile::default(),
        FFMPEG_PROXY_COMPATIBILITY,
    )
    .to_string();
    let relative = format!("proxies/{key}.mp4");
    tokio::fs::create_dir_all(state.storage.join("proxies"))
        .await
        .unwrap();
    tokio::fs::write(state.storage.join(&relative), bytes)
        .await
        .unwrap();
    let clock = json!({"timeBase":{"numerator":1,"denominator":90000},"durationTicks":900000,"startTicks":0});
    let media = json!({
        "clock": clock, "codedWidth": 640, "codedHeight": 360,
        "displayWidth": 640, "displayHeight": 360, "videoCodec":"h264",
        "frameRate":{"numerator":30,"denominator":1}, "audioCodec":"aac",
        "audioSampleRate":48000, "audioChannels":2,
        "colorManagement": {
            "status":"supported", "provenance":"signaled",
            "descriptor":{"primaries":"bt709","transfer":"bt709","matrix":"bt709","range":"limited","pixelModel":"yuv","chromaLocation":"left"}
        }
    });
    let manifest = json!({
        "schemaVersion":4, "key":key, "sourceId":"source", "sourceFingerprint":source_fingerprint,
        "profile":{"maxWidth":960,"codec":"h264","quality":28,"includeAudio":true},
        "producerCompatibility":FFMPEG_PROXY_COMPATIBILITY, "sourceMedia":media, "proxyMedia":media,
        "file":{"path":relative,"size":bytes.len(),"sha256":Fingerprint::digest(bytes)}
    });
    tokio::fs::write(
        state.storage.join(format!("proxies/{key}.json")),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .await
    .unwrap();
    let app = router(state);
    let (status, body, _) = send(
        &app,
        get(&format!("/api/proxies/{key}/status?sourceId=source")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "ready");
    assert_eq!(
        body["previewUrl"],
        format!("/api/proxies/{key}/preview?sourceId=source")
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/proxies/{key}/preview?sourceId=source"))
                .header("range", "bytes=0-7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers()["content-range"],
        format!("bytes 0-7/{}", bytes.len())
    );
    assert_eq!(
        &to_bytes(response.into_body(), 1024).await.unwrap()[..],
        b"verified"
    );

    let (status, body, _) = send(&app, post_empty(&format!("/api/proxies/{key}/invalidate"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["regenerationQueued"], false);
    let (status, _, _) = send(
        &app,
        get(&format!("/api/proxies/{key}/status?sourceId=source")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn optimized_preview_frame_renders_once_and_reuses_content_key() {
    let (state, _dir) = make_state(true, false).await;
    let source = state.storage.join("sources/preview-source.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x36:r=1",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&source)
        .status()
        .await;
    if !generated.is_ok_and(|status| status.success()) {
        return; // Capability-gated on developer/CI images without FFmpeg.
    }
    let app = router(state.clone());
    let request = || {
        post_json(
            "/api/preview-frames",
            json!({
                "edit": { "videoId": "preview-source" },
                "timelineTick": 0, "timelineTimeBase": 1_000_000,
                "width": 64, "height": 36, "quality": 82
            }),
        )
    };
    let first = app.clone().oneshot(request()).await.unwrap();
    if first.status() != StatusCode::OK {
        let status = first.status();
        let error = to_bytes(first.into_body(), 4096).await.unwrap();
        panic!(
            "preview response {status}: {}",
            String::from_utf8_lossy(&error)
        );
    }
    assert_eq!(first.headers()["content-type"], "image/jpeg");
    let first_key = first.headers()["x-preview-cache-key"].clone();
    let first_bytes = to_bytes(first.into_body(), 2 * 1024 * 1024).await.unwrap();
    assert!(first_bytes.starts_with(&[0xff, 0xd8]));
    let second = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.headers()["x-preview-cache-key"], first_key);
    let second_bytes = to_bytes(second.into_body(), 2 * 1024 * 1024).await.unwrap();
    assert_eq!(first_bytes, second_bytes);
    let shard = state
        .storage
        .join("frames")
        .join(&first_key.to_str().unwrap()[..2])
        .join(first_key.to_str().unwrap());
    let mut entries = tokio::fs::read_dir(shard).await.unwrap();
    let mut published = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        published.push(entry.file_name().to_string_lossy().into_owned());
    }
    assert_eq!(
        published.len(),
        2,
        "one JPEG and one manifest should be published: {published:?}"
    );
}

#[tokio::test]
async fn optimized_preview_frame_trim_and_speed_use_the_edited_output_clock() {
    let (state, _dir) = make_state(true, false).await;
    let source = state.storage.join("sources/preview-timeline.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=green:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=white:s=64x36:r=10:d=1"])
        .args([
            "-filter_complex",
            "[0:v][1:v][2:v][3:v]concat=n=4:v=1:a=0[v]",
            "-map",
            "[v]",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&source)
        .status()
        .await;
    if !generated.is_ok_and(|status| status.success()) {
        return;
    }

    // Raw source t=2.2 lies 1.2 s into trim [1,4]. At speed=2 the same
    // semantic frame is output t=0.6. The blue segment makes the mapping a
    // stable, visually distinguishable golden fixture.
    let response = router(state.clone())
        .oneshot(post_json(
            "/api/preview-frames",
            json!({
                "edit": {
                    "videoId": "preview-timeline",
                    "trim": {"start": 1.0, "end": 4.0},
                    "speed": 2.0
                },
                "timelineTick": 600_000,
                "timelineTimeBase": 1_000_000,
                "width": 64,
                "height": 36,
                "quality": 95
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let jpeg = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let frame_path = state.storage.join("preview-timeline-golden.jpg");
    tokio::fs::write(&frame_path, jpeg).await.unwrap();
    let decoded = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(&frame_path)
        .args([
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(decoded.status.success());
    let pixel = &decoded.stdout[..3];
    assert!(
        pixel[2] > pixel[0].saturating_add(80) && pixel[2] > pixel[1].saturating_add(80),
        "expected the edited-output tick to resolve to the blue source segment, got {pixel:?}"
    );
}

#[tokio::test]
async fn optimized_preview_frame_preserves_fades_on_the_full_output_clock() {
    let (state, _dir) = make_state(true, false).await;
    let source = state.storage.join("sources/preview-fade.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg("color=c=white:s=64x36:r=10:d=4")
        .args(["-pix_fmt", "yuv420p"])
        .arg(&source)
        .status()
        .await;
    if !generated.is_ok_and(|status| status.success()) {
        return;
    }
    let request = |tick| {
        post_json(
            "/api/preview-frames",
            json!({
                "edit": { "videoId": "preview-fade", "fadeIn": 1.0, "fadeOut": 1.0 },
                "timelineTick": tick, "timelineTimeBase": 1_000_000,
                "width": 64, "height": 36, "quality": 95
            }),
        )
    };
    let middle = router(state.clone())
        .oneshot(request(2_000_000))
        .await
        .unwrap();
    assert_eq!(middle.status(), StatusCode::OK);
    let middle = to_bytes(middle.into_body(), 2 * 1024 * 1024).await.unwrap();
    let middle = decode_jpeg_pixel(&state.storage.join("fade-mid.jpg"), &middle).await;
    let ending = router(state.clone())
        .oneshot(request(3_800_000))
        .await
        .unwrap();
    assert_eq!(ending.status(), StatusCode::OK);
    let ending = to_bytes(ending.into_body(), 2 * 1024 * 1024).await.unwrap();
    let ending = decode_jpeg_pixel(&state.storage.join("fade-end.jpg"), &ending).await;
    assert!(
        middle[0] > 220,
        "mid-fade frame should be white: {middle:?}"
    );
    assert!(ending[0] < 100, "fade-out frame should be dark: {ending:?}");
}

#[tokio::test]
async fn optimized_preview_concat_boundary_is_next_segment_and_eof_is_rejected() {
    let (state, _dir) = make_state(true, false).await;
    let source = state.storage.join("sources/preview-boundary.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=black:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=64x36:r=10:d=1"])
        .args([
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map",
            "[v]",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&source)
        .status()
        .await;
    if !generated.is_ok_and(|status| status.success()) {
        return;
    }
    let body = |tick| {
        post_json(
            "/api/preview-frames",
            json!({
                "edit": { "videoId": "preview-boundary", "segments": [
                    {"start": 0.0, "end": 1.0}, {"start": 2.0, "end": 3.0}
                ]},
                "timelineTick": tick, "timelineTimeBase": 1_000_000,
                "width": 64, "height": 36, "quality": 95
            }),
        )
    };
    let boundary = router(state.clone())
        .oneshot(body(1_000_000))
        .await
        .unwrap();
    assert_eq!(boundary.status(), StatusCode::OK);
    let jpeg = to_bytes(boundary.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let pixel = decode_jpeg_pixel(&state.storage.join("concat-boundary.jpg"), &jpeg).await;
    assert!(
        pixel[2] > pixel[0].saturating_add(80),
        "boundary must select next blue segment: {pixel:?}"
    );
    let eof = router(state).oneshot(body(2_000_000)).await.unwrap();
    assert_eq!(eof.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn optimized_preview_applies_export_fps_clock_before_frame_selection() {
    let (state, _dir) = make_state(true, false).await;
    let source = state.storage.join("sources/preview-fps.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=64x36:r=10:d=1"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=64x36:r=10:d=1"])
        .args([
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0[v]",
            "-map",
            "[v]",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&source)
        .status()
        .await;
    if !generated.is_ok_and(|status| status.success()) {
        return;
    }
    // CFR 1 fps has frames at edited PTS 0 and 1. Selection at 0.6 therefore
    // resolves to the blue frame at PTS 1, not the red 10-fps source frame.
    let response = router(state.clone())
        .oneshot(post_json(
            "/api/preview-frames",
            json!({
                "edit": { "videoId": "preview-fps", "fps": 1.0 },
                "timelineTick": 600_000, "timelineTimeBase": 1_000_000,
                "width": 64, "height": 36, "quality": 95
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let jpeg = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let pixel = decode_jpeg_pixel(&state.storage.join("fps-clock.jpg"), &jpeg).await;
    assert!(
        pixel[2] > pixel[0].saturating_add(80),
        "1-fps export clock must select the blue frame at PTS 1: {pixel:?}"
    );
}

#[tokio::test]
async fn project_archive_round_trip_reports_missing_media_and_rejects_corruption() {
    let (state, _dir) = make_state(true, false).await;
    let document = video_editor_backend::domain::project::ProjectDocument::from_legacy(
        "portable",
        "missing-source",
        json!({"id":"missing-source","duration":1.0,"mediaKind":"video"}),
        json!({}),
    )
    .unwrap();
    state
        .db
        .cas_upsert_project_document("archive-source", 0, &document)
        .await
        .unwrap();
    let app = router(state.clone());
    let exported = app
        .clone()
        .oneshot(get("/api/projects/archive-source/archive"))
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    assert_eq!(
        exported.headers()["content-type"],
        "application/vnd.vkadr.project"
    );
    let archive = to_bytes(exported.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let imported = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project-archives/import")
                .header("content-type", "application/vnd.vkadr.project")
                .body(Body::from(archive.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);
    let imported: Value =
        serde_json::from_slice(&to_bytes(imported.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_ne!(imported["projectId"], "archive-source");
    assert_eq!(imported["revision"], 1);
    assert_eq!(imported["missingMedia"], json!(["missing-source"]));

    let mut corrupt = archive.to_vec();
    *corrupt.last_mut().unwrap() ^= 1;
    let rejected = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project-archives/import")
                .body(Body::from(corrupt))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let mut oversized_manifest = b"VKADRv1\n".to_vec();
    oversized_manifest.extend_from_slice(&(16_u32 * 1024 * 1024 + 1).to_be_bytes());
    let rejected = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/project-archives/import")
                .body(Body::from(oversized_manifest))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
}

#[cfg(unix)]
#[tokio::test]
async fn project_archive_original_export_rejects_symlink_source() {
    use std::os::unix::fs::symlink;
    let (state, _dir) = make_state(true, false).await;
    let secret = state.storage.join("secret.mp4");
    tokio::fs::write(&secret, b"not archive media")
        .await
        .unwrap();
    let filename = "linked.mp4";
    symlink(&secret, state.storage.join("sources").join(filename)).unwrap();
    assert!(
        state
            .library
            .add(video_editor_backend::library::MediaEntry {
                id: "linked-asset".into(),
                kind: "source".into(),
                filename: filename.into(),
                url: format!("/files/sources/{filename}"),
                title: None,
                duration: Some(1.0),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: None,
                media_kind: Some("video".into()),
                size_bytes: Some(17),
                fingerprint: None,
                color_management: None,
                created_at: video_editor_backend::library::now_secs(),
            })
            .await
    );
    let mut document = video_editor_backend::domain::project::ProjectDocument::from_legacy(
        "linked",
        "media",
        json!({"id":"media","duration":1,"mediaKind":"video"}),
        json!({}),
    )
    .unwrap();
    document.media[0].asset_ref = Some("linked-asset".into());
    state
        .db
        .cas_upsert_project_document("linked-project", 0, &document)
        .await
        .unwrap();
    let response = router(state)
        .oneshot(get(
            "/api/projects/linked-project/archive?originalMedia=true",
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn capabilities_report_runtime_availability_and_fingerprint() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/capabilities")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["schemaVersion"], 1);
    assert_eq!(body["toolFingerprint"].as_str().unwrap().len(), 16);
    assert_eq!(body["formats"][0]["id"], "mp4");
    assert_eq!(body["formats"][0]["available"], true);
    assert!(body["hardware"].as_array().is_some());

    let (missing_state, _d) = make_state(false, false).await;
    let (_, missing, _) = send(&router(missing_state), get("/api/capabilities")).await;
    assert_eq!(missing["formats"][0]["available"], false);
    assert!(missing["formats"][0]["reason"].as_str().is_some());
}

#[tokio::test]
async fn request_id_is_validated_and_propagated() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);

    let request = Request::builder()
        .uri("/api/health?token=CANARY")
        .header("x-request-id", "client-trace_1")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.headers()["x-request-id"], "client-trace_1");

    let invalid = Request::builder()
        .uri("/api/health")
        .header("x-request-id", "unsafe id with spaces")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(invalid).await.unwrap();
    let generated = response.headers()["x-request-id"].to_str().unwrap();
    assert_ne!(generated, "unsafe id with spaces");
    assert_eq!(generated.len(), 36);

    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/health")
        .header("origin", "http://localhost:5173")
        .header("access-control-request-method", "GET")
        .header("access-control-request-headers", "x-request-id")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(preflight).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .contains("x-request-id"));

    let cors_get = Request::builder()
        .uri("/api/health")
        .header("origin", "http://localhost:5173")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(cors_get).await.unwrap();
    assert!(response.headers()["access-control-expose-headers"]
        .to_str()
        .unwrap()
        .contains("x-request-id"));
}

#[tokio::test]
async fn unknown_job_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/jobs/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn duplicate_import_reuses_job_and_failed_actions_are_audited() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state.clone());
    let request = json!({ "url": "http://127.0.0.1/private?token=CANARY" });

    let (first_status, first, _) = send(&app, post_json("/api/import", request.clone())).await;
    let (second_status, second, _) = send(&app, post_json("/api/import", request)).await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first["jobId"], second["jobId"]);
    let id = first["jobId"].as_str().unwrap();
    assert_eq!(poll_terminal(&app, id).await["status"], "error");

    let (status, failed, _) = send(&app, get("/api/jobs/failed")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(failed["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(failed["jobs"][0]["jobId"], id);
    assert_eq!(failed["jobs"][0]["errorKind"], "security");
    assert!(!failed.to_string().contains("CANARY"));

    let (status, registry, _) = send(&app, get("/api/jobs/registry")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(registry["counts"]["failed"], 1);

    let (status, retry, _) = send(&app, post_empty(&format!("/api/jobs/{id}/retry"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&retry, "conflict");

    let (status, discarded, _) = send(&app, post_empty(&format!("/api/jobs/{id}/discard"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(discarded["status"], "discarded");
    assert_eq!(state.job_store.operator_action_count(id).await.unwrap(), 1);
    let (_, failed, _) = send(&app, get("/api/jobs/failed")).await;
    assert!(failed["jobs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn media_search_endpoint_uses_rebuildable_sqlite_index() {
    let (state, _d) = make_state(true, true).await;
    tokio::fs::write(state.sources_dir().join("interview.mp4"), b"media")
        .await
        .unwrap();
    let entry = MediaEntry::from_result(
        "source",
        &json!({
            "id": "interview",
            "title": "Summer interview",
            "filename": "interview.mp4",
            "url": "/files/sources/interview.mp4"
        }),
    );
    assert!(state.library.add(entry).await);
    state.rebuild_media_search().await;
    let app = router(state);

    let (status, hits, _) = send(&app, get("/api/library/search?q=interv&limit=10")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hits[0]["id"], "interview");
    assert_eq!(hits[0]["title"], "Summer interview");

    let (status, empty, _) = send(&app, get("/api/library/search?q=%22%20OR%20%2A")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_json_and_routing_errors_use_the_api_envelope() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);

    let invalid_json = Request::builder()
        .method("POST")
        .uri("/api/import")
        .header("content-type", "application/json")
        .body(Body::from("{"))
        .unwrap();
    let (status, body, _) = send(&app, invalid_json).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");

    let (status, body, _) = send(&app, delete("/api/import")).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_api_error(&body, "method_not_allowed");

    let (status, body, _) = send(&app, get("/api/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn wire_dtos_reject_unknown_fields_but_project_documents_remain_open() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/import",
            json!({ "url": "https://example.com/video", "urll": "typo" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({ "schemaVersion": 2, "videoId": "clip" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "clip",
                "crop": { "x": 0, "y": 0, "w": 10, "h": 10, "width": 10 }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/projects",
            json!({
                "videoId": "clip",
                "video": { "filename": "clip.mp4", "futureVideoField": true },
                "edit": { "futureEffect": { "amount": 0.5 } }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["edit"]["futureEffect"]["amount"], 0.5);

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/projects",
            json!({
                "videoId": "clip",
                "video": {},
                "edit": {},
                "videoo": {}
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");
}

#[tokio::test]
async fn import_accepts_and_returns_job_id() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(
        &app,
        post_json("/api/import", json!({ "url": "https://example.com/v.mp4" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["jobId"].as_str().is_some());
}

#[tokio::test]
async fn import_rejects_unsafe_url_via_job_error() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    // localhost is blocked by the SSRF guard before any download is attempted.
    let (_s, body, _) = send(
        &app,
        post_json("/api/import", json!({ "url": "http://localhost/secret" })),
    )
    .await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "error");
    assert_eq!(job["error"], "Недопустимый URL");
}

#[tokio::test]
async fn import_url_validation_error_survives_restart() {
    let (state, dir) = make_state(true, true).await;
    let storage = dir.path().to_path_buf();
    let app = router(state);

    let (_s, body, _) = send(
        &app,
        post_json("/api/import", json!({ "url": "http://localhost/secret" })),
    )
    .await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "error");

    let db2 = Db::open(&storage).await.unwrap();
    let lib2 = Library::load(storage.clone()).await;
    let st2 = AppState::new(storage, 2, ToolInfo::default(), lib2, db2);
    st2.recover_jobs().await;
    let app2 = router(st2);
    let (status, recovered, _) = send(&app2, get(&format!("/api/jobs/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recovered["status"], "error");
    assert_eq!(recovered["error"], "Недопустимый URL");
}

#[tokio::test]
async fn edit_with_missing_source_fails_job() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (_s, body, _) = send(
        &app,
        post_json("/api/edit", json!({ "videoId": "no-such-id" })),
    )
    .await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "error");
    assert!(
        job["error"].as_str().unwrap().contains("no-such-id"),
        "error should name the missing source: {}",
        job["error"]
    );
}

#[tokio::test]
async fn color_grade_path_like_lut_id_is_rejected_before_enqueue() {
    let (state, storage) = make_state(true, true).await;
    let outside = storage.path().join("outside.cube");
    tokio::fs::write(&outside, b"path-canary").await.unwrap();
    let app = router(state);

    let (status, queued, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "missing-source-behind-lut",
                "lut": { "id": "../outside", "intensity": 1.0 }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&queued, "bad_request");
    assert!(queued.get("jobId").is_none());
    assert_eq!(tokio::fs::read(outside).await.unwrap(), b"path-canary");
}

#[tokio::test]
async fn color_grade_unknown_well_formed_lut_fails_before_source_lookup() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let (status, queued, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "missing-source-behind-lut",
                "lut": {
                    "id": "00000000-0000-4000-8000-000000000000",
                    "intensity": 1.0
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = queued["jobId"].as_str().expect("edit should be enqueued");
    let job = poll_terminal(&app, id).await;
    assert_eq!(job["status"], "error");
    assert_eq!(job["error"], "LUT не найден");
}

#[tokio::test]
async fn color_grade_zero_intensity_lut_bypasses_asset_resolution() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let source_id = "missing-source-after-zero-lut";

    let (status, queued, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": source_id,
                "lut": { "id": "missing-lut", "intensity": 0.0 }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = queued["jobId"].as_str().expect("edit should be enqueued");
    let job = poll_terminal(&app, id).await;
    assert_eq!(job["status"], "error");
    assert!(job["error"].as_str().unwrap().contains(source_id));
    assert!(!job["error"].as_str().unwrap().contains("LUT"));

    let (plain_status, plain, _) = send(
        &app,
        post_json("/api/edit", json!({ "videoId": source_id })),
    )
    .await;
    assert_eq!(plain_status, StatusCode::OK);
    assert_eq!(plain["jobId"], queued["jobId"]);
}

#[tokio::test]
async fn mp3_canonicalizes_video_grading_before_validation_and_dedupe() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let (first_status, first, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "missing-audio-source",
                "format": "mp3",
                "temperature": 2.0,
                "tint": -2.0,
                "highlights": 2.0,
                "shadows": -2.0,
                "colorWheels": {
                    "lift": {"master": 2.0},
                    "gamma": {"red": -2.0},
                    "gain": {"blue": 2.0}
                },
                "hslSelective": {
                    "selection": {"centerDegrees": 999, "halfWidthDegrees": 999, "featherDegrees": 999},
                    "adjustment": {"hueDegrees": 999, "saturation": 999, "lightness": -999}
                },
                "lut": { "id": "../ignored.cube", "intensity": 2.0 },
                "curves": {
                    "master": (0..17).map(|index| {
                        let value = index as f64 / 16.0;
                        json!({ "x": value, "y": value })
                    }).collect::<Vec<_>>()
                }
            }),
        ),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);

    let (second_status, second, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({ "videoId": "missing-audio-source", "format": "mp3" }),
        ),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(first["jobId"], second["jobId"]);
}

#[tokio::test]
async fn color_grade_invalid_curve_wire_payload_is_rejected_before_enqueue() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "curve-source",
                "curves": {
                    "red": [
                        { "x": 0.0, "y": 0.0 },
                        { "x": 1.0, "y": 1.0, "unexpected": true }
                    ]
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");
    assert!(body.get("jobId").is_none());
}

#[tokio::test]
async fn selective_hsl_fails_closed_when_geq_and_format_are_unavailable() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId":"selective-source",
                "hslSelective":{
                    "selection":{"centerDegrees":0,"halfWidthDegrees":30,"featherDegrees":15},
                    "adjustment":{"hueDegrees":10}
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(body["error"].as_str().unwrap().contains("geq"));
    assert!(body.get("jobId").is_none());
}

#[tokio::test]
async fn selective_hsl_invalid_ranges_are_rejected_before_enqueue() {
    let (mut state, _storage) = make_state(true, true).await;
    Arc::make_mut(&mut state.tools).ffmpeg_filters = ["geq", "format", "colorspace", "setparams"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let app = router(state);
    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId":"selective-source",
                "hslSelective":{
                    "selection":{"centerDegrees":0,"halfWidthDegrees":170,"featherDegrees":11},
                    "adjustment":{"hueDegrees":0}
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(body.get("jobId").is_none());
}

#[tokio::test]
async fn selective_hsl_canonical_identity_precedes_dedupe_and_cache_keys() {
    let (mut state, _storage) = make_state(true, true).await;
    Arc::make_mut(&mut state.tools).ffmpeg_filters = ["geq", "format", "colorspace", "setparams"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let app = router(state);
    let enqueue = |payload| send(&app, post_json("/api/edit", payload));

    let (_, absent, _) = enqueue(json!({ "videoId": "missing-hsl-source" })).await;
    let (_, neutral, _) = enqueue(json!({
        "videoId": "missing-hsl-source",
        "hslSelective": {
            "selection": {"centerDegrees": 240, "halfWidthDegrees": 5, "featherDegrees": 2},
            "adjustment": {"hueDegrees": -0.0, "saturation": 0.0, "lightness": 0.0}
        }
    }))
    .await;
    assert_eq!(absent["jobId"], neutral["jobId"]);

    let (_, wrapped, _) = enqueue(json!({
        "videoId": "missing-hsl-source",
        "hslSelective": {
            "selection": {"centerDegrees": 360, "halfWidthDegrees": 30, "featherDegrees": 15},
            "adjustment": {"hueDegrees": 1}
        }
    }))
    .await;
    let (_, zero, _) = enqueue(json!({
        "videoId": "missing-hsl-source",
        "hslSelective": {
            "selection": {"centerDegrees": 0, "halfWidthDegrees": 30, "featherDegrees": 15},
            "adjustment": {"hueDegrees": 1}
        }
    }))
    .await;
    assert_eq!(wrapped["jobId"], zero["jobId"]);
    assert_ne!(absent["jobId"], wrapped["jobId"]);
}

#[tokio::test]
async fn color_grade_curve_cardinality_is_rejected_before_enqueue() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let points: Vec<_> = (0..17)
        .map(|index| {
            let value = index as f64 / 16.0;
            json!({ "x": value, "y": value })
        })
        .collect();

    let (status, body, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "curve-source",
                "curves": { "master": points }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(body.get("jobId").is_none());
}

#[tokio::test]
async fn color_grade_uploaded_lut_is_resolved_before_the_source_lookup() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let (upload_status, uploaded, _) = send(&app, post_identity_lut()).await;
    assert_eq!(upload_status, StatusCode::CREATED);
    let lut_id = uploaded["id"].as_str().expect("uploaded LUT id");
    let source_id = "missing-source-after-resolved-lut";

    let (status, queued, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": source_id,
                "lut": { "id": lut_id, "intensity": 1.0 }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = queued["jobId"].as_str().expect("edit should be enqueued");
    let job = poll_terminal(&app, id).await;
    assert_eq!(job["status"], "error");
    assert!(job["error"].as_str().unwrap().contains(source_id));
    assert!(!job["error"].as_str().unwrap().contains("LUT"));
}

#[tokio::test]
async fn edit_cache_hit_returns_existing_output() {
    let (state, _d) = make_state(true, true).await;
    // Pre-seed the render cache for a specific edit, with its output file present.
    let req_json = json!({ "videoId": "vidX", "trim": { "start": 0.0, "end": 5.0 } });
    let req: EditRequest = serde_json::from_value(req_json.clone()).unwrap();
    let key =
        video_editor_backend::handlers::render_cache_key_for_tools(&req, state.tools.as_ref());
    let filename = "cached.mp4";
    tokio::fs::write(state.outputs_dir().join(filename), b"x")
        .await
        .unwrap();
    state
        .db
        .cache_put(
            &key,
            &json!({
                "id": "cached",
                "url": format!("/files/outputs/{filename}"),
                "filename": filename,
                "sizeBytes": 1
            }),
            filename,
        )
        .await
        .unwrap();

    // The identical request must resolve from cache (no ffmpeg) to a done job.
    let app = router(state);
    let (_s, body, _) = send(&app, post_json("/api/edit", req_json)).await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "done");
    assert_eq!(job["result"]["filename"], "cached.mp4");
}

#[tokio::test]
async fn edit_stale_render_cache_entry_is_evicted() {
    let (state, _d) = make_state(true, true).await;
    let req_json = json!({ "videoId": "missing-video", "trim": { "start": 0.0, "end": 5.0 } });
    let req: EditRequest = serde_json::from_value(req_json.clone()).unwrap();
    let key =
        video_editor_backend::handlers::render_cache_key_for_tools(&req, state.tools.as_ref());
    state
        .db
        .cache_put(
            &key,
            &json!({
                "id": "stale",
                "url": "/files/outputs/stale.mp4",
                "filename": "stale.mp4"
            }),
            "stale.mp4",
        )
        .await
        .unwrap();
    assert!(state.db.cache_get(&key).await.unwrap().is_some());

    let app = router(state.clone());
    let (_s, body, _) = send(&app, post_json("/api/edit", req_json)).await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "error");
    assert!(state.db.cache_get(&key).await.unwrap().is_none());
}

#[tokio::test]
async fn closed_job_queue_marks_job_error() {
    let (state, _d) = make_state(true, true).await;
    state.close_job_queue();
    let app = router(state);
    let (_s, body, _) = send(
        &app,
        post_json("/api/edit", json!({ "videoId": "no-such-id" })),
    )
    .await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "error");
    assert_eq!(job["error"], "очередь задач закрыта");
}

#[tokio::test]
async fn cancel_unknown_job_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, post_empty("/api/jobs/ghost/cancel")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn cancel_terminal_job_is_conflict() {
    let (state, _d) = make_state(true, true).await;
    let mut job = Job::pending("done-job".into());
    job.status = JobStatus::Done;
    state.set_job(job).await;
    let app = router(state);
    let (status, body, _) = send(&app, post_empty("/api/jobs/done-job/cancel")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "Задача уже завершена");
    assert_api_error(&body, "conflict");
}

#[tokio::test]
async fn cancel_pending_job_marks_cancelled() {
    let (state, _d) = make_state(true, true).await;
    state.set_job(Job::pending("live-job".into())).await;
    let app = router(state);
    let (status, body, _) = send(&app, post_empty("/api/jobs/live-job/cancel")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "cancelled");
    let (_s, jb, _) = send(&app, get("/api/jobs/live-job")).await;
    assert_eq!(jb["status"], "cancelled");
}

#[tokio::test]
async fn cancel_queued_edit_does_not_wait_for_permit() {
    let (state, _d) = make_state(true, true).await;
    let _p1 = state.acquire_job_slot().await.unwrap();
    let _p2 = state.acquire_job_slot().await.unwrap();
    let app = router(state);

    let (_s, body, _) = send(
        &app,
        post_json("/api/edit", json!({ "videoId": "blocked-behind-permits" })),
    )
    .await;
    let id = body["jobId"].as_str().unwrap().to_string();
    let (status, cancel, _) = send(&app, post_empty(&format!("/api/jobs/{id}/cancel"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancel["status"], "cancelled");

    let job = poll_terminal(&app, &id).await;
    assert_eq!(job["status"], "cancelled");
}

#[tokio::test]
async fn library_list_add_delete_flow() {
    let (state, _d) = make_state(true, true).await;

    // Empty to start.
    let app = router(state.clone());
    let (status, body, _) = send(&app, get("/api/library")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Register a real source file so list() keeps it.
    let fname = "abc.mp4";
    tokio::fs::write(state.sources_dir().join(fname), b"data")
        .await
        .unwrap();
    state
        .library
        .add(MediaEntry::from_result(
            "source",
            &json!({ "id": "abc", "filename": fname, "url": format!("/files/sources/{fname}") }),
        ))
        .await;

    let (_s, body, _) = send(&app, get("/api/library")).await;
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], "abc");

    // Delete it -> 204 and the file is gone.
    let (status, _b, _) = send(&app, delete("/api/library/abc")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_s, body, _) = send(&app, get("/api/library")).await;
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Deleting again is a 404.
    let (status, _b, _) = send(&app, delete("/api/library/abc")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn library_delete_invalidates_output_render_cache() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state.clone());
    let fname = "cached-output.mp4";
    tokio::fs::write(state.outputs_dir().join(fname), b"data")
        .await
        .unwrap();
    let output = json!({
        "id": "out1",
        "filename": fname,
        "url": format!("/files/outputs/{fname}")
    });
    state
        .library
        .add(MediaEntry::from_result("output", &output))
        .await;
    state
        .db
        .cache_put("cache-key", &output, fname)
        .await
        .unwrap();
    assert!(state.db.cache_get("cache-key").await.unwrap().is_some());

    let (status, _b, _) = send(&app, delete("/api/library/out1")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(state.db.cache_get("cache-key").await.unwrap().is_none());
}

#[tokio::test]
async fn files_route_serves_only_media_subdirectories() {
    let (state, _d) = make_state(true, true).await;
    tokio::fs::write(state.sources_dir().join("clip.mp4"), b"source")
        .await
        .unwrap();
    tokio::fs::write(state.outputs_dir().join("render.mp4"), b"output")
        .await
        .unwrap();

    let app = router(state);
    let (status, _body, raw) = send(&app, get("/files/sources/clip.mp4")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, "source");

    let (status, _body, raw) = send(&app, get("/files/outputs/render.mp4")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, "output");

    let (status, _body, raw) = send(&app, get("/files/app.db")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!raw.contains("SQLite"));
}

#[tokio::test]
async fn project_upsert_list_get_delete_flow() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);

    // Empty to start.
    let (status, body, _) = send(&app, get("/api/projects")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Create.
    let (status, p, _) = send(
        &app,
        post_json(
            "/api/projects",
            json!({
                "videoId": "vid1",
                "video": { "id": "vid1", "filename": "a.mp4", "duration": 10 },
                "edit": { "trimStart": 0, "trimEnd": 5, "filter": "sepia" },
                "name": "My edit"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pid = p["id"].as_str().unwrap().to_string();
    assert_eq!(p["videoId"], "vid1");
    assert_eq!(p["edit"]["filter"], "sepia");

    // by-video lookup finds it.
    let (_s, bv, _) = send(&app, get("/api/projects/by-video/vid1")).await;
    assert_eq!(bv["id"], pid);

    // Upsert for the same video updates in place (no duplicate).
    let (_s, p2, _) = send(
        &app,
        post_json(
            "/api/projects",
            json!({
                "videoId": "vid1",
                "video": { "id": "vid1", "filename": "a.mp4" },
                "edit": { "filter": "warm" }
            }),
        ),
    )
    .await;
    assert_eq!(p2["id"], pid);
    let (_s, list, _) = send(&app, get("/api/projects")).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["edit"]["filter"], "warm");

    // Get by id, then delete.
    let (status, _g, _) = send(&app, get(&format!("/api/projects/{pid}"))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _b, _) = send(&app, delete(&format!("/api/projects/{pid}"))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _b, _) = send(&app, get(&format!("/api/projects/{pid}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn jobs_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    tokio::fs::create_dir_all(storage.join("sources"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(storage.join("outputs"))
        .await
        .unwrap();

    // Session 1: one job finishes, one is still running when the process stops.
    {
        let db = Db::open(&storage).await.unwrap();
        let lib = Library::load(storage.clone()).await;
        let st = AppState::new(storage.clone(), 2, ToolInfo::default(), lib, db);
        st.set_job(Job::pending("done1".into())).await; // persists pending
        st.update_job("done1", |j| {
            j.status = JobStatus::Done;
            j.result = Some(json!({ "url": "/files/outputs/x.mp4" }));
            j.progress = Some(100.0);
        })
        .await;
        st.persist_job("done1").await; // persists the terminal state
        st.set_job(Job::pending("run1".into())).await;
        st.update_job("run1", |j| j.status = JobStatus::Running)
            .await; // memory only
    }

    // Session 2: a fresh state over the same DB recovers the jobs.
    let db2 = Db::open(&storage).await.unwrap();
    let lib2 = Library::load(storage.clone()).await;
    let st2 = AppState::new(storage.clone(), 2, ToolInfo::default(), lib2, db2);
    st2.recover_jobs().await;
    let app = router(st2);

    // The finished job is recovered with its result.
    let (status, body, _) = send(&app, get("/api/jobs/done1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "done");
    assert_eq!(body["result"]["url"], "/files/outputs/x.mp4");

    // The in-flight job became interrupted (not a 404, not an endless spinner).
    let (status, body, _) = send(&app, get("/api/jobs/run1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "interrupted");
}

#[tokio::test]
async fn outbox_row_created_before_a_crash_is_executed_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    for subdirectory in ["sources", "outputs", "staging"] {
        tokio::fs::create_dir_all(storage.join(subdirectory))
            .await
            .unwrap();
    }

    {
        let db = Db::open(&storage).await.unwrap();
        let store = video_editor_backend::jobs::SqliteJobStore::new(db);
        let outcome = store
            .enqueue(
                "crash-job".into(),
                JobKind::Edit,
                &json!({
                    "schemaVersion": 1,
                    "request": { "videoId": "missing-source" },
                    "outputId": "output",
                    "cacheKey": "cache"
                }),
                "crash-dedupe",
                QueueLimits::default(),
            )
            .await
            .unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Created(_)));
        // Simulated power loss: no in-memory registration and no worker spawn.
    }

    let db = Db::open(&storage).await.unwrap();
    let library = Library::load(storage.clone()).await;
    let state = AppState::new(storage, 1, ToolInfo::default(), library, db);
    state.recover_jobs().await;
    resume_pending_jobs(&state).await;
    let app = router(state.clone());
    let job = poll_terminal(&app, "crash-job").await;
    assert_eq!(job["status"], "error");
    assert!(state.job_store.deliverable_ids().await.unwrap().is_empty());
    assert!(
        state
            .job_store
            .event_history("crash-job")
            .await
            .unwrap()
            .len()
            >= 4
    );
}

#[tokio::test]
async fn project_by_video_missing_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/projects/by-video/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn project_upsert_requires_video_and_edit() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, post_json("/api/projects", json!({ "videoId": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}

#[tokio::test]
async fn project_upsert_rejects_oversized_json_fields() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let huge = "x".repeat(70 * 1024);

    let (status, _b, raw) = send(
        &app,
        post_json(
            "/api/projects",
            json!({
                "videoId": "huge",
                "video": { "id": "huge", "blob": huge },
                "edit": { "filter": "warm" }
            }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(raw.contains("video"));

    let (_status, body, _) = send(&app, get("/api/projects")).await;
    assert!(body.as_array().unwrap().is_empty());
}
