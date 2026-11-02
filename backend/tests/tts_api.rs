mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
fn request(token: Option<&str>, body: serde_json::Value) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/speech/synthesize")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request.body(Body::from(body.to_string())).unwrap()
}
#[tokio::test]
async fn authenticated_admission_without_configured_voice_model() {
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("tts-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let valid = json!({"text":"Welcome to our project","voiceId":3,"speed":1.});
    assert_eq!(
        app.clone()
            .oneshot(request(None, valid.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    for body in [
        json!({"text":"","voiceId":3,"speed":1.}),
        json!({"text":"Hello","voiceId":35,"speed":1.}),
        json!({"text":"Hello","voiceId":3,"speed":8.}),
        json!({"text":"Hello","voiceId":3,"speed":1.,"output":"/tmp/foreign"}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(request(Some(&owner.token), body))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&owner.token), valid))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/speech/voices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(value["available"], false);
    assert_eq!(value["voices"].as_array().unwrap().len(), 36);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[tokio::test]
#[ignore = "requires official configured Kokoro and sherpa-onnx executable"]
async fn real_narration_is_private_wav_response_and_leaves_no_staging() {
    let (state, _dir) = support::make_state(true, false).await;
    let state = state.with_tts_config(video_kadr_backend::services::local_tts::TtsConfig {
        executable: std::env::var("VIDEO_KADR_TEST_TTS_BIN").unwrap().into(),
        model_dir: Some(std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap().into()),
        threads: 2,
    });
    let owner = state
        .db
        .register_auth_user("tts-real-owner", "narration test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let response = app
        .oneshot(request(
            Some(&owner.token),
            json!({"text":"Welcome to our video project.","voiceId":3,"speed":1.}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "audio/wav");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.headers()["x-tts-model-sha256"],
        "4b86207ef680e394d8343bee22dfc4c512e5c707c6d9578e3f35ab09bffd6b36"
    );
    let bytes = to_bytes(response.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap();
    let audio =
        video_kadr_backend::domain::filmcraft_wav_audio::WavSource::parse(bytes.to_vec().into())
            .unwrap();
    assert_eq!(audio.sample_rate(), 24000);
    assert!(audio.frames() > 24000);
    assert_eq!(
        std::fs::read_dir(state.storage.join("tts-staging"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(std::fs::read_dir(state.outputs_dir()).unwrap().count(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
