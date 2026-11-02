mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
const WAVE: &str = "Waveform Audio 48 kHz 16-bit";
fn document() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    sequence["settings"]["width"] = json!(64);
    sequence["settings"]["height"] = json!(48);
    for tracks in [
        "video_tracks",
        "audio_tracks",
        "submix_tracks",
        "caption_tracks",
    ] {
        sequence[tracks] = json!([]);
    }
    sequence["mark_in"] = json!(10584000000i64);
    sequence["mark_out"] = json!(21168000000i64);
    document
}
async fn post(app: &Router, token: &str, path: &str, payload: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}
async fn command(app: &Router, token: &str, params: Value) -> (u16, Value) {
    post(
        app,
        token,
        "/api/compositions/filmcraft/command",
        json!({"command":"export.quick","params":params}),
    )
    .await
}
#[tokio::test]
async fn quick_command_shares_remembered_owner_presets_with_native_route_and_waits_for_real_outputs(
) {
    let (state, _dir) = support::make_state(false, false).await;
    let alice = state
        .db
        .register_auth_user("quick-command-alice", "test password")
        .await
        .unwrap()
        .unwrap();
    let bob = state
        .db
        .register_auth_user("quick-command-bob", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let base = json!({"document":serde_json::to_string(&document()).unwrap(),"sequence":6,"sources":{},"wait":true});
    let mut first = base.clone();
    first["preset"] = json!(WAVE);
    first["format"] = json!("wav");
    first["fps"] = json!(30);
    first["settings"] = json!({"audio":{"channels":1,"bits":24,"sample_rate":48000}});
    first["range"] = json!({"startFrame":1,"endFrame":3});
    let (status, first) = command(&app, &alice.token, first).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["status"], "done");
    assert_eq!(first["job"], first["jobId"]);
    assert_eq!(first["preset"], WAVE);
    assert!(first.get("path").is_none());
    assert_eq!(first["result"]["bitsPerSample"], 24);
    assert_eq!(first["result"]["channels"], 1);
    let probe = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(
            state
                .outputs_dir()
                .join(first["result"]["filename"].as_str().unwrap()),
        )
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["duration_ts"], 4000);
    assert_eq!(probe["streams"][0]["bits_per_sample"], 24);
    assert_eq!(probe["streams"][0]["channels"], 1);
    let mut invalid = base.clone();
    invalid["preset"] = json!("Waveform Audio 48 kHz 24-bit");
    invalid["sequence"] = json!(999);
    assert_eq!(command(&app, &alice.token, invalid).await.0, 400);
    let mut invalid = base.clone();
    invalid["path"] = json!("/private/output.wav");
    assert_eq!(command(&app, &alice.token, invalid).await.0, 400);
    let (status, remembered) = command(&app, &alice.token, base.clone()).await;
    assert_eq!(status, 200, "{remembered}");
    assert_eq!(remembered["status"], "done");
    assert_eq!(remembered["preset"], WAVE);
    assert_eq!(remembered["result"]["bitsPerSample"], 16);
    assert_eq!(remembered["result"]["channels"], 2);
    let (status, native) = post(
        &app,
        &alice.token,
        "/api/compositions/filmcraft/quick-export",
        json!({"document":document(),"sequenceId":"6","sources":{},"wait":true}),
    )
    .await;
    assert_eq!(status, 202, "{native}");
    assert_eq!(native["status"], "done");
    assert_eq!(native["presetName"], WAVE);
    assert_eq!(native["result"]["bitsPerSample"], 16);
    let mut nullable = base.clone();
    nullable["preset"] = Value::Null;
    nullable["settings"] = Value::Null;
    let (status, nullable) = command(&app, &alice.token, nullable).await;
    assert_eq!(status, 200, "{nullable}");
    assert_eq!(nullable["status"], "done");
    assert_eq!(nullable["preset"], WAVE);
    let (status, other) = command(&app, &bob.token, base).await;
    assert_eq!(status, 200, "{other}");
    assert_eq!(other["status"], "done");
    assert_eq!(other["preset"], "Match Source – Adaptive High Bitrate");
    assert_eq!(other["result"]["container"], "mp4");
    let (_, queue) = post(
        &app,
        &alice.token,
        "/api/compositions/filmcraft/command",
        json!({"command":"export.queue.list","params":{}}),
    )
    .await;
    assert!(queue["items"].as_array().unwrap().is_empty());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
