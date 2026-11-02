mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
async fn call(
    app: &Router,
    token: &str,
    command: &str,
    params: Value,
    deadline: Option<u64>,
) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/filmcraft/command")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"command":command,"params":params,"deadlineUnixMs":deadline})
                        .to_string(),
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
fn save(name: &str) -> Value {
    json!({"id":"export.presets.save","params":{"name":name,"format":"wav"}})
}
#[tokio::test]
async fn batches_validate_envelopes_first_but_preserve_successes_across_command_failures() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("batch-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let (status, result) = call(
        &app,
        &owner.token,
        "command.batch",
        json!({"steps":[save("First"),{"id":"unknown.command"},save("After")]}),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(result["completed"], 1);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["results"].as_array().unwrap().len(), 2);
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"First"}),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"After"}),
            None
        )
        .await
        .0,
        400
    );
    let(_,result)=call(&app,&owner.token,"command.batch",json!({"stop_on_error":false,"steps":[{"id":"unknown.command"},save("After"),{"id":"export.presets.get","params":{"name":"First"}}]}),None).await;
    assert_eq!(result["completed"], 2);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["results"].as_array().unwrap().len(), 3);
    assert_eq!(result["results"][0]["ok"], false);
    assert_eq!(result["results"][1]["ok"], true);
    let (status, _) = call(
        &app,
        &owner.token,
        "command.batch",
        json!({"steps":[save("Malformed never"),{"params":{}}]}),
        None,
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"Malformed never"}),
            None
        )
        .await
        .0,
        400
    );
    let (_, empty) = call(
        &app,
        &owner.token,
        "command.batch",
        json!({"steps":[],"stop_on_error":null}),
        None,
    )
    .await;
    assert_eq!(empty, json!({"completed":0,"failed":0,"results":[]}));
    assert_eq!(
        call(
            &app,
            &owner.token,
            "command.batch",
            json!({"steps":[save("Expired never")]}),
            Some(now() - 1)
        )
        .await
        .0,
        408
    );
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"Expired never"}),
            None
        )
        .await
        .0,
        400
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[tokio::test]
async fn a_shared_deadline_stops_later_steps_while_an_accepted_export_finishes() {
    use video_kadr_backend::config::resource_classes::ResourceClass;
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("batch-deadline-owner", "test password")
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
    let(status,added)=call(&app,&owner.token,"export.queue.add",json!({"document":document,"sequence":6,"format":"wav","range":{"startTicks":"0","endTicks":"10584000000"}}),None).await;
    assert_eq!(status, 200, "{added}");
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let(status,result)=call(&app,&owner.token,"command.batch",json!({"stop_on_error":false,"steps":[{"id":"export.queue.start","params":{"wait":true}},save("After deadline never")]}),Some(now()+1500)).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["completed"], 0);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["results"].as_array().unwrap().len(), 1);
    assert!(result["results"][0]["error"]
        .as_str()
        .unwrap()
        .contains("unconfirmed"));
    assert_eq!(
        call(
            &app,
            &owner.token,
            "export.presets.get",
            json!({"name":"After deadline never"}),
            None
        )
        .await
        .0,
        400
    );
    let (_, queue) = call(&app, &owner.token, "export.queue.list", json!({}), None).await;
    assert_eq!(queue["items"][0]["status"], "encoding");
    drop(permits);
    let done = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let (_, queue) = call(&app, &owner.token, "export.queue.list", json!({}), None).await;
            if queue["items"][0]["status"] == "done" {
                break queue;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let job = state
        .get_job(done["items"][0]["jobId"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let result = job.result.unwrap();
    assert!(state
        .outputs_dir()
        .join(result["filename"].as_str().unwrap())
        .is_file());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
