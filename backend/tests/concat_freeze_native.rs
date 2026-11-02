use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::{Composition, CompositionSource},
    concat_freeze_native, concat_project_import,
};
fn setup() -> (Composition, CompositionSource) {
    let source = json!({"id":"source","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false});
    let doc = json!({"version":1,"activeTimelineId":"t","media":[{"id":"source","path":"/unopened/source.mov","kind":"video","duration":10.,"width":16,"height":16,"hasAudio":false}],"timelines":[{"id":"t","name":"Freeze","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"v","name":"Video"}],"clips":[{"id":"clip","trackId":"v","mediaId":"source","kind":"video","start":0.,"duration":4.,"sourceStart":0.,"speed":1.,"flipH":true,"crop":{"left":0.1},"videoEffects":[{"id":"concat.exposure","params":{"stops":0.5}}]},{"id":"later","trackId":"v","mediaId":"source","kind":"video","start":5.,"duration":1.,"sourceStart":0.,"speed":1.}]}]});
    let report = concat_project_import::import(
        serde_json::from_value(
            json!({"schemaVersion":1,"document":doc,"mediaBindings":{"source":source}}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let c = project.compositions[&project.root].content.clone();
    (c,serde_json::from_value(json!({"id":"still","kind":"image","durationTicks":0,"width":1280,"height":720,"hasAudio":false})).unwrap())
}
#[test]
fn video_lane_becomes_head_still_tail_and_unrelated_native_state_is_preserved() {
    let (c, still) = setup();
    let mut value = serde_json::to_value(c).unwrap();
    value["canvas"]["background"] = json!({"red":0.2,"green":0.3,"blue":0.4,"alpha":1.});
    let mut unrelated = value["tracks"][0].clone();
    unrelated["id"] = json!("other");
    unrelated["clips"][0]["id"] = json!("other-clip");
    unrelated["clips"].as_array_mut().unwrap().truncate(1);
    value["tracks"]
        .as_array_mut()
        .unwrap()
        .insert(0, unrelated.clone());
    let c: Composition = serde_json::from_value(value.clone()).unwrap();
    let changed =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    let out = serde_json::to_value(changed).unwrap();
    assert_eq!(serde_json::to_value(&c).unwrap(), value);
    assert_eq!(out["canvas"], value["canvas"]);
    assert_eq!(out["tracks"][0], unrelated);
    let video = &out["tracks"][2];
    assert_eq!(video["id"], value["tracks"][1]["id"]);
    let clips = video["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 3);
    let by_id = |id: &str| -> &Value { clips.iter().find(|clip| clip["id"] == id).unwrap() };
    assert_eq!(
        by_id("clip")["placement"]["timelineDurationTick"],
        Value::Null
    );
    assert_eq!(by_id("clip")["placement"]["sourceOutTick"], 1250000);
    assert_eq!(by_id("tail")["placement"]["timelineStartTick"], 2250000);
    assert_eq!(by_id("later")["placement"]["timelineStartTick"], 6000000);
    let image = &out["tracks"][1]["clips"][0];
    assert_eq!(image["id"], "hold");
    assert_eq!(image["sourceId"], "still");
    assert_eq!(image["timelineStartTick"], 1250000);
    assert_eq!(image["durationTicks"], 1000000);
    assert_eq!(
        image["transform"],
        value["tracks"][1]["clips"][0]["transform"]
    );
    assert_eq!(image["effects"], value["tracks"][1]["clips"][0]["effects"]);
}

#[test]
fn repeated_freeze_rejoins_typed_siblings_and_ripples_the_existing_still() {
    let (c, still) = setup();
    let first =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    let mut second_still = still;
    second_still.id = video_kadr_backend::domain::composition::SourceId::parse("still2").unwrap();
    let second = concat_freeze_native::splice(
        &first,
        "clip",
        0.5,
        &second_still,
        "tail2",
        "hold2",
        "unused-image-lane",
    )
    .unwrap()
    .unwrap();
    let wire = serde_json::to_value(&second).unwrap();
    assert_eq!(
        wire["metadata"]["clipNames"]["hold"],
        wire["metadata"]["clipNames"]["clip"]
    );
    assert_eq!(
        wire["metadata"]["clipNames"]["tail2"],
        wire["metadata"]["clipNames"]["clip"]
    );
    assert_eq!(wire["tracks"].as_array().unwrap().len(), 2);
    let images = wire["tracks"][0]["clips"].as_array().unwrap();
    assert_eq!(wire["tracks"][0]["id"], "image-lane");
    assert_eq!(
        images.iter().find(|c| c["id"] == "hold").unwrap()["timelineStartTick"],
        2250000
    );
    assert_eq!(
        images.iter().find(|c| c["id"] == "hold2").unwrap()["timelineStartTick"],
        500000
    );
    let mut moved = wire.clone();
    moved["tracks"].as_array_mut().unwrap().swap(0, 1);
    let mut third = second_still;
    third.id = video_kadr_backend::domain::composition::SourceId::parse("still3").unwrap();
    assert!(concat_freeze_native::splice(
        &serde_json::from_value(moved).unwrap(),
        "tail",
        3.5,
        &third,
        "tail3",
        "hold3",
        "unused-image"
    )
    .is_err());
}

fn wrapped(
    content: Composition,
) -> video_kadr_backend::domain::nested_composition::NestedCompositionProject {
    serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}})).unwrap()
}
fn paths() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::from([
        ("source".into(), "/unopened/source.mov".into()),
        ("still".into(), "/unopened/still.jpg".into()),
    ])
}
fn reimport(document: Value, c: &Composition) -> Composition {
    let report = concat_project_import::import(
        serde_json::from_value(
            json!({"schemaVersion":1,"document":document,"mediaBindings":c.sources}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    project.compositions[&project.root].content.clone()
}
#[test]
fn source_export_rejoins_the_pair_and_import_restores_membership_for_another_freeze() {
    let (c, still) = setup();
    let first =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    let before = serde_json::to_value(&first).unwrap();
    let document = video_kadr_backend::domain::concat_project_export::export_with_paths(
        &wrapped(first.clone()),
        &paths(),
    )
    .unwrap();
    assert_eq!(
        document["timelines"][0]["tracks"].as_array().unwrap().len(),
        1
    );
    let owner = document["timelines"][0]["tracks"][0]["id"]
        .as_str()
        .unwrap();
    assert!(document["timelines"][0]["clips"]
        .as_array()
        .unwrap()
        .iter()
        .all(|clip| clip["trackId"] == owner));
    if let Ok(path) = std::env::var("CONCAT_GROUPED_EXPORTED_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&document).unwrap()).unwrap();
    }
    let restored = reimport(document, &first);
    assert_eq!(
        restored.metadata.as_ref().unwrap().concat_lane_groups.len(),
        1
    );
    let compare = |a: &Composition, b: &Composition| {
        let am = a.metadata.as_ref().unwrap();
        let bm = b.metadata.as_ref().unwrap();
        assert_eq!(am.clip_names, bm.clip_names);
        assert_eq!(am.source_names, bm.source_names);
        assert_eq!(am.sequence_name, bm.sequence_name);
        for track in &a.tracks {
            let kind = serde_json::to_value(track).unwrap()["kind"].clone();
            let other = b
                .tracks
                .iter()
                .find(|other| serde_json::to_value(other).unwrap()["kind"] == kind)
                .unwrap();
            assert_eq!(
                serde_json::to_value(track).unwrap()["clips"],
                serde_json::to_value(other).unwrap()["clips"]
            );
        }
    };
    compare(&first, &restored);
    if let Ok(path) = std::env::var("CONCAT_GROUPED_REWRITTEN_INPUT") {
        let doc = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        compare(&first, &reimport(doc, &first));
    }
    let mut next = still;
    next.id = video_kadr_backend::domain::composition::SourceId::parse("second-still").unwrap();
    let changed = concat_freeze_native::splice(
        &restored,
        "clip",
        0.5,
        &next,
        "tail2",
        "hold2",
        "unused-image",
    )
    .unwrap()
    .unwrap();
    let value = serde_json::to_value(changed).unwrap();
    let old_hold = value["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|track| track["clips"].as_array().unwrap())
        .find(|clip| clip["id"] == "hold")
        .unwrap();
    assert_eq!(old_hold["timelineStartTick"], 2250000);
    assert_eq!(serde_json::to_value(first).unwrap(), before);
}
#[test]
fn grouped_export_refuses_visibility_order_or_overlap_loss_and_keeps_input() {
    let (c, still) = setup();
    let first =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    for mode in ["visibility", "order", "overlap"] {
        let mut value = serde_json::to_value(&first).unwrap();
        match mode {
            "visibility" => value["tracks"][0]["hidden"] = json!(true),
            "order" => value["tracks"].as_array_mut().unwrap().swap(0, 1),
            _ => value["tracks"][0]["clips"][0]["timelineStartTick"] = json!(0),
        }
        let changed: Composition = serde_json::from_value(value.clone()).unwrap();
        if mode != "overlap" {
            assert!(changed.validate_native_surface().is_err());
        }
        assert!(
            video_kadr_backend::domain::concat_project_export::export_with_paths(
                &wrapped(changed.clone()),
                &paths()
            )
            .is_err(),
            "{mode}"
        );
        assert_eq!(serde_json::to_value(changed).unwrap(), value);
    }
}
#[test]
fn empty_typed_sibling_exports_as_one_untyped_source_lane() {
    let (c, still) = setup();
    let first =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    for empty in [0, 1] {
        let mut value = serde_json::to_value(&first).unwrap();
        value["tracks"][empty]["clips"] = json!([]);
        let changed: Composition = serde_json::from_value(value).unwrap();
        let document = video_kadr_backend::domain::concat_project_export::export_with_paths(
            &wrapped(changed),
            &paths(),
        )
        .unwrap();
        assert_eq!(
            document["timelines"][0]["tracks"].as_array().unwrap().len(),
            1
        );
    }
}

#[test]
fn track_budget_precedes_group_scans_for_oversized_documents() {
    use video_kadr_backend::domain::composition::CompositionError;
    let (c, still) = setup();
    let mut c =
        concat_freeze_native::splice(&c, "clip", 1.25, &still, "tail", "hold", "image-lane")
            .unwrap()
            .unwrap();
    c.tracks.swap(0, 1);
    while c.tracks.len() <= 16 {
        c.tracks.push(c.tracks[0].clone());
    }
    assert_eq!(
        c.validate_native_surface(),
        Err(CompositionError::TooManyTracks)
    );
}
