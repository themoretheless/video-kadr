mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
#[tokio::test]
async fn cancellation_before_submission_is_durable_and_scoped() {
    let (state, _root) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("operation-owner", "public fixture password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("operation-outsider", "public fixture password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space("operation-owner", "Operations")
        .await
        .unwrap();
    let document = serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(
        Default::default(),
    ))
    .unwrap();
    let project = state
        .db
        .create_space_composition_project("Operation", &document, &[], "operation-owner", &space.id)
        .await
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let operation = uuid::Uuid::new_v4().to_string();
    let prefix = format!(
        "/api/composition-projects/{}/concat-video-cutout/smart-region",
        project.id
    );
    let path = format!("{prefix}/operations/{operation}");
    let request = |method: &str, path: &str, token: &str, body: Value| {
        Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &path, &owner.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &(path.clone() + "/cancel"),
                &outsider.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &(path.clone() + "/cancel"),
            &owner.token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(first["cancelled"], true);
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            &(path.clone() + "/cancel"),
            &owner.token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let again: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(again, first);
    let response = app
        .clone()
        .oneshot(request("GET", &path, &owner.token, Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(receipt["cancelled"], true);
    assert_eq!(receipt["enqueued"], false);
    assert!(receipt["started"].is_null());
    let start = json!({"operationId":operation,"clipId":"clip","expectedRevision":project.revision,"timeUs":0,"points":[[0.5,0.5]]});
    assert_eq!(
        app.clone()
            .oneshot(request("POST", &prefix, &owner.token, start))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let malformed = format!("{prefix}/operations/not-a-uuid");
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &malformed, &owner.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sam_job_access")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let other = state
        .db
        .create_space_composition_project("Other", &document, &[], "operation-owner", &space.id)
        .await
        .unwrap();
    let other_path = path.replace(&project.id, &other.id);
    assert_eq!(
        app.clone()
            .oneshot(request("GET", &other_path, &owner.token, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}
