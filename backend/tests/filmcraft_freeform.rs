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
