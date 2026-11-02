mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn request(method: &str, path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[tokio::test]
async fn stock_voice_refuses_user_reference_and_absent_owned_pocket() {
    let (state, _dir) = support::make_state(true, false).await;
    let actor = state
        .db
        .register_auth_user("stock-admission", "public stock test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state);
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-jobs",
            &actor.token,
            json!({"text":"Hello","voiceId":2000,"speed":1,"referenceId":uuid::Uuid::new_v4().to_string()}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app
        .oneshot(request(
            "POST",
            "/api/speech/tts-jobs",
            &actor.token,
            json!({"text":"Hello","voiceId":2000,"speed":1}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
#[tokio::test]
#[ignore = "requires verified Chatterbox/Pocket bundles and native runtime"]
async fn actual_stock_voice_is_owner_scoped_and_survives_pocket_removal() {
    use video_kadr_backend::{
        config::resource_classes::ResourceClass,
        model::JobStatus,
        services::{local_chatterbox::Runtime, tts_model_catalog::ModelCatalog},
    };
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_chatterbox_runtime(Some(Runtime {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    }));
    let chatter =
        ModelCatalog::find("3d500b0e333d1661a40e854cbdbc93d8f3e9f01942b6a4e1cea3f6b4aa8c9521")
            .unwrap()
            .unwrap();
    let pocket =
        ModelCatalog::find("f85f04c7acf76154571a1e6b398534c257574cece490d2bb7991eae5e8426d54")
            .unwrap()
            .unwrap();
    for (catalog, variable) in [
        (&chatter, "VIDEO_KADR_TEST_TTS_MODEL"),
        (&pocket, "VIDEO_KADR_TEST_POCKET_MODEL"),
    ] {
        let source = std::path::PathBuf::from(std::env::var(variable).unwrap());
        catalog.verify_directory(&source).unwrap();
        for file in &catalog.files {
            let target = state
                .storage
                .join("models/tts")
                .join(&catalog.id)
                .join(&file.path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(source.join(&file.path), target).unwrap();
        }
    }
    let owner = state
        .db
        .register_auth_user("stock-owner", "public stock test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("stock-other", "public stock test password")
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
    for (actor, catalog) in [(&owner, &chatter), (&owner, &pocket), (&other, &chatter)] {
        sqlx::query("INSERT INTO tts_models(owner,id,name,bytes) VALUES(?,?,?,?)")
            .bind(&actor.user.username)
            .bind(&catalog.id)
            .bind(&catalog.name)
            .bind(catalog.bytes as i64)
            .execute(&pool)
            .await
            .unwrap();
    }
    let app = support::router(state.clone());
    for actor in [&owner, &other] {
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "PUT",
                    "/api/speech/tts-models/selection",
                    &actor.token,
                    json!({"modelId":chatter.id})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    let voices = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/voices",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        voices["voices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![2000, 2099]
    );
    let other_voices = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/voices",
                &other.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(other_voices["voices"].as_array().unwrap().len(), 1);
    assert_eq!(other_voices["voices"][0]["id"], 2099);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/tts-jobs",
                &other.token,
                json!({"text":"Owner isolation","voiceId":2000,"speed":1})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
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
    let admitted = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/speech/tts-jobs",
            &owner.token,
            json!({"text":"welcome   to our video project","voiceId":2000,"speed":1}),
        ))
        .await
        .unwrap();
    assert_eq!(admitted.status(), StatusCode::ACCEPTED);
    let admitted = value(admitted).await;
    let id = admitted["jobId"].as_str().unwrap();
    let refs = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/tts-references",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(refs["references"], json!([]));
    let hidden: String = sqlx::query_scalar("SELECT id FROM tts_references WHERE owner=?")
        .bind(&owner.user.username)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(state
        .storage
        .join("tts-references")
        .join(format!("{hidden}.wav"))
        .exists());
    let removed = app
        .clone()
        .oneshot(request(
            "DELETE",
            "/api/speech/tts-models",
            &owner.token,
            json!({"modelId":pocket.id}),
        ))
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::OK);
    assert!(!state.storage.join("models/tts").join(&pocket.id).exists());
    let voices = value(
        app.clone()
            .oneshot(request(
                "GET",
                "/api/speech/voices",
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(voices["voices"].as_array().unwrap().len(), 1);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "POST",
                "/api/speech/tts-jobs",
                &owner.token,
                json!({"text":"No missing voice fallback","voiceId":2000,"speed":1})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    state.begin_shutdown();
    drop(held);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let state = video_kadr_backend::state::AppState::new(
        state.storage.clone(),
        2,
        video_kadr_backend::state::ToolInfo::default(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
    )
    .with_chatterbox_runtime(state.chatterbox.as_ref().clone());
    state.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&state).await;
    let app = support::router(state.clone());
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(job.status, JobStatus::Done | JobStatus::Error) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result.status, JobStatus::Done, "{result:?}");
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["result"]["voiceId"], 2000);
    assert_eq!(result["result"]["engine"], "chatterbox-onnx");
    assert!(result["result"]["referenceHash"]
        .as_str()
        .is_some_and(|h| h.len() == 64));
    let audio = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/speech/tts-jobs/{id}/audio"),
            &owner.token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(audio.status(), StatusCode::OK);
    assert_eq!(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/speech/tts-jobs/{id}/audio"),
                &other.token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    std::fs::write(
        std::env::var("VIDEO_KADR_TEST_STOCK_OUTPUT").unwrap(),
        to_bytes(audio.into_body(), 64 * 1024 * 1024).await.unwrap(),
    )
    .unwrap();
    let refs = app
        .clone()
        .oneshot(request(
            "GET",
            "/api/speech/tts-references",
            &owner.token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(refs.status(), StatusCode::OK);
    assert!(!state
        .storage
        .join("tts-references")
        .join(format!("{hidden}.wav"))
        .exists());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
