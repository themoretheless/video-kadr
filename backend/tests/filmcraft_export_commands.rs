mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn rpc(app: &Router, token: Option<&str>, command: &str, params: Value) -> (u16, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/compositions/filmcraft/command")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(
                    json!({"command":command,"params":params}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    (
        status,
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap(),
    )
}
async fn ok(app: &Router, token: &str, command: &str, params: Value) -> Value {
    let (status, body) = rpc(app, Some(token), command, params).await;
    assert_eq!(status, 200, "{command}: {body}");
    body
}
#[tokio::test]
async fn source_command_names_share_owner_libraries_queue_rules_and_real_native_exports() {
    let (state, _dir) = support::make_state(false, false).await;
    let alice = state
        .db
        .register_auth_user("commands-alice", "test password")
        .await
        .unwrap()
        .unwrap();
    let bob = state
        .db
        .register_auth_user("commands-bob", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        rpc(&app, None, "export.presets.list", json!({})).await.0,
        401
    );
    assert_eq!(
        rpc(&app, Some(&alice.token), "export.queue.unknown", json!({}))
            .await
            .0,
        400
    );
    assert_eq!(
        rpc(&app, Some(&alice.token), "export.queue.list", json!([]))
            .await
            .0,
        400
    );
    assert_eq!(
        ok(&app, &alice.token, "export.presets.list", json!({})).await["presets"]
            .as_array()
            .unwrap()
            .len(),
        30
    );
    ok(
        &app,
        &alice.token,
        "export.presets.save",
        json!({"name":"Command review","format":"wav","audio":true}),
    )
    .await;
    let preset = ok(
        &app,
        &alice.token,
        "export.presets.get",
        json!({"name":"Command review"}),
    )
    .await;
    assert!(preset.to_string().contains("Command review"));
    ok(
        &app,
        &alice.token,
        "export.presets.favorite",
        json!({"name":"Command review","favorite":true}),
    )
    .await;
    let filtered = ok(
        &app,
        &alice.token,
        "export.presets.list",
        json!({"favorites":true}),
    )
    .await;
    assert_eq!(filtered["presets"].as_array().unwrap().len(), 1);
    assert_eq!(
        rpc(
            &app,
            Some(&bob.token),
            "export.presets.get",
            json!({"name":"Command review"})
        )
        .await
        .0,
        400
    );
    let portable = ok(
        &app,
        &alice.token,
        "export.presets.export",
        json!({"names":["Command review"]}),
    )
    .await;
    ok(
        &app,
        &bob.token,
        "export.presets.import",
        json!({"file":portable}),
    )
    .await;
    ok(
        &app,
        &bob.token,
        "export.presets.get",
        json!({"name":"Command review"}),
    )
    .await;
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
    let add = json!({"document":document,"sequence":6,"sources":{},"preset":"Command review","range":{"startTicks":"0","endTicks":"21168000000"}});
    let added = ok(&app, &alice.token, "export.queue.add", add.clone()).await;
    let id = added["added"][0].as_str().unwrap().parse::<u64>().unwrap();
    assert_eq!(added["items"][0]["preset"], "Command review");
    assert_eq!(added["items"][0]["status"], "ready");
    assert!(added["items"][0]["jobId"].is_null());
    assert!(
        ok(&app, &bob.token, "export.queue.list", json!({})).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        rpc(
            &app,
            Some(&bob.token),
            "export.queue.remove",
            json!({"id":id})
        )
        .await
        .0,
        400
    );
    ok(
        &app,
        &alice.token,
        "export.queue.move",
        json!({"id":id,"by":-100}),
    )
    .await;
    ok(&app, &alice.token, "export.queue.stop", json!({})).await;
    assert_eq!(
        ok(
            &app,
            &alice.token,
            "export.queue.cancel",
            json!({"id":id,"wait":true})
        )
        .await["items"][0]["status"],
        "cancelled"
    );
    ok(&app, &alice.token, "export.queue.retry", json!({"id":id})).await;
    let resumed = ok(
        &app,
        &alice.token,
        "export.queue.start",
        json!({"wait":true}),
    )
    .await;
    assert_eq!(resumed["items"][0]["status"], "done");
    ok(&app, &alice.token, "export.queue.remove", json!({"id":id})).await;
    let mut started = add.clone();
    started["start"] = json!(true);
    started["wait"] = json!(true);
    let done = ok(&app, &alice.token, "export.queue.add", started).await;
    assert_eq!(done["items"][0]["status"], "done");
    let job = state
        .get_job(done["items"][0]["jobId"].as_str().unwrap())
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
    ok(&app, &alice.token, "export.queue.clear", json!({})).await;
    assert_eq!(
        rpc(
            &app,
            Some(&alice.token),
            "export.queue.start",
            json!({"wait":true})
        )
        .await
        .0,
        400
    );
    let mut unsupported = add.clone();
    unsupported["path"] = json!("/private/arbitrary.wav");
    assert_eq!(
        rpc(&app, Some(&alice.token), "export.queue.add", unsupported)
            .await
            .0,
        400
    );
    assert_eq!(
        rpc(
            &app,
            Some(&alice.token),
            "export.queue.remove",
            json!({"id":1.5})
        )
        .await
        .0,
        400
    );
    ok(
        &app,
        &alice.token,
        "export.presets.delete",
        json!({"name":"Command review"}),
    )
    .await;
    ok(
        &app,
        &bob.token,
        "export.presets.get",
        json!({"name":"Command review"}),
    )
    .await;
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
