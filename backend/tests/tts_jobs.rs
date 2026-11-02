mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass, model::JobStatus, services::local_tts::TtsConfig,
    state::AppState,
};
fn req(method: &str, path: &str, token: Option<&str>, data: Value) -> Request<Body> {
    let mut r = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    r.body(Body::from(data.to_string())).unwrap()
}
async fn value(r: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(r.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
async fn done(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let j = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                j.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break j;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
async fn cold(state: &AppState, config: Option<TtsConfig>) -> AppState {
    let state = AppState::new_with_runtime(
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
    if let Some(config) = config {
        state.with_tts_config(config)
    } else {
        state
    }
}
#[cfg(unix)]
fn fixture(root: &std::path::Path, hang_first: bool) -> TtsConfig {
    use std::os::unix::fs::PermissionsExt;
    let model = root.join("model");
    std::fs::create_dir(&model).unwrap();
    std::fs::create_dir(model.join("espeak-ng-data")).unwrap();
    std::fs::write(model.join("model.onnx"), vec![0u8; 1_000_000]).unwrap();
    for name in [
        "voices.bin",
        "tokens.txt",
        "lexicon-us-en.txt",
        "lexicon-zh.txt",
    ] {
        std::fs::write(model.join(name), "fixture").unwrap();
    }
    let mut b = vec![];
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&48036u32.to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24000u32.to_le_bytes());
    b.extend_from_slice(&48000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&48000u32.to_le_bytes());
    b.resize(48044, 0);
    std::fs::write(root.join("fixture.wav"), b).unwrap();
    let executable = root.join("engine");
    let hang = if hang_first {
        format!(
            "if [ ! -e '{0}/seen' ]; then touch '{0}/seen' '{0}/ready'; sleep 20; fi",
            root.display()
        )
    } else {
        "".into()
    };
    std::fs::write(&executable,format!("#!/bin/sh\nfor arg in \"$@\"; do case \"$arg\" in --) break;; --output-filename=*) output=${{arg#--output-filename=}};; esac; done\ncp '{}/fixture.wav' \"$output\"\n{}\n",root.display(),hang)).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    TtsConfig {
        executable,
        model_dir: Some(model),
        threads: 2,
    }
}
#[cfg(unix)]
#[tokio::test]
async fn queued_cancel_owner_history_and_encrypted_request() {
    let model = tempfile::tempdir().unwrap();
    let config = fixture(model.path(), false);
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_tts_config(config);
    let a = state
        .db
        .register_auth_user("tts-job-a", "narration job password")
        .await
        .unwrap()
        .unwrap();
    let b = state
        .db
        .register_auth_user("tts-job-b", "narration job password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        app.clone()
            .oneshot(req(
                "POST",
                "/api/speech/tts-jobs",
                None,
                json!({"text":"Hello","voiceId":3,"speed":1.})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut permits = vec![];
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        permits.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let text = "private narration that must not be stored as plain text";
    let response = app
        .clone()
        .oneshot(req(
            "POST",
            "/api/speech/tts-jobs",
            Some(&a.token),
            json!({"text":text,"voiceId":3,"speed":1.}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let job = value(response).await;
    let id = job["jobId"].as_str().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let payload: String =
        sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!payload.contains(text));
    assert!(payload.contains("sealed"));
    for (method, suffix) in [
        ("GET", ""),
        ("POST", "/cancel"),
        ("POST", "/retry"),
        ("POST", "/discard"),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(req(
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
    assert_eq!(
        app.clone()
            .oneshot(req(
                "GET",
                &format!("/api/speech/tts-jobs/{id}/audio"),
                Some(&b.token),
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let history = value(
        app.clone()
            .oneshot(req(
                "GET",
                "/api/speech/tts-jobs",
                Some(&a.token),
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(history["jobs"][0]["jobId"], id);
    assert!(!history.to_string().contains(text));
    let other = value(
        app.clone()
            .oneshot(req(
                "GET",
                "/api/speech/tts-jobs",
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
            .oneshot(req(
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
    assert_eq!(done(&state, id).await.status, JobStatus::Cancelled);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[cfg(unix)]
#[tokio::test]
async fn cold_recovery_and_model_ciphertext_invariants() {
    for mode in [
        "queued",
        "legacy",
        "running",
        "model_changed",
        "cipher_changed",
        "key_missing",
    ] {
        let model = tempfile::tempdir().unwrap();
        let config = fixture(model.path(), mode == "running");
        let (state, _dir) = support::make_state(true, false).await;
        let state = state.with_tts_config(config.clone());
        let owner = state
            .db
            .register_auth_user("tts-recovery-owner", "narration job password")
            .await
            .unwrap()
            .unwrap();
        let app = support::router(state.clone());
        let mut permits = vec![];
        if mode != "running" {
            for _ in 0..state.resource_limit(ResourceClass::Analysis) {
                permits.push(
                    state
                        .acquire_job_slot(ResourceClass::Analysis)
                        .await
                        .unwrap(),
                );
            }
        }
        let response = app
            .oneshot(req(
                "POST",
                "/api/speech/tts-jobs",
                Some(&owner.token),
                json!({"text":"Recover this narration","voiceId":3,"speed":1.}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let admission = value(response).await;
        let id = admission["jobId"].as_str().unwrap();
        if mode == "running" {
            let ready = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while !model.path().join("ready").exists() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await;
            if ready.is_err() {
                let diagnostic = state.get_job(id).await.unwrap();
                state.begin_shutdown();
                let _ = state
                    .wait_for_tasks(std::time::Duration::from_secs(5))
                    .await;
                panic!("engine readiness deadline, job={diagnostic:?}");
            }
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
        if mode == "legacy" {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(state.storage.join("app.db")),
                )
                .await
                .unwrap();
            let raw: String =
                sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let mut payload: Value = serde_json::from_str(&raw).unwrap();
            let key: [u8; 32] = std::fs::read(state.storage.join(".tts-job-key"))
                .unwrap()
                .try_into()
                .unwrap();
            let cipher = video_kadr_backend::youtube::TokenCipher::new(key).unwrap();
            let context = [
                "tts-job",
                owner.user.username.as_str(),
                id,
                payload["identity"].as_str().unwrap(),
            ]
            .join(&char::from(0).to_string());
            let sealed: Vec<u8> = serde_json::from_value(payload["sealed"].clone()).unwrap();
            let input: Value =
                serde_json::from_slice(&cipher.decrypt(&context, &sealed).unwrap()).unwrap();
            payload["schema_version"] = json!(1);
            payload["sealed"] = serde_json::to_value(
                cipher
                    .encrypt(&context, &serde_json::to_vec(&input["request"]).unwrap())
                    .unwrap(),
            )
            .unwrap();
            sqlx::query("UPDATE job_requests SET payload_json=? WHERE job_id=?")
                .bind(payload.to_string())
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        if mode == "model_changed" {
            std::fs::write(
                config.model_dir.as_ref().unwrap().join("tokens.txt"),
                "changed",
            )
            .unwrap();
        }
        if mode == "cipher_changed" {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    sqlx::sqlite::SqliteConnectOptions::new()
                        .filename(state.storage.join("app.db")),
                )
                .await
                .unwrap();
            let raw: String =
                sqlx::query_scalar("SELECT payload_json FROM job_requests WHERE job_id=?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let mut payload: Value = serde_json::from_str(&raw).unwrap();
            payload["sealed"][0] = json!((payload["sealed"][0].as_u64().unwrap() + 1) % 256);
            sqlx::query("UPDATE job_requests SET payload_json=? WHERE job_id=?")
                .bind(payload.to_string())
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        if mode == "key_missing" {
            std::fs::rename(
                state.storage.join(".tts-job-key"),
                state.storage.join(".tts-job-key.saved"),
            )
            .unwrap();
        }
        let recovered = cold(&state, Some(config)).await;
        recovered.recover_jobs().await;
        video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
        let mut job = done(&recovered, id).await;
        if mode == "key_missing" {
            assert_eq!(job.status, JobStatus::Error);
            assert_eq!(job.error_kind.as_deref(), Some("storage"));
            assert!(
                !state.storage.join(".tts-job-key").exists(),
                "worker must never rotate a missing key"
            );
            let new_request = support::router(recovered.clone())
                .oneshot(req(
                    "POST",
                    "/api/speech/tts-jobs",
                    Some(&owner.token),
                    json!({"text":"Do not rotate the key for new work","voiceId":3,"speed":1.}),
                ))
                .await
                .unwrap();
            assert_eq!(new_request.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!state.storage.join(".tts-job-key").exists());
            std::fs::rename(
                state.storage.join(".tts-job-key.saved"),
                state.storage.join(".tts-job-key"),
            )
            .unwrap();
            let response = support::router(recovered.clone())
                .oneshot(req(
                    "POST",
                    &format!("/api/jobs/{id}/retry"),
                    Some(&owner.token),
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            job = done(&recovered, id).await;
        }
        if mode.ends_with("changed") {
            assert_eq!(job.status, JobStatus::Error, "{job:?}");
            assert_eq!(
                job.error_kind.as_deref(),
                Some(if mode == "model_changed" {
                    "validation"
                } else {
                    "security"
                })
            );
            assert!(job.result.is_none());
        } else {
            assert_eq!(job.status, JobStatus::Done, "{job:?}");
            let no_model = cold(&recovered, None).await;
            let response = support::router(no_model)
                .oneshot(req(
                    "GET",
                    &format!("/api/speech/tts-jobs/{id}/audio"),
                    Some(&owner.token),
                    Value::Null,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                to_bytes(response.into_body(), 100000).await.unwrap().len(),
                48044
            );
        }
        recovered.begin_shutdown();
        assert!(
            recovered
                .wait_for_tasks(std::time::Duration::from_secs(5))
                .await
        );
    }
}
#[tokio::test]
#[ignore = "requires official Kokoro engine and model"]
async fn actual_learned_job_remains_downloadable_without_model_after_reopen() {
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_tts_config(TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    });
    let owner = state
        .db
        .register_auth_user("tts-learned-owner", "narration job password")
        .await
        .unwrap()
        .unwrap();
    let response = support::router(state.clone())
        .oneshot(req(
            "POST",
            "/api/speech/tts-jobs",
            Some(&owner.token),
            json!({"text":"Welcome to our video project.","voiceId":3,"speed":1.}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let a = value(response).await;
    let id = a["jobId"].as_str().unwrap();
    let job = done(&state, id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    assert_eq!(
        job.result.unwrap()["modelHash"],
        "4b86207ef680e394d8343bee22dfc4c512e5c707c6d9578e3f35ab09bffd6b36"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let cold = cold(&state, None).await;
    let response = support::router(cold)
        .oneshot(req(
            "GET",
            &format!("/api/speech/tts-jobs/{id}/audio"),
            Some(&owner.token),
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        to_bytes(response.into_body(), 64 * 1024 * 1024)
            .await
            .unwrap()
            .len()
            > 48000
    );
}
