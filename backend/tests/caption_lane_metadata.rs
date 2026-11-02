use serde_json::json;
use video_kadr_backend::domain::composition::Composition;

#[test]
fn caption_lane_roles_survive_native_document_roundtrip() {
    let mut value = serde_json::to_value(Composition::new(Default::default())).unwrap();
    value["metadata"] = json!({"captionTextTracks": ["subtitle-track-1", "subtitle-track-2"]});
    let document: Composition = serde_json::from_value(value).unwrap();
    let saved = serde_json::to_value(&document).unwrap();
    assert_eq!(saved["metadata"]["captionTextTracks"], json!(["subtitle-track-1", "subtitle-track-2"]));
    let reopened: Composition = serde_json::from_value(saved).unwrap();
    assert_eq!(document, reopened);
}

#[test]
fn older_metadata_does_not_gain_empty_caption_roles() {
    let mut value = serde_json::to_value(Composition::new(Default::default())).unwrap();
    value["metadata"] = json!({"sequenceName": "Legacy"});
    let document: Composition = serde_json::from_value(value).unwrap();
    let saved = serde_json::to_value(document).unwrap();
    assert_eq!(saved["metadata"]["sequenceName"], "Legacy");
    assert!(saved["metadata"].get("captionTextTracks").is_none());
}
