mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_sequence_inspect::inspect;
const FRAME: i64 = 254_016_000_000 / 25;
fn document() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut doc = fixture["cases"][0]["document"].clone();
    let seq = &mut doc["project"]["items"]["6"]["kind"]["Sequence"];
    seq["settings"]["frame_rate"] = json!({"num":25,"den":1});
    seq["audio_tracks"] = json!([]);
    seq["caption_tracks"] = json!([]);
    let mut clip = seq["video_tracks"][0]["items"][0].clone();
    clip["start"] = json!(FRAME * 200);
    clip["duration"] = json!(FRAME * 10);
    clip["source_in"] = json!(FRAME * 17);
    clip["speed"] = json!(2.);
    clip["reverse"] = json!(true);
    clip["gain_db"] = json!(-6.5);
    clip["frame_hold"] = json!(FRAME * 18);
    clip["effects"][0]["params"] = json!({"opacity":{"value":{"Float":100.},"keyframes":[{"time":0,"value":{"Float":50.}}]},"auto":{"value":{"Vec2":{"x":null,"y":2.}}},"color":{"value":{"Color":[0.,0.25,0.5,1.]}},"curve":{"value":{"Curve":[[0.,0.],[1.,1.]]}},"text":{"value":{"Text":"a\nb"}},"on":{"value":{"Bool":true}},"mode":{"value":{"Choice":2}},"path":{"value":{"Path":{"vertices":[{"p":{"x":1.,"y":2.}}],"closed":true}}}});
    seq["video_tracks"][0]["items"] = json!([clip]);
    seq["video_tracks"][0]["transitions"] = json!([{"id":44,"effect":{"effect":"dissolve","params":{"curve":{"value":{"Curve":[[0.,0.],[1.,1.]]}}}},"start":FRAME*200,"duration":FRAME,"from":null,"to":null,"reverse":true}]);
    seq["markers"] = json!([{"id":42,"start":FRAME*12,"duration":0,"kind":"Comment","name":"Beat","comment":"cut on the downbeat","color":"Rose"}]);
    doc
}
#[test]
fn source_inspection_reports_clip_ends_gain_reverse_markers_and_all_parameter_kinds() {
    let doc = document();
    let before = doc.clone();
    let clip = doc["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]
        ["id"]
        .clone();
    let result=inspect(json!({"document":doc,"context":{"activeSequence":"6","playheads":{"6":(FRAME*12).to_string()},"selection":[clip]}})).unwrap();
    let item = &result["video"][0]["items"][0];
    assert_eq!(item["startFrame"], 200);
    assert_eq!(item["durationFrames"], 10);
    assert_eq!(item["endFrame"], 210);
    assert_eq!(item["end"], FRAME * 210);
    assert_eq!(item["sourceOut"], FRAME * 37);
    assert_eq!(item["gainDb"], -6.5);
    assert_eq!(item["reverse"], true);
    assert_eq!(result["durationFrames"], 210);
    assert_eq!(result["playhead"], FRAME * 12);
    assert_eq!(result["selection"], json!([clip]));
    assert_eq!(result["markers"][0]["comment"], "cut on the downbeat");
    let params = &item["effects"][0]["params"];
    assert_eq!(
        params["opacity"],
        json!({"value":"Float(100.0)","keyframes":1})
    );
    assert_eq!(params["auto"]["value"], "Vec2(Vec2 { x: NaN, y: 2.0 })");
    assert_eq!(params["color"]["value"], "Color([0.0, 0.25, 0.5, 1.0])");
    assert_eq!(params["curve"]["value"], "Curve([[0.0, 0.0], [1.0, 1.0]])");
    assert_eq!(params["text"]["value"], "Text(\"a\\nb\")");
    assert_eq!(params["path"]["value"],"Path(MaskPath { vertices: [MaskVertex { p: Vec2 { x: 1.0, y: 2.0 }, t_in: Vec2 { x: 0.0, y: 0.0 }, t_out: Vec2 { x: 0.0, y: 0.0 } }], closed: true })");
    assert_eq!(
        result["video"][0]["transitions"][0]["params"]["curve"],
        json!({"Curve":[[0.,0.],[1.,1.]]})
    );
    assert_eq!(doc, before);
}
#[test]
fn item_override_exact_ids_and_missing_sequences() {
    let mut doc = document();
    let id = u64::MAX;
    let mut seq = doc["project"]["items"]["6"].take();
    seq["id"] = json!(id);
    doc["project"]["items"].as_object_mut().unwrap().remove("6");
    doc["project"]["items"][id.to_string()] = seq;
    let result=inspect(json!({"document":doc,"item":id.to_string(),"context":{"activeSequence":"1","selection":[id.to_string()]}})).unwrap();
    assert_eq!(result["id"], id.to_string());
    assert_eq!(result["selection"][0], id.to_string());
    assert!(inspect(json!({"document":document()})).is_err());
    assert!(inspect(json!({"document":document(),"item":"1"})).is_err());
    assert!(inspect(json!({"document":document(),"item":"-1"})).is_err());
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
async fn authenticated_sequence_query_is_discoverable_and_batchable_without_export_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("sequence-inspect-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let params = json!({"document":serde_json::to_string(&document()).unwrap(),"item":"6"});
    assert_eq!(
        call(&app, "invalid", "sequence.inspect", params.clone())
            .await
            .0,
        401
    );
    let (status, result) = call(&app, &owner.token, "sequence.inspect", params.clone()).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["durationFrames"], 210);
    let (status, list) = call(
        &app,
        &owner.token,
        "command.list",
        json!({"filter":"sequence.inspect"}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["id"], "sequence.inspect");
    let(status,batch)=call(&app,&owner.token,"command.batch",json!({"steps":[{"id":"sequence.inspect","params":params},{"id":"project.inspect","params":{"document":document()}}]})).await;
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
fn negative_frame_floor_reverse_speed_and_typed_transition_values() {
    let mut doc = document();
    let seq = &mut doc["project"]["items"]["6"]["kind"]["Sequence"];
    let clip = &mut seq["video_tracks"][0]["items"][0];
    clip["start"] = json!(-1);
    clip["duration"] = json!(FRAME);
    clip["speed"] = json!(-2.);
    seq["video_tracks"][0]["transitions"][0]["effect"]["params"]["color"] =
        json!({"value":{"Color":[0.123456789,0.,0.,1.]}});
    let result = inspect(json!({"document":doc,"item":"6"})).unwrap();
    let clip = &result["video"][0]["items"][0];
    assert_eq!(clip["startFrame"], -1);
    assert_eq!(clip["endFrame"], 0);
    assert_eq!(clip["sourceOut"], FRAME * 19);
    assert_eq!(
        result["video"][0]["transitions"][0]["params"]["color"],
        json!({"Color":[0.12345679104328156,0.,0.,1.]})
    );
}
