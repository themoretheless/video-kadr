mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(token: Option<&str>, method: &str, uri: &str, body: Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        b = b.header("authorization", format!("Bearer {token}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}
#[tokio::test]
async fn selection_is_authenticated_owned_and_persistent() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("tts-model-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("tts-model-other", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(None, "GET", "/api/speech/tts-models", json!(null)))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for (id, expected) in [
        ("../escape", StatusCode::BAD_REQUEST),
        (catalog.id.as_str(), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    Some(&owner.token),
                    "PUT",
                    "/api/speech/tts-models/selection",
                    json!({"modelId":id})
                ))
                .await
                .unwrap()
                .status(),
            expected
        );
    }
    sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .bind(&catalog.name)
        .bind(catalog.bytes as i64)
        .execute(&pool)
        .await
        .unwrap();
    // Ownership alone cannot select an absent/corrupt bundle.
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&owner.token),
                "PUT",
                "/api/speech/tts-models/selection",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    for (token, count) in [(&owner.token, 1), (&other.token, 0)] {
        let response = app
            .clone()
            .oneshot(request(
                Some(token),
                "GET",
                "/api/speech/tts-models",
                json!(null),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(value["models"].as_array().unwrap().len(), count);
        assert_eq!(value["selectedId"], "configured");
        assert!(value["catalog"][0].get("files").is_none());
    }
    sqlx::query("INSERT INTO tts_model_selection(owner,model_id) VALUES(?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&other.token),
                "PUT",
                "/api/speech/tts-models/selection",
                json!({"modelId":"configured"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let retained: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tts_model_selection WHERE owner=?")
            .bind(&owner.user.username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(retained, 1);
    assert_eq!(
        app.oneshot(request(
            Some(&owner.token),
            "PUT",
            "/api/speech/tts-models/selection",
            json!({"modelId":"configured"})
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    let retained: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tts_model_selection")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(retained, 0);
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(
                None,
                "DELETE",
                "/api/speech/tts-models",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&other.token),
                "DELETE",
                "/api/speech/tts-models",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.oneshot(request(
            Some(&owner.token),
            "DELETE",
            "/api/speech/tts-models",
            json!({"modelId":catalog.id})
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
#[ignore = "requires official Kokoro model and sherpa executable"]
async fn managed_model_job_keeps_selection_after_owner_reset() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass, model::JobStatus, services::local_tts::TtsConfig,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_tts_config(TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: None,
        threads: 2,
    });
    let catalog = match std::env::var("VIDEO_KADR_TEST_TTS_CATALOG") {
        Ok(id) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::find(&id)
            .unwrap()
            .unwrap(),
        Err(_) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap(),
    };
    let original = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    let installed = state.storage.join("models/tts").join(&catalog.id);
    // Seed an installed bundle, not a download qualification. Copies preserve the public reference.
    for file in &catalog.files {
        let target = installed.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(original.join(&file.path), target).unwrap();
    }
    let owner = state
        .db
        .register_auth_user("tts-managed-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .bind(&catalog.name)
        .bind(catalog.bytes as i64)
        .execute(&pool)
        .await
        .unwrap();
    let voice_id: u32 = std::env::var("VIDEO_KADR_TEST_TTS_VOICE")
        .ok()
        .map(|v| v.parse().unwrap())
        .unwrap_or(3);
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&owner.token),
                "PUT",
                "/api/speech/tts-models/selection",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    if catalog.family == video_kadr_backend::services::local_tts::TtsFamily::Pocket {
        let response = app
            .clone()
            .oneshot(request(
                Some(&owner.token),
                "GET",
                "/api/speech/voices",
                json!(null),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let status: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(status["engine"], "pocket-sherpa-onnx");
        assert_eq!(
            status["voices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1000, 1001, 1099]
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    Some(&owner.token),
                    "POST",
                    "/api/speech/tts-jobs",
                    json!({"text":"wrong family","voiceId":3,"speed":1.})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let mut held = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        held.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            Some(&owner.token),
            "POST",
            "/api/speech/tts-jobs",
            json!({"text":"Welcome to this video project.","voiceId":voice_id,"speed":1.}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = value["jobId"].as_str().unwrap();
    assert_eq!(
        state.get_job(id).await.unwrap().unwrap().status,
        JobStatus::Pending
    );
    let snapshot: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&snapshot).unwrap()["model_id"],
        catalog.id
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&owner.token),
                "PUT",
                "/api/speech/tts-models/selection",
                json!({"modelId":"configured"})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    drop(held);
    let job = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let audio = app
        .oneshot(request(
            Some(&owner.token),
            "GET",
            &format!("/api/speech/tts-jobs/{id}/audio"),
            json!(null),
        ))
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    let network = catalog
        .files
        .iter()
        .find(|file| {
            file.path == "model.int8.onnx"
                || file.path == "model.onnx"
                || file.path == "lm_main.int8.onnx"
        })
        .unwrap();
    assert_eq!(audio.headers()["x-tts-model-sha256"], network.sha256);
    assert!(
        to_bytes(audio.into_body(), 64 * 1024 * 1024)
            .await
            .unwrap()
            .len()
            > 44
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
#[ignore = "requires official Kokoro model and sherpa executable"]
async fn removed_model_remains_pinned_until_job_finishes() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass, model::JobStatus, services::local_tts::TtsConfig,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_tts_config(TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: None,
        threads: 2,
    });
    let catalog = video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap();
    let original = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    let installed = state.storage.join("models/tts").join(&catalog.id);
    // Seed an installed bundle, not a download qualification. Copies preserve the public reference.
    for file in &catalog.files {
        let target = installed.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(original.join(&file.path), target).unwrap();
    }
    let owner = state
        .db
        .register_auth_user("tts-managed-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
        .bind(&owner.user.username)
        .bind(&catalog.id)
        .bind(&catalog.name)
        .bind(catalog.bytes as i64)
        .execute(&pool)
        .await
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&owner.token),
                "PUT",
                "/api/speech/tts-models/selection",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let mut held = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        held.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let response = app
        .clone()
        .oneshot(request(
            Some(&owner.token),
            "POST",
            "/api/speech/tts-jobs",
            json!({"text":"Welcome to this video project.","voiceId":3,"speed":1.}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = value["jobId"].as_str().unwrap();
    assert_eq!(
        state.get_job(id).await.unwrap().unwrap().status,
        JobStatus::Pending
    );
    let snapshot: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&snapshot).unwrap()["model_id"],
        catalog.id
    );
    assert_eq!(
        app.clone()
            .oneshot(request(
                Some(&owner.token),
                "DELETE",
                "/api/speech/tts-models",
                json!({"modelId":catalog.id})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(installed.exists(), "admitted job pins removed bundle");
    let selected = app
        .clone()
        .oneshot(request(
            Some(&owner.token),
            "PUT",
            "/api/speech/tts-models/selection",
            json!({"modelId":catalog.id}),
        ))
        .await
        .unwrap();
    assert_eq!(selected.status(), StatusCode::NOT_FOUND);
    let hidden = app
        .clone()
        .oneshot(request(
            Some(&owner.token),
            "GET",
            "/api/speech/tts-models",
            json!(null),
        ))
        .await
        .unwrap();
    let hidden: Value =
        serde_json::from_slice(&to_bytes(hidden.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert!(hidden["models"].as_array().unwrap().is_empty());
    assert_eq!(hidden["selectedId"], "configured");
    assert!(installed.exists());
    drop(held);
    let job = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let audio = app
        .clone()
        .oneshot(request(
            Some(&owner.token),
            "GET",
            &format!("/api/speech/tts-jobs/{id}/audio"),
            json!(null),
        ))
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    assert!(
        to_bytes(audio.into_body(), 64 * 1024 * 1024)
            .await
            .unwrap()
            .len()
            > 44
    );
    let refreshed = app
        .oneshot(request(
            Some(&owner.token),
            "GET",
            "/api/speech/tts-models",
            json!(null),
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    assert!(
        !installed.exists(),
        "unreferenced removed bundle released after completion"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
#[ignore = "requires public Kokoro bundle"]
async fn shared_model_is_released_only_after_last_owner_removes_it() {
    let (state, _dir) = support::make_state(true, false).await;
    let catalog = match std::env::var("VIDEO_KADR_TEST_TTS_CATALOG") {
        Ok(id) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::find(&id)
            .unwrap()
            .unwrap(),
        Err(_) => video_kadr_backend::services::tts_model_catalog::ModelCatalog::bundled().unwrap(),
    };
    let source = std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap());
    catalog.verify_directory(&source).unwrap();
    let installed = state.storage.join("models/tts").join(&catalog.id);
    for file in &catalog.files {
        let target = installed.join(&file.path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(&file.path), target).unwrap();
    }
    let a = state
        .db
        .register_auth_user("tts-remove-shared-a", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let b = state
        .db
        .register_auth_user("tts-remove-shared-b", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    for owner in [&a.user.username, &b.user.username] {
        sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
            .bind(owner)
            .bind(&catalog.id)
            .bind(&catalog.name)
            .bind(catalog.bytes as i64)
            .execute(&pool)
            .await
            .unwrap();
    }
    let app = support::router(state.clone());
    for (owner, released) in [(&a, false), (&b, true)] {
        let response = app
            .clone()
            .oneshot(request(
                Some(&owner.token),
                "DELETE",
                "/api/speech/tts-models",
                json!({"modelId":catalog.id}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(result["fileReleased"], released);
        assert_eq!(installed.exists(), !released);
        if !released {
            catalog.verify_directory(&installed).unwrap();
            let response = app
                .clone()
                .oneshot(request(
                    Some(&b.token),
                    "PUT",
                    "/api/speech/tts-models/selection",
                    json!({"modelId":catalog.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let reinstalled = app
                .clone()
                .oneshot(request(
                    Some(&a.token),
                    "POST",
                    "/api/speech/tts-model-downloads",
                    json!({"catalogId":catalog.id}),
                ))
                .await
                .unwrap();
            assert_eq!(reinstalled.status(), StatusCode::ACCEPTED);
            let reinstalled: Value = serde_json::from_slice(
                &to_bytes(reinstalled.into_body(), 1024 * 1024)
                    .await
                    .unwrap(),
            )
            .unwrap();
            let id = reinstalled["jobId"].as_str().unwrap();
            let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let job = state.get_job(id).await.unwrap().unwrap();
                    if matches!(
                        job.status,
                        video_kadr_backend::model::JobStatus::Done
                            | video_kadr_backend::model::JobStatus::Error
                    ) {
                        break job;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                job.status,
                video_kadr_backend::model::JobStatus::Done,
                "{job:?}"
            );
            let library = app
                .clone()
                .oneshot(request(
                    Some(&a.token),
                    "GET",
                    "/api/speech/tts-models",
                    json!(null),
                ))
                .await
                .unwrap();
            let library: Value =
                serde_json::from_slice(&to_bytes(library.into_body(), 1024 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(library["models"][0]["id"], catalog.id);
            assert_eq!(library["selectedId"], "configured");
            let removed_again = app
                .clone()
                .oneshot(request(
                    Some(&a.token),
                    "DELETE",
                    "/api/speech/tts-models",
                    json!({"modelId":catalog.id}),
                ))
                .await
                .unwrap();
            assert_eq!(removed_again.status(), StatusCode::OK);
            assert!(installed.exists());
        }
    }
    catalog.verify_directory(&source).unwrap();
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
