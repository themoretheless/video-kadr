mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(token: Option<&str>, body: Value) -> Request<Body> {
    let mut r = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/import-ready")
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
#[tokio::test]
async fn saved_ready_import_requires_identity_project_access_and_server_owned_references() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("owner", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("outsider", "synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state.db.create_space("owner", "Private").await.unwrap();
    let document = serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(
        Default::default(),
    ))
    .unwrap();
    let project = state
        .db
        .create_space_composition_project("Source", &document, &[], "owner", &space.id)
        .await
        .unwrap();
    let body = json!({"request":{"schemaVersion":1,"document":{"version":1,"media":[],"timelines":[{"id":"main","clips":[{"id":"clip","kind":"image","cutout":{"mode":"auto","feather":0}}]}]},"mediaBindings":{},"preserveMediaIds":true},"preparedStills":[{"timelineId":"main","clipId":"clip","projectId":project.id,"compositionId":null,"sourceClipId":"missing","expectedRevision":project.revision}]});
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(None, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&outsider.token), body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&owner.token), body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut empty = body.clone();
    empty["preparedStills"] = json!([]);
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&owner.token), empty))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut injected = body;
    injected["preparedStills"][0]["maskPath"] = json!("/tmp/untrusted.png");
    assert_eq!(
        app.oneshot(request(Some(&owner.token), injected))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(state.library.list().await.is_empty());
    assert_eq!(
        state
            .db
            .get_composition_project_for(&project.id, "owner")
            .await
            .unwrap()
            .unwrap()
            .revision,
        project.revision
    );
}

#[tokio::test]
async fn saved_ready_import_keeps_its_body_budget_before_file_verification() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let limit = 4 * 1024 * 1024
        + video_kadr_backend::domain::concat_project_fonts::MAX_FONT_JSON_BYTES
        + 1024 * 1024;
    let request = Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-project/import-ready")
        .header("content-type", "application/json")
        .body(Body::from(vec![b' '; limit + 1]))
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}
