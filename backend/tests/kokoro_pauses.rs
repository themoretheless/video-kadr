mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass,
    model::JobStatus,
    services::{
        local_kokoro::Runtime,
        local_tts::{SynthesisRequest, TtsConfig},
    },
    state::AppState,
};
fn request(method: &str, path: &str, token: &str, input: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(input.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}
#[test]
fn native_settings_and_pause_admission() {
    assert!(Runtime::from_lookup(&mut |_| None).unwrap().is_none());
    assert!(
        Runtime::from_lookup(&mut |key| if key == "KOKORO_NATIVE_BIN" {
            Some("/native".into())
        } else {
            None
        })
        .is_err()
    );
    assert!(Runtime::from_lookup(&mut |_| Some("relative".into())).is_err());
    assert!(Runtime::from_lookup(&mut |_| Some("/host/absolute".into()))
        .unwrap()
        .is_some());
    let mut input: SynthesisRequest =
        serde_json::from_value(json!({"text":"Hello","voiceId":3,"speed":1})).unwrap();
    assert!(input.pauses.is_none());
    for pauses in [0., 0.2, 1.] {
        input.pauses = Some(pauses);
        input.validate_for_model(&TtsConfig::default()).unwrap();
        let restored: SynthesisRequest =
            serde_json::from_slice(&serde_json::to_vec(&input).unwrap()).unwrap();
        assert_eq!(restored.pauses, Some(pauses));
    }
    for pauses in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        input.pauses = Some(pauses);
        assert!(input.validate().is_err());
    }
}
#[tokio::test]
#[ignore = "requires native Kokoro runtime, public learned model and libraries"]
async fn actual_authenticated_pause_jobs_recover_with_exact_zero_and_owner_audio() {
    let runtime = Runtime {
        executable: std::env::var("VIDEO_KADR_NATIVE_KOKORO_BIN")
            .unwrap()
            .into(),
        library: std::env::var("VIDEO_KADR_SHERPA_LIBRARY").unwrap().into(),
        ort_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let config = TtsConfig {
        executable: "/unused/legacy-engine".into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let state = state
        .with_tts_config(config.clone())
        .with_kokoro_runtime(Some(runtime.clone()));
    let owner = state
        .db
        .register_auth_user("kokoro-pauses-owner", "public pause test password")
        .await
        .unwrap()
        .unwrap();
    let other = state
        .db
        .register_auth_user("kokoro-pauses-other", "public pause test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let status = value(
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
    assert_eq!(status["supportsPauses"], true);
    let mut held = vec![];
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        held.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let mut ids = vec![];
    for pauses in [0., 1.] {
        let response=app.clone().oneshot(request("POST","/api/speech/tts-jobs",&owner.token,json!({"text":"Welcome to our video project. We edit pictures and add clear narration. Thank you for listening.","voiceId":3,"speed":1,"pauses":pauses}))).await.unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        ids.push(value(response).await["jobId"].as_str().unwrap().to_owned());
    }
    state.begin_shutdown();
    drop(held);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let recovered = AppState::new(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
    )
    .with_tts_config(config)
    .with_kokoro_runtime(Some(runtime));
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    let app = support::router(recovered.clone());
    let mut durations = vec![];
    for id in &ids {
        let job = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            loop {
                let job = recovered.get_job(id).await.unwrap().unwrap();
                if matches!(job.status, JobStatus::Done | JobStatus::Error) {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await
            }
        })
        .await
        .unwrap();
        assert_eq!(job.status, JobStatus::Done, "{job:?}");
        let result = serde_json::to_value(job).unwrap();
        durations.push(result["result"]["durationUs"].as_u64().unwrap());
        let path = format!("/api/speech/tts-jobs/{id}/audio");
        assert_eq!(
            app.clone()
                .oneshot(request("GET", &path, &owner.token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(request("GET", &path, &other.token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert!(durations[1] > durations[0]);
    recovered.begin_shutdown();
    assert!(
        recovered
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
