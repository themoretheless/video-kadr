mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_effect_edit::edit;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-effect-edit-oracle.json")).unwrap()
}
fn document(oracle: &Value, rack: &Value) -> Value {
    let mut doc =
        json!({"format":"filmcraft.project","schema_version":12,"project":oracle["project"]});
    doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"]
        [0]["items"][0]["effects"] = rack.clone();
    doc
}
fn rack<'a>(doc: &'a Value, oracle: &Value) -> &'a Value {
    &doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]
        ["items"][0]["effects"]
}
#[test]
fn resets_for_every_definition_and_rack_operations_match_actual_original_engine() {
    let oracle = oracle();
    assert_eq!(
        oracle["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 311);
    for case in oracle["cases"].as_array().unwrap() {
        let doc = document(&oracle, &case["before"]);
        let before = doc.clone();
        let result = edit(
            case["command"].as_str().unwrap(),
            json!({"document":doc,"sequence":oracle["sequence"].to_string(),"clip":oracle["clip"].to_string(),"index":case["index"]}),
        );
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "{} {}: {result:?}",
            case["command"],
            case["before"][0]["effect"]
        );
        if let Ok(result) = result {
            let actual = rack(&result["document"], &oracle);
            let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(actual).unwrap()));
            assert_eq!(
                hash, case["afterSha256"],
                "{} {}",
                case["command"], case["before"][0]["effect"]
            );
            assert_eq!(
                serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),
                result["document"]
            );
            assert!(result["result"].is_null());
        }
        assert_eq!(doc, before);
    }
}
#[test]
fn snapshot_edits_preserve_unknown_fields_and_allow_locked_tracks_like_source() {
    let oracle = oracle();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "effects.toggleEnabled" && c["index"] == 0)
        .unwrap();
    let mut doc = document(&oracle, &case["before"]);
    doc["project"]["opaque"] = json!({"kept":true});
    let track = &mut doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]
        ["video_tracks"][0];
    track["locked"] = json!(true);
    track["items"][0]["effects"][0]["opaque"] = json!("kept");
    let result=edit("effects.toggleEnabled",json!({"document":doc,"clip":oracle["clip"],"context":{"activeSequence":oracle["sequence"]}})).unwrap();
    assert_eq!(
        result["document"]["project"]["opaque"],
        json!({"kept":true})
    );
    assert_eq!(rack(&result["document"], &oracle)[0]["opaque"], "kept");
    assert_eq!(rack(&result["document"], &oracle)[0]["enabled"], false);
    assert!(edit(
        "effects.remove",
        json!({"document":doc,"sequence":oracle["sequence"],"clip":oracle["clip"]})
    )
    .is_err());
    assert!(edit(
        "effects.reset",
        json!({"document":doc,"sequence":oracle["sequence"],"clip":"18446744073709551615"})
    )
    .is_err());
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
async fn authenticated_rack_commands_return_reusable_snapshots_without_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("effect-edit-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let oracle = oracle();
    let case = &oracle["cases"][0];
    let params = json!({"document":document(&oracle,&case["before"]),"sequence":oracle["sequence"],"clip":oracle["clip"],"index":0});
    assert_eq!(
        call(&app, "invalid", "effects.reset", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "effects.reset", params.clone()).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(rack(&result["document"], &oracle)[0]["enabled"], true);
    let(status,toggled)=call(&app,&owner.token,"effects.toggleEnabled",json!({"document":result["documentText"],"sequence":oracle["sequence"],"clip":oracle["clip"]})).await;
    assert_eq!(status, 200, "{toggled}");
    assert_eq!(rack(&toggled["document"], &oracle)[0]["enabled"], false);
    assert_eq!(
        call(&app, &owner.token, "effects.remove", params).await.0,
        400
    );
    let (status, catalog) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"effects."}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(catalog.as_array().unwrap().len(), 5);
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
