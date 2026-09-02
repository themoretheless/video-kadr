//! HTTP-level integration tests. They drive the real router via
//! `tower::ServiceExt::oneshot` (no socket bound) against an isolated temp
//! storage dir. Upload-specific threat regressions live in
//! `upload_security.rs` and share the same test harness.

mod support;

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
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
    let (status, accepted, _) = send(&app, post_json("/api/compositions/render", legacy)).await;
    assert_eq!(status, StatusCode::OK);
    let job_id = accepted["jobId"].as_str().unwrap();

    let mut canonical = composition_request("missing-source");
    canonical["output"] = json!({
        "profile": { "container": "mp4", "codec": "h264" },
        "qualityTier": "medium"
    });
    let (status, duplicate, _) = send(&app, post_json("/api/compositions/render", canonical)).await;
    assert_eq!(status, StatusCode::OK);
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
    let (created_status, project, _) = send(
        &app,
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
        delete(&format!("/api/composition-projects/{project_id}")),
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

    let (status, empty, _) = send(&app, get("/api/composition-projects")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty.as_array().unwrap().is_empty());

    let (status, first, _) = send(
        &app,
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
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = first["id"].as_str().unwrap().to_owned();
    assert!(uuid::Uuid::parse_str(&first_id).is_ok());
    assert_eq!(first["schemaVersion"], 2);
    assert_eq!(first["mode"], "composition");
    assert_eq!(first["sourceIds"], json!(["source-a", "source-b"]));

    let (status, stored, _) =
        send(&app, get(&format!("/api/composition-projects/{first_id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stored, first);

    let (status, second, _) = send(
        &app,
        post_json(
            "/api/composition-projects",
            json!({
                "schemaVersion": 2,
                "mode": "composition",
                "document": {"schemaVersion": 1, "sources": {}, "tracks": []}
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["name"], "Без названия");

    let (status, updated, _) = send(
        &app,
        put_json(
            &format!("/api/composition-projects/{first_id}"),
            json!({
                "schemaVersion": 2,
                "mode": "composition",
                "name": "Updated",
                "document": {
                    "schemaVersion": 1,
                    "sources": {"source-c": {"id": "source-c", "kind": "image"}},
                    "tracks": [],
                    "revision": 2
                }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["id"], first_id);
    assert_eq!(updated["createdAt"], first["createdAt"]);
    assert_eq!(updated["sourceIds"], json!(["source-c"]));
    assert_eq!(updated["document"]["revision"], 2);

    let (status, projects, _) = send(&app, get("/api/composition-projects")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    assert_eq!(projects[0]["id"], first_id, "last write sorts first");

    let (status, legacy, _) = send(&app, get("/api/projects")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(legacy.as_array().unwrap().is_empty());

    let (status, _, _) = send(
        &app,
        delete(&format!("/api/composition-projects/{first_id}")),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body, _) = send(&app, get(&format!("/api/composition-projects/{first_id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn composition_project_rejects_invalid_envelopes_and_updates_without_partial_writes() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let (_, created, _) = send(
        &app,
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
            put_json(&format!("/api/composition-projects/{id}"), invalid),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_api_error(&body, "bad_request");
    }

    let (status, unchanged, _) = send(&app, get(&format!("/api/composition-projects/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unchanged["document"]["revision"], 1);
    assert_eq!(unchanged["sourceIds"], json!(["source-old"]));

    let (status, body, _) = send(
        &app,
        put_json(
            "/api/composition-projects/00000000-0000-4000-8000-000000000000",
            json!({
                "schemaVersion": 2,
                "mode": "composition",
                "document": {"schemaVersion": 1, "sources": {}}
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
}

#[tokio::test]
async fn composition_project_document_has_an_independent_two_mib_limit() {
    let (state, _directory) = make_state(true, true).await;
    let app = router(state);
    let oversized = "x".repeat(MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES + 1);

    let (status, body, _) = send(
        &app,
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
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_api_error(&body, "payload_too_large");
    let (_, projects, _) = send(&app, get("/api/composition-projects")).await;
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
