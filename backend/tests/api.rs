//! HTTP-level integration tests. They drive the real router via
//! `tower::ServiceExt::oneshot` (no socket bound) against an isolated temp
//! storage dir. Most tests avoid external tools; one upload security regression
//! test generates a tiny MP4 and skips itself when ffmpeg is unavailable.

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use video_editor_backend::build_router;
use video_editor_backend::db::Db;
use video_editor_backend::library::{Library, MediaEntry};
use video_editor_backend::model::{EditRequest, Job, JobStatus};
use video_editor_backend::state::{AppState, ToolInfo};

const UPLOAD_LIMIT: usize = 64 * 1024 * 1024;

/// An app state backed by a fresh temp dir. The `TempDir` is returned so the
/// caller keeps it alive (dropping it deletes the storage).
async fn make_state(ffmpeg: bool, ytdlp: bool) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    tokio::fs::create_dir_all(storage.join("sources"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(storage.join("outputs"))
        .await
        .unwrap();
    let lib = Library::load(storage.clone()).await;
    let db = Db::open(&storage).await.unwrap();
    let tools = ToolInfo {
        ffmpeg,
        ytdlp,
        ffmpeg_version: None,
        ytdlp_version: None,
    };
    (AppState::new(storage, 2, tools, lib, db), dir)
}

fn router(state: AppState) -> Router {
    build_router(state, UPLOAD_LIMIT)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

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

fn post_multipart_file(filename: &str, bytes: &[u8]) -> Request<Body> {
    let boundary = "BOUNDARY_UPLOAD_SECURITY";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
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

fn delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn assert_api_error(body: &Value, code: &str) {
    assert_eq!(body["code"], code);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "expected a non-empty API error, got: {body}"
    );
}

/// Send a request and return (status, parsed-json-or-Null, raw-text).
async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value, String) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, text)
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
async fn unknown_job_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, get("/api/jobs/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_api_error(&body, "not_found");
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
async fn edit_cache_hit_returns_existing_output() {
    let (state, _d) = make_state(true, true).await;
    // Pre-seed the render cache for a specific edit, with its output file present.
    let req_json = json!({ "videoId": "vidX", "trim": { "start": 0.0, "end": 5.0 } });
    let req: EditRequest = serde_json::from_value(req_json.clone()).unwrap();
    let key = video_editor_backend::handlers::render_cache_key(&req);
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
    let key = video_editor_backend::handlers::render_cache_key(&req);
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

#[tokio::test]
async fn upload_rejects_empty_file() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let boundary = "BOUNDARYX";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\n\r\n\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let (status, body, text) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(text.contains("пустой файл"), "got: {text}");
}

#[tokio::test]
async fn upload_limit_fails_fast_and_recovers_after_a_slot_is_released() {
    let (state, _d) = make_state(true, true).await;
    let slot_a = state.try_acquire_upload_slot().unwrap();
    let _slot_b = state.try_acquire_upload_slot().unwrap();
    let app = router(state);

    let (status, body, text) = send(&app, post_multipart_file("clip.mp4", b"data")).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_api_error(&body, "too_many_requests");
    assert!(text.contains("слишком много одновременных загрузок"));

    drop(slot_a);
    let (status, body, text) = send(&app, post_multipart_file("clip.mp4", b"")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(text.contains("пустой файл"));
}

#[tokio::test]
async fn upload_without_file_field_is_400() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let boundary = "BOUNDARYY";
    // A plain text field, not a file: the handler skips it and reports none found.
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"hello\"\r\n\r\nworld\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let (status, body, text) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
    assert!(text.contains("файл не найден"), "got: {text}");
}

#[tokio::test]
async fn upload_body_limit_uses_json_error_envelope() {
    let (state, _d) = make_state(true, true).await;
    let app = build_router(state, 64);

    let (status, body, _) = send(&app, post_multipart_file("clip.mp4", &[b'x'; 128])).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_api_error(&body, "payload_too_large");
}

#[tokio::test]
async fn upload_ignores_spoofed_html_extension_and_static_files_do_not_sniff() {
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
    let uploaded: Value = serde_json::from_slice(&body).unwrap();
    let filename = uploaded["filename"].as_str().unwrap();
    assert!(filename.ends_with(".mp4"), "got {filename}");
    assert!(!filename.ends_with(".html"));

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
