use serde_json::{json, Value};
use video_kadr_backend::{
    domain::filmcraft_freeform::move_cards, error::ApiJson, handlers::filmcraft_project::import,
};

#[tokio::test]
async fn freeform_move_is_atomic_and_round_trips_filmcraft_metadata() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["items"]["1"]["metadata"]["Freeform Position"] = json!("17,22,143");
    document["project"]["items"]["1"]["metadata"]["Poster Frame"] = json!("123");
    let result = move_cards(
        json!({"document":document,"positions":{"1":[48.0,31.0],"2":[-4.0,80.0]},"snap":true}),
    )
    .unwrap();
    assert_eq!(
        result["document"]["project"]["items"]["1"]["metadata"]["Freeform Position"],
        "40,40,143"
    );
    assert_eq!(
        result["document"]["project"]["items"]["1"]["metadata"]["Poster Frame"],
        "123"
    );
    assert_eq!(
        result["document"]["project"]["items"]["2"]["metadata"]["Freeform Position"],
        "0,80"
    );
    assert!(
        move_cards(json!({"document":document,"positions":{"1":[1.0,2.0],"999":[0.0,0.0]}}))
            .is_err()
    );
    let request = serde_json::from_value(json!({"document":result["documentText"]})).unwrap();
    let imported = import(ApiJson(request)).await.unwrap().0;
    let item = imported["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "1")
        .unwrap();
    assert_eq!(
        item["freeformPosition"],
        json!({"x":40.0,"y":40.0,"size":143.0})
    );
}

#[test]
fn unstacking_the_lead_releases_and_spreads_all_members_in_one_snapshot() {
    use video_kadr_backend::domain::filmcraft_freeform::stack_cards;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let document = &fixture["cases"][0]["document"];
    let stacked = stack_cards(
        json!({"document":document,"items":["1","2"],"width":800}),
        true,
    )
    .unwrap();
    let metadata = |id: &str| &stacked["document"]["project"]["items"][id]["metadata"];
    assert_eq!(metadata("1")["Freeform Stack"], "1");
    assert_eq!(metadata("2")["Freeform Stack"], "1");
    assert_eq!(metadata("6")["Freeform Position"], "316,12");
    let released = stack_cards(
        json!({"document":stacked["documentText"],"items":["1"]}),
        false,
    )
    .unwrap();
    let metadata = |id: &str| &released["document"]["project"]["items"][id]["metadata"];
    assert!(metadata("1")["Freeform Stack"].is_null());
    assert!(metadata("2")["Freeform Stack"].is_null());
    assert_eq!(metadata("1")["Freeform Position"], "12,12");
    assert_eq!(metadata("2")["Freeform Position"], "164,12");
    assert!(stack_cards(json!({"document":document,"items":["1","999"]}), true).is_err());
}

#[test]
fn named_arrangements_preserve_size_and_restore_only_the_requested_bin() {
    use video_kadr_backend::domain::filmcraft_freeform::arrangement;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["root"]["children"] =
        json!([{"Item":1},{"Bin":{"id":99,"name":"Child","children":[{"Item":2}]}},{"Item":6}]);
    document["project"]["items"]["1"]["metadata"]["Freeform Position"] = json!("15,24,140");
    let saved = arrangement(
        "project.freeform.saveArrangement",
        json!({"document":document,"name":"  Rough cut  ","bin":"0"}),
    )
    .unwrap();
    let mut document = saved["document"].clone();
    assert_eq!(
        document["project"]["items"]["1"]["metadata"]["Freeform Arrangement: Rough cut"],
        "15,24,140"
    );
    assert!(
        document["project"]["items"]["2"]["metadata"]["Freeform Arrangement: Rough cut"].is_null()
    );
    document["project"]["items"]["1"]["metadata"]["Freeform Position"] = json!("90,100");
    document["project"]["items"]["2"]["metadata"]["Freeform Position"] = json!("300,200");
    let names = arrangement(
        "project.freeform.arrangements",
        json!({"document":document,"bin":"0"}),
    )
    .unwrap();
    assert_eq!(names["arrangements"], json!(["Rough cut"]));
    let restored = arrangement(
        "project.freeform.restoreArrangement",
        json!({"document":document,"name":"Rough cut","bin":"0"}),
    )
    .unwrap();
    assert_eq!(
        restored["document"]["project"]["items"]["1"]["metadata"]["Freeform Position"],
        "15,24,140"
    );
    assert_eq!(
        restored["document"]["project"]["items"]["2"]["metadata"]["Freeform Position"],
        "300,200"
    );
    let deleted = arrangement(
        "project.freeform.deleteArrangement",
        json!({"document":restored["documentText"],"name":"Rough cut","bin":"0"}),
    )
    .unwrap();
    assert!(deleted["document"]["project"]["items"]["1"]["metadata"]
        ["Freeform Arrangement: Rough cut"]
        .is_null());
    assert_eq!(
        deleted["document"]["project"]["items"]["1"]["metadata"]["Freeform Position"],
        "15,24,140"
    );
    assert!(arrangement(
        "project.freeform.restoreArrangement",
        json!({"document":deleted["documentText"],"name":"Rough cut","bin":"0"})
    )
    .is_err());
}

#[test]
fn card_size_align_and_reset_share_the_bin_layout() {
    use video_kadr_backend::domain::filmcraft_freeform::edit_layout;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let metadata = &mut document["project"]["items"]["1"]["metadata"];
    metadata["Freeform Position"] = json!("17,32,140");
    metadata["Freeform Stack"] = json!("1");
    metadata["Freeform Arrangement: Saved"] = json!("1,2,128");
    let resized = edit_layout(
        "project.freeform.resize",
        json!({"document":document,"items":["1"],"step":1}),
    )
    .unwrap();
    let key = "/project/items/1/metadata";
    assert_eq!(
        resized["document"].pointer(key).unwrap()["Freeform Position"],
        "17,32,175"
    );
    let aligned = edit_layout(
        "project.freeform.alignToGrid",
        json!({"document":resized["documentText"],"grid":20}),
    )
    .unwrap();
    assert_eq!(
        aligned["document"].pointer(key).unwrap()["Freeform Position"],
        "20,40,175"
    );
    let reset = edit_layout(
        "project.freeform.reset",
        json!({"document":aligned["documentText"]}),
    )
    .unwrap();
    let metadata = reset["document"].pointer(key).unwrap();
    assert!(metadata["Freeform Position"].is_null());
    assert!(metadata["Freeform Stack"].is_null());
    assert_eq!(metadata["Freeform Arrangement: Saved"], "1,2,128");
}
