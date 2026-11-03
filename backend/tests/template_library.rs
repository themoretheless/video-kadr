mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use serde_json::{json, Value};
use std::io::Cursor;
use tower::ServiceExt;
use video_kadr_backend::{
    domain::{
        composition::{CanvasSpec, Composition},
        nested_composition::NestedCompositionProject,
    },
    template_bundle,
};
fn upload(bytes: &[u8], token: &str) -> Request<Body> {
    let mut body=b"--TPL\r\nContent-Disposition: form-data; name=\"file\"; filename=\"edit.vktemplate\"\r\n\r\n".to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--TPL--\r\n");
    Request::post("/api/template-library")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=TPL")
        .body(Body::from(body))
        .unwrap()
}
fn req(method: &str, path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn private_library_survives_reopen_creates_without_reupload_and_rolls_back_failed_install() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("library-owner", "template library password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("library-other", "template library password")
        .await
        .unwrap()
        .unwrap();
    let content = Composition::new(CanvasSpec::default());
    let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}})).unwrap();
    let bytes = template_bundle::write(Cursor::new(Vec::new()), "Saved design", &project, &[])
        .unwrap()
        .into_inner();
    let app = support::router(state.clone());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_library_install BEFORE INSERT ON native_template_library BEGIN SELECT RAISE(ABORT,'controlled library failure'); END").execute(&pool).await.unwrap();
    assert_eq!(
        app.clone()
            .oneshot(upload(&bytes, &owner.token))
            .await
            .unwrap()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(
        std::fs::read_dir(state.storage.join("template-library"))
            .unwrap()
            .count(),
        0
    );
    sqlx::query("DROP TRIGGER reject_library_install")
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(upload(&bytes, &owner.token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let entry = json_body(response).await;
    let id = entry["id"].as_str().unwrap();
    let path = format!("/api/template-library/{id}");
    assert_eq!(entry["name"], "Saved design");
    assert_eq!(
        app.clone()
            .oneshot(upload(&bytes, &owner.token))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        std::fs::read_dir(state.storage.join("template-library"))
            .unwrap()
            .count(),
        1
    );
    assert!(entry.get("sha256").is_none());
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let listing = json_body(
        app.clone()
            .oneshot(req("GET", "/api/template-library", &owner.token))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listing.as_array().unwrap().len(), 1);
    assert_eq!(
        json_body(
            app.clone()
                .oneshot(req("GET", "/api/template-library", &other.token))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
    for (method, uri) in [
        ("GET", path.clone()),
        ("DELETE", path.clone()),
        ("POST", format!("{path}/create?fills=%5B%5D")),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(req(method, &uri, &other.token))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    sqlx::query("ALTER TABLE native_template_library DROP COLUMN poster_png")
        .execute(&pool)
        .await
        .unwrap();
    let mut reopened = state.clone();
    reopened.db = video_kadr_backend::db::Db::open(&state.storage)
        .await
        .unwrap()
        .into();
    let restarted = support::router(reopened);
    let response = restarted
        .clone()
        .oneshot(req("GET", &path, &owner.token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .as_ref(),
        bytes.as_slice()
    );
    let response = restarted
        .clone()
        .oneshot(req(
            "POST",
            &format!("{path}/create?fills=%5B%5D"),
            &owner.token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_body(response).await;
    assert_eq!(created["project"]["name"], "Saved design");
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let stored = state
        .storage
        .join("template-library")
        .join(format!("{id}.vktemplate"));
    let mut tampered = bytes.clone();
    tampered[0] ^= 1;
    std::fs::write(&stored, &tampered).unwrap();
    assert_eq!(
        app.clone()
            .oneshot(req("GET", &path, &owner.token))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.clone()
            .oneshot(req(
                "POST",
                &format!("{path}/create?fills=%5B%5D"),
                &owner.token
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(
        app.clone()
            .oneshot(req("DELETE", &path, &owner.token))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(!stored.exists());
    assert_eq!(
        json_body(
            app.clone()
                .oneshot(req("GET", "/api/template-library", &owner.token))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
    let disposable = state
        .db
        .register_auth_user("disposable-template-owner", "template library password")
        .await
        .unwrap()
        .unwrap();
    let response = app
        .clone()
        .oneshot(upload(&bytes, &disposable.token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let private = json_body(response).await;
    sqlx::query("DELETE FROM auth_users WHERE username='disposable-template-owner'")
        .execute(&pool)
        .await
        .unwrap();
    let replacement = state
        .db
        .register_auth_user("disposable-template-owner", "replacement account password")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        json_body(
            app.clone()
                .oneshot(req("GET", "/api/template-library", &replacement.token))
                .await
                .unwrap()
        )
        .await,
        json!([])
    );
    assert_eq!(
        app.oneshot(req(
            "GET",
            &format!("/api/template-library/{}", private["id"].as_str().unwrap()),
            &replacement.token
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::NOT_FOUND
    );
}
