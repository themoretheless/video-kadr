//! HTTP-level integration tests. They drive the real router via
//! `tower::ServiceExt::oneshot` (no socket bound) against an isolated temp
//! storage dir. Upload-specific threat regressions live in
//! `upload_security.rs` and share the same test harness.

mod support;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use video_kadr_backend::db::{Db, MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES};
use video_kadr_backend::handlers::resume_pending_jobs;
use video_kadr_backend::jobs::{EnqueueOutcome, JobKind, QueueLimits};
use video_kadr_backend::library::{Library, MediaEntry};
use video_kadr_backend::model::{EditRequest, Job, JobStatus};
use video_kadr_backend::state::{AppState, ToolInfo};

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

fn put_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
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

fn bearer(mut request: Request<Body>, token: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

fn session_cookie(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        "cookie",
        format!("video_kadr_session={token}").parse().unwrap(),
    );
    request
}

fn selected_space(mut request: Request<Body>, space_id: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert("x-space-id", space_id.parse().unwrap());
    request
}

fn if_none_match(mut request: Request<Body>, etag: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert("if-none-match", etag.parse().unwrap());
    request
}

async fn register_token(app: &Router, username: &str) -> String {
    let (status, session, _) = send(
        app,
        post_json(
            "/api/auth/register",
            json!({"username": username, "password": "shared test password"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    session["token"].as_str().unwrap().to_owned()
}

fn composition_request(source_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "composition": {
            "schemaVersion": 1,
            "timeBase": 1_000_000,
            "canvas": {
                "width": 1280,
                "height": 720,
                "fpsMilli": 30_000,
                "background": { "red": 0.0, "green": 0.0, "blue": 0.0, "alpha": 1.0 }
            },
            "sources": {
                (source_id): {
                    "id": source_id,
                    "kind": "video",
                    "durationTicks": 2_000_000,
                    "width": 1280,
                    "height": 720,
                    "hasAudio": false
                }
            },
            "tracks": [{
                "kind": "video",
                "id": "video-main",
                "name": "Main",
                "hidden": false,
                "locked": false,
                "clips": [{
                    "id": "clip-main",
                    "sourceId": source_id,
                    "placement": {
                        "timelineStartTick": 0,
                        "sourceInTick": 0,
                        "sourceOutTick": 1_000_000,
                        "speed": 1.0
                    },
                    "transform": {
                        "x": { "mode": "constant", "value": 0.0 },
                        "y": { "mode": "constant", "value": 0.0 },
                        "scaleX": { "mode": "constant", "value": 1.0 },
                        "scaleY": { "mode": "constant", "value": 1.0 },
                        "rotationDegrees": { "mode": "constant", "value": 0.0 },
                        "anchorX": 0.5,
                        "anchorY": 0.5
                    },
                    "opacity": { "mode": "constant", "value": 1.0 },
                    "blendMode": "normal",
                    "effects": [],
                    "enabled": true
                }],
                "transitions": []
            }]
        },
        "output": { "format": "mp4", "codec": "h264", "qualityTier": "medium" }
    })
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

/// Poll a job until it reaches a terminal state. The background worker runs on
/// the test runtime; the sleeps give it slots to make progress.
///
/// The bound is deliberately generous: the worker only makes progress when the
/// runtime hands it a slot, so on a loaded machine (the first run right after
/// compilation, or the whole suite running in parallel) a short budget fails
/// even though nothing is wrong. A healthy job still returns in milliseconds,
/// so the ceiling only costs wall-clock time when something is genuinely stuck.
async fn poll_terminal(app: &Router, id: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..3_000 {
        let (status, body, _) = send(app, get(&format!("/api/jobs/{id}"))).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "job {id} should exist while polling"
        );
        match body["status"].as_str().unwrap_or("") {
            "done" | "error" | "cancelled" => return body,
            _ => {
                last = body;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
    panic!("job {id} never reached a terminal state after 3000 polls; last snapshot: {last}");
}

#[tokio::test]
async fn health_reflects_tool_availability() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state.clone());
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

    let patch_preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/library/item/metadata")
        .header("origin", "http://localhost:5173")
        .header("access-control-request-method", "PATCH")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(patch_preflight).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()["access-control-allow-methods"]
        .to_str()
        .unwrap()
        .contains("PATCH"));

    let composition_preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/compositions/render")
        .header("origin", "http://localhost:5173")
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "content-type")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(composition_preflight).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()["access-control-allow-methods"]
        .to_str()
        .unwrap()
        .contains("POST"));

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
async fn metrics_exposition_uses_only_low_cardinality_dimensions() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let (status, _json, text) = send(&app, get("/metrics")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(text.contains("video_kadr_queue_wait_seconds"));
    assert!(text.contains("class=\"ingest\""));
    for forbidden in ["job_id=", "request_id=", "url=", "filename="] {
        assert!(!text.contains(forbidden));
    }
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
async fn composition_render_is_durable_and_reports_a_missing_source() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let legacy = composition_request("missing-source");
    let (status, body, _) = send(
        &app,
        bearer(
            post_json("/api/compositions/render", legacy.clone()),
            "invalid-session",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let (status, accepted, _) = send(&app, post_json("/api/compositions/render", legacy)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = accepted["jobId"].as_str().unwrap();

    let mut canonical = composition_request("missing-source");
    canonical["output"] = json!({
        "profile": { "container": "mp4", "codec": "h264" },
        "qualityTier": "medium"
    });
    let (status, duplicate, _) = send(&app, post_json("/api/compositions/render", canonical)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(duplicate["jobId"], job_id);

    let terminal = poll_terminal(&app, job_id).await;
    assert_eq!(terminal["status"], "error");
    assert!(terminal["error"].as_str().unwrap().contains("не найден"));
}

#[tokio::test]
async fn composition_delivery_profile_is_request_gated_and_invalid_pairs_fail_closed() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);

    let mut unavailable = composition_request("missing-source");
    unavailable["output"] = json!({
        "profile": { "container": "mov", "profile": "hq" },
        "qualityTier": "high"
    });
    let (status, body, _) = send(&app, post_json("/api/compositions/render", unavailable)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(body["error"].as_str().unwrap().contains("pcm_s16le"));

    let mut invalid = composition_request("missing-source");
    invalid["output"] = json!({
        "profile": { "container": "mp4", "codec": "vp9" },
        "qualityTier": "medium"
    });
    let (status, body, _) = send(&app, post_json("/api/compositions/render", invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");
}

#[tokio::test]
async fn composition_render_rejects_path_like_source_ids_before_enqueue() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(
        &app,
        post_json("/api/compositions/render", composition_request("../escape")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "invalid_json");
}

#[tokio::test]
async fn duplicate_import_reuses_job_and_failed_actions_are_audited() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state.clone());
    let request = json!({ "url": "http://127.0.0.1/private?token=CANARY" });

    let (first_status, first, _) = send(&app, post_json("/api/import", request.clone())).await;
    let (second_status, second, _) = send(&app, post_json("/api/import", request)).await;
    assert_eq!(first_status, StatusCode::ACCEPTED);
    assert_eq!(second_status, StatusCode::ACCEPTED);
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
    assert_eq!(status, StatusCode::ACCEPTED);
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
async fn edit_handler_renders_a_real_source_when_ffmpeg_is_available() {
    let ffmpeg_available = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    let ffprobe_available = tokio::process::Command::new("ffprobe")
        .arg("-version")
        .output()
        .await
        .is_ok_and(|output| output.status.success());
    if !ffmpeg_available || !ffprobe_available {
        eprintln!("skipping real edit handler test: ffmpeg/ffprobe not on PATH");
        return;
    }

    let (state, _directory) = make_state(true, true).await;
    let source = state.sources_dir().join("api-real-source.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x48:d=0.25",
            "-pix_fmt",
            "yuv420p",
            "-y",
        ])
        .arg(&source)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "failed to generate real API fixture: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(
        state
            .library
            .add(MediaEntry::from_result(
                "source",
                &json!({
                    "id": "api-real-source",
                    "filename": "api-real-source.mp4",
                    "url": "/files/sources/api-real-source.mp4",
                    "duration": 0.25,
                    "width": 64,
                    "height": 48
                }),
            ))
            .await
    );

    let app = router(state.clone());
    let (status, accepted, _) = send(
        &app,
        post_json(
            "/api/edit",
            json!({
                "videoId": "api-real-source",
                "brightness": 0.1,
                "format": "mp4",
                "codec": "h264"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let job_id = accepted["jobId"].as_str().unwrap();
    let terminal = poll_terminal(&app, job_id).await;
    assert_eq!(terminal["status"], "done", "terminal job: {terminal}");
    let filename = terminal["result"]["filename"].as_str().unwrap();
    assert!(state.outputs_dir().join(filename).is_file());
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
    assert_eq!(status, StatusCode::ACCEPTED);
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
    assert_eq!(status, StatusCode::ACCEPTED);
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
    assert_eq!(plain_status, StatusCode::ACCEPTED);
    assert_eq!(plain["jobId"], queued["jobId"]);
}

#[tokio::test]
async fn audio_only_exports_canonicalize_video_grading_before_validation_and_dedupe() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    for format in ["mp3", "wav"] {
        let (first_status, first, _) = send(
            &app,
            post_json(
                "/api/edit",
                json!({
                    "videoId": "missing-audio-source",
                    "format": format,
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
        assert_eq!(first_status, StatusCode::ACCEPTED);

        let (second_status, second, _) = send(
            &app,
            post_json(
                "/api/edit",
                json!({ "videoId": "missing-audio-source", "format": format }),
            ),
        )
        .await;
        assert_eq!(second_status, StatusCode::ACCEPTED);
        assert_eq!(first["jobId"], second["jobId"], "{format}");
    }
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
    assert_eq!(status, StatusCode::ACCEPTED);
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
    let key = video_kadr_backend::handlers::render_cache_key_for_tools(&req, state.tools.as_ref());
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
    let key = video_kadr_backend::handlers::render_cache_key_for_tools(&req, state.tools.as_ref());
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
    let _p1 = state
        .acquire_job_slot(video_kadr_backend::config::resource_classes::ResourceClass::Export)
        .await
        .unwrap();
    let _p2 = state
        .acquire_job_slot(video_kadr_backend::config::resource_classes::ResourceClass::Export)
        .await
        .unwrap();
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
    assert_eq!(arr[0]["favorite"], false);
    assert_eq!(arr[0]["tags"], json!([]));

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
async fn library_metadata_put_patch_search_fields_and_delete_cleanup() {
    let (state, _directory) = make_state(true, true).await;
    let filename = "interview-audio.webm";
    tokio::fs::write(state.sources_dir().join(filename), b"audio")
        .await
        .unwrap();
    state
        .library
        .add(MediaEntry::from_result(
            "source",
            &json!({
                "id": "voice-1",
                "filename": filename,
                "url": format!("/files/sources/{filename}"),
                "title": "Imported title",
                "mediaType": "audio"
            }),
        ))
        .await;
    let app = router(state.clone());

    let (put_status, put_body, _) = send(
        &app,
        put_json(
            "/api/library/voice-1/metadata",
            json!({
                "title": "  Customer interview  ",
                "favorite": true,
                "tags": [" work ", "voice"]
            }),
        ),
    )
    .await;
    assert_eq!(put_status, StatusCode::OK);
    assert_eq!(put_body["title"], "Customer interview");
    assert_eq!(put_body["favorite"], true);
    assert_eq!(put_body["tags"], json!(["work", "voice"]));

    let (patch_status, patch_body, _) = send(
        &app,
        patch_json(
            "/api/library/voice-1/metadata",
            json!({ "favorite": false }),
        ),
    )
    .await;
    assert_eq!(patch_status, StatusCode::OK);
    assert_eq!(patch_body["favorite"], false);
    assert_eq!(patch_body["title"], "Customer interview");
    assert_eq!(patch_body["tags"], json!(["work", "voice"]));

    let (clear_status, clear_body, _) = send(
        &app,
        patch_json("/api/library/voice-1/metadata", json!({ "title": null })),
    )
    .await;
    assert_eq!(clear_status, StatusCode::OK);
    assert_eq!(clear_body["title"], "Imported title");

    let (list_status, list_body, _) = send(&app, get("/api/library")).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body[0]["title"], "Imported title");
    assert_eq!(list_body[0]["tags"], json!(["work", "voice"]));

    let (delete_status, _, _) = send(&app, delete("/api/library/voice-1")).await;
    assert_eq!(delete_status, StatusCode::NO_CONTENT);
    assert!(state
        .db
        .get_library_metadata("voice-1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn library_metadata_rejects_invalid_or_ambiguous_payloads() {
    let (state, _directory) = make_state(true, true).await;
    let filename = "clip.mp4";
    tokio::fs::write(state.sources_dir().join(filename), b"video")
        .await
        .unwrap();
    state
        .library
        .add(MediaEntry::from_result(
            "source",
            &json!({ "id": "clip-meta", "filename": filename, "url": "/files/sources/clip.mp4" }),
        ))
        .await;
    let app = router(state);

    for payload in [
        json!({ "title": "x".repeat(121), "favorite": false, "tags": [] }),
        json!({ "title": null, "favorite": false, "tags": ["Work", "work"] }),
        json!({ "title": null, "favorite": false, "tags": (0..21).map(|i| format!("tag-{i}")).collect::<Vec<_>>() }),
    ] {
        let (status, body, _) =
            send(&app, put_json("/api/library/clip-meta/metadata", payload)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_api_error(&body, "bad_request");
    }

    let (empty_status, empty_body, _) = send(
        &app,
        patch_json("/api/library/clip-meta/metadata", json!({})),
    )
    .await;
    assert_eq!(empty_status, StatusCode::BAD_REQUEST);
    assert_api_error(&empty_body, "bad_request");

    let (missing_status, missing_body, _) = send(
        &app,
        patch_json("/api/library/missing/metadata", json!({ "favorite": true })),
    )
    .await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_api_error(&missing_body, "not_found");
}

#[tokio::test]
async fn library_source_delete_is_blocked_while_a_composition_project_references_it() {
    let (state, _directory) = make_state(true, true).await;
    let filename = "referenced.mp4";
    tokio::fs::write(state.sources_dir().join(filename), b"fixture")
        .await
        .unwrap();
    state
        .library
        .add(MediaEntry::from_result(
            "source",
            &json!({
                "id": "referenced-source",
                "filename": filename,
                "url": format!("/files/sources/{filename}"),
                "mediaType": "video"
            }),
        ))
        .await;
    state
        .db
        .replace_library_metadata(
            "referenced-source",
            Some("Protected source".into()),
            true,
            vec!["composition".into()],
        )
        .await
        .unwrap();
    let app = router(state.clone());
    let token = register_token(&app, "source-owner").await;
    let (created_status, project, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "name": "Uses source",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {
                            "referenced-source": {"id": "referenced-source"}
                        }
                    }
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(created_status, StatusCode::CREATED);

    let (blocked_status, blocked, _) = send(&app, delete("/api/library/referenced-source")).await;
    assert_eq!(blocked_status, StatusCode::CONFLICT);
    assert_api_error(&blocked, "conflict");
    assert!(tokio::fs::metadata(state.sources_dir().join(filename))
        .await
        .is_ok());
    assert!(state
        .db
        .get_library_metadata("referenced-source")
        .await
        .unwrap()
        .is_some());

    let project_id = project["id"].as_str().unwrap();
    let (deleted_project, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{project_id}")),
            &token,
        ),
    )
    .await;
    assert_eq!(deleted_project, StatusCode::NO_CONTENT);
    let (deleted_source, _, _) = send(&app, delete("/api/library/referenced-source")).await;
    assert_eq!(deleted_source, StatusCode::NO_CONTENT);
    assert!(state
        .db
        .get_library_metadata("referenced-source")
        .await
        .unwrap()
        .is_none());
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
async fn composition_project_create_update_list_get_delete_flow() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let token = register_token(&app, "project-owner").await;
    let outsider_token = register_token(&app, "project-outsider").await;

    let (status, body, _) = send(&app, get("/api/composition-projects")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");

    let (status, empty, _) = send(&app, bearer(get("/api/composition-projects"), &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty.as_array().unwrap().is_empty());

    let (status, first, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "name": "Two sources",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {
                            "source-a": {"id": "source-a", "kind": "video"},
                            "source-b": {"id": "source-b", "kind": "audio"}
                        },
                        "tracks": []
                    }
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = first["id"].as_str().unwrap().to_owned();
    assert!(uuid::Uuid::parse_str(&first_id).is_ok());
    assert_eq!(first["schemaVersion"], 2);
    assert_eq!(first["mode"], "composition");
    assert_eq!(first["revision"], 1);
    assert_eq!(first["sourceIds"], json!(["source-a", "source-b"]));

    let (status, outsider_projects, _) = send(
        &app,
        bearer(get("/api/composition-projects"), &outsider_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(outsider_projects.as_array().unwrap().is_empty());
    let (status, body, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{first_id}")),
            &outsider_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");

    let (status, stored, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{first_id}")),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored, first);
    let unchanged = app
        .clone()
        .oneshot(if_none_match(
            bearer(
                get(&format!("/api/composition-projects/{first_id}")),
                &token,
            ),
            "\"revision-1\"",
        ))
        .await
        .unwrap();
    assert_eq!(unchanged.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(unchanged.headers()["etag"], "\"revision-1\"");

    let (status, missing_revision, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/composition-projects/{first_id}"),
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "document": first["document"].clone()
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&missing_revision, "bad_request");

    let (status, second, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["name"], "Без названия");

    let (status, updated, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/composition-projects/{first_id}"),
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "baseRevision": 1,
                    "name": "Updated",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {"source-c": {"id": "source-c", "kind": "image"}},
                        "tracks": [],
                        "revision": 2
                    }
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["id"], first_id);
    assert_eq!(updated["createdAt"], first["createdAt"]);
    assert_eq!(updated["sourceIds"], json!(["source-c"]));
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["document"]["revision"], 2);

    let (status, stale, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/composition-projects/{first_id}"),
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "baseRevision": 1,
                    "name": "Stale overwrite",
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&stale, "conflict");
    let (_, after_conflict, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{first_id}")),
            &token,
        ),
    )
    .await;
    assert_eq!(after_conflict["name"], "Updated");
    assert_eq!(after_conflict["revision"], 2);

    let (status, projects, _) = send(&app, bearer(get("/api/composition-projects"), &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    assert_eq!(projects[0]["id"], first_id, "last write sorts first");
    let list_response = app
        .clone()
        .oneshot(bearer(get("/api/composition-projects"), &token))
        .await
        .unwrap();
    let list_etag = list_response.headers()["etag"].clone();
    let list_unchanged = app
        .clone()
        .oneshot(if_none_match(
            bearer(get("/api/composition-projects"), &token),
            list_etag.to_str().unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(list_unchanged.status(), StatusCode::NOT_MODIFIED);

    let (status, legacy, _) = send(&app, get("/api/projects")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(legacy.as_array().unwrap().is_empty());

    let (status, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{first_id}")),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{first_id}")),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn composition_review_threads_persist_reply_resolve_and_follow_project_lifecycle() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let mut session_tokens = std::collections::HashMap::new();
    for username in ["alice", "bob", "viewer"] {
        session_tokens.insert(username, register_token(&app, username).await);
    }
    let alice_token = &session_tokens["alice"];
    let bob_token = &session_tokens["bob"];
    let viewer_token = &session_tokens["viewer"];
    let (_, project, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "name": "Reviewable cut",
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            alice_token,
        ),
    )
    .await;
    let project_id = project["id"].as_str().unwrap();

    let (status, missing_session, _) = send(
        &app,
        post_json(
            &format!("/api/composition-projects/{project_id}/reviews"),
            json!({"body": "No session", "timelineTick": 1}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&missing_session, "unauthorized");

    let (status, spoofed, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/composition-projects/{project_id}/reviews"),
                json!({"actor": "mallory", "body": "Spoof", "timelineTick": 1}),
            ),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&spoofed, "invalid_json");

    let (status, thread, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/composition-projects/{project_id}/reviews"),
                json!({
                    "body": "Move this cut earlier",
                    "timelineTick": 90000
                }),
            ),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let thread_id = thread["id"].as_str().unwrap();
    assert_eq!(thread["comments"][0]["timelineTick"], 90_000);

    let (status, hidden, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{project_id}/reviews")),
            bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&hidden, "forbidden");

    let (status, owner, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{project_id}/members")),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(owner, json!([{"actor": "alice", "role": "owner"}]));
    for (actor, role) in [("bob", "commenter"), ("viewer", "viewer")] {
        let (status, member, _) = send(
            &app,
            bearer(
                put_json(
                    &format!("/api/composition-projects/{project_id}/members/{actor}"),
                    json!({"role": role}),
                ),
                alice_token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(member["role"], role);
    }
    let (status, body, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/composition-projects/{project_id}/members/mallory"),
                json!({"role": "owner"}),
            ),
            bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/review-threads/{thread_id}/replies"),
                json!({"body": "No access"}),
            ),
            viewer_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let (status, replied, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/review-threads/{thread_id}/replies"),
                json!({"body": "Done"}),
            ),
            bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replied["comments"].as_array().unwrap().len(), 2);
    assert_eq!(replied["comments"][1]["parentId"], thread_id);

    let (status, resolved, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/review-threads/{thread_id}/resolution"),
                json!({"resolved": true}),
            ),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resolved["resolvedBy"], "alice");

    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/review-threads/{thread_id}/replies"),
                json!({"body": "Too late"}),
            ),
            bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");

    let (status, listed, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{project_id}/reviews")),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["comments"].as_array().unwrap().len(), 2);

    let (status, share, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/composition-projects/{project_id}/review-shares"),
                json!({"ttlSeconds": 3600}),
            ),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let share_id = share["grant"]["id"].as_str().unwrap();
    let token = share["token"].as_str().unwrap();
    let (status, public_review, _) = send(&app, get(&format!("/api/review-shares/{token}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public_review["projectId"], project_id);
    assert_eq!(public_review["threads"].as_array().unwrap().len(), 1);
    assert!(public_review.get("document").is_none());
    assert!(public_review.get("sourceIds").is_none());
    let (status, _, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/composition-projects/{project_id}/review-shares/{share_id}/revoke"),
                json!({}),
            ),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body, _) = send(&app, get(&format!("/api/review-shares/{token}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");

    let (status, audit, _) = send(
        &app,
        bearer(
            get(&format!(
                "/api/composition-projects/{project_id}/review-audit"
            )),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "thread.created",
            "member.role_set",
            "member.role_set",
            "comment.replied",
            "thread.resolved",
            "share.created",
            "share.revoked",
        ]
    );

    let (status, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{project_id}")),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, listed, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{project_id}/reviews")),
            alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&listed, "forbidden");
}

#[tokio::test]
async fn composition_project_ownership_transfer_is_atomic_and_audited() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let alice_token = register_token(&app, "handoff-alice").await;
    let bob_token = register_token(&app, "handoff-bob").await;
    let (_, project, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            &alice_token,
        ),
    )
    .await;
    let project_id = project["id"].as_str().unwrap();
    let member_uri = format!("/api/composition-projects/{project_id}/members/handoff-bob");
    let (status, _, _) = send(
        &app,
        bearer(
            put_json(&member_uri, json!({"role": "editor"})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body, _) = send(
        &app,
        bearer(
            put_json(&member_uri, json!({"role": "owner"})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let transfer_uri = format!("/api/composition-projects/{project_id}/ownership-transfer");
    let (status, owner, _) = send(
        &app,
        bearer(
            post_json(&transfer_uri, json!({"targetActor": "handoff-bob"})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(owner, json!({"actor": "handoff-bob", "role": "owner"}));

    let (status, members, _) = send(
        &app,
        bearer(
            get(&format!("/api/composition-projects/{project_id}/members")),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        members,
        json!([
            {"actor": "handoff-alice", "role": "editor"},
            {"actor": "handoff-bob", "role": "owner"}
        ])
    );

    let (status, body, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{project_id}")),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, audit, _) = send(
        &app,
        bearer(
            get(&format!(
                "/api/composition-projects/{project_id}/review-audit"
            )),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        audit.as_array().unwrap().last().unwrap()["action"],
        "ownership.transferred"
    );
    let (status, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{project_id}")),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn collaboration_spaces_are_private_and_owner_manages_members() {
    let (state, _directory) = make_state(true, true).await;
    let filename = "space-source.mp4";
    tokio::fs::write(state.sources_dir().join(filename), b"space-private-bytes")
        .await
        .unwrap();
    let space_entry = MediaEntry::from_result(
        "source",
        &json!({
            "id": "space-source",
            "filename": filename,
            "url": format!("/files/sources/{filename}"),
            "mediaType": "video"
        }),
    );
    state.library.add(space_entry.clone()).await;
    state.index_media(&space_entry).await;
    let app = router(state.clone());
    let alice_token = register_token(&app, "space-alice").await;
    let bob_token = register_token(&app, "space-bob").await;
    let viewer_token = register_token(&app, "space-viewer").await;
    let invitee_token = register_token(&app, "space-invitee").await;

    let (status, body, _) = send(&app, get("/api/spaces")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let (status, space, _) = send(
        &app,
        bearer(
            post_json("/api/spaces", json!({"name": "Launch Team"})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(space["name"], "Launch Team");
    assert_eq!(space["role"], "owner");
    let space_id = space["id"].as_str().unwrap();

    let (status, spaces, _) = send(&app, bearer(get("/api/spaces"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spaces.as_array().unwrap().is_empty());
    let members_uri = format!("/api/spaces/{space_id}/members");
    let (status, body, _) = send(&app, bearer(get(&members_uri), &bob_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let invites_uri = format!("/api/spaces/{space_id}/invites");
    let (status, invite, _) = send(
        &app,
        bearer(
            post_json(&invites_uri, json!({"role": "viewer", "ttlSeconds": 3600})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(invite["role"], "viewer");
    let accept_uri = format!(
        "/api/space-invites/{}/accept",
        invite["token"].as_str().unwrap()
    );
    let (status, accepted, _) = send(
        &app,
        bearer(post_json(&accept_uri, json!({})), &invitee_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["id"], space_id);
    assert_eq!(accepted["role"], "viewer");
    let (status, body, _) = send(
        &app,
        bearer(post_json(&accept_uri, json!({})), &invitee_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");

    let bob_uri = format!("/api/spaces/{space_id}/members/space-bob");
    let (status, member, _) = send(
        &app,
        bearer(put_json(&bob_uri, json!({"role": "editor"})), &alice_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(member, json!({"actor": "space-bob", "role": "editor"}));

    let (status, bob_project, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "spaceId": space_id,
                    "name": "Bob owned cut",
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let rename_uri = format!("/api/spaces/{space_id}");
    let base_updated_at = space["updatedAt"].as_u64().unwrap();
    let (status, body, _) = send(
        &app,
        bearer(
            patch_json(
                &rename_uri,
                json!({"name": "Hijacked", "baseUpdatedAt": base_updated_at}),
            ),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, renamed, _) = send(
        &app,
        bearer(
            patch_json(
                &rename_uri,
                json!({"name": "Launch Studio", "baseUpdatedAt": base_updated_at}),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["name"], "Launch Studio");
    assert!(renamed["updatedAt"].as_u64().unwrap() > base_updated_at);
    let (status, body, _) = send(
        &app,
        bearer(
            patch_json(
                &rename_uri,
                json!({"name": "Stale name", "baseUpdatedAt": base_updated_at}),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");

    let (status, project, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "spaceId": space_id,
                    "name": "Space cut",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {"space-source": {"id": "space-source"}},
                        "tracks": []
                    }
                }),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(project["spaceId"], space_id);
    let unauthenticated_events = app
        .clone()
        .oneshot(get("/api/composition-projects/events"))
        .await
        .unwrap();
    assert_eq!(unauthenticated_events.status(), StatusCode::UNAUTHORIZED);
    let mut project_events = app
        .clone()
        .oneshot(session_cookie(
            get("/api/composition-projects/events"),
            &bob_token,
        ))
        .await
        .unwrap();
    assert_eq!(project_events.status(), StatusCode::OK);
    assert_eq!(
        project_events.headers()["content-type"],
        "text/event-stream"
    );
    let mut outsider_events = app
        .clone()
        .oneshot(session_cookie(
            get("/api/composition-projects/events"),
            &viewer_token,
        ))
        .await
        .unwrap();
    let project_id = project["id"].as_str().unwrap();
    let (status, pushed_update, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/composition-projects/{project_id}"),
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "baseRevision": 1,
                    "name": "Space cut pushed",
                    "document": project["document"].clone()
                }),
            ),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pushed_update["revision"], 2);
    let event_frame =
        tokio::time::timeout(Duration::from_secs(1), project_events.body_mut().frame())
            .await
            .expect("project SSE event timed out")
            .expect("project SSE stream ended")
            .expect("project SSE frame failed")
            .into_data()
            .expect("project SSE frame was not data");
    let event_text = String::from_utf8(event_frame.to_vec()).unwrap();
    assert!(event_text.contains("event: project"));
    assert!(event_text.contains(project_id));
    assert!(
        tokio::time::timeout(
            Duration::from_millis(100),
            outsider_events.body_mut().frame()
        )
        .await
        .is_err(),
        "outsider received a private project event"
    );
    assert!(tokio::fs::metadata(state.sources_dir().join(filename))
        .await
        .is_err());
    let isolated_entry = state.library.get("space-source").await.unwrap();
    assert_eq!(
        isolated_entry.storage_key.as_deref(),
        Some(format!("spaces/{space_id}/{filename}").as_str())
    );
    assert_eq!(
        tokio::fs::read(
            state
                .library
                .resolve_media_path(&isolated_entry)
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        b"space-private-bytes"
    );

    let (status, library, _) = send(&app, get("/api/library")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(library.as_array().unwrap().is_empty());
    let (status, library, _) = send(&app, bearer(get("/api/library"), &viewer_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(library.as_array().unwrap().is_empty());
    let (status, body, _) = send(
        &app,
        selected_space(bearer(get("/api/library"), &viewer_token), space_id),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, library, _) = send(
        &app,
        selected_space(bearer(get("/api/library"), &bob_token), space_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(library.as_array().unwrap().len(), 1);
    assert_eq!(library[0]["id"], "space-source");
    let (status, search, _) = send(
        &app,
        selected_space(
            bearer(get("/api/library/search?q=space"), &bob_token),
            space_id,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search.as_array().unwrap().len(), 1);
    let (status, body, _) = send(
        &app,
        patch_json(
            "/api/library/space-source/metadata",
            json!({"favorite": true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let (status, body, _) = send(
        &app,
        bearer(
            patch_json(
                "/api/library/space-source/metadata",
                json!({"favorite": true}),
            ),
            &viewer_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, _, _) = send(
        &app,
        bearer(
            patch_json(
                "/api/library/space-source/metadata",
                json!({"favorite": true}),
            ),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body, _) = send(&app, delete("/api/library/space-source")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let (status, body, _) = send(
        &app,
        bearer(delete("/api/library/space-source"), &viewer_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, projects, _) =
        send(&app, bearer(get("/api/composition-projects"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    assert!(projects
        .as_array()
        .unwrap()
        .iter()
        .any(|listed| listed["id"] == project["id"]));
    let source_uri = format!("/files/sources/{filename}");
    let (status, body, _) = send(&app, get(&source_uri)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let (status, body, _) = send(&app, bearer(get(&source_uri), &viewer_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    for protected_uri in [
        "/api/library/space-source/thumbnail",
        "/api/library/space-source/filmstrip",
        "/api/library/space-source/proxies",
        "/api/library/space-source/proxies/invalid/content",
    ] {
        let (status, body, _) = send(&app, get(protected_uri)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{protected_uri}");
        assert_api_error(&body, "unauthorized");
        let (status, body, _) = send(&app, bearer(get(protected_uri), &viewer_token)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{protected_uri}");
        assert_api_error(&body, "forbidden");
    }
    let (status, _, raw) = send(&app, bearer(get(&source_uri), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, "space-private-bytes");
    let (status, _, raw) = send(&app, session_cookie(get(&source_uri), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(raw, "space-private-bytes");
    let (_, other_space, _) = send(
        &app,
        bearer(
            post_json("/api/spaces", json!({"name": "Other Team"})),
            &alice_token,
        ),
    )
    .await;
    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "spaceId": other_space["id"],
                    "document": {
                        "schemaVersion": 1,
                        "sources": {"space-source": {"id": "space-source"}},
                        "tracks": []
                    }
                }),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let viewer_uri = format!("/api/spaces/{space_id}/members/space-viewer");
    let (status, _, _) = send(
        &app,
        bearer(
            put_json(&viewer_uri, json!({"role": "viewer"})),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, projects, _) = send(
        &app,
        bearer(get("/api/composition-projects"), &viewer_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "spaceId": space_id,
                    "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
                }),
            ),
            &viewer_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let (status, spaces, _) = send(&app, bearer(get("/api/spaces"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(spaces.as_array().unwrap().len(), 1);
    assert_eq!(spaces[0]["role"], "editor");
    let (status, members, _) = send(&app, bearer(get(&members_uri), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members.as_array().unwrap().len(), 4);
    let (status, body, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/spaces/{space_id}/members/mallory"),
                json!({"role": "viewer"}),
            ),
            &bob_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let (status, body, _) = send(&app, bearer(delete(&bob_uri), &bob_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let owner_uri = format!("/api/spaces/{space_id}/members/space-alice");
    let (status, body, _) = send(&app, bearer(delete(&owner_uri), &alice_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, _, _) = send(&app, bearer(delete(&bob_uri), &alice_token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, spaces, _) = send(&app, bearer(get("/api/spaces"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(spaces.as_array().unwrap().is_empty());
    let (status, projects, _) =
        send(&app, bearer(get("/api/composition-projects"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(projects.as_array().unwrap().is_empty());

    let (status, body, _) = send(&app, bearer(delete(&rename_uri), &alice_token)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");
    let bob_project_id = bob_project["id"].as_str().unwrap();
    let (status, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{bob_project_id}")),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let project_id = project["id"].as_str().unwrap();
    let (status, _, _) = send(
        &app,
        bearer(
            delete(&format!("/api/composition-projects/{project_id}")),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut source_delete = bearer(delete("/api/library/space-source"), &alice_token);
    source_delete
        .headers_mut()
        .insert("x-space-id", space_id.parse().unwrap());
    let (status, _, _) = send(&app, source_delete).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        state.db.source_space_id("space-source").await.unwrap(),
        None
    );
    let (status, _, _) = send(&app, bearer(delete(&rename_uri), &alice_token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, spaces, _) = send(&app, bearer(get("/api/spaces"), &alice_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!spaces
        .as_array()
        .unwrap()
        .iter()
        .any(|space| space["id"] == space_id));

    let other_space_id = other_space["id"].as_str().unwrap();
    let (status, _, _) = send(
        &app,
        bearer(
            put_json(
                &format!("/api/spaces/{other_space_id}/members/space-bob"),
                json!({"role": "editor"}),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, owner, _) = send(
        &app,
        bearer(
            post_json(
                &format!("/api/spaces/{other_space_id}/ownership-transfer"),
                json!({"targetActor": "space-bob"}),
            ),
            &alice_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(owner, json!({"actor": "space-bob", "role": "owner"}));
    let (status, alice_spaces, _) = send(&app, bearer(get("/api/spaces"), &alice_token)).await;
    assert_eq!(status, StatusCode::OK);
    let transferred = alice_spaces
        .as_array()
        .unwrap()
        .iter()
        .find(|space| space["id"] == other_space_id)
        .unwrap();
    assert_eq!(transferred["role"], "editor");
    let (status, bob_spaces, _) = send(&app, bearer(get("/api/spaces"), &bob_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bob_spaces[0]["role"], "owner");
}

#[tokio::test]
async fn space_templates_are_shared_with_members_and_revision_protected() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let owner_token = register_token(&app, "template-owner").await;
    let editor_token = register_token(&app, "template-editor").await;
    let viewer_token = register_token(&app, "template-viewer").await;
    let outsider_token = register_token(&app, "template-outsider").await;
    let (status, space, _) = send(
        &app,
        bearer(
            post_json("/api/spaces", json!({"name": "Brand Studio"})),
            &owner_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let space_id = space["id"].as_str().unwrap();
    for (actor, role) in [("template-editor", "editor"), ("template-viewer", "viewer")] {
        let (status, _, _) = send(
            &app,
            bearer(
                put_json(
                    &format!("/api/spaces/{space_id}/members/{actor}"),
                    json!({"role": role}),
                ),
                &owner_token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let templates_uri = format!("/api/spaces/{space_id}/templates");
    let template = json!({
        "schemaVersion": 1,
        "id": "team-promo",
        "name": "Team Promo",
        "composition": {"schemaVersion": 1, "sources": {}, "tracks": []},
        "slots": []
    });

    let (status, body, _) = send(
        &app,
        bearer(
            post_json(&templates_uri, json!({"template": template})),
            &viewer_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, created, _) = send(
        &app,
        bearer(
            post_json(&templates_uri, json!({"template": template})),
            &editor_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["createdBy"], "template-editor");
    assert_eq!(created["revision"], 1);
    let template_id = created["id"].as_str().unwrap();

    let (status, listed, _) = send(&app, bearer(get(&templates_uri), &viewer_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["template"]["name"], "Team Promo");
    let (status, body, _) = send(&app, bearer(get(&templates_uri), &outsider_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");

    let item_uri = format!("{templates_uri}/{template_id}");
    let renamed = json!({"schemaVersion": 1, "id": "team-promo", "name": "Team Promo v2", "composition": {"schemaVersion": 1, "sources": {}, "tracks": []}, "slots": []});
    let (status, updated, _) = send(
        &app,
        bearer(
            put_json(&item_uri, json!({"baseRevision": 1, "template": renamed})),
            &owner_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["template"]["name"], "Team Promo v2");
    let (status, body, _) = send(
        &app,
        bearer(
            put_json(&item_uri, json!({"baseRevision": 1, "template": template})),
            &editor_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");

    let brand_uri = format!("/api/spaces/{space_id}/brand-kit");
    let (status, empty_brand, _) = send(&app, bearer(get(&brand_uri), &viewer_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty_brand["revision"], 0);
    let brand_kit = json!({
        "colors": [{"name": "Primary", "value": "#3366ff"}],
        "fonts": ["Noto Sans"],
        "logoSourceIds": []
    });
    let (status, body, _) = send(
        &app,
        bearer(
            put_json(&brand_uri, json!({"baseRevision": 0, "kit": brand_kit})),
            &viewer_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
    let (status, brand, _) = send(
        &app,
        bearer(
            put_json(&brand_uri, json!({"baseRevision": 0, "kit": brand_kit})),
            &editor_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(brand["revision"], 1);
    assert_eq!(brand["kit"]["colors"][0]["value"], "#3366ff");
    let (status, body, _) = send(
        &app,
        bearer(
            put_json(&brand_uri, json!({"baseRevision": 0, "kit": brand_kit})),
            &owner_token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");
    let (status, _, _) = send(&app, bearer(delete(&item_uri), &owner_token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn stock_catalog_requires_authentication_and_provider_configuration() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let uri = "/api/stock/search?q=ocean&kind=video&page=1";
    let (status, body, _) = send(&app, get(uri)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let token = register_token(&app, "stock-user").await;
    let (status, body, _) = send(&app, bearer(get(uri), &token)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_api_error(&body, "service_unavailable");
}

#[tokio::test]
async fn youtube_connect_is_authenticated_config_gated_and_uses_one_time_state() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let status_uri = "/api/publish/youtube/status";
    let (status, body, _) = send(&app, get(status_uri)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&body, "unauthorized");
    let token = register_token(&app, "youtube-disabled").await;
    let (status, body, _) = send(&app, bearer(get(status_uri), &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"configured": false, "connected": false}));
    let (status, body, _) = send(
        &app,
        bearer(post_json("/api/publish/youtube/connect", json!({})), &token),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_api_error(&body, "service_unavailable");

    let (state, _directory) = make_state(true, true).await;
    let cipher = video_kadr_backend::youtube::TokenCipher::new([4; 32]).unwrap();
    let state = state.with_youtube_oauth(
        video_kadr_backend::youtube::YouTubeOAuthClient::new(
            video_kadr_backend::youtube::YouTubeOAuthConfig {
                client_id: "client-id".into(),
                client_secret: "server-secret".into(),
                redirect_uri: "http://127.0.0.1:8080/api/publish/youtube/callback".into(),
            },
        )
        .unwrap(),
        cipher.clone(),
    );
    let db = state.db.clone();
    let publish_output_id = uuid::Uuid::new_v4().to_string();
    tokio::fs::write(
        state.outputs_dir().join(format!("{publish_output_id}.mp4")),
        b"rendered-video",
    )
    .await
    .unwrap();
    db.grant_output_access(&publish_output_id, "youtube-user")
        .await
        .unwrap();
    let app = router(state);
    let token = register_token(&app, "youtube-user").await;
    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                "/api/publish/youtube",
                json!({"outputId": publish_output_id, "title": "", "privacyStatus": "private"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");
    let (status, body, _) = send(
        &app,
        bearer(post_json("/api/publish/youtube/connect", json!({})), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let url = url::Url::parse(body["authorizationUrl"].as_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("accounts.google.com"));
    assert!(!url.as_str().contains("server-secret"));
    let csrf_state = url
        .query_pairs()
        .find(|(name, _)| name == "state")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        db.consume_publish_oauth_state("youtube", &csrf_state)
            .await
            .unwrap()
            .as_deref(),
        Some("youtube-user")
    );
    assert_eq!(
        db.consume_publish_oauth_state("youtube", &csrf_state)
            .await
            .unwrap(),
        None
    );

    db.create_publish_oauth_state("youtube", "youtube-user", "denied-state")
        .await
        .unwrap();
    let (status, body, _) = send(
        &app,
        get("/api/publish/youtube/callback?state=denied-state&error=access_denied"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    let (status, body, _) = send(
        &app,
        get("/api/publish/youtube/callback?state=denied-state&error=access_denied"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");

    db.save_youtube_tokens(
        &cipher,
        "youtube-user",
        video_kadr_backend::youtube::YouTubeTokens {
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            expires_at: 2_000_000_000,
            scope: video_kadr_backend::youtube::YOUTUBE_UPLOAD_SCOPE.into(),
            token_type: "Bearer".into(),
        },
    )
    .await
    .unwrap();
    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                "/api/publish/youtube",
                json!({"outputId": publish_output_id, "title": "", "privacyStatus": "private"}),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    let (status, body, _) = send(&app, bearer(get(status_uri), &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"configured": true, "connected": true}));
}

#[tokio::test]
async fn auth_accounts_issue_resolve_and_revoke_hashed_bearer_sessions() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let credentials =
        json!({"username": "Alice.Editor", "password": "correct horse battery staple"});

    let (status, registered, _) =
        send(&app, post_json("/api/auth/register", credentials.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(registered["user"]["username"], "Alice.Editor");
    assert!(registered.get("password").is_none());
    let token = registered["token"].as_str().unwrap();
    assert!(token.len() > 64);

    let (status, duplicate, _) = send(
        &app,
        post_json(
            "/api/auth/register",
            json!({"username": "alice.editor", "password": "another secure password"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&duplicate, "conflict");

    let (status, denied, _) = send(
        &app,
        post_json(
            "/api/auth/login",
            json!({"username": "Alice.Editor", "password": "incorrect password"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&denied, "unauthorized");

    let authenticated = |method: &str, uri: &str, token: &str| {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    let (status, session, _) = send(&app, authenticated("GET", "/api/auth/session", token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["username"], "Alice.Editor");

    let (status, _, _) = send(&app, authenticated("POST", "/api/auth/logout", token)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, revoked, _) = send(&app, authenticated("GET", "/api/auth/session", token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_api_error(&revoked, "unauthorized");
}

#[tokio::test]
async fn composition_project_rejects_invalid_envelopes_and_updates_without_partial_writes() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let token = register_token(&app, "invalid-owner").await;
    let (_, created, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {"source-old": {"id": "source-old"}},
                        "revision": 1
                    }
                }),
            ),
            &token,
        ),
    )
    .await;
    let id = created["id"].as_str().unwrap();

    for invalid in [
        json!({
            "schemaVersion": 3,
            "mode": "composition",
            "document": {"schemaVersion": 1, "sources": {}}
        }),
        json!({
            "schemaVersion": 2,
            "mode": "clip",
            "document": {"schemaVersion": 1, "sources": {}}
        }),
        json!({
            "schemaVersion": 2,
            "mode": "composition",
            "document": {"schemaVersion": 2, "sources": {}}
        }),
        json!({
            "schemaVersion": 2,
            "mode": "composition",
            "document": {"schemaVersion": 1, "sources": {"source-key": {"id": "other"}}}
        }),
        json!({
            "schemaVersion": 2,
            "mode": "composition",
            "document": {"schemaVersion": 1, "sources": {"../escape": {"id": "../escape"}}}
        }),
    ] {
        let (status, body, _) = send(
            &app,
            bearer(
                put_json(&format!("/api/composition-projects/{id}"), invalid),
                &token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_api_error(&body, "bad_request");
    }

    let (status, unchanged, _) = send(
        &app,
        bearer(get(&format!("/api/composition-projects/{id}")), &token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unchanged["document"]["revision"], 1);
    assert_eq!(unchanged["sourceIds"], json!(["source-old"]));

    let (status, body, _) = send(
        &app,
        bearer(
            put_json(
                "/api/composition-projects/00000000-0000-4000-8000-000000000000",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "baseRevision": 1,
                    "document": {"schemaVersion": 1, "sources": {}}
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_api_error(&body, "forbidden");
}

#[tokio::test]
async fn composition_project_document_has_an_independent_two_mib_limit() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let token = register_token(&app, "size-owner").await;
    let oversized = "x".repeat(MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES + 1);

    let (status, body, _) = send(
        &app,
        bearer(
            post_json(
                "/api/composition-projects",
                json!({
                    "schemaVersion": 2,
                    "mode": "composition",
                    "document": {
                        "schemaVersion": 1,
                        "sources": {},
                        "blob": oversized
                    }
                }),
            ),
            &token,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_api_error(&body, "payload_too_large");
    let (_, projects, _) = send(&app, bearer(get("/api/composition-projects"), &token)).await;
    assert!(projects.as_array().unwrap().is_empty());
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
        let store = video_kadr_backend::jobs::SqliteJobStore::new(db);
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
