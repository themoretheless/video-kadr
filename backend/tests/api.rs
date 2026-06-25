//! HTTP-level integration tests. They drive the real router via
//! `tower::ServiceExt::oneshot` (no socket bound) against an isolated temp
//! storage dir, and deliberately avoid needing ffmpeg/yt-dlp: the import/edit
//! error paths reject before shelling out, so the suite is deterministic.

use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use video_editor_backend::build_router;
use video_editor_backend::db::Db;
use video_editor_backend::library::{Library, MediaEntry};
use video_editor_backend::model::{Job, JobStatus};
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

fn delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
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
    let (status, _b, _) = send(&app, get("/api/jobs/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
async fn cancel_unknown_job_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, _b, _) = send(&app, post_empty("/api/jobs/ghost/cancel")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
    assert_eq!(body["error"], "job already finished");
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
async fn project_by_video_missing_is_404() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, _b, _) = send(&app, get("/api/projects/by-video/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn project_upsert_requires_video_and_edit() {
    let (state, _d) = make_state(true, true).await;
    let app = router(state);
    let (status, _b, _) = send(&app, post_json("/api/projects", json!({ "videoId": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
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
    let (status, _b, text) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("пустой файл"), "got: {text}");
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
    let (status, _b, text) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("файл не найден"), "got: {text}");
}
