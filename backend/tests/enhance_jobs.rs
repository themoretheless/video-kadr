mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, db::SpaceRole, library::MediaEntry, model::JobStatus,
    services::local_enhance::Config, state::AppState,
};
fn request(method: &str, path: &str, token: &str, data: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(data.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Cancelled | JobStatus::Error
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires native enhancement executable, verified model and ONNX Runtime"]
async fn actual_owned_jobs_recovery_cancel_source_mutation_and_acl_revocation() {
    let config = Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_ENHANCE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    for mode in ["recover", "changed", "revoked", "engine_changed", "managed_removed", "managed_running_removed"] {
        eprintln!("enhancement recovery mode: {mode}");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_enhance_config(Some(config.clone()));
        let owner = state
            .db
            .register_auth_user("enhance-owner", "public enhancement test password")
            .await
            .unwrap()
            .unwrap();
        let viewer = state
            .db
            .register_auth_user("enhance-viewer", "public enhancement test password")
            .await
            .unwrap()
            .unwrap();
        let source = state.sources_dir().join("synthetic.png");
        image::RgbaImage::from_fn(257, if mode == "managed_running_removed" {259} else {9}, |x, y| {
            image::Rgba([
                ((x * 13 + y * 7) % 256) as u8,
                ((x * 9 + y * 17) % 256) as u8,
                80,
                255,
            ])
        })
        .save(&source)
        .unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"enhance-source","filename":"synthetic.png","url":"/files/sources/synthetic.png","mediaType":"image"}))).await);
        let space = state
            .db
            .create_space(&owner.user.username, "Enhancement source")
            .await
            .unwrap();
        let project = state
            .db
            .create_space_composition_project(
                "Image",
                &json!({"schemaVersion":1,"sources":{"enhance-source":{"id":"enhance-source"}}}),
                &["enhance-source".into()],
                &owner.user.username,
                &space.id,
            )
            .await
            .unwrap();
        let app = support::router(state.clone());
        if matches!(mode, "managed_removed" | "managed_running_removed") {
            let response=app.clone().oneshot(request("POST","/api/enhance/models/downloads",&owner.token,json!({"catalogId":"realesr-general-x4v3"}))).await.unwrap();
            assert_eq!(response.status(),StatusCode::ACCEPTED);
            let download=value(response).await["jobId"].as_str().unwrap().to_owned();
            assert_eq!(terminal(&state,&download).await.status,JobStatus::Done);
            assert_eq!(app.clone().oneshot(request("POST","/api/enhance/models/select",&owner.token,json!({"modelId":"realesr-general-x4v3"}))).await.unwrap().status(),StatusCode::OK);
        }
        let data = json!({"sourceId":"enhance-source","factor":2});
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/api/enhance/jobs",
                    &viewer.token,
                    data.clone()
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        let status = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/enhance/status",
                    &owner.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status["available"], true);
        let mut held = vec![];
        for _ in 0..state.resource_limit(ResourceClass::Analysis) {
            held.push(
                state
                    .acquire_job_slot(ResourceClass::Analysis)
                    .await
                    .unwrap(),
            );
        }
        let actor = if mode == "revoked" {
            state
                .db
                .set_space_member(
                    &space.id,
                    &owner.user.username,
                    &viewer.user.username,
                    SpaceRole::Viewer,
                )
                .await
                .unwrap()
                .unwrap();
            &viewer
        } else {
            &owner
        };
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/enhance/jobs",
                &actor.token,
                data.clone(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let id = value(response).await["jobId"].as_str().unwrap().to_owned();
        let cancelled = value(
            app.clone()
                .oneshot(request("POST", "/api/enhance/jobs", &actor.token, data))
                .await
                .unwrap(),
        )
        .await["jobId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    &format!("/api/jobs/{cancelled}/cancel"),
                    &actor.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let first = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/enhance/jobs?sourceId=enhance-source&limit=1",
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(first["jobs"].as_array().unwrap().len(), 1);
        let cursor = first["nextCursor"].as_str().unwrap();
        let second = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/enhance/jobs?sourceId=enhance-source&limit=1&cursor={cursor}"),
                    &actor.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(second["jobs"].as_array().unwrap().len(), 1);
        assert_ne!(first["jobs"][0]["jobId"], second["jobs"][0]["jobId"]);
        assert!(second["nextCursor"].is_null());
        assert_eq!(first["jobs"][0]["sourceId"], "enhance-source");
        assert_eq!(first["jobs"][0]["factor"], 2);
        assert!(!first.to_string().contains("engine_hash"));
        let outsider = if mode == "revoked" { &owner } else { &viewer };
        let outside = value(
            app.clone()
                .oneshot(request(
                    "GET",
                    "/api/enhance/jobs",
                    &outsider.token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert!(outside["jobs"].as_array().unwrap().is_empty());
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "GET",
                    &format!("/api/enhance/jobs?cursor={cursor}"),
                    &outsider.token,
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
                    "GET",
                    "/api/enhance/jobs?limit=51",
                    &actor.token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        if matches!(mode, "managed_removed" | "managed_running_removed") {
            if mode == "managed_running_removed" {
                held.clear();
                tokio::time::timeout(std::time::Duration::from_secs(60),async {
                    loop {
                        let job=state.get_job(&id).await.unwrap().unwrap();
                        assert!(!matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled),"must observe intermediate native progress: {job:?}");
                        if job.status==JobStatus::Running && job.progress.unwrap_or(0.)>0. {break;}
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                }).await.unwrap();
            }
            let removed=app.clone().oneshot(request("POST","/api/enhance/models/remove",&owner.token,json!({"modelId":"realesr-general-x4v3"}))).await.unwrap();
            assert_eq!(removed.status(),StatusCode::OK);
            let removed=value(removed).await;
            assert_eq!(removed["fileReleased"],false);
            let models=value(app.clone().oneshot(request("GET","/api/enhance/models",&owner.token,Value::Null)).await.unwrap()).await;
            assert_eq!(models["selectedId"],"configured");assert!(models["models"].as_array().unwrap().is_empty());
            assert_eq!(app.clone().oneshot(request("POST","/api/enhance/models/select",&owner.token,json!({"modelId":"realesr-general-x4v3"}))).await.unwrap().status(),StatusCode::NOT_FOUND);
            let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db"))).await.unwrap();
            let payload:String=sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?").bind(&id).fetch_one(&pool).await.unwrap();
            assert_eq!(serde_json::from_str::<Value>(&payload).unwrap()["model_id"],"realesr-general-x4v3");
            video_kadr_backend::services::enhance_model::verify(&state.storage.join("models/enhance/realesr-general-x4v3/realesr-general-x4v3.onnx")).unwrap();
        }
        // Admission can relocate a newly claimed source into its Space directory.
        let source = state
            .library
            .resolve_media_path(&state.library.get("enhance-source").await.unwrap())
            .await
            .unwrap();
        if mode == "changed" {
            tokio::fs::write(&source, b"source changed after admission")
                .await
                .unwrap();
            assert_eq!(
                video_kadr_backend::services::local_speech::fingerprint_model(&source)
                    .await
                    .unwrap()
                    .1,
                video_kadr_backend::services::local_speech::fingerprint_model(
                    &state
                        .library
                        .resolve_media_path(&state.library.get("enhance-source").await.unwrap())
                        .await
                        .unwrap()
                )
                .await
                .unwrap()
                .1,
                "mutation must affect the actual captured media path"
            );
        }
        if mode == "revoked" {
            state
                .db
                .remove_space_member(&space.id, &owner.user.username, &viewer.user.username)
                .await
                .unwrap()
                .unwrap();
            assert!(!state
                .db
                .is_space_member(&space.id, &viewer.user.username)
                .await
                .unwrap());
            let hidden = value(
                app.clone()
                    .oneshot(request(
                        "GET",
                        "/api/enhance/jobs",
                        &viewer.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert!(hidden["jobs"].as_array().unwrap().is_empty());
        }
        state.begin_shutdown();
        drop(held);
        assert!(
            state
                .wait_for_tasks(std::time::Duration::from_secs(5))
                .await
        );
        let mut recovered_config = config.clone();
        if matches!(mode, "managed_removed" | "managed_running_removed") { recovered_config.model=state.storage.join("configured-model-is-unavailable.onnx"); }
        if mode == "engine_changed" {
            recovered_config.runtime_library = source.clone();
        }
        let recovered = AppState::new(
            state.storage.clone(),
            2,
            (*state.tools).clone(),
            video_kadr_backend::library::Library::load(state.storage.clone()).await,
            video_kadr_backend::db::Db::open(&state.storage)
                .await
                .unwrap(),
        )
        .with_enhance_config(Some(recovered_config));
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let job = terminal(&recovered, &id).await;
        assert_eq!(
            terminal(&recovered, &cancelled).await.status,
            JobStatus::Cancelled
        );
        let app = support::router(recovered.clone());
        if matches!(mode, "recover" | "managed_removed" | "managed_running_removed") {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            let asset_path = format!("/api/enhance/jobs/{id}/asset");
            let target = json!({"projectId":project.id});
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_path, &viewer.token, target.clone()))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let response = app
                .clone()
                .oneshot(request("POST", &asset_path, &owner.token, target.clone()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let saved = value(response).await;
            let repeated = value(
                app.clone()
                    .oneshot(request("POST", &asset_path, &owner.token, target.clone()))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(saved["id"], repeated["id"]);
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        saved["url"].as_str().unwrap(),
                        &owner.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        saved["url"].as_str().unwrap(),
                        &viewer.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            let saved_entry = recovered
                .library
                .get(saved["id"].as_str().unwrap())
                .await
                .unwrap();
            let accepted_path = recovered
                .library
                .resolve_media_path(&saved_entry)
                .await
                .unwrap();
            let accepted_bytes = std::fs::read(&accepted_path).unwrap();
            std::fs::write(&accepted_path, b"changed accepted copy").unwrap();
            assert_eq!(
                app.clone()
                    .oneshot(request("POST", &asset_path, &owner.token, target))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
            assert_eq!(
                std::fs::read(&accepted_path).unwrap(),
                b"changed accepted copy"
            );
            std::fs::write(&accepted_path, accepted_bytes).unwrap();
            let meta = job.result.unwrap();
            assert_eq!(meta["width"], 514);
            assert_eq!(meta["height"], if mode == "managed_running_removed" {518} else {18});
            let path = format!("/api/enhance/jobs/{id}/image");
            let image = app
                .clone()
                .oneshot(request("GET", &path, &owner.token, Value::Null))
                .await
                .unwrap();
            assert_eq!(image.status(), StatusCode::OK);
            let bytes = to_bytes(image.into_body(), 1024 * 1024).await.unwrap();
            assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 514);
            assert_eq!(
                app.clone()
                    .oneshot(request("GET", &path, &viewer.token, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                app.clone()
                    .oneshot(request(
                        "GET",
                        &format!("/api/jobs/{id}"),
                        &viewer.token,
                        Value::Null
                    ))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            recovered.begin_shutdown();
            assert!(
                recovered
                    .wait_for_tasks(std::time::Duration::from_secs(5))
                    .await
            );
            let reopened = AppState::new(
                recovered.storage.clone(),
                2,
                (*recovered.tools).clone(),
                video_kadr_backend::library::Library::load(recovered.storage.clone()).await,
                video_kadr_backend::db::Db::open(&recovered.storage)
                    .await
                    .unwrap(),
            );
            reopened.recover_jobs().await;
            let fresh = support::router(reopened.clone());
            let history = value(
                fresh
                    .clone()
                    .oneshot(request(
                        "GET",
                        "/api/enhance/jobs?sourceId=enhance-source",
                        &owner.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(history["jobs"].as_array().unwrap().len(), 2);
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(reopened.storage.join("app.db")),
                )
                .await
                .unwrap();
            sqlx::query("DELETE FROM enhance_job_settings WHERE job_id=?")
                .bind(&cancelled)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("DELETE FROM job_requests WHERE job_id=?")
                .bind(&cancelled)
                .execute(&pool)
                .await
                .unwrap();
            let legacy = value(
                fresh
                    .clone()
                    .oneshot(request(
                        "GET",
                        "/api/enhance/jobs?sourceId=enhance-source",
                        &owner.token,
                        Value::Null,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert!(legacy["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["jobId"] == cancelled)
                .unwrap()["factor"]
                .is_null());

            let response = fresh
                .oneshot(request(
                    "POST",
                    &asset_path,
                    &owner.token,
                    json!({"projectId":project.id}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(value(response).await["id"], saved["id"]);
            reopened.begin_shutdown();
            assert!(
                reopened
                    .wait_for_tasks(std::time::Duration::from_secs(5))
                    .await
            );
        } else {
            assert_eq!(job.status, JobStatus::Error, "{mode}: {job:?}");
            assert_eq!(
                job.error_kind.as_deref(),
                Some(if matches!(mode, "changed" | "engine_changed") {
                    "validation"
                } else {
                    "security"
                })
            );
            assert!(!recovered
                .storage
                .join("enhance-jobs")
                .join(format!("{id}.png"))
                .exists());
        }
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(5))
                .await
        );
    }
}
