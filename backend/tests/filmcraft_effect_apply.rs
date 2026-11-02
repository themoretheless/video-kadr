mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_effect_apply::apply;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-effect-apply-oracle.json")).unwrap()
}
fn document(o: &Value) -> Value {
    json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]})
}
#[test]
fn every_source_effect_apply_outcome_matches_original_project_and_result() {
    let oracle = oracle();
    assert_eq!(
        oracle["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 325);
    for case in oracle["cases"].as_array().unwrap() {
        let doc = document(&oracle);
        let before = doc.clone();
        let mut params = case["params"].clone();
        params["document"] = doc.clone();
        params["sequence"] = oracle["sequence"].clone();
        let result = apply(params);
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "{}: {result:?}",
            case["params"]
        );
        if let Ok(result) = result {
            let hash = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())
            );
            assert_eq!(hash, case["projectSha256"], "{}", case["params"]);
            assert_eq!(result["result"], case["result"], "{}", case["params"]);
            assert_eq!(
                serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),
                result["document"]
            );
        }
        assert_eq!(doc, before);
    }
}
async fn call(app: &axum::Router, token: &str, command: &str, params: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/compositions/filmcraft/command")
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
async fn authenticated_apply_returns_reusable_snapshot_and_transition_id() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("effect-apply-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let o = oracle();
    let params = json!({"document":document(&o),"context":{"activeSequence":o["sequence"],"selection":[o["videoClip"]]},"effect":"Gaussian Blur"});
    assert_eq!(
        call(&app, "invalid", "effects.apply", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "effects.apply", params).await;
    assert_eq!(status, 200, "{result}");
    assert!(result["documentText"].is_string());
    let(status,transition)=call(&app,&owner.token,"effects.apply",json!({"document":result["documentText"],"sequence":o["sequence"],"clip":o["videoClip"],"effect":"cross_dissolve","edge":"out","frames":7})).await;
    assert_eq!(status, 200, "{transition}");
    assert!(transition["result"]["transition"].is_u64());
    let (status, list) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"effects."}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(list.as_array().unwrap().len(), 5);
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[test]
fn extreme_frame_and_preference_clocks_return_errors_instead_of_panicking() {
    let oracle = oracle();
    let mut doc = document(&oracle);
    doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]["settings"]
        ["frame_rate"] = json!({"num":u32::MAX,"den":u32::MAX});
    assert!(apply(json!({"document":doc,"sequence":oracle["sequence"],"clip":oracle["videoClip"],"effect":"cross_dissolve","frames":i64::MAX})).is_err());
    assert!(apply(json!({"document":doc,"sequence":oracle["sequence"],"clip":oracle["videoClip"],"effect":"cross_dissolve","context":{"videoTransitionDuration":1e100,"videoTransitionUnit":"frames"}})).is_err());
}
