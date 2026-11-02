mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
async fn value(r: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn enhance_catalog_queue_access_and_cancel() {
    let (state, _dir) = support::make_state(true, false).await;
    let a = state
        .db
        .register_auth_user("download-a", "download test password")
        .await
        .unwrap()
        .unwrap();
    let b = state
        .db
        .register_auth_user("download-b", "download test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let model = video_kadr_backend::services::enhance_model::Catalog::bundled().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/enhance/models/downloads",
                None,
                json!({"catalogId":model.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"catalogId":"https://example.org/model"}),
        json!({"catalogId":model.id,"url":"https://example.org"}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/enhance/models/downloads",
                    Some(&a.token),
                    body
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut permits = vec![];
    for _ in
        0..state.resource_limit(video_kadr_backend::config::resource_classes::ResourceClass::Ingest)
    {
        permits.push(
            state
                .acquire_job_slot(
                    video_kadr_backend::config::resource_classes::ResourceClass::Ingest,
                )
                .await
                .unwrap(),
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/enhance/models/downloads",
            Some(&a.token),
            json!({"catalogId":model.id}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let job = value(response).await;
    let id = job["jobId"].as_str().unwrap();
    for (method, suffix) in [
        ("GET", ""),
        ("POST", "/cancel"),
        ("POST", "/retry"),
        ("POST", "/discard"),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    method,
                    &format!("/api/jobs/{id}{suffix}"),
                    Some(&b.token),
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let own = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/enhance/models/downloads",
                Some(&a.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(own["jobs"][0]["jobId"], id);
    let other = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/enhance/models/downloads",
                Some(&b.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(other["jobs"].as_array().unwrap().is_empty());
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/enhance/models/downloads?cursor={id}"),
                Some(&b.token),
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/jobs/{id}/cancel"),
                Some(&a.token),
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    drop(permits);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if state.get_job(id).await.unwrap().unwrap().status
                == video_kadr_backend::model::JobStatus::Cancelled
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let library = value(
        app.oneshot(request(
            "GET",
            "/api/enhance/models",
            Some(&a.token),
            Value::Null,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(library["models"].as_array().unwrap().is_empty());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
#[ignore = "downloads the explicitly selected complete pinned public ENHANCE bundle"]
async fn fresh_public_bundle_download_and_explicit_selection() {
    use video_kadr_backend::{
        db::Db,
        library::Library,
        model::JobStatus,
        state::{AppState, ToolInfo},
    };
    let root = std::path::PathBuf::from(
        std::env::var("VIDEO_KADR_ENHANCE_DOWNLOAD_TEST_STORAGE").expect("fresh retained test storage"),
    );
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh storage must not already exist");
    let state = AppState::new(
        root.clone(),
        2,
        ToolInfo::default(),
        Library::load(root.clone()).await,
        Db::open(&root).await.unwrap(),
    );
    let state=state.with_enhance_config(Some(video_kadr_backend::services::local_enhance::Config{executable:"/not-required-for-selection".into(),model:"/unused-model".into(),runtime_library:"/unused-runtime".into()}));
    let owner = state
        .db
        .register_auth_user("enhance-fresh-download", "download test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let catalog = match std::env::var("VIDEO_KADR_ENHANCE_DOWNLOAD_TEST_CATALOG") {
        Ok(id) => video_kadr_backend::services::enhance_model::Catalog::find(&id)
            .unwrap()
            .unwrap(),
        Err(_) => video_kadr_backend::services::enhance_model::Catalog::bundled().unwrap(),
    };
    let mut permits=vec![];
    for _ in 0..state.resource_limit(video_kadr_backend::config::resource_classes::ResourceClass::Ingest) {
        permits.push(state.acquire_job_slot(video_kadr_backend::config::resource_classes::ResourceClass::Ingest).await.unwrap());
    }
    let admitted = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/enhance/models/downloads",
            Some(&owner.token),
            json!({"catalogId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    state.begin_shutdown();drop(permits);
    assert!(state.wait_for_tasks(std::time::Duration::from_secs(10)).await);
    let state=AppState::new(root.clone(),2,ToolInfo::default(),Library::load(root.clone()).await,Db::open(&root).await.unwrap())
        .with_enhance_config(Some(video_kadr_backend::services::local_enhance::Config{executable:"/not-required-for-selection".into(),model:"/unused-model".into(),runtime_library:"/unused-runtime".into()}));
    state.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&state).await;
    let app=support::router(state.clone());
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(900), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
    .await;
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
    let job = outcome.unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    catalog
        .verify_directory(&root.join("models/enhance").join(&catalog.id))
        .unwrap();
    let library = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/enhance/models",
                Some(&owner.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(library["selectedId"], "configured");
    assert_eq!(library["models"][0]["id"], catalog.id);
    assert_eq!(
        app.clone().oneshot(request(
            "POST",
            "/api/enhance/models/select",
            Some(&owner.token),
            json!({"modelId":catalog.id})
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    let other=state.db.register_auth_user("enhance-other", "download test password").await.unwrap().unwrap();
    assert_eq!(app.clone().oneshot(request("POST","/api/enhance/models/select",Some(&other.token),json!({"modelId":catalog.id}))).await.unwrap().status(),StatusCode::FORBIDDEN);
    let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(root.join("app.db"))).await.unwrap();
    // A retained enhancement request must pin bytes after owner removal.
    sqlx::query("INSERT INTO job_requests(job_id,kind,payload_json,dedupe_key,created_at) VALUES('retained-enhancement','enhance',?,'retained-enhancement',0)").bind(json!({"model_id":catalog.id}).to_string()).execute(&pool).await.unwrap();
    let removed=value(app.clone().oneshot(request("POST","/api/enhance/models/remove",Some(&owner.token),json!({"modelId":catalog.id}))).await.unwrap()).await;
    assert_eq!(removed["removed"],true); assert_eq!(removed["fileReleased"],false);
    catalog.verify_directory(&root.join("models/enhance").join(&catalog.id)).unwrap();
    assert_eq!(app.clone().oneshot(request("POST","/api/enhance/models/select",Some(&owner.token),json!({"modelId":catalog.id}))).await.unwrap().status(),StatusCode::NOT_FOUND);
    sqlx::query("DELETE FROM job_requests WHERE job_id='retained-enhancement'").execute(&pool).await.unwrap();
    let listed=value(app.clone().oneshot(request("GET","/api/enhance/models",Some(&owner.token),Value::Null)).await.unwrap()).await;
    assert!(listed["models"].as_array().unwrap().is_empty());
    assert!(!root.join("models/enhance").join(&catalog.id).exists());

}

