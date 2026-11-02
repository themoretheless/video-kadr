mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_project_inspect::inspect;
fn document() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    fixture["cases"][0]["document"].clone()
}
#[test]
fn bin_types_source_duration_rules_and_exact_large_ids() {
    let mut document = document();
    let project = &mut document["project"];
    let mut items = vec![];
    for (id, kind, label, expected) in [
        (
            50,
            json!({"Graphic":{"width":64,"height":48,"rate":{"num":24,"den":1}}}),
            "Violet",
            "Graphic",
        ),
        (
            51,
            json!({"AdjustmentLayer":{"duration":123}}),
            "Rose",
            "Adjustment Layer",
        ),
        (
            52,
            json!({"Subclip":{"parent":1,"range":{"start":10,"duration":99}}}),
            "Mango",
            "Subclip",
        ),
    ] {
        project["items"][id.to_string()] =
            json!({"id":id,"name":expected,"label":label,"kind":kind});
        items.push(json!({"Item":id}));
    }
    items.push(json!({"Item":999}));
    project["root"]["children"] =
        json!([{"Bin":{"id":9,"name":"Nested","children":items}},{"Item":6}]);
    let seq = &mut project["items"]["6"]["kind"]["Sequence"];
    seq["video_tracks"] = json!([]);
    seq["audio_tracks"] = json!([]);
    seq["caption_tracks"] = json!([{"captions":[{"start":100,"duration":900}],"enabled":false}]);
    let before = document.clone();
    let result=inspect(json!({"document":document,"context":{"revision":7,"activeSequence":"18446744073709551615","sourceItem":"9007199254740993","dirty":true,"path":"/never-read/private.fcproj"}})).unwrap();
    let nested = &result["root"]["children"][0]["children"];
    assert_eq!(
        nested[0],
        json!({"item":50,"name":"Graphic","type":"Graphic","duration":914457600000000i64,"label":"Violet"})
    );
    assert_eq!(nested[1]["duration"], 123);
    assert_eq!(nested[2]["duration"], 99);
    assert!(nested[3]["name"].is_null());
    assert_eq!(result["root"]["children"][1]["duration"], 1000);
    assert_eq!(result["activeSequence"], "18446744073709551615");
    assert_eq!(result["sourceItem"], "9007199254740993");
    assert_eq!(result["revision"], 7);
    assert_eq!(document, before);
}
#[test]
fn invalid_context_and_overflow_are_rejected() {
    assert!(inspect(json!({"document":document(),"context":{"sourceItem":"-1"}})).is_err());
    assert!(inspect(json!({"document":document(),"unexpected":true})).is_err());
    let mut doc = document();
    doc["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["start"] =
        json!(i64::MAX);
    assert!(inspect(json!({"document":doc})).is_err());
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
async fn authenticated_inspection_runs_in_catalog_and_batch_without_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("inspect-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let params = json!({"document":serde_json::to_string(&document()).unwrap()});
    assert_eq!(
        call(&app, "invalid", "project.inspect", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "project.inspect", params.clone()).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["name"], "Linked trim");
    assert_eq!(result["root"]["children"][0]["type"], "Synthetic");
    let (status, list) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"project.inspect"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], "project.inspect");
    let (status, batch) = call(
        &app,
        &owner.token,
        "command.batch",
        json!({"steps":[{"id":"project.inspect","params":params},{"id":"export.queue.list"}]}),
    )
    .await;
    assert_eq!(status, 200, "{batch}");
    assert_eq!(batch["completed"], 2);
    assert_eq!(batch["results"][0]["result"], result);
    assert_eq!(state.db.load_jobs().await.unwrap().len(), 0);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
