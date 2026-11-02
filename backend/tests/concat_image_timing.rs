use serde_json::{json, Value};
use std::collections::BTreeMap;
use video_kadr_backend::domain::{
    composition::Composition,
    concat_project_export::export_with_paths,
    concat_project_import::{import, ImportRequest},
    flat_surface::render_generated_images,
    group_surface::GroupSurface,
};
fn document(source_start: f64, speed: f64) -> Value {
    json!({"version":1,"media":[{"id":"m","kind":"image","name":"Still","path":"/never/open"}],"timelines":[{"id":"timeline","video":{"width":8,"height":8,"rateNum":24,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"still","trackId":"v","mediaId":"m","kind":"image","start":1.,"duration":2.,"sourceStart":source_start,"speed":speed,"videoEffects":[{"id":"concat.exposure","keys":{"stops":[{"at":0.,"value":0.},{"at":1.,"value":2.}]}}]}]}]})
}
fn converted(
    document: Value,
) -> video_kadr_backend::domain::nested_composition::NestedCompositionProject {
    let media_id = document["media"][0]["id"].as_str().unwrap().to_owned();
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":{(media_id):{"id":"image","kind":"image","durationTicks":0,"width":8,"height":8,"hasAudio":false}}})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
fn frame(composition: &Composition, time: u64) -> Vec<u8> {
    let mut decode = |_: &_| {
        Ok(GroupSurface {
            width: 8,
            height: 8,
            rgba: [32, 64, 96, 255].repeat(64),
        })
    };
    render_generated_images(composition, time, &mut || Ok(()), Some(&mut decode))
        .unwrap()
        .unwrap()
        .rgba
}
#[test]
fn source_in_and_speed_roundtrip_exactly_without_retiming_still_effects_or_placement() {
    let baseline = converted(document(0., 1.));
    let reference = &baseline.compositions[&baseline.root].content;
    for source_start in [0., 9.123456789] {
        for speed in [0.0625, 0.5, 1., 2., 16.] {
            let project = converted(document(source_start, speed));
            let content = &project.compositions[&project.root].content;
            for time in [1_000_000, 1_500_000, 2_000_000, 2_999_999] {
                assert_eq!(
                    frame(content, time),
                    frame(reference, time),
                    "In {source_start} speed {speed} at {time}"
                );
            }
            assert_ne!(
                frame(content, 1_000_000),
                frame(content, 2_999_999),
                "effect still animates on the timeline"
            );
            let wire = export_with_paths(
                &project,
                &BTreeMap::from([("image".to_owned(), "/never/open".to_owned())]),
            )
            .unwrap();
            let clip = &wire["timelines"][0]["clips"][0];
            assert_eq!(clip["start"], 1.);
            assert_eq!(clip["duration"], 2.);
            assert_eq!(
                clip.get("sourceStart")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.),
                source_start
            );
            assert_eq!(
                clip.get("speed").and_then(Value::as_f64).unwrap_or(1.),
                speed
            );
            let imported = converted(wire);
            assert_eq!(
                imported.compositions[&imported.root].content.tracks,
                content.tracks
            );
        }
    }
}
#[test]
fn invalid_retained_timing_is_rejected_before_render() {
    let original = converted(document(3., 2.));
    let wire = serde_json::to_value(&original).unwrap();
    for timing in [
        json!({"sourceStart":-1.,"speed":2.}),
        json!({"sourceStart":0.,"speed":0.}),
        json!({"sourceStart":0.,"speed":17.}),
    ] {
        let mut bad = wire.clone();
        bad["compositions"][&original.root]["content"]["tracks"][0]["clips"][0]
            ["concatSourceTiming"] = timing;
        let project: video_kadr_backend::domain::nested_composition::NestedCompositionProject =
            serde_json::from_value(bad).unwrap();
        assert!(project.dependency_order().is_err());
    }
}

#[test]
fn image_curve_survives_fill_import_and_export_without_changing_still_effect_time() {
    let points = json!([{"at":0.,"speed":1.},{"at":0.5,"speed":4.},{"at":0.5,"speed":2.},{"at":1.,"speed":1.}]);
    let mut source = document(7.123456789, 2.);
    source["media"][0]["kind"] = json!("video");
    source["media"][0]["placeholder"] = json!(true);
    source["timelines"][0]["clips"][0]["kind"] = json!("video");
    source["timelines"][0]["clips"][0]["speedCurve"] = points.clone();
    let filled=video_kadr_backend::domain::concat_media_slots::fill(&source,"m",&json!({"path":"library:image","name":"Replacement","kind":"image","width":8,"height":8,"hasAudio":false})).unwrap();
    let project = converted(filled);
    let content = &project.compositions[&project.root].content;
    let baseline = converted(document(0., 1.));
    let reference = &baseline.compositions[&baseline.root].content;
    for time in [1_000_000, 1_500_000, 2_000_000, 2_999_999] {
        assert_eq!(frame(content, time), frame(reference, time));
    }
    let wire = export_with_paths(
        &project,
        &BTreeMap::from([("image".to_owned(), "/never/open".to_owned())]),
    )
    .unwrap();
    let clip = &wire["timelines"][0]["clips"][0];
    assert_eq!(clip["speedCurve"], points);
    assert_eq!(clip["sourceStart"], 0.);
    assert_eq!(clip["speed"], 2.);
    assert_eq!(clip["duration"], 2.);
    let restored = converted(wire);
    assert_eq!(
        restored.compositions[&restored.root].content.tracks,
        content.tracks
    );
    let native = serde_json::to_value(&project).unwrap();
    for curve in [
        json!([]),
        json!([{"at":-1.,"speed":2.}]),
        json!([{"at":0.,"speed":0.}]),
    ] {
        let mut bad = native.clone();
        bad["compositions"][&project.root]["content"]["tracks"][0]["clips"][0]
            ["concatSourceTiming"]["speedCurve"] = curve;
        let candidate: video_kadr_backend::domain::nested_composition::NestedCompositionProject =
            serde_json::from_value(bad).unwrap();
        assert!(candidate.dependency_order().is_err());
    }
}
