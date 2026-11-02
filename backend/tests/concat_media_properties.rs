use serde_json::{json, Value};
use std::collections::BTreeMap;
use video_kadr_backend::domain::{
    concat_project_export::export_with_paths,
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
};
fn source_document() -> Value {
    json!({"version":1,"media":[{"id":"media","name":"Camera","kind":"video","path":"/inert/creator.mkv","duration":1.,"width":16,"height":16,"hasAudio":true,"frameRate":29.97002997002997,"frameRateFraction":"30000/1001","videoCodec":"ffv1","audioCodec":"pcm_s16le","audioTracks":[{"index":1,"codec":"pcm_s16le","channels":2,"sampleRate":44100,"title":"Desktop","language":"eng"},{"index":3,"codec":"pcm_s16le","channels":1,"sampleRate":48000,"title":"Mic"}],"origin":"speech","colorSpace":"sdr"}],"timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"clip","trackId":"v","mediaId":"media","kind":"video","start":0.,"duration":1.}]}]})
}
fn converted(document: Value) -> NestedCompositionProject {
    let media_id = document["media"][0]["id"].as_str().unwrap().to_owned();
    let input:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":{(media_id):{"id":"registered","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":true}}})).unwrap();
    let report = import(input).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
#[tokio::test]
async fn imported_properties_survive_native_storage_and_concat_export() {
    let original = source_document();
    let project = converted(original.clone());
    let id = video_kadr_backend::domain::composition::SourceId::parse("registered").unwrap();
    let properties = &project.project_media[&id].concat_metadata;
    for key in [
        "frameRate",
        "frameRateFraction",
        "videoCodec",
        "audioCodec",
        "audioTracks",
        "origin",
    ] {
        assert_eq!(properties[key], original["media"][0][key], "{key}");
    }
    let document = serde_json::to_value(&project).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let db = video_kadr_backend::db::Db::open(directory.path())
        .await
        .unwrap();
    let saved = db
        .create_composition_project("Probed media", &document, &["registered".into()])
        .await
        .unwrap();
    let loaded = db
        .get_composition_project(&saved.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.document, document);
    assert_eq!(loaded.source_ids, vec!["registered"]);
    let restored: NestedCompositionProject = serde_json::from_value(loaded.document).unwrap();
    let out = export_with_paths(
        &restored,
        &BTreeMap::from([("registered".into(), "/portable/camera.mkv".into())]),
    )
    .unwrap();
    for key in [
        "frameRate",
        "frameRateFraction",
        "videoCodec",
        "audioCodec",
        "audioTracks",
        "origin",
    ] {
        assert_eq!(out["media"][0][key], original["media"][0][key], "{key}");
    }
    assert_eq!(out["media"][0]["path"], "/portable/camera.mkv");
    assert_eq!(converted(out).project_media, project.project_media);
}
#[test]
fn metadata_cannot_override_identity_paths_or_unqualified_color_mapping() {
    let original = converted(source_document());
    let value = serde_json::to_value(&original).unwrap();
    for metadata in [
        json!({"path":"/must-not-open"}),
        json!({"id":"other"}),
        json!({"colorSpace":"hlg"}),
        json!({"colorRange":"full"}),
        json!({"videoCodec":"x".repeat(65537)}),
    ] {
        let mut bad = value.clone();
        bad["projectMedia"]["registered"]["concatMetadata"] = metadata;
        let candidate: NestedCompositionProject = serde_json::from_value(bad).unwrap();
        assert!(candidate.dependency_order().is_err());
    }
    assert_eq!(serde_json::to_value(&original).unwrap(), value);
}

#[test]
fn unused_unmarked_media_keeps_probed_properties_after_last_placement_is_removed() {
    use video_kadr_backend::domain::{composition::SourceId, project_media::set_placeholder};
    let original = converted(source_document());
    let id = SourceId::parse("registered").unwrap();
    let (marked, applied) = set_placeholder(&original, &id, true).unwrap();
    assert!(applied);
    let (mut unused, applied) = set_placeholder(&marked, &id, false).unwrap();
    assert!(applied);
    assert_eq!(
        unused.project_media[&id].concat_metadata,
        original.project_media[&id].concat_metadata
    );
    for node in unused.compositions.values_mut() {
        node.content.tracks.clear();
        node.content.sources.clear();
        node.content.metadata = None;
    }
    unused.dependency_order().unwrap();
    let encoded = serde_json::to_value(&unused).unwrap();
    let restored: NestedCompositionProject = serde_json::from_value(encoded).unwrap();
    let out = export_with_paths(
        &restored,
        &BTreeMap::from([("registered".into(), "/portable/unused.mkv".into())]),
    )
    .unwrap();
    assert_eq!(
        out["media"][0]["audioTracks"],
        source_document()["media"][0]["audioTracks"]
    );
    assert_eq!(out["media"][0]["frameRateFraction"], "30000/1001");
    assert_eq!(out["media"][0]["placeholder"], false);
    assert_eq!(converted(out).project_media, unused.project_media);
}
