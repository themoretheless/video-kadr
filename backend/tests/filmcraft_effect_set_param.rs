mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_effect_set_param::edit;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-set-param-oracle.json")).unwrap()
}
fn document(oracle: &Value, case: &Value) -> Value {
    let mut doc =
        json!({"format":"filmcraft.project","schema_version":12,"project":oracle["project"]});
    let item = &mut doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0];
    for (key, value) in case["clipChanges"].as_object().unwrap() {
        item[key] = value.clone();
    }
    for key in case["clipRemovals"].as_array().unwrap() {
        item.as_object_mut().unwrap().remove(key.as_str().unwrap());
    }
    item["effects"] =
        json!([oracle["instances"][case["instance"].as_u64().unwrap() as usize].clone()]);
    doc
}
#[test]
fn all_source_parameter_types_and_animated_clock_outcomes_match_original_engine() {
    let oracle = oracle();
    assert_eq!(
        oracle["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 1463);
    for (case_index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
        let doc = document(&oracle, case);
        let before = doc.clone();
        let mut params = case["params"].clone();
        params["document"] = doc.clone();
        params["sequence"] = oracle["sequence"].clone();
        let result = edit(params);
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {case_index} {}: {result:?}",
            case["params"]
        );
        if let Ok(result) = result {
            let actual = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())
            );
            assert_eq!(
                actual, case["projectSha256"],
                "case {case_index} {}",
                case["params"]
            );
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
async fn authenticated_set_param_reuses_exact_snapshots_and_refuses_wrong_values() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("set-param-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let o = oracle();
    let case = &o["cases"][0];
    let mut params = case["params"].clone();
    params["document"] = json!(serde_json::to_string(&document(&o, case)).unwrap());
    params["context"] = json!({"activeSequence":o["sequence"],"playhead":"0"});
    assert_eq!(
        call(&app, "invalid", "effects.setParam", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "effects.setParam", params.clone()).await;
    assert_eq!(status, 200, "{result}");
    assert!(result["documentText"].is_string());
    params["document"] = result["documentText"].clone();
    params["value"] = json!(true);
    assert_eq!(
        call(&app, &owner.token, "effects.setParam", params).await.0,
        400
    );
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
