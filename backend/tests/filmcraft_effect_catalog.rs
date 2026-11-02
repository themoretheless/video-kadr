mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_effect_catalog::{definitions, list};
fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
#[test]
fn all_effect_definitions_and_filtered_command_outputs_match_actual_original_engine() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-effects-list-oracle.json")).unwrap();
    assert_eq!(
        oracle["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(definitions().len(), 303);
    assert_eq!(hash(&json!(definitions())), oracle["definitionsSha256"]);
    for case in oracle["cases"].as_array().unwrap() {
        let result = list(&case["params"]);
        assert_eq!(
            result.as_array().unwrap().len(),
            case["count"].as_u64().unwrap() as usize,
            "{}",
            case["params"]
        );
        assert_eq!(hash(&result), case["sha256"], "{}", case["params"]);
    }
    let detailed = list(&json!({"detail":true}));
    assert_eq!(
        detailed
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["paramInfo"].as_array().unwrap())
            .count(),
        1426
    );
    assert_eq!(
        detailed[0]["paramInfo"][0]["default"],
        json!({"Vec2":{"x":null,"y":null}})
    );
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
async fn authenticated_catalog_and_batch_retain_full_parameters_without_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("effects-catalog-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    assert_eq!(
        call(&app, "invalid", "effects.list", json!({})).await.0,
        401
    );
    let (status, result) = call(&app, &owner.token, "effects.list", json!({"detail":true})).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result.as_array().unwrap().len(), 303);
    assert_eq!(result, list(&json!({"detail":true})));
    let (status, catalog) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"effects.list"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(catalog[0]["id"], "effects.list");
    let(status,batch)=call(&app,&owner.token,"command.batch",json!({"steps":[{"id":"effects.list","params":{"kind":"AudioTransition","detail":true}},{"id":"effects.list","params":{"kind":"unknown"}}]})).await;
    assert_eq!(status, 200, "{batch}");
    assert_eq!(batch["completed"], 2);
    assert_eq!(batch["results"][0]["result"].as_array().unwrap().len(), 3);
    assert_eq!(batch["results"][1]["result"], json!([]));
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
