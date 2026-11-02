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
async fn pinned_catalog_queue_access_and_cancel() {
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
    let catalog = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/model-catalog",
                None,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        catalog["revision"],
        "5359861c739e955e79d9a303bcbc70fb988958b1"
    );
    let models = catalog["models"].as_array().unwrap();
    assert_eq!(models.len(), 33);
    let tiny = models.iter().find(|m| m["id"] == "tiny-q5_1").unwrap();
    assert_eq!(tiny["bytes"], 32152673);
    assert_eq!(
        tiny["sha256"],
        "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7"
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/model-downloads",
                None,
                json!({"catalogId":"tiny-q5_1"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"catalogId":"https://example.org/model"}),
        json!({"catalogId":"tiny-q5_1","url":"https://example.org"}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/speech/model-downloads",
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
            "/api/speech/model-downloads",
            Some(&a.token),
            json!({"catalogId":"tiny-q5_1"}),
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
                "/api/speech/model-downloads",
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
                "/api/speech/model-downloads",
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
                &format!("/api/speech/model-downloads?cursor={id}"),
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
            "/api/speech/model-library",
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
#[ignore = "downloads the pinned public 32MB Whisper model over HTTPS"]
async fn real_pinned_download_installs_without_changing_selection() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("download-real", "download test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/model-downloads",
            Some(&owner.token),
            json!({"catalogId":"tiny-q5_1"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let admitted = value(response).await;
    let id = admitted["jobId"].as_str().unwrap();
    let observed = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                video_kadr_backend::model::JobStatus::Done
                    | video_kadr_backend::model::JobStatus::Error
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await;
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let job = observed.expect("real public model download observation deadline");
    assert_eq!(
        job.status,
        video_kadr_backend::model::JobStatus::Done,
        "{job:?}"
    );
    let hash = "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7";
    let path = state
        .storage
        .join("models/whisper")
        .join(format!("{hash}.bin"));
    let (size, actual) = video_kadr_backend::services::local_speech::fingerprint_model(&path)
        .await
        .unwrap();
    assert_eq!(size, 32152673);
    assert_eq!(actual, hash);
    let library = value(
        app.oneshot(request(
            "GET",
            "/api/speech/model-library",
            Some(&owner.token),
            Value::Null,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(library["selectedId"], "configured");
    assert_eq!(library["models"][0]["id"], hash);
}

#[tokio::test]
#[ignore = "downloads pinned public Whisper model during cold-state recovery"]
async fn downloads_recover_same_identity_and_refuse_changed_catalog_snapshot() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass, model::JobStatus, state::AppState,
    };
    for mode in ["queued", "running", "changed"] {
        let (state, _dir) = support::make_state(true, false).await;
        let owner = state
            .db
            .register_auth_user("recover-owner", "download recovery password")
            .await
            .unwrap()
            .unwrap();
        let other = state
            .db
            .register_auth_user("recover-other", "download recovery password")
            .await
            .unwrap()
            .unwrap();
        let mut permits = vec![];
        if mode != "running" {
            for _ in 0..state.resource_limit(ResourceClass::Ingest) {
                permits.push(state.acquire_job_slot(ResourceClass::Ingest).await.unwrap());
            }
        }
        let response = support::router(state.clone())
            .oneshot(request(
                "POST",
                "/api/speech/model-downloads",
                Some(&owner.token),
                json!({"catalogId":"tiny-q5_1"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let admitted = value(response).await;
        let id = admitted["jobId"].as_str().unwrap();
        if mode == "running" {
            tokio::time::timeout(std::time::Duration::from_secs(60), async {
                loop {
                    let job = state.get_job(id).await.unwrap().unwrap();
                    assert!(
                        !matches!(job.status, JobStatus::Done | JobStatus::Error),
                        "download finished before interruption: {job:?}"
                    );
                    if job.status == JobStatus::Running
                        && job.progress.is_some_and(|p| p >= 5. && p < 95.)
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("download must reach real response before shutdown");
        }
        state.begin_shutdown();
        drop(permits);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(5))
                .await
        );
        assert_eq!(
            state.get_job(id).await.unwrap().unwrap().status,
            if mode == "running" {
                JobStatus::Running
            } else {
                JobStatus::Pending
            }
        );
        if mode == "changed" {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(state.storage.join("app.db")),
                )
                .await
                .unwrap();
            sqlx::query("UPDATE job_requests SET payload_json=json_set(payload_json,'$.model.sha256',?) WHERE job_id=?").bind("0".repeat(64)).bind(id).execute(&pool).await.unwrap();
        }
        let cold = AppState::new_with_runtime(
            state.storage.clone(),
            2,
            (*state.tools).clone(),
            video_kadr_backend::library::Library::load(state.storage.clone()).await,
            video_kadr_backend::db::Db::open(&state.storage)
                .await
                .unwrap(),
            state.encode_budget.clone(),
            2,
        )
        .unwrap();
        cold.recover_jobs().await;
        assert_eq!(
            cold.get_job(id).await.unwrap().unwrap().status,
            JobStatus::Pending
        );
        video_kadr_backend::handlers::resume_pending_jobs(&cold).await;
        let app = support::router(cold.clone());
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/jobs/{id}"),
                    Some(&other.token),
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
            loop {
                let job = cold.get_job(id).await.unwrap().unwrap();
                if matches!(job.status, JobStatus::Done | JobStatus::Error) {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await;
        cold.begin_shutdown();
        assert!(cold.wait_for_tasks(std::time::Duration::from_secs(5)).await);
        let job = result.expect("cold download completion");
        assert_eq!(job.id, id);
        let library = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/speech/model-library",
                    Some(&owner.token),
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(library["selectedId"], "configured");
        if mode == "changed" {
            assert_eq!(job.status, JobStatus::Error, "{job:?}");
            assert!(job.result.is_none());
            assert!(library["models"].as_array().unwrap().is_empty());
        } else {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            let hash = "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7";
            assert_eq!(library["models"][0]["id"], hash);
            let path = cold
                .storage
                .join("models/whisper")
                .join(format!("{hash}.bin"));
            assert_eq!(
                video_kadr_backend::services::local_speech::fingerprint_model(&path)
                    .await
                    .unwrap(),
                (32152673, hash.to_string())
            );
        }
        let history = value(
            app.oneshot(request(
                "GET",
                "/api/speech/model-downloads",
                Some(&owner.token),
                Value::Null,
            ))
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(history["jobs"].as_array().unwrap().len(), 1);
        assert_eq!(history["jobs"][0]["jobId"], id);
        assert_eq!(history["jobs"][0]["catalogId"], "tiny-q5_1");
    }
}
