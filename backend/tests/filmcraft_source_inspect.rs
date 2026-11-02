mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_source_inspect::inspect;
const FRAME: i64 = 254_016_000_000 / 25;
fn document() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut doc = fixture["cases"][0]["document"].clone();
    let media = &mut doc["project"]["items"]["1"]["kind"]["Media"];
    media["info"]["kind"] = json!("Movie");
    media["info"]["duration"] = json!(FRAME * 100);
    media["info"]["video"]["frame_rate"] = json!({"num":25,"den":1});
    media["interpret"]["frame_rate"] = json!({"num":50,"den":1});
    media["mark_in"] = json!(FRAME * 7);
    media["mark_out"] = json!(FRAME * 20);
    media["markers"] = json!([{"id":10,"start":FRAME*9,"duration":0,"name":"before"},{"id":11,"start":FRAME*10,"duration":FRAME,"name":"first"},{"id":12,"start":FRAME*29,"duration":0,"name":"last"},{"id":13,"start":FRAME*30,"duration":0,"name":"after"}]);
    doc
}
fn subclip(doc: &mut Value, id: u64, parent: u64, start: i64, duration: i64, restrict: bool) {
    doc["project"]["items"][id.to_string()] = json!({"id":id,"name":"Subclip","label":"Iris","kind":{"Subclip":{"parent":parent,"range":{"start":start,"duration":duration},"restrict_trims":restrict}}});
}
#[test]
fn media_rate_marks_and_unclamped_playhead_match_source_monitor() {
    let result=inspect(json!({"document":document(),"context":{"sourceItem":"1","sourcePlayhead":(FRAME*150).to_string()}})).unwrap();
    assert_eq!(
        result,
        json!({"item":1,"media":1,"start":0,"end":FRAME*100,"fps":50.,"playhead":FRAME*150,"markIn":FRAME*7,"markOut":FRAME*20,"markers":[{"id":10,"start":FRAME*9,"duration":0,"name":"before"},{"id":11,"start":FRAME*10,"duration":FRAME,"name":"first"},{"id":12,"start":FRAME*29,"duration":0,"name":"last"},{"id":13,"start":FRAME*30,"duration":0,"name":"after"}],"subclip":null})
    );
    assert_eq!(
        inspect(json!({"document":{},"context":{}})).unwrap(),
        json!({"item":null})
    );
}
#[test]
fn restricted_and_nested_unrestricted_subclips_filter_markers_and_resolve_root() {
    let mut doc = document();
    subclip(&mut doc, 50, 1, FRAME * 10, FRAME * 20, true);
    subclip(&mut doc, 51, 50, FRAME * 10, FRAME * 20, false);
    let restricted = inspect(json!({"document":doc,"item":"50"})).unwrap();
    let unrestricted = inspect(json!({"document":doc,"item":"51"})).unwrap();
    assert_eq!(restricted["media"], 1);
    assert_eq!(restricted["start"], FRAME * 10);
    assert_eq!(restricted["end"], FRAME * 30);
    assert!(restricted["markIn"].is_null());
    assert!(restricted["markOut"].is_null());
    assert_eq!(
        restricted["markers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [11, 12]
    );
    assert_eq!(unrestricted["start"], 0);
    assert_eq!(unrestricted["end"], FRAME * 100);
    assert_eq!(unrestricted["markIn"], FRAME * 10);
    assert_eq!(unrestricted["markOut"], FRAME * 30 - FRAME / 2);
    assert_eq!(
        unrestricted["subclip"],
        json!({"start":FRAME*10,"end":FRAME*30,"restrictTrims":false})
    );
    subclip(&mut doc, 52, 52, 0, FRAME, true);
    assert!(inspect(json!({"document":doc,"item":"52"})).is_err());
    assert!(inspect(json!({"document":doc,"item":"404"})).is_err());
}
#[test]
fn still_preferences_and_exact_large_identity() {
    let mut doc = document();
    let media = &mut doc["project"]["items"]["1"]["kind"]["Media"];
    media["info"]["kind"] = json!("Still");
    media["info"]["duration"] = json!(0);
    for (duration, unit, expected) in [
        (3., "frames", FRAME * 3 / 2),
        (0., "seconds", FRAME / 2),
        (5., "seconds", 254_016_000_000 * 5),
    ] {
        assert_eq!(inspect(json!({"document":doc,"item":"1","context":{"stillImageDuration":duration,"stillImageUnit":unit}})).unwrap()["end"],expected);
    }
    let mut item = doc["project"]["items"]["1"].take();
    doc["project"]["items"].as_object_mut().unwrap().remove("1");
    item["id"] = json!(u64::MAX);
    doc["project"]["items"][u64::MAX.to_string()] = item;
    let result=inspect(json!({"document":doc,"item":u64::MAX.to_string(),"context":{"sourcePlayhead":"9007199254740993"}})).unwrap();
    assert_eq!(result["item"], u64::MAX.to_string());
    assert_eq!(result["media"], u64::MAX.to_string());
    assert_eq!(result["playhead"], "9007199254740993");
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
async fn authenticated_source_inspection_is_discoverable_and_batchable_without_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("source-inspect-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let params = json!({"document":serde_json::to_string(&document()).unwrap(),"context":{"sourceItem":"1"}});
    assert_eq!(
        call(&app, "invalid", "source.inspect", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "source.inspect", params.clone()).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["fps"], 50.);
    let (status, list) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"source.inspect"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["id"], "source.inspect");
    let (status, batch) = call(
        &app,
        &owner.token,
        "command.batch",
        json!({"steps":[{"id":"source.inspect","params":params},{"id":"export.queue.list"}]}),
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
#[test]
fn sequence_graphic_adjustment_and_still_subclip_views_follow_source_rules() {
    let mut doc = document();
    let seq = &mut doc["project"]["items"]["6"]["kind"]["Sequence"];
    for key in ["video_tracks", "audio_tracks", "caption_tracks"] {
        seq[key] = json!([]);
    }
    seq["settings"]["frame_rate"] = json!({"num":25,"den":1});
    seq["mark_in"] = json!(17);
    seq["mark_out"] = json!(30);
    let result = inspect(json!({"document":doc,"item":"6"})).unwrap();
    assert_eq!(result["end"], 0);
    assert_eq!(result["fps"], 25.);
    assert_eq!(result["markIn"], 17);
    assert_eq!(result["markOut"], 30);
    for (id, kind, expected) in [
        (
            60,
            json!({"Graphic":{"width":64,"height":48,"rate":{"num":25,"den":1}}}),
            3600 * 254_016_000_000i64,
        ),
        (
            61,
            json!({"AdjustmentLayer":{"width":64,"height":48,"rate":{"num":25,"den":1},"duration":123}}),
            123,
        ),
    ] {
        doc["project"]["items"][id.to_string()] =
            json!({"id":id,"name":"generated","label":"Iris","kind":kind});
        let result = inspect(json!({"document":doc,"item":id})).unwrap();
        assert_eq!(result["end"], expected);
        assert_eq!(result["fps"], 25.);
        assert!(result["markIn"].is_null());
        assert_eq!(result["markers"], json!([]));
    }
    doc["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Still");
    subclip(&mut doc, 50, 1, FRAME * 10, FRAME * 20, false);
    let result = inspect(json!({"document":doc,"item":"50"})).unwrap();
    assert_eq!(result["end"], FRAME * 30);
}
