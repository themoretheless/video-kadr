mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(app: &Router, token: &str, command: &str, params: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/filmcraft/command")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"command":command,"params":params}).to_string(),
                ))
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
#[tokio::test]
async fn command_ranges_use_the_explicit_active_clock_and_admit_all_ranges_atomically() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("time-command-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
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
    let mut target = document["project"]["items"]["6"].clone();
    target["id"] = json!(7);
    target["name"] = json!("25 fps target");
    target["kind"]["Sequence"]["settings"]["frame_rate"] = json!({"num":25,"den":1});
    document["project"]["items"]["7"] = target;
    let base = json!({"document":document,"sequence":7,"activeSequenceId":6,"format":"wav","fps":30,"sources":{}});
    let mut invalid = base.clone();
    invalid["ranges"] = json!([{"startFrame":0,"endFrame":2},{"startTimecode":"bad","endFrame":4}]);
    assert_eq!(
        call(&app, &owner.token, "export.queue.add", invalid)
            .await
            .0,
        400
    );
    assert!(call(&app, &owner.token, "export.queue.list", json!({}))
        .await
        .1["items"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut batch = base.clone();
    batch["sequences"] = json!([6, 7]);
    batch["range"] = json!({"startFrame":1,"endFrame":3});
    let (status, batch) = call(&app, &owner.token, "export.queue.add", batch).await;
    assert_eq!(status, 200, "{batch}");
    assert_eq!(batch["items"].as_array().unwrap().len(), 2);
    for item in batch["items"].as_array().unwrap() {
        assert_eq!(
            item["sourceRange"],
            json!({"startTicks":"10584000000","endTicks":"31752000000"})
        );
        assert!(item["jobId"].is_null());
    }
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.queue.clear",
            json!({"all":true})
        )
        .await
        .0,
        200
    );
    let mut exported = base.clone();
    exported["range"] = json!("custom");
    exported["startTimecode"] = json!("+1");
    exported["endTimecode"] = json!("+3");
    exported["playheadTime"] = json!("105840000000");
    exported["start"] = json!(true);
    exported["wait"] = json!(true);
    let (status, exported) = call(&app, &owner.token, "export.queue.add", exported).await;
    assert_eq!(status, 200, "{exported}");
    assert_eq!(exported["items"][0]["status"], "done");
    assert_eq!(
        exported["items"][0]["sourceRange"],
        json!({"startTicks":"116424000000","endTicks":"137592000000"})
    );
    let job = state
        .get_job(exported["items"][0]["jobId"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let result = job.result.unwrap();
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
                .join(result["filename"].as_str().unwrap()),
        )
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["duration_ts"], 4000);
    call(
        &app,
        &owner.token,
        "export.queue.clear",
        json!({"all":true}),
    )
    .await;
    let mut fallback = base;
    fallback["ranges"] = json!([]);
    fallback["startTime"] = json!("9007199254740993");
    fallback["endTime"] = json!("9007209838740993");
    let (status, fallback) = call(&app, &owner.token, "export.queue.add", fallback).await;
    assert_eq!(status, 200, "{fallback}");
    assert_eq!(
        fallback["items"][0]["sourceRange"]["startTicks"],
        "9007199254740993"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
