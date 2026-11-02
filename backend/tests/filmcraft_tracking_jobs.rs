mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{library::MediaEntry, state::AppState};
mod inputs {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scripts/reference/filmcraft_mask_tracking_frames.rs"
    ));
}
const FD: i64 = 10_584_000_000;
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/filmcraft-mask-track-batch-oracle.json"
    ))
    .unwrap()
}
fn input(frames: i64) -> Value {
    let o = fixture();
    let mut d = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
    d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]
        ["items"][0] = o["cases"][0]["clip"].clone();
    let clip = &mut d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0];
    clip["duration"] = json!((frames + 2) * FD);
    let clip_id = clip["id"].as_u64().unwrap().to_string();
    json!({"document":serde_json::to_string(&d).unwrap(),"sequence":o["sequence"].as_u64().unwrap().to_string(),"clip":clip_id,"effect":"opacity","mask":0,"frames":frames,"sources":{(o["source"].as_u64().unwrap().to_string()):"texture"},"context":{"playhead":"0"}})
}
fn keys(text: &Value) -> Value {
    let d: Value = serde_json::from_str(text.as_str().unwrap()).unwrap();
    let o = fixture();
    d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"]
        [0]["effects"][0]["masks"][0]["path"]["keyframes"]
        .clone()
}
async fn call(app: &axum::Router, token: &str, command: &str, p: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/compositions/filmcraft/command")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"command":command,"params":p}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        serde_json::from_slice(
            &to_bytes(response.into_body(), 4 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}
async fn ok(app: &axum::Router, token: &str, command: &str, p: Value) -> Value {
    let (status, result) = call(app, token, command, p).await;
    assert_eq!(status, 200, "{command}: {result}");
    result
}
async fn poll(app: &axum::Router, token: &str, id: &Value, snapshot: bool) -> Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let rows = ok(
            app,
            token,
            "jobs.list",
            json!({"job":id,"snapshot":snapshot}),
        )
        .await;
        let row = rows[0].clone();
        if row["finished"] == true {
            return row;
        }
        assert!(tokio::time::Instant::now() < deadline, "{rows}");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}
async fn owner(state: &AppState, name: &str) -> String {
    state
        .db
        .register_auth_user(name, "shared test password")
        .await
        .unwrap()
        .unwrap()
        .token
}
async fn still(state: &AppState, dir: &std::path::Path) {
    image::save_buffer(
        dir.join("sources/texture.png"),
        &inputs::frame(240, 180, 0, "still"),
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry::from_result(
                "source",
                &json!({"id":"texture","filename":"texture.png","mediaType":"image"})
            ))
            .await
    );
}
async fn close(state: &AppState) {
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}
#[tokio::test]
async fn waiting_and_background_tracking_have_owner_isolation_and_retained_cancelled_keys() {
    let (state, dir) = support::make_state(false, false).await;
    still(&state, dir.path()).await;
    let alice = owner(&state, "tracking-alice").await;
    let bob = owner(&state, "tracking-bob").await;
    let app = support::router(state.clone());
    let mut p = input(4);
    p["wait"] = json!(true);
    assert_eq!(call(&app, "invalid", "masks.track", p.clone()).await.0, 401);
    let done = ok(&app, &alice, "masks.track", p).await;
    assert_eq!(done["status"]["finished"], true);
    assert_eq!(done["status"]["done"], 4);
    assert!(done["status"]["result"].get("extra_files").is_none());
    assert!(done["status"]["result"].get("error").is_none());
    assert_eq!(
        keys(&done["status"]["documentText"])
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(ok(&app, &bob, "jobs.list", json!({})).await, json!([]));
    assert_eq!(
        call(&app, &bob, "jobs.cancel", json!({"job":done["job"]}))
            .await
            .0,
        404
    );
    let p = input(2000);
    let running = ok(&app, &alice, "masks.track", p.clone()).await;
    assert_eq!(running["status"]["finished"], false);
    assert_eq!(call(&app, &alice, "masks.track", p).await.0, 409);
    assert_eq!(
        call(
            &app,
            &alice,
            "jobs.cancel",
            json!({"job":running["job"],"discard":true})
        )
        .await
        .0,
        409
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let rows = ok(&app, &alice, "jobs.list", json!({"job":running["job"]})).await;
        if rows[0]["done"].as_u64().unwrap() >= 2 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        ok(
            &app,
            &alice,
            "jobs.cancel",
            json!({"job":running["job"].as_u64().unwrap().to_string()})
        )
        .await,
        Value::Null
    );
    let cancelled = poll(&app, &alice, &running["job"], true).await;
    assert!(cancelled["status"].as_str().unwrap().contains("stopped"));
    let count = cancelled["done"].as_u64().unwrap();
    assert!(count >= 2 && count < 2000);
    assert_eq!(
        keys(&cancelled["documentText"]).as_array().unwrap().len(),
        count as usize + 1
    );
    assert_eq!(cancelled["result"]["frames"], count);
    assert_eq!(
        ok(&app, &alice, "jobs.cancel", json!({"job":running["job"]})).await,
        Value::Null
    );
    assert!(state.db.load_jobs().await.unwrap().is_empty());
    close(&state).await;
}
#[tokio::test]
async fn immutable_media_revision_and_private_access_are_checked_before_snapshot_publication() {
    let (state, dir) = support::make_state(false, false).await;
    still(&state, dir.path()).await;
    let alice = state
        .db
        .register_auth_user("revision-alice", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let bob = owner(&state, "revision-bob").await;
    let space = state
        .db
        .create_space(&alice.user.username, "Private tracking")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Tracking claim",
            &json!({"schemaVersion":1,"sources":{"texture":{"id":"texture"}}}),
            &["texture".into()],
            &alice.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let p = input(2000);
    assert_eq!(call(&app, &bob, "masks.track", p.clone()).await.0, 403);
    let started = ok(&app, &alice.token, "masks.track", p).await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let rows = ok(
            &app,
            &alice.token,
            "jobs.list",
            json!({"job":started["job"]}),
        )
        .await;
        if rows[0]["done"].as_u64().unwrap() > 0 {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let entry = state.library.get("texture").await.unwrap();
    let path = state.library.resolve_media_path(&entry).await.unwrap();
    image::save_buffer(
        path,
        &vec![0; 240 * 180 * 4],
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let failed = poll(&app, &alice.token, &started["job"], false).await;
    assert!(failed["result"]["error"]
        .as_str()
        .unwrap()
        .contains("changed"));
    assert_eq!(
        call(
            &app,
            &alice.token,
            "jobs.list",
            json!({"job":started["job"],"snapshot":true})
        )
        .await
        .0,
        400
    );
    close(&state).await;
}
#[tokio::test]
async fn generated_feature_loss_retains_seed_and_unbound_files_do_not_get_opened() {
    let (state, _dir) = support::make_state(false, false).await;
    let token = owner(&state, "generated-tracker").await;
    let app = support::router(state.clone());
    let p = input(3);
    assert_eq!(call(&app, &token, "masks.track", p.clone()).await.0, 404);
    let mut p = p;
    let mut d: Value = serde_json::from_str(p["document"].as_str().unwrap()).unwrap();
    let o = fixture();
    d["project"]["items"][o["source"].to_string()]["kind"]["Media"]["media"] =
        json!({"Generator":{"ColorMatte":{"color":[0.2,0.4,0.8,1.]}}});
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p.as_object_mut().unwrap().remove("sources");
    p["wait"] = json!(true);
    let tracked = ok(&app, &token, "masks.track", p).await;
    assert_eq!(tracked["status"]["done"], 0);
    assert!(tracked["status"]["result"]["error"]
        .as_str()
        .unwrap()
        .contains("lost track"));
    assert_eq!(
        keys(&tracked["status"]["documentText"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    close(&state).await;
}
#[tokio::test]
async fn authorized_library_movie_and_image_sequence_track_multiple_actual_frames() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let (state, dir) = support::make_state(false, false).await;
    let token = owner(&state, "movie-tracker").await;
    let movie = dir.path().join("sources/texture.mov");
    let mut encoder = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "240x180",
            "-framerate",
            "24",
            "-i",
            "pipe:0",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-y",
        ])
        .arg(&movie)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = encoder.stdin.take().unwrap();
        for step in 0..6 {
            stdin
                .write_all(&inputs::frame(240, 180, step, "translation"))
                .unwrap();
        }
    }
    assert!(encoder.wait_with_output().unwrap().status.success());
    assert!(
        state
            .library
            .add(MediaEntry::from_result(
                "source",
                &json!({"id":"texture","filename":"texture.mov","mediaType":"video"})
            ))
            .await
    );
    let app = support::router(state.clone());
    let mut p = input(4);
    p["wait"] = json!(true);
    let tracked = ok(&app, &token, "masks.track", p.clone()).await;
    assert_eq!(tracked["status"]["done"], 4, "{tracked}");
    assert_eq!(
        keys(&tracked["status"]["documentText"])
            .as_array()
            .unwrap()
            .len(),
        5
    );
    for step in 0..6 {
        let id = format!("seq-{step}");
        let filename = format!("{id}.png");
        image::save_buffer(
            dir.path().join("sources").join(&filename),
            &inputs::frame(240, 180, step, "translation"),
            240,
            180,
            image::ColorType::Rgba8,
        )
        .unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"originalFilename":format!("frame_{step:04}.png"),"mediaType":"image"}))).await);
    }
    let mut d: Value = serde_json::from_str(p["document"].as_str().unwrap()).unwrap();
    let o = fixture();
    d["project"]["items"][o["source"].to_string()]["kind"]["Media"]["info"]["kind"] =
        json!("ImageSequence");
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p["sources"][o["source"].as_u64().unwrap().to_string()] = json!("seq-0");
    let tracked = ok(&app, &token, "masks.track", p).await;
    assert_eq!(tracked["status"]["done"], 4, "{tracked}");
    assert_eq!(
        keys(&tracked["status"]["documentText"])
            .as_array()
            .unwrap()
            .len(),
        5
    );
    image::save_buffer(
        dir.path().join("sources/seq-6.png"),
        &inputs::frame(240, 180, 6, "translation"),
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"seq-6","filename":"seq-6.png","originalFilename":"frame_0006.png","mediaType":"image"}))).await);
    assert_eq!(
        call(
            &app,
            &token,
            "jobs.list",
            json!({"job":tracked["job"],"snapshot":true})
        )
        .await
        .0,
        400
    );
    close(&state).await;
}

#[tokio::test]
async fn tracking_decoder_admits_sources_wider_than_full_preview_budget_at_original_analysis_scale()
{
    let (state, dir) = support::make_state(false, false).await;
    let path = dir.path().join("sources/wide.mkv");
    let result = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=4096x2160:r=1",
            "-frames:v",
            "1",
            "-c:v",
            "ffv1",
            "-threads",
            "1",
            "-pix_fmt",
            "bgra",
            "-y",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cache = &state.filmcraft_video_previews;
    let runtime = &state.process_runtime;
    assert!(
        video_kadr_backend::services::filmcraft_video_preview::frame_cached_full(
            cache, runtime, &path, 0
        )
        .await
        .is_err()
    );
    let frame = video_kadr_backend::services::filmcraft_video_preview::frame_tracking(
        cache, runtime, &path, 0,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!((frame.surface.width, frame.surface.height), (960, 506));
    assert_eq!(frame.surface.rgba.len(), 960 * 506 * 4);
    let thumbnail = video_kadr_backend::services::filmcraft_video_preview::frame_cached(
        cache, runtime, &path, 0,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        (thumbnail.surface.width, thumbnail.surface.height),
        (480, 253)
    );
    close(&state).await;
}

#[tokio::test]
async fn snapshot_capacity_never_evicts_completed_work_without_explicit_discard() {
    let (state, _dir) = support::make_state(false, false).await;
    let token = owner(&state, "retained-tracking").await;
    let app = support::router(state.clone());
    let mut p = input(2);
    let o = fixture();
    let mut d: Value = serde_json::from_str(p["document"].as_str().unwrap()).unwrap();
    d["project"]["items"][o["source"].to_string()]["kind"]["Media"]["media"] =
        json!({"Generator":{"ColorMatte":{"color":[0.2,0.4,0.8,1.]}}});
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p.as_object_mut().unwrap().remove("sources");
    p["wait"] = json!(true);
    let mut first = Value::Null;
    let mut last = Value::Null;
    for i in 0..16 {
        let tracked = ok(&app, &token, "masks.track", p.clone()).await;
        if i == 0 {
            first = tracked["job"].clone();
        }
        last = tracked["job"].clone();
    }
    assert_eq!(call(&app, &token, "masks.track", p.clone()).await.0, 429);
    let rows = ok(
        &app,
        &token,
        "jobs.list",
        json!({"job":first,"snapshot":true}),
    )
    .await;
    assert!(rows[0]["documentText"].is_string());
    assert_eq!(keys(&rows[0]["documentText"]).as_array().unwrap().len(), 1);
    assert_eq!(
        ok(
            &app,
            &token,
            "jobs.cancel",
            json!({"job":first,"discard":true})
        )
        .await,
        Value::Null
    );
    assert_eq!(
        ok(&app, &token, "jobs.list", json!({"job":first})).await,
        json!([])
    );
    let next = ok(&app, &token, "masks.track", p).await;
    assert_eq!(next["job"].as_u64().unwrap(), last.as_u64().unwrap() + 1);
    close(&state).await;
}
#[tokio::test]
async fn task_instance_prevents_reading_or_discarding_a_reused_id_after_server_restart() {
    let (state, _dir) = support::make_state(false, false).await;
    let token = owner(&state, "restart-tracking").await;
    let app = support::router(state.clone());
    let mut p = input(2);
    let o = fixture();
    let mut d: Value = serde_json::from_str(p["document"].as_str().unwrap()).unwrap();
    d["project"]["items"][o["source"].to_string()]["kind"]["Media"]["media"] =
        json!({"Generator":{"ColorMatte":{"color":[0.2,0.4,0.8,1.]}}});
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p.as_object_mut().unwrap().remove("sources");
    p["wait"] = json!(true);
    let old = ok(&app, &token, "masks.track", p.clone()).await;
    let restarted = AppState::new_with_runtime(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        (*state.library).clone(),
        (*state.db).clone(),
        state.encode_budget.clone(),
        2,
    )
    .unwrap();
    let new_app = support::router(restarted.clone());
    let new = ok(&new_app, &token, "masks.track", p).await;
    assert_eq!(old["job"], new["job"]);
    assert_ne!(old["status"]["instance"], new["status"]["instance"]);
    assert_eq!(
        ok(
            &new_app,
            &token,
            "jobs.list",
            json!({"job":old["job"],"instance":old["status"]["instance"],"snapshot":true})
        )
        .await,
        json!([])
    );
    assert_eq!(
        call(
            &new_app,
            &token,
            "jobs.cancel",
            json!({"job":old["job"],"instance":old["status"]["instance"],"discard":true})
        )
        .await
        .0,
        404
    );
    let actual = ok(
        &new_app,
        &token,
        "jobs.list",
        json!({"job":new["job"],"instance":new["status"]["instance"],"snapshot":true}),
    )
    .await;
    assert!(actual[0]["documentText"].is_string());
    close(&restarted).await;
    close(&state).await;
}
