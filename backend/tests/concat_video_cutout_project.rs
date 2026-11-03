use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use video_kadr_backend::domain::{
    composition::{CompositionSource, CompositionTrack, SourceId},
    concat_cutout_import::plan,
    concat_project_import::{
        import, import_with_ready_bindings, ImportRequest, ReadyVideoBinding, StillBindings,
        VideoBindings,
    },
    cutout::SELFIE_MODEL_HASH,
};
fn source(id: &str, w: u32, h: u32) -> CompositionSource {
    serde_json::from_value(json!({"id":id,"kind":"video","durationTicks":500000,"width":w,"height":h,"hasAudio":false})).unwrap()
}
fn cutout() -> Value {
    json!({"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"smartBrush","size":0.1,"points":[[0.5,0.5]],"at":0.1}]})
}
fn document() -> Value {
    let timeline = |id: &str, media: &str| json!({"id":id,"video":{"width":32,"height":16,"rateNum":10,"rateDen":1},"tracks":[{"id":"video"}],"clips":[{"id":"same","kind":"video","mediaId":media,"trackId":"video","duration":0.2,"cutout":cutout()}]});
    json!({"version":1,"media":[{"id":"slot","kind":"video"},{"id":"alias","kind":"video"},{"id":"region","kind":"video"}],"timelines":[timeline("a","slot"),timeline("b","alias")]})
}
fn request(doc: Value, preserve: bool) -> ImportRequest {
    serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":preserve,"document":doc,"mediaBindings":{"slot":source("raw",32,16),"alias":source("raw",32,16),"region":source("decoy",32,16)}})).unwrap()
}
fn videos() -> VideoBindings {
    let settings = plan(&cutout()).unwrap();
    let prompt = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&vec![[0.5f64, 0.5]]).unwrap())
    );
    let snapshot = json!({"sourceId":"raw","maskSourceId":"region","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"encoderHash":"d".repeat(64),"decoderHash":"e".repeat(64),"runtimeHash":"f".repeat(64),"promptHash":prompt,"points":[[0.5,0.5]],"width":32,"height":16,"frames":3,"frameRate":{"numerator":10,"denominator":1},"frameTimesUs":[0,100000,400000,500000],"seedFrame":1,"trackedFrames":3,"forwardLostAt":null,"backwardLostAt":null});
    let binding:video_kadr_backend::domain::concat_video_binding::Binding=serde_json::from_value(json!({"sourceHash":"a".repeat(64),"modelHash":SELFIE_MODEL_HASH,"runtimeHash":"9".repeat(64),"subject":"person","correction":settings.correction,"regionTracks":[{"stroke":0,"snapshot":snapshot}]})).unwrap();
    ["a", "b"]
        .into_iter()
        .map(|timeline| {
            (
                timeline.into(),
                BTreeMap::from([(
                    "same".into(),
                    ReadyVideoBinding {
                        source: source("raw", 32, 16),
                        source_settings: settings.clone(),
                        binding: binding.clone(),
                        regions: vec![source("region", 256, 256)],
                    },
                )]),
            )
        })
        .collect()
}
#[test]
fn logical_video_cutouts_keep_scoped_slots_and_rebind_vfr_regions_without_overwriting_media() {
    let doc = document();
    let report =
        import_with_ready_bindings(request(doc.clone(), true), &StillBindings::new(), &videos())
            .unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, doc);
    let project = report.project.unwrap();
    project.dependency_order().unwrap();
    let mut masks = vec![];
    for (index, slot) in ["slot", "alias"].into_iter().enumerate() {
        let content = &project.compositions[&format!("concat-timeline-{index}")].content;
        let CompositionTrack::Video { clips, .. } = &content.tracks[0] else {
            panic!()
        };
        assert_eq!(clips[0].source_id.as_str(), slot);
        let binding = clips[0].concat_cutout.as_ref().unwrap();
        let region = &binding.region_tracks[0].snapshot;
        assert_eq!(region.source_id, slot);
        assert_eq!(
            region.frame_times_us.as_deref(),
            Some([0, 100000, 400000, 500000].as_slice())
        );
        assert_eq!(binding.source_hash, "a".repeat(64));
        assert_eq!(binding.settings(), plan(&cutout()).unwrap().correction);
        assert_ne!(region.mask_source_id, "region");
        assert_eq!(
            project
                .library_source_id(&SourceId::parse(&region.mask_source_id).unwrap())
                .as_str(),
            "region"
        );
        masks.push(region.mask_source_id.clone());
        content.validate_native_surface().unwrap();
    }
    assert_eq!(masks[0], masks[1]);
    assert_eq!(
        project
            .library_source_id(&SourceId::parse("region").unwrap())
            .as_str(),
        "decoy"
    );
    assert_eq!(
        project
            .library_source_id(&SourceId::parse("slot").unwrap())
            .as_str(),
        "raw"
    );
}
#[test]
fn legacy_video_cutouts_keep_physical_source_identity() {
    let report =
        import_with_ready_bindings(request(document(), false), &StillBindings::new(), &videos())
            .unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    for node in project.compositions.values() {
        let CompositionTrack::Video { clips, .. } = &node.content.tracks[0] else {
            panic!()
        };
        assert_eq!(clips[0].source_id.as_str(), "raw");
        let region = &clips[0].concat_cutout.as_ref().unwrap().region_tracks[0].snapshot;
        assert_eq!(region.source_id, "raw");
        assert_eq!(region.mask_source_id, "region");
    }
}
#[test]
fn unverified_stale_or_mismatched_video_bindings_never_return_partial_projects() {
    let doc = document();
    assert_eq!(
        import(request(doc.clone(), true)).unwrap().status,
        "needs_attention"
    );
    for mode in 0..13 {
        let mut videos = videos();
        let ready = videos.get_mut("a").unwrap().get_mut("same").unwrap();
        match mode {
            0 => ready.source.id = SourceId::parse("other").unwrap(),
            1 => ready.binding.subject = "object".into(),
            2 => ready.binding.source_hash = "0".repeat(64),
            3 => ready.binding.correction.as_mut().unwrap().feather = 0.03,
            4 => ready.binding.region_tracks[0].snapshot.source_id = "other".into(),
            5 => ready.regions.clear(),
            6 => ready.regions.push(source("unused", 256, 256)),
            7 => ready.binding.region_tracks[0].snapshot.mask_source_id = "raw".into(),
            8 => {
                videos.insert("missing".into(), BTreeMap::new());
            }
            9 => {
                videos.remove("b");
            }
            10 => ready.regions.push(ready.regions[0].clone()),
            11 => ready.binding.region_tracks[0].snapshot.width += 2,
            _ => ready.binding.region_tracks[0].snapshot.height += 2,
        }
        let report =
            import_with_ready_bindings(request(doc.clone(), true), &StillBindings::new(), &videos)
                .unwrap();
        assert_eq!(report.status, "needs_attention", "mode {mode}");
        assert!(report.project.is_none());
        assert_eq!(report.source_document, doc);
    }
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire = json!({"schemaVersion":1,"document":wire,"mediaBindings":{},"videoBindings":{}});
    assert!(serde_json::from_value::<ImportRequest>(wire).is_err());
}
#[test]
fn video_binding_preserves_existing_clip_audio_timing_and_pose_conversion() {
    let mut doc = document();
    for timeline in doc["timelines"].as_array_mut().unwrap() {
        timeline["clips"][0]["volume"] = json!(0.6);
        timeline["clips"][0]["fadeIn"] = json!(0.05);
        timeline["clips"][0]["fadeOut"] = json!(0.05);
        timeline["clips"][0]["sourceStart"] = json!(0.1);
        timeline["clips"][0]["offsetX"] = json!(0.1);
    }
    let mut prepared_request = request(doc.clone(), true);
    for id in ["slot", "alias"] {
        prepared_request
            .media_bindings
            .get_mut(id)
            .unwrap()
            .has_audio = true;
    }
    let mut bindings = videos();
    for clips in bindings.values_mut() {
        clips.get_mut("same").unwrap().source.has_audio = true;
    }
    let ready =
        import_with_ready_bindings(prepared_request, &StillBindings::new(), &bindings).unwrap();
    assert_eq!(ready.status, "converted", "{:?}", ready.diagnostics);
    for timeline in doc["timelines"].as_array_mut().unwrap() {
        timeline["clips"][0]
            .as_object_mut()
            .unwrap()
            .remove("cutout");
    }
    let mut plain_request = request(doc, true);
    for id in ["slot", "alias"] {
        plain_request.media_bindings.get_mut(id).unwrap().has_audio = true;
    }
    let plain = import(plain_request).unwrap();
    assert_eq!(plain.status, "converted", "{:?}", plain.diagnostics);
    let ready = ready.project.unwrap();
    let plain = plain.project.unwrap();
    for (id, node) in ready.compositions {
        let CompositionTrack::Video { clips, .. } = &node.content.tracks[0] else {
            panic!()
        };
        let CompositionTrack::Video {
            clips: original, ..
        } = &plain.compositions[&id].content.tracks[0]
        else {
            panic!()
        };
        let mut actual = serde_json::to_value(&clips[0]).unwrap();
        actual.as_object_mut().unwrap().remove("concatCutout");
        assert_eq!(actual, serde_json::to_value(&original[0]).unwrap());
    }
}

#[test]
fn mixed_still_and_video_cutouts_bind_atomically_without_sharing_clip_versions() {
    use video_kadr_backend::domain::{
        composition::CompositionClipId, concat_cutout_binding::StillAssets,
        concat_project_import::ReadyStillBinding,
    };
    let image = |id: &str, size: u32| -> CompositionSource {
        serde_json::from_value(json!({"id":id,"kind":"image","durationTicks":1000000,"width":size,"height":size,"hasAudio":false})).unwrap()
    };
    let still_settings = json!({"subject":"person","mode":"auto","feather":0});
    let mut doc = document();
    doc["media"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"still","kind":"image"}));
    doc["timelines"][0]["tracks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"images"}));
    doc["timelines"][0]["clips"].as_array_mut().unwrap().push(json!({"id":"picture","trackId":"images","mediaId":"still","kind":"image","duration":0.2,"cutout":still_settings}));
    let snapshot=serde_json::from_value(json!({"sourceId":"still-raw","maskSourceId":"still-mask","sourceHash":"1".repeat(64),"maskHash":"2".repeat(64),"maskFileHash":"3".repeat(64),"modelHash":SELFIE_MODEL_HASH,"runtimeHash":"4".repeat(64),"subject":"person","correction":{"custom":false,"strokes":[],"feather":0}})).unwrap();
    let stills = StillBindings::from([(
        "a".into(),
        BTreeMap::from([(
            "picture".into(),
            ReadyStillBinding {
                assets: StillAssets {
                    original: image("still-raw", 2),
                    copy: image("still-copy", 2),
                    mask: image("still-mask", 256),
                    regions: vec![],
                    snapshot,
                },
                source_settings: plan(&still_settings).unwrap(),
                source_hash: "1".repeat(64),
                model_hash: SELFIE_MODEL_HASH.into(),
                runtime_hash: "4".repeat(64),
            },
        )]),
    )]);
    let mut input = request(doc.clone(), true);
    input
        .media_bindings
        .insert("still".into(), image("still-raw", 2));
    let report = import_with_ready_bindings(input, &stills, &videos()).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, doc);
    let project = report.project.unwrap();
    project.dependency_order().unwrap();
    let content = &project.compositions["concat-timeline-0"].content;
    assert!(content
        .metadata
        .as_ref()
        .unwrap()
        .media_versions
        .contains_key(&CompositionClipId::parse("picture").unwrap()));
    assert!(!content
        .metadata
        .as_ref()
        .unwrap()
        .media_versions
        .contains_key(&CompositionClipId::parse("same").unwrap()));
    assert!(content.tracks.iter().any(|track|matches!(track,CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.id.as_str()=="same"&&clip.concat_cutout.is_some()))));
}
