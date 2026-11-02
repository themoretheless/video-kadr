mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::config::resource_classes::ResourceClass;
const URI: &str = "/api/compositions/filmcraft/export-queue";
const FD: i64 = 10_584_000_000;

async fn request(app: &Router, token: &str, command: Option<Value>) -> (u16, Value) {
    let mut request = Request::builder()
        .uri(URI)
        .header("authorization", format!("Bearer {token}"));
    let body = if let Some(command) = command {
        request = request
            .method("POST")
            .header("content-type", "application/json");
        Body::from(command.to_string())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
        .unwrap();
    (status, body)
}
async fn command(app: &Router, token: &str, body: Value) -> Value {
    let (status, result) = request(app, token, Some(body)).await;
    assert_eq!(status, 200, "{result}");
    result
}
async fn until(app: &Router, token: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let (status, result) = request(app, token, None).await;
            assert_eq!(status, 200, "{result}");
            if predicate(&result) {
                return result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn owner_queue_admits_atomic_batches_stops_cancels_and_executes_remaining_snapshots() {
    let (state, _dir) = support::make_state(false, false).await;
    let alice = state
        .db
        .register_auth_user("queue-alice", "test password")
        .await
        .unwrap()
        .unwrap();
    let bob = state
        .db
        .register_auth_user("queue-bob", "test password")
        .await
        .unwrap()
        .unwrap();
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
    let app = support::router(state.clone());
    let add = json!({"action":"add","wait":true,"input":{"document":document,"sequenceId":"6","sources":{},"preset":"wav16"},"sequences":["6","6"],"ranges":[{"startTicks":"0","endTicks":(2*FD).to_string()},{"startTicks":(3*FD).to_string(),"endTicks":(4*FD).to_string()}]});
    let mut invalid = add.clone();
    invalid["sequences"] = json!(["6", "missing"]);
    assert_eq!(request(&app, &alice.token, Some(invalid)).await.0, 400);
    assert!(request(&app, &alice.token, None).await.1["items"]
        .as_array()
        .unwrap()
        .is_empty());
    let added = command(&app, &alice.token, add).await;
    let items = added["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert!(items
        .iter()
        .all(|item| item["status"] == "ready" && item["jobId"].is_null()));
    assert!(!added.to_string().contains("\"document\""));
    assert!(request(&app, &bob.token, None).await.1["items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        request(
            &app,
            &bob.token,
            Some(json!({"action":"remove","id":items[0]["id"]}))
        )
        .await
        .0,
        400
    );
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    command(&app, &alice.token, json!({"action":"start"})).await;
    let active = until(&app, &alice.token, |v| {
        v["items"][0]["status"] == "encoding"
    })
    .await;
    assert!(active["items"].as_array().unwrap()[1..]
        .iter()
        .all(|item| item["status"] == "ready" && item["jobId"].is_null()));
    // Source wait=true does not wait on an already running async encode.
    // All export permits remain held, so a mistakenly draining call would hang.
    let repeated = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        command(&app, &alice.token, json!({"action":"start","wait":true})),
    )
    .await
    .unwrap();
    assert_eq!(repeated["items"][0]["status"], "encoding");
    let id = active["items"][0]["id"].clone();
    let job_id = active["items"][0]["jobId"].as_str().unwrap().to_owned();
    state
        .update_job(&job_id, |job| job.progress = Some(0.0))
        .await;
    assert!(request(&app, &alice.token, None).await.1["items"][0]["etaSeconds"].is_null());
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    state
        .update_job(&job_id, |job| job.progress = Some(25.0))
        .await;
    let measured = request(&app, &alice.token, None).await.1;
    assert_eq!(measured["items"][0]["progress"], 0.25);
    let eta = measured["items"][0]["etaSeconds"].as_f64().unwrap();
    assert!(eta > 0.0 && eta <= 360000.0, "{measured}");
    assert!(measured["items"][0]["etaText"].as_str().is_some());
    state
        .update_job(&job_id, |job| job.progress = Some(10.0))
        .await;
    assert!(request(&app, &alice.token, None).await.1["items"][0]["etaSeconds"].is_null());
    command(&app, &alice.token, json!({"action":"stop"})).await;
    for action in [
        json!({"action":"remove","id":id}),
        json!({"action":"clear","all":true}),
    ] {
        assert_eq!(request(&app, &alice.token, Some(action)).await.0, 400);
    }
    let cancelled = command(
        &app,
        &alice.token,
        json!({"action":"cancel","id":id,"wait":true}),
    )
    .await;
    assert_eq!(cancelled["items"][0]["status"], "cancelled");
    assert!(cancelled["items"][0]["etaSeconds"].is_null());
    until(&app, &alice.token, |v| {
        v["items"][0]["status"] == "cancelled"
    })
    .await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/jobs/{job_id}/retry"))
                .header("authorization", format!("Bearer {}", alice.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    command(&app, &alice.token, json!({"action":"remove","id":id})).await;
    command(
        &app,
        &alice.token,
        json!({"action":"cancel","id":items[3]["id"]}),
    )
    .await;
    let remaining = command(&app, &alice.token, json!({"action":"clear"})).await;
    assert_eq!(remaining["items"].as_array().unwrap().len(), 2);
    drop(permits);
    let done = command(&app, &alice.token, json!({"action":"start","wait":true})).await;
    assert_eq!(done["running"], false);
    assert!(done["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["status"] == "done"));
    assert!(done["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["etaSeconds"].is_null() && item["etaText"].is_null()));
    let mut jobs = Vec::new();
    for (item, samples) in done["items"].as_array().unwrap().iter().zip([2000, 4000]) {
        let id = item["jobId"].as_str().unwrap();
        jobs.push(id);
        let job = state.get_job(id).await.unwrap().unwrap();
        let result = job.result.unwrap();
        let file = state
            .outputs_dir()
            .join(result["filename"].as_str().unwrap());
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
            .arg(file)
            .output()
            .await
            .unwrap();
        assert!(probe.status.success());
        let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(probe["streams"][0]["duration_ts"], samples);
    }
    assert_ne!(jobs[0], jobs[1]);
    let retry = command(
        &app,
        &alice.token,
        json!({"action":"retry","id":done["items"][0]["id"],"start":true,"wait":true}),
    )
    .await;
    assert_eq!(retry["items"][0]["status"], "done");
    assert_ne!(retry["items"][0]["jobId"], done["items"][0]["jobId"]);
    let mut immediate = json!({"action":"add","input":{"document":document,"sequenceId":"6","sources":{},"preset":"wav16"},"ranges":[{"startTicks":"0","endTicks":FD.to_string()}],"start":true,"wait":true});
    // This snapshot is independent of the already completed/retried items.
    immediate["input"]["audio"] = json!({"channels":1,"sampleRate":48000});
    let immediate = command(&app, &alice.token, immediate).await;
    assert_eq!(immediate["running"], false);
    assert_eq!(immediate["added"].as_array().unwrap().len(), 1);
    assert_eq!(
        immediate["items"].as_array().unwrap().last().unwrap()["status"],
        "done"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
