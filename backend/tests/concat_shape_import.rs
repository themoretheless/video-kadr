use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::CompositionTrack,
    concat_project_import::{import, ImportRequest},
    flat_surface,
};
fn source(kind: &str, blend: &str) -> Value {
    json!({"version":1,"media":[],"activeTimelineId":"TL","timelines":[{"id":"TL","name":"Shapes","video":{"width":160,"height":120,"rateNum":10,"rateDen":1},"tracks":[{"id":"shapes","name":"Shapes","visible":true,"muted":false}],"clips":[{"id":"shape","trackId":"shapes","kind":"shape","start":1,"duration":2,"blend":blend,"shape":{"kind":kind,"size":0.5,"fill":"#ff804080","stroke":"#102030ff","strokeWidth":0.025}}]}]})
}
fn converted(source: Value) -> video_kadr_backend::domain::concat_project_import::ImportReport {
    import(
        serde_json::from_value::<ImportRequest>(
            json!({"schemaVersion":1,"document":source,"mediaBindings":{}}),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn imports_all_seven_source_figures_without_media_and_preserves_original_document() {
    for name in [
        "square",
        "circle",
        "triangle",
        "parallelogram",
        "trapezoid",
        "line",
        "arrow",
    ] {
        let original = source(name, "normal");
        let report = converted(original.clone());
        assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
        assert_eq!(report.source_document, original);
        let project = report.project.unwrap();
        let composition = &project.compositions[&project.root].content;
        assert!(composition.sources.is_empty());
        let CompositionTrack::Graphic { clips, .. } = &composition.tracks[0] else {
            panic!("graphic lane required")
        };
        assert_eq!(
            serde_json::to_value(clips[0].concat_shape.as_ref().unwrap()).unwrap(),
            original["timelines"][0]["clips"][0]["shape"]
        );
        assert_eq!(clips[0].timeline_start_tick, 1000000);
        assert_eq!(clips[0].timeline_end_tick, 3000000);
        let frame = flat_surface::render_generated(composition, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap();
        assert!(frame.rgba.chunks_exact(4).any(|pixel| pixel[0] > 0));
    }
}
#[test]
fn all_source_shape_blends_match_independent_linear_light_formulas() {
    for mode in ["normal", "multiply", "screen", "add", "lighten", "darken"] {
        let report = converted(source("square", mode));
        assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
        let mut project = report.project.unwrap();
        let composition = &mut project.compositions.get_mut(&project.root).unwrap().content;
        composition.canvas.background =
            serde_json::from_value(json!({"red":0.2,"green":0.4,"blue":0.6,"alpha":1})).unwrap();
        let CompositionTrack::Graphic { clips, .. } = &composition.tracks[0] else {
            panic!("graphic required")
        };
        let raw = clips[0]
            .concat_shape
            .as_ref()
            .unwrap()
            .render((160, 120), &mut || Ok(()))
            .unwrap();
        let frame = flat_surface::render_generated(composition, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap();
        for (actual, pixel) in frame.rgba.chunks_exact(4).zip(raw.rgba.chunks_exact(4)) {
            let a = pixel[3] as f32 / 255.;
            for channel in 0..3 {
                let source = (pixel[channel] as f32 / 255.).powf(2.4);
                let under = ([0.2f32, 0.4, 0.6][channel]).powf(2.4);
                let over = source * a;
                let mixed = match mode {
                    "normal" => over + under * (1. - a),
                    "multiply" => over * under + under * (1. - a),
                    "screen" => over * (1. - under) + under,
                    "add" => over + under,
                    "lighten" => source.max(under) * a + under * (1. - a),
                    "darken" => source.min(under) * a + under * (1. - a),
                    _ => unreachable!(),
                };
                let expected = (mixed.clamp(0., 1.).powf(1. / 2.4) * 255.).round() as u8;
                assert!(
                    actual[channel].abs_diff(expected) <= 1,
                    "{mode}: {actual:?} expected channel {expected}"
                );
            }
        }
    }
}
#[test]
fn shape_import_retains_crop_motion_effect_keys_and_refuses_malformed_or_overlapping_input() {
    let mut original = source("arrow", "screen");
    original["timelines"][0]["clips"][0]["crop"] =
        json!({"left":0.1,"top":0.2,"right":0,"bottom":0});
    original["timelines"][0]["clips"][0]["flipH"] = json!(true);
    original["timelines"][0]["clips"][0]["rotation"] = json!(15);
    original["timelines"][0]["clips"][0]["videoEffects"] =
        json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0},{"at":1,"value":2}]}}]);
    let report = converted(original.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let composition = &project.compositions[&project.root].content;
    let CompositionTrack::Graphic { clips, .. } = &composition.tracks[0] else {
        panic!("graphic required")
    };
    assert!(
        clips[0]
            .transform
            .concat_preparation
            .as_ref()
            .unwrap()
            .flip_h
    );
    assert_eq!(clips[0].effects.len(), 1);
    assert_ne!(
        flat_surface::render_generated(composition, 1100000, &mut || Ok(()))
            .unwrap()
            .unwrap()
            .rgba,
        flat_surface::render_generated(composition, 2800000, &mut || Ok(()))
            .unwrap()
            .unwrap()
            .rgba
    );
    let mut bad = original.clone();
    bad["timelines"][0]["clips"][0]["shape"]["size"] = json!(-1);
    let report = converted(bad);
    assert_ne!(report.status, "converted");
    assert!(report.project.is_none());
    let mut other = original["timelines"][0]["clips"][0].clone();
    other["id"] = json!("other");
    original["timelines"][0]["clips"]
        .as_array_mut()
        .unwrap()
        .push(other);
    let report = converted(original);
    assert_ne!(report.status, "converted");
    assert!(report.project.is_none());
}

#[test]
fn shape_temporal_effects_resample_the_keyed_prefix_on_the_source_clock() {
    let mut original = source("triangle", "normal");
    original["timelines"][0]["clips"][0]["videoEffects"] = json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0,"ease":"linear"},{"at":1,"value":2}]}}]);
    let report = converted(original);
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let mut project = report.project.unwrap();
    let composition = &mut project.compositions.get_mut(&project.root).unwrap().content;
    let before = flat_surface::render_generated(composition, 1750000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    let expected = flat_surface::render_generated(composition, 1000000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    assert_ne!(before, expected);
    if let CompositionTrack::Graphic { clips, .. } = &mut composition.tracks[0] {
        clips[0].effects.push(
            serde_json::from_value(json!({"kind":"posterize_time","parameters":{"rate":1}}))
                .unwrap(),
        );
    }
    composition.validate().unwrap();
    assert_eq!(
        flat_surface::render_generated(composition, 1750000, &mut || Ok(()))
            .unwrap()
            .unwrap(),
        expected
    );
}

#[test]
fn shape_motion_keys_remain_editable_source_data_and_change_the_rendered_pose() {
    let mut original = source("arrow", "normal");
    original["timelines"][0]["clips"][0]["keys"] =
        json!([{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":90}]);
    let report = converted(original);
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let composition = &project.compositions[&project.root].content;
    let CompositionTrack::Graphic { clips, .. } = &composition.tracks[0] else {
        panic!("graphic required")
    };
    assert_eq!(
        clips[0].transform.concat_animation.as_ref().unwrap().keys["rotation"].len(),
        2
    );
    assert_ne!(
        flat_surface::render_generated(composition, 1100000, &mut || Ok(()))
            .unwrap()
            .unwrap(),
        flat_surface::render_generated(composition, 2800000, &mut || Ok(()))
            .unwrap()
            .unwrap()
    );
}

#[test]
fn imported_source_tracks_keep_bottom_to_top_paint_order_and_visibility() {
    let mut original = source("square", "normal");
    original["timelines"][0]["tracks"] = json!([{"id":"bottom","visible":true,"muted":false},{"id":"top","visible":true,"muted":false}]);
    original["timelines"][0]["clips"] = json!([
        {"id":"red","trackId":"bottom","kind":"shape","start":0,"duration":2,"shape":{"kind":"square","size":0.9,"fill":"#ff0000","strokeWidth":0}},
        {"id":"blue","trackId":"top","kind":"shape","start":0,"duration":2,"shape":{"kind":"circle","size":0.4,"fill":"#0000ff","strokeWidth":0}}
    ]);
    let report = converted(original.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let composition = &project.compositions[&project.root].content;
    assert_eq!(composition.tracks[0].id().as_str(), "lane-1-shape");
    let frame = flat_surface::render_generated(composition, 1000000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    let pixel = |x: usize, y: usize| &frame.rgba[(y * 160 + x) * 4..(y * 160 + x + 1) * 4];
    assert_eq!(pixel(80, 60), [0, 0, 255, 255]);
    assert_eq!(pixel(40, 60), [255, 0, 0, 255]);
    original["timelines"][0]["tracks"][1]["visible"] = json!(false);
    let report = converted(original);
    assert_eq!(report.status, "converted");
    let project = report.project.unwrap();
    let frame = flat_surface::render_generated(
        &project.compositions[&project.root].content,
        1000000,
        &mut || Ok(()),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        &frame.rgba[(60 * 160 + 80) * 4..(60 * 160 + 81) * 4],
        [255, 0, 0, 255]
    );
}
