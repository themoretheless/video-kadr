use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::{CompositionTrack, TextClip},
    concat_project_import::{import, ImportRequest},
    legacy_text_surface,
};
fn document() -> Value {
    json!({"version":1,"media":[],"activeTimelineId":"TL","timelines":[{"id":"TL","name":"Titles","video":{"width":320,"height":180,"rateNum":10,"rateDen":1},"tracks":[{"id":"titles","name":"Title lane","visible":true,"muted":false}],"clips":[{"id":"title","trackId":"titles","kind":"text","start":1,"duration":2,"text":{"content":"Title words\nКадр","fontFamily":"Hanken Grotesk","fontSize":0.15,"fontWeight":700.0,"italic":true,"color":"#ff8040ff","align":"right","opacity":1.0,"strokeWidth":0.005,"strokeColor":"#102030ff","shadow":true,"background":"#10203080","backgroundRadius":0.03,"backgroundPaddingX":0.02,"backgroundPaddingY":0.01,"lineHeight":1.4,"tracking":0.003,"maxWidth":0.6,"maxHeight":0.5}}]}]})
}
fn convert(source: Value) -> video_kadr_backend::domain::concat_project_import::ImportReport {
    import(
        serde_json::from_value::<ImportRequest>(
            json!({"schemaVersion":1,"document":source,"mediaBindings":{}}),
        )
        .unwrap(),
    )
    .unwrap()
}
fn title(report: &video_kadr_backend::domain::concat_project_import::ImportReport) -> &TextClip {
    let project = report.project.as_ref().unwrap();
    let CompositionTrack::Text { clips, .. } =
        &project.compositions[&project.root].content.tracks[0]
    else {
        panic!("expected title lane")
    };
    &clips[0]
}
fn pixels(clip: &TextClip, time: u64) -> Option<Vec<u8>> {
    legacy_text_surface::render(clip, (320, 180), time, &mut || Ok(()))
        .unwrap()
        .map(|frame| frame.to_surface(&mut || Ok(())).unwrap().rgba)
}
#[test]
fn imports_retained_rich_titles_without_media_and_uses_original_rust_pixels() {
    let source = document();
    let report = convert(source.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, source);
    let clip = title(&report);
    let style = clip.concat_title.as_ref().unwrap();
    let mut original = source["timelines"][0]["clips"][0]["text"].clone();
    original.as_object_mut().unwrap().remove("content");
    assert_eq!(serde_json::to_value(style).unwrap(), original);
    assert_eq!(clip.text, "Title words\nКадр");
    assert_eq!(clip.timeline_start_tick, 1000000);
    assert_eq!(clip.timeline_end_tick, 3000000);
    let rendered = pixels(clip, 1500000).unwrap();
    let expected = concat_text::render_frame(
        video_kadr_backend::domain::concat_title::fonts(),
        &style.original(&clip.text),
        320,
        180,
    )
    .unwrap();
    assert!(rendered.chunks_exact(4).any(|p| p[3] > 0));
    assert!(
        rendered
            .iter()
            .zip(&expected.rgba)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "native linear composition differs from original title by more than one byte unit"
    );
    assert!(pixels(clip, 999999).is_none());
    assert!(pixels(clip, 3000000).is_none());
    let encoded = serde_json::to_value(report.project.unwrap()).unwrap();
    assert!(
        encoded["compositions"]["concat-timeline-0"]["content"]["sources"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn title_opacity_transform_and_invalid_input_are_not_silently_lost() {
    let source = document();
    let report = convert(source.clone());
    let clip = title(&report);
    let base = pixels(clip, 1500000).unwrap();
    let mut changed = clip.clone();
    changed.concat_title.as_mut().unwrap().opacity = 0.5;
    let faded = pixels(&changed, 1500000).unwrap();
    assert_ne!(faded, base);
    assert!(faded.chunks_exact(4).all(|p| p[3] <= 128));
    changed = clip.clone();
    changed.transform.x = video_kadr_backend::domain::composition::AnimatableValue::constant(20.);
    assert_ne!(pixels(&changed, 1500000).unwrap(), base);
    for property in ["fontSize", "fontWeight", "lineHeight", "tracking"] {
        let mut invalid = source.clone();
        invalid["timelines"][0]["clips"][0]["text"][property] = json!("invalid");
        let rejected = convert(invalid.clone());
        assert!(rejected.project.is_none());
        assert_eq!(rejected.source_document, invalid);
    }
    let mut overlapping = source.clone();
    let mut second = overlapping["timelines"][0]["clips"][0].clone();
    second["id"] = json!("other");
    overlapping["timelines"][0]["clips"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert!(convert(overlapping).project.is_none());
    let mut unsupported = source;
    unsupported["timelines"][0]["clips"][0]["blend"] = json!("overlay");
    assert!(convert(unsupported).project.is_none());
}

#[test]
fn all_source_title_blends_match_the_original_linear_light_formula() {
    use video_kadr_backend::domain::{composition::Rgba, flat_surface};
    let mut frames = std::collections::BTreeSet::new();
    for mode in ["normal", "multiply", "screen", "add", "lighten", "darken"] {
        let mut source = document();
        source["timelines"][0]["clips"][0]["blend"] = json!(mode);
        source["timelines"][0]["clips"][0]["opacity"] = json!(0.4);
        source["timelines"][0]["clips"][0]["text"]["opacity"] = json!(0.5);
        let mut report = convert(source);
        assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
        let clip = title(&report).clone();
        let style = clip.concat_title.as_ref().unwrap();
        let raw = concat_text::render_frame(
            video_kadr_backend::domain::concat_title::fonts(),
            &style.original(&clip.text),
            320,
            180,
        )
        .unwrap();
        let project = report.project.as_mut().unwrap();
        let composition = &mut project.compositions.get_mut(&project.root).unwrap().content;
        composition.canvas.background = Rgba {
            red: 0.2,
            green: 0.4,
            blue: 0.6,
            alpha: 1.,
        };
        let actual = flat_surface::render_generated(composition, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap();
        let mut expected = vec![];
        for p in raw.rgba.chunks_exact(4) {
            let a = p[3] as f32 / 255. * 0.2;
            for k in 0..3 {
                let source = (p[k] as f32 / 255.).powf(2.4);
                let under = ([51., 102., 153.][k] / 255f32).powf(2.4);
                let over = source * a;
                let mixed = match mode {
                    "normal" => over + under * (1. - a),
                    "multiply" => over * under + under * (1. - a),
                    "screen" => over * (1. - under) + under,
                    "add" => over + under,
                    "lighten" => source.max(under) * a + under * (1. - a),
                    _ => source.min(under) * a + under * (1. - a),
                };
                expected.push((mixed.clamp(0., 1.).powf(1. / 2.4) * 255.).round() as u8);
            }
            expected.push(255);
        }
        assert!(
            actual
                .rgba
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "source blend {mode} differs"
        );
        frames.insert(actual.rgba);
    }
    assert_eq!(frames.len(), 6);
}

#[test]
fn source_title_crop_and_flips_precede_fit_transform_and_blend() {
    use video_kadr_backend::domain::{
        concat_clip_animation::Pose, concat_picture::Preparation, flat_surface,
        group_surface::GroupSurface,
    };
    for (flip_h, flip_v) in [(true, false), (false, true), (true, true)] {
        let mut source = document();
        let clip = &mut source["timelines"][0]["clips"][0];
        clip["crop"] = json!({"left":0.1,"top":0.2,"right":0.05,"bottom":0.1});
        clip["flipH"] = json!(flip_h);
        clip["flipV"] = json!(flip_v);
        clip["scale"] = json!(1.2);
        clip["rotation"] = json!(23);
        clip["offsetX"] = json!(0.06);
        clip["offsetY"] = json!(0.02);
        clip["opacity"] = json!(0.7);
        clip["blend"] = json!("screen");
        let report = convert(source);
        assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
        let clip = title(&report);
        let style = clip.concat_title.as_ref().unwrap();
        let preparation = Preparation {
            crop: [0.1, 0.2, 0.05, 0.1],
            canvas: [320, 180],
            flip_h,
            flip_v,
        };
        assert_eq!(
            clip.transform.concat_preparation.as_ref(),
            Some(&preparation)
        );
        let raster = concat_text::render_frame(
            video_kadr_backend::domain::concat_title::fonts(),
            &style.original(&clip.text),
            320,
            180,
        )
        .unwrap();
        let mut expected = GroupSurface {
            width: 320,
            height: 180,
            rgba: [0, 0, 0, 255].repeat(320 * 180),
        };
        preparation
            .composite_reference(
                &GroupSurface {
                    width: 320,
                    height: 180,
                    rgba: raster.rgba,
                },
                &mut expected,
                Pose {
                    scale_x: 1.2,
                    scale_y: 1.2,
                    x: 0.06 * 320.,
                    y: 0.02 * 180.,
                    rotation_degrees: 23.,
                    opacity: 0.7,
                },
                video_kadr_backend::domain::composition::BlendMode::Screen,
                &mut || Ok(()),
            )
            .unwrap();
        let project = report.project.as_ref().unwrap();
        let actual = flat_surface::render_generated(
            &project.compositions[&project.root].content,
            1500000,
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn title_effect_order_source_keys_bypass_and_crop_match_the_explicit_native_pipeline() {
    use video_kadr_backend::domain::{
        composition::BlendMode, concat_clip_animation::Pose, concat_contrast::ConcatContrast,
        concat_exposure::ConcatExposure, concat_gaussian_blur::GaussianBlur,
        concat_picture::Preparation, flat_surface, group_surface::GroupSurface,
        linear_color_frame::LinearColorFrame,
    };
    let mut source = document();
    let clip = &mut source["timelines"][0]["clips"][0];
    clip["crop"] = json!({"left":0.1,"right":0.05,"top":0.2,"bottom":0.1});
    clip["flipH"] = json!(true);
    clip["videoEffects"] = json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0,"ease":"linear"},{"at":1,"value":2}]}},{"id":"concat.contrast","params":{"amount":40}},{"id":"concat.gaussian-blur","params":{"radius":1}},{"id":"unknown.saved.package","enabled":false}]);
    let report = convert(source.clone());
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, source);
    let clip = title(&report);
    let effects = serde_json::to_value(&clip.effects).unwrap();
    assert_eq!(effects.as_array().unwrap().len(), 4);
    assert_eq!(effects[0]["kind"], "concat_exposure");
    assert_eq!(effects[1]["kind"], "concat_contrast");
    assert_eq!(effects[2]["kind"], "concat_gaussian_blur");
    assert_eq!(effects[3]["kind"], "concat_bypass");
    assert_eq!(effects[3]["link"]["id"], "unknown.saved.package");
    let style = clip.concat_title.as_ref().unwrap();
    let raster = concat_text::render_frame(
        video_kadr_backend::domain::concat_title::fonts(),
        &style.original(&clip.text),
        320,
        180,
    )
    .unwrap();
    let preparation = Preparation {
        crop: [0.1, 0.2, 0.05, 0.1],
        canvas: [320, 180],
        flip_h: true,
        flip_v: false,
    };
    let prepared = preparation
        .prepare(
            &GroupSurface {
                width: 320,
                height: 180,
                rgba: raster.rgba,
            },
            &mut || Ok(()),
        )
        .unwrap()
        .surface;
    let project = report.project.as_ref().unwrap();
    let composition = &project.compositions[&project.root].content;
    let mut outputs = std::collections::BTreeSet::new();
    for time in [1000000, 1500000, 2500000] {
        let mut frame = LinearColorFrame::from_surface(&prepared, &mut || Ok(())).unwrap();
        ConcatExposure {
            stops: (time - 1000000) as f32 / 1000000.,
        }
        .apply_linear(&mut frame, &mut || Ok(()))
        .unwrap();
        ConcatContrast { amount: 40. }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
        GaussianBlur { radius: 1. }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
        let mut expected = GroupSurface {
            width: 320,
            height: 180,
            rgba: [0, 0, 0, 255].repeat(320 * 180),
        };
        Preparation {
            crop: [0.; 4],
            flip_h: false,
            flip_v: false,
            ..preparation.clone()
        }
        .composite_reference(
            &frame.to_surface(&mut || Ok(())).unwrap(),
            &mut expected,
            Pose {
                scale_x: 1.,
                scale_y: 1.,
                x: 0.,
                y: 0.,
                rotation_degrees: 0.,
                opacity: 1.,
            },
            BlendMode::Normal,
            &mut || Ok(()),
        )
        .unwrap();
        let actual = flat_surface::render_generated(composition, time, &mut || Ok(()))
            .unwrap()
            .unwrap();
        assert_eq!(actual, expected, "effect clock {time}");
        outputs.insert(actual.rgba);
    }
    assert_eq!(outputs.len(), 3);
    let mut reordered = composition.clone();
    if let CompositionTrack::Text { clips, .. } = &mut reordered.tracks[0] {
        clips[0].effects.swap(0, 1);
    }
    assert_ne!(
        flat_surface::render_generated(&reordered, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap(),
        flat_surface::render_generated(composition, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap()
    );
    let mut invalid = source;
    invalid["timelines"][0]["clips"][0]["videoEffects"][0]["id"] = json!("unknown.active.package");
    assert!(convert(invalid).project.is_none());
}

#[test]
fn temporal_title_effects_resample_color_and_keyed_prefix_at_the_requested_clock() {
    use video_kadr_backend::domain::flat_surface;
    let mut source = document();
    source["timelines"][0]["clips"][0]["videoEffects"] = json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0,"ease":"linear"},{"at":1,"value":2}]}}]);
    let mut report = convert(source);
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.as_mut().unwrap();
    let composition = &mut project.compositions.get_mut(&project.root).unwrap().content;
    if let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] {
        clips[0].color_animation=Some(serde_json::from_value(json!({"keys":[{"t_us":0,"value":{"r":255,"g":0,"b":0,"a":255},"in_":[0,0],"out":[1,1],"segment":{"kind":"Linear"}},{"t_us":1000000,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0,0],"out":[1,1],"segment":{"kind":"Linear"}}],"extrapolate":{"before":"Hold","after":"Hold"}})).unwrap());
    }
    let before = flat_surface::render_generated(composition, 1750000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    let expected = flat_surface::render_generated(composition, 1000000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    assert_ne!(before, expected);
    if let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] {
        clips[0].effects.push(
            serde_json::from_value(json!({"kind":"posterize_time","parameters":{"rate":1}}))
                .unwrap(),
        );
    }
    composition.validate().unwrap();
    let actual = flat_surface::render_generated(composition, 1750000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn initializing_and_resetting_title_preparation_preserves_the_original_raster() {
    let report = convert(document());
    let mut value = serde_json::to_value(title(&report)).unwrap();
    value["transform"].as_object_mut().unwrap().remove("concatPreparation");
    let bare: TextClip = serde_json::from_value(value.clone()).unwrap();
    let original = pixels(&bare, 1500000).unwrap();
    value["transform"]["concatPreparation"] = json!({"crop":[0,0,0,0],"canvas":[320,180]});
    let initialized: TextClip = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(pixels(&initialized, 1500000).unwrap(), original);
    value["transform"]["concatPreparation"]["flipH"] = json!(true);
    let reflected: TextClip = serde_json::from_value(value.clone()).unwrap();
    assert_ne!(pixels(&reflected, 1500000).unwrap(), original);
    value["transform"]["concatPreparation"]["flipH"] = json!(false);
    let reset: TextClip = serde_json::from_value(value).unwrap();
    assert_eq!(pixels(&reset, 1500000).unwrap(), original);
}

#[test]
fn initializing_title_source_motion_preserves_native_rest_pose_and_animates_local_clock() {
    let report = convert(document());
    let mut value = serde_json::to_value(title(&report)).unwrap();
    value["transform"].as_object_mut().unwrap().remove("concatAnimation");
    for (field, number) in [("x", 64.), ("y", -18.), ("scaleX", 1.5), ("scaleY", 0.7), ("rotationDegrees", 23.)] {
        value["transform"][field] = json!({"mode":"constant","value":number});
    }
    value["opacity"] = json!({"mode":"constant","value":0.4});
    let bare: TextClip = serde_json::from_value(value.clone()).unwrap();
    value["transform"]["concatAnimation"] = json!({"durationTicks":2000000,"keys":{},"canvas":[320,180],"stretch":[1.5,0.7],"rest":{"scaleX":1,"scaleY":1,"x":0.2,"y":-0.1,"rotationDegrees":23,"opacity":0.4}});
    let initialized: TextClip = serde_json::from_value(value.clone()).unwrap();
    for clock in [1000000, 1500000, 2500000] {
        assert_eq!(pixels(&initialized, clock), pixels(&bare, clock));
    }
    value["transform"]["concatAnimation"]["keys"]["rotation"] = json!([{"at":0,"value":23},{"at":1,"value":90}]);
    let animated: TextClip = serde_json::from_value(value).unwrap();
    assert_eq!(pixels(&animated, 1000000), pixels(&bare, 1000000));
    assert_ne!(pixels(&animated, 1500000), pixels(&bare, 1500000));
    assert_ne!(pixels(&animated, 1500000), pixels(&animated, 2500000));
}
