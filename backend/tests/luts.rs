//! HTTP contract tests for private immutable LUT assets.

mod support;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use uuid::Uuid;

use support::{assert_api_error, get, make_state, router, send};

fn identity_cube() -> &'static [u8] {
    b"# upload comment\nTITLE \"Ignored internal title\"\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n"
}

fn upload(filename: &str, bytes: &[u8]) -> Request<Body> {
    let boundary = "BOUNDARY_LUT_UPLOAD";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/luts")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn upload_is_canonical_private_and_resolvable() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state.clone());
    let (status, uploaded, _) =
        send(&app, upload("../private/Film Look.cube", identity_cube())).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(uploaded["schemaVersion"], 1);
    assert_eq!(uploaded["name"], "Film Look");
    assert_eq!(uploaded["kind"], "cube3d");
    assert_eq!(uploaded["cubeSize"], 2);
    assert!(uploaded.get("filename").is_none());
    assert!(uploaded.get("url").is_none());
    let id = uploaded["id"].as_str().unwrap();
    assert!(Uuid::parse_str(id).is_ok());

    let asset = state.db.get_lut(id).await.unwrap().unwrap();
    let stored = tokio::fs::read(storage.path().join("luts").join(&asset.filename))
        .await
        .unwrap();
    let stored_text = String::from_utf8(stored).unwrap();
    assert!(stored_text.starts_with("LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\n"));
    assert!(!stored_text.contains("TITLE"));
    assert!(!stored_text.contains("upload comment"));

    let (get_status, got, _) = send(&app, get(&format!("/api/luts/{id}"))).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(got["sha256"], uploaded["sha256"]);
    let (list_status, listed, _) = send(&app, get("/api/luts")).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let (private_status, _, _) = send(&app, get(&format!("/files/luts/{}", asset.filename))).await;
    assert_eq!(private_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_canonical_content_reuses_the_immutable_asset() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);
    let (_, first, _) = send(&app, upload("First.cube", identity_cube())).await;
    let (status, second, _) = send(&app, upload("Second.cube", identity_cube())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["id"], first["id"]);
    let mut entries = tokio::fs::read_dir(storage.path().join("luts"))
        .await
        .unwrap();
    assert!(entries.next_entry().await.unwrap().is_some());
    assert!(entries.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn duplicate_upload_repairs_a_missing_or_corrupt_stored_asset() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state.clone());
    let (_, first, _) = send(&app, upload("First.cube", identity_cube())).await;
    let id = first["id"].as_str().unwrap();
    let asset = state.db.get_lut(id).await.unwrap().unwrap();
    let path = storage.path().join("luts").join(&asset.filename);
    let canonical = tokio::fs::read(&path).await.unwrap();

    tokio::fs::remove_file(&path).await.unwrap();
    let (missing_status, after_missing, _) =
        send(&app, upload("Restore missing.cube", identity_cube())).await;
    assert_eq!(missing_status, StatusCode::OK);
    assert_eq!(after_missing["id"], first["id"]);
    assert_eq!(tokio::fs::read(&path).await.unwrap(), canonical);

    tokio::fs::write(&path, b"corrupt").await.unwrap();
    let (corrupt_status, after_corrupt, _) =
        send(&app, upload("Restore corrupt.cube", identity_cube())).await;
    assert_eq!(corrupt_status, StatusCode::OK);
    assert_eq!(after_corrupt["id"], first["id"]);
    assert_eq!(tokio::fs::read(&path).await.unwrap(), canonical);

    let mut entries = tokio::fs::read_dir(storage.path().join("luts"))
        .await
        .unwrap();
    assert!(entries.next_entry().await.unwrap().is_some());
    assert!(entries.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn malformed_and_one_dimensional_luts_fail_closed() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state);
    let (status, body, _) = send(&app, upload("short.cube", b"LUT_3D_SIZE 2\n0 0 0\n")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");

    let (status, body, _) =
        send(&app, upload("one-d.cube", b"LUT_1D_SIZE 2\n0 0 0\n1 1 1\n")).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_api_error(&body, "unsupported_media_type");
    let mut staged = tokio::fs::read_dir(storage.path().join("staging"))
        .await
        .unwrap();
    assert!(staged.next_entry().await.unwrap().is_none());
    let mut published = tokio::fs::read_dir(storage.path().join("luts"))
        .await
        .unwrap();
    assert!(published.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn unknown_or_invalid_ids_are_not_found() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    for id in ["not-a-uuid", "00000000-0000-4000-8000-000000000000"] {
        let (status, body, _) = send(&app, get(&format!("/api/luts/{id}"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_api_error(&body, "not_found");
    }
}

#[tokio::test]
async fn catalog_search_and_favorites_are_bounded_and_persistent() {
    let (state, _storage) = make_state(true, true).await;
    let app = router(state);
    let (_, uploaded, _) = send(&app, upload("Кино 100%_Look.cube", identity_cube())).await;
    let id = uploaded["id"].as_str().unwrap();
    let favorite = Request::builder()
        .method("PUT")
        .uri(format!("/api/luts/{id}/favorite"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"favorite":true}"#))
        .unwrap();
    let (status, updated, _) = send(&app, favorite).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["favorite"], true);

    let (status, listed, _) = send(&app, get("/api/luts?favorite=true&q=100%25_")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], id);
    let (status, listed, _) = send(&app, get("/api/luts?q=кино")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed[0]["id"], id);
    let too_long = "x".repeat(129);
    let (status, body, _) = send(&app, get(&format!("/api/luts?q={too_long}"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}

#[tokio::test]
async fn content_download_revalidates_immutable_bytes() {
    let (state, storage) = make_state(true, true).await;
    let app = router(state.clone());
    let (_, uploaded, _) = send(&app, upload("Look.cube", identity_cube())).await;
    let id = uploaded["id"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(get(&format!("/api/luts/{id}/content")))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/x-cube");
    assert!(response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains(id));
    let downloaded = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&downloaded).starts_with("LUT_3D_SIZE 2\n"));

    let asset = state.db.get_lut(id).await.unwrap().unwrap();
    tokio::fs::write(storage.path().join("luts").join(asset.filename), b"corrupt")
        .await
        .unwrap();
    let (status, body, _) = send(&app, get(&format!("/api/luts/{id}/content"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_api_error(&body, "conflict");
}

#[tokio::test]
async fn baker_is_strict_and_returns_a_canonical_33_cube() {
    let (state, _storage) = make_state(false, false).await;
    let app = router(state);
    let request = Request::builder().method("POST").uri("/api/luts/bake")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"size":33,"edit":{"brightness":0.1,"contrast":1.2,"saturation":0.8,"filter":"grayscale","curves":{"master":[{"x":0.0,"y":0.0},{"x":1.0,"y":1.0}]}}}"#)).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/x-cube");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed = video_editor_backend::luts::parse_cube(&bytes).unwrap();
    assert_eq!(parsed.cube_size, 33);

    let invalid = Request::builder()
        .method("POST")
        .uri("/api/luts/bake")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"size":17,"edit":{}}"#))
        .unwrap();
    let (status, body, _) = send(&app, invalid).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_api_error(&body, "bad_request");
}
