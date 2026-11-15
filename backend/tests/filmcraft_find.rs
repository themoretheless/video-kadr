use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_find::find;

#[test]
fn project_find_uses_source_columns_and_cycles_depth_first() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["root"]["children"] =
        json!([{"Item":6},{"Bin":{"id":99,"name":"Nested","children":[{"Item":2}]}},{"Item":1}]);
    document["project"]["items"]["1"]["metadata"]["Review owner"] = json!("Alex");
    document["project"]["items"]["2"]["metadata"]["Description"] = json!("WAVES");
    let run = |command, params: Value| {
        let mut request = params;
        request["document"] = document.clone();
        find(command, request).unwrap()
    };
    let result = run(
        "edit.find",
        json!({
            "rows":[{"column":"Name","operator":"beginsWith","text":"Seq"},
                    {"column":"All","operator":"contains","text":"waves"},
                    {"column":"review owner","operator":"matches","text":"alex"}],
            "matchAll":false
        }),
    );
    assert_eq!(result["items"], json!(["6", "2", "1"]));
    let next = json!({"text":"Synthetic","column":"Video Info","index":0});
    assert_eq!(run("edit.findNext", next)["item"], "1");
    assert_eq!(
        run(
            "edit.findNext",
            json!({"text":"Synthetic","column":"Video Info","index":1})
        )["item"],
        "2"
    );
    assert_eq!(
        run(
            "edit.find",
            json!({"rows":[{"column":"File Path","op":"ends_with","text":".mkv"},{"column":"All","op":"doesNotContain","text":"waves"}]})
        )["items"],
        json!(["1"])
    );
    assert_eq!(
        run(
            "edit.find",
            json!({"column":"Description","operator":"matches","text":"waves","caseSensitive":true})
        )["matches"],
        0
    );
    assert!(find(
        "edit.find",
        json!({"document":document,"rows":[{"text":""}]})
    )
    .is_err());
}

#[tokio::test]
async fn saved_search_bins_round_trip_and_recompute_after_metadata_changes() {
    use video_kadr_backend::{
        domain::filmcraft_search_bins::run, error::ApiJson, handlers::filmcraft_project::import,
    };
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["items"]["1"]["metadata"]["Scene"] = json!("Opening");
    let new = run("file.newSearchBin", json!({
        "document":document,"name":"Scenes","query":{"rows":[{"column":"Scene","operator":"matches","text":"opening"}]}
    })).unwrap();
    let bin = &new["result"]["bin"];
    assert_eq!(new["result"]["items"], json!(["1"]));
    let mut saved: Value = serde_json::from_str(new["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(
        saved["project"]["search_bins"][0]["query"]["rows"][0]["op"],
        "matches"
    );
    assert_eq!(
        saved["project"]["search_bins"][0]["query"]["match_all"],
        true
    );
    saved["project"]["items"]["2"]["metadata"]["Scene"] = json!("Opening");
    let contents = run(
        "project.searchBinItems",
        json!({"document":saved,"bin":bin}),
    )
    .unwrap();
    assert_eq!(contents["items"], json!(["1", "2"]));
    let edited = run(
        "project.editSearchBin",
        json!({"document":saved,"bin":bin,"name":"Renamed",
        "query":{"rows":[{"column":"Name","operator":"beginsWith","text":"Tone"}]}}),
    )
    .unwrap();
    assert_eq!(edited["result"]["name"], "Renamed");
    assert_eq!(edited["result"]["items"], json!(["1"]));
    let imported = import(ApiJson(
        serde_json::from_value(json!({"document":edited["documentText"]})).unwrap(),
    ))
    .await
    .unwrap()
    .0;
    assert_eq!(imported["searchBins"], json!([{"id":bin,"name":"Renamed"}]));
    let deleted = run(
        "project.deleteSearchBin",
        json!({"document":edited["documentText"],"bin":bin}),
    )
    .unwrap();
    assert!(run(
        "project.searchBinItems",
        json!({"document":deleted["documentText"],"bin":bin})
    )
    .is_err());
}
