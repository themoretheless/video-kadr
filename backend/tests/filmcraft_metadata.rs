use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_metadata::run;

#[test]
fn metadata_edits_resolve_timeline_items_and_preserve_source_properties() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let document = &fixture["cases"][0]["document"];
    let clip =
        &document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["id"];
    let edited = run("metadata.set", json!({
        "document":document, "clip":clip, "context":{"activeSequence":"6"},
        "fields":{"Name":"  New Name  ","Label":"Mango","Description":"Opening","Review owner":"Alex"}
    })).unwrap();
    assert_eq!(edited["metadataEdit"], true);
    assert_eq!(edited["result"]["item"], 1);
    assert_eq!(edited["result"]["name"], "New Name");
    assert_eq!(edited["result"]["label"], "Mango");
    assert_eq!(
        edited["document"]["project"]["items"]["1"]["kind"],
        document["project"]["items"]["1"]["kind"]
    );
    let document = &edited["documentText"];
    let inspected = run(
        "metadata.get",
        json!({"document":document,"context":{"projectSelection":["1"]}}),
    )
    .unwrap();
    assert_eq!(inspected["result"]["metadata"]["Review owner"], "Alex");
    assert_eq!(inspected["result"]["fields"]["videoCodec"], "Synthetic");
    let no_change = run(
        "metadata.set",
        json!({"document":document,"item":"1","field":"Description","value":"Opening"}),
    )
    .unwrap();
    assert_eq!(no_change["metadataEdit"], false);
    let removed = run(
        "metadata.set",
        json!({"document":document,"item":"1","field":"Review owner","value":""}),
    )
    .unwrap();
    assert!(removed["result"]["metadata"].get("Review owner").is_none());
    assert!(run(
        "metadata.set",
        json!({"document":document,"item":"1","fields":{"Description":"Other","Media Type":"fake"}})
    )
    .is_err());
}
