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
    let response = restarted
        .clone()
        .oneshot(req(
            "POST",
            &format!("{path}/create?fills=%5B%5D&name=Second+cut"),
            &owner.token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let named = json_body(response).await;
    assert_eq!(named["project"]["name"], "Second cut");
    assert_ne!(named["project"]["id"], created["project"]["id"]);

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

#[tokio::test]
async fn space_catalogue_shares_packages_and_checks_current_membership() {
    let (state, _dir) = support::make_state(false, false).await;
    let register = |name| state.db.register_auth_user(name, "template library password");
    let owner = register("catalogue-owner").await.unwrap().unwrap();
    let editor = register("catalogue-editor").await.unwrap().unwrap();
    let viewer = register("catalogue-viewer").await.unwrap().unwrap();
    let outsider = register("catalogue-outsider").await.unwrap().unwrap();
    let space = state.db.create_space(&owner.user.username, "Designs").await.unwrap();
    for (actor, role) in [
        (&editor, video_kadr_backend::db::SpaceRole::Editor),
        (&viewer, video_kadr_backend::db::SpaceRole::Viewer),
    ] {
        state.db.set_space_member(&space.id, &owner.user.username, &actor.user.username, role)
            .await.unwrap().unwrap();
    }
    let project: NestedCompositionProject = serde_json::from_value(json!({
        "schemaVersion":1,"root":"root","compositions":{
            "root":{"content":Composition::new(CanvasSpec::default()),"instances":[]}
        }
    })).unwrap();
    let package = template_bundle::write(Cursor::new(Vec::new()), "Design", &project, &[])
        .unwrap().into_inner();
    let scoped = |mut request: Request<Body>| {
        request.headers_mut().insert("x-space-id", space.id.parse().unwrap());
        request
    };
    let app = support::router(state.clone());
    let personal = app.clone().oneshot(upload(&package, &owner.token)).await.unwrap();
    assert_eq!(personal.status(), StatusCode::CREATED);
    let personal = json_body(personal).await;
    let denied = app.clone().oneshot(scoped(upload(&package, &viewer.token))).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let shared = app.clone().oneshot(scoped(upload(&package, &owner.token))).await.unwrap();
    assert_eq!(shared.status(), StatusCode::CREATED);
    let shared = json_body(shared).await;
    let path = format!("/api/template-library/{}", shared["id"].as_str().unwrap());

    let list = app.clone().oneshot(scoped(req("GET", "/api/template-library", &viewer.token))).await.unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list = json_body(list).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], shared["id"]);
    assert_eq!(list[0]["canManage"], false);
    assert_eq!(list[0]["spaceId"], space.id);
    let private = app.clone().oneshot(req("GET", "/api/template-library", &owner.token)).await.unwrap();
    assert_eq!(json_body(private).await, json!([personal]));
    let denied = app.clone().oneshot(req("GET", &path, &outsider.token)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);

    let download = app.clone().oneshot(req("GET", &path, &viewer.token)).await.unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(to_bytes(download.into_body(), 1 << 20).await.unwrap().as_ref(), package);
    let create = format!("{path}/create?fills=%5B%5D");
    let denied = app.clone().oneshot(req("POST", &create, &viewer.token)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let denied = app.clone().oneshot(req("DELETE", &path, &viewer.token)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let created = app.clone().oneshot(req("POST", &create, &editor.token)).await.unwrap();
    let status = created.status();
    let created = json_body(created).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let created = state.db.get_composition_project_for(
        created["project"]["id"].as_str().unwrap(), &viewer.user.username,
    ).await.unwrap().unwrap();
    assert_eq!(created.space_id.as_deref(), Some(space.id.as_str()));

    assert_eq!(
        state.db.remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
            .await.unwrap(),
        Some(true),
    );
    let denied = app.clone().oneshot(req("GET", &path, &viewer.token)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);

    let deleted = app.oneshot(req("DELETE", &path, &editor.token)).await.unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}
