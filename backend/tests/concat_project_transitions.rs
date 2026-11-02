use serde_json::{json, Value};
use std::collections::BTreeMap;
use video_kadr_backend::domain::{
    composition::{CompositionSource, CompositionTrack},
    concat_project_export::export_with_paths,
    concat_project_import::{import, ImportRequest},
};
fn input(id: &str, source_start: f64) -> (Value, BTreeMap<String, CompositionSource>) {
    let document = json!({"version":1,"media":[{"id":"a","kind":"video","path":"/missing/a.mov"},{"id":"b","kind":"video","path":"/missing/b.mov"}],"timelines":[{"id":"TL","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"out","trackId":"v","mediaId":"a","kind":"video","start":0,"duration":2,"sourceStart":1},{"id":"in","trackId":"v","mediaId":"b","kind":"video","start":2,"duration":2,"sourceStart":source_start,"transitionIn":{"id":id,"duration":1}}]}]});
    let bindings=serde_json::from_value(json!({"a":{"id":"a","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false},"b":{"id":"b","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false}})).unwrap();
    (document, bindings)
}
fn convert(
    document: Value,
    bindings: &BTreeMap<String, CompositionSource>,
) -> video_kadr_backend::domain::nested_composition::NestedCompositionProject {
    let request: ImportRequest = serde_json::from_value(
        json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}),
    )
    .unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
#[test]
fn packaged_transitions_import_and_export_with_exact_centered_or_late_windows() {
    for name in [
        "dissolve",
        "fade-black",
        "fade-white",
        "linear-wipe",
        "clock-wipe",
        "iris",
        "shape-wipe",
        "push",
        "slide",
        "rgb-split-wipe",
        "spin",
        "swirl",
        "blur-dissolve",
        "whip-pan",
        "zoom-transition",
        "light-leak-flash",
        "light-leak-transition",
        "zoom-blur-transition",
        "film-burn",
        "cube",
        "glitch-transition",
        "glitch-warp",
    ] {
        for source_start in [0., 0.2, 1.] {
            let (document, bindings) = input(&format!("concat.{name}"), source_start);
            let project = convert(document, &bindings);
            let CompositionTrack::Video { transitions, .. } =
                &project.compositions[&project.root].content.tracks[0]
            else {
                panic!()
            };
            let span = transitions[0].timeline.unwrap().source_span().unwrap();
            let expected = ((2. - source_start.min(0.5)) * 25.).round() as i64 * 10160640000;
            assert_eq!(span, (expected, 254016000000));
            let paths = BTreeMap::from([
                ("a".into(), "/portable/a.mov".into()),
                ("b".into(), "/portable/b.mov".into()),
            ]);
            let output = export_with_paths(&project, &paths).unwrap();
            if name == "dissolve" && source_start == 1. {
                if let Ok(path) = std::env::var("CONCAT_EXPORTED_TRANSITION_OUTPUT") {
                    std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
                }
            }
            if let Ok(path) = std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE") {
                let records = std::fs::read_to_string(path).unwrap();
                let record = records
                    .lines()
                    .map(|line| {
                        line.split_whitespace()
                            .map(|v| v.parse::<f64>().unwrap())
                            .collect::<Vec<_>>()
                    })
                    .find(|row| row[0] == source_start)
                    .unwrap();
                assert_eq!(span.0, ((record[1] * 25.).round() as i64) * 10160640000);
                assert_eq!(span.1, ((record[2] * 25.).round() as i64) * 10160640000);
            }
            if name == "dissolve" && source_start == 1. {
                if let Ok(path) = std::env::var("CONCAT_REWRITTEN_TRANSITION_INPUT") {
                    let rewritten: Value =
                        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                    let original_writer = convert(rewritten, &bindings);
                    assert_eq!(
                        project.compositions[&project.root].content.tracks,
                        original_writer.compositions[&original_writer.root]
                            .content
                            .tracks
                    );
                }
            }
            let restored = convert(output, &bindings);
            for time in [
                span.0 / 254016 + 40000,
                span.0 / 254016 + 240000,
                span.0 / 254016 + 520000,
                span.0 / 254016 + 960000,
            ] {
                use video_kadr_backend::domain::{flat_surface, group_surface::GroupSurface};
                let picture = |source: &str, clock: u64| {
                    let mut rgba = Vec::new();
                    for y in 0..16 {
                        for x in 0..16 {
                            rgba.extend_from_slice(&[
                                if source == "a" {
                                    (clock / 40000 % 170) as u8 + 40
                                } else {
                                    (x * 11) as u8
                                },
                                (y * 13) as u8,
                                if source == "a" {
                                    (x * 7) as u8
                                } else {
                                    (clock / 40000 % 170) as u8 + 50
                                },
                                255,
                            ]);
                        }
                    }
                    GroupSurface {
                        width: 16,
                        height: 16,
                        rgba,
                    }
                };
                let mut seen = vec![];
                let mut decoder =
                    |id: &video_kadr_backend::domain::composition::SourceId, clock: u64, _| {
                        seen.push((id.as_str().to_owned(), clock));
                        Ok(picture(id.as_str(), clock))
                    };
                let actual = flat_surface::render_media_layers(
                    &project.compositions[&project.root].content,
                    time as u64,
                    &mut || Ok(()),
                    None,
                    Some(&mut decoder),
                )
                .unwrap()
                .unwrap();
                let restored_frame = flat_surface::render_media_layers(
                    &restored.compositions[&restored.root].content,
                    time as u64,
                    &mut || Ok(()),
                    None,
                    Some(&mut decoder),
                )
                .unwrap()
                .unwrap();
                assert_eq!(actual, restored_frame);
                let head = source_start.min(0.5);
                let incoming_clock = (((source_start - head) * 25.).round() / 25. * 1e6) as u64
                    + time as u64
                    - (((2. - head) * 25.).round() / 25. * 1e6) as u64;
                assert_eq!(
                    seen,
                    vec![
                        ("a".into(), 1000000 + time as u64),
                        ("b".into(), incoming_clock),
                        ("a".into(), 1000000 + time as u64),
                        ("b".into(), incoming_clock)
                    ],
                    "original source decode clock {name}"
                );
            }
            assert_eq!(
                project.compositions[&project.root].content.tracks,
                restored.compositions[&restored.root].content.tracks,
                "{name}, head={source_start}"
            );
        }
    }
}
#[test]
fn unconverted_transition_ownership_is_retained_in_diagnostic_source() {
    for mode in ["orphan", "short", "legacy", "speed", "offgrid"] {
        let (mut document, bindings) = input("concat.dissolve", 1.);
        match mode {
            "orphan" => document["timelines"][0]["clips"][1]["start"] = json!(3),
            "short" => {
                document["timelines"][0]["clips"][1]["transitionIn"]["duration"] = json!(0.01)
            }
            "speed" => document["timelines"][0]["clips"][1]["speed"] = json!(1.5),
            "offgrid" => document["timelines"][0]["clips"][1]["sourceStart"] = json!(1.01),
            "legacy" => {
                document["timelines"][0]["clips"][1]["transitionIn"]["id"] = json!("unknown-transition")
            }
            _ => {
                document["timelines"][0]["clips"][1]["videoEffects"] =
                    json!([{"id":"concat.exposure"}])
            }
        };
        let report = import(
            serde_json::from_value(
                json!({"schemaVersion":1,"document":document,"mediaBindings":bindings}),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(report.status, "needs_attention", "{mode}");
        assert_eq!(report.source_document, document);
    }
}

#[test]
fn capped_source_duration_and_incompatible_native_windows_are_explicit() {
    let (mut document, bindings) = input("concat.spin", 1.);
    document["timelines"][0]["clips"][1]["transitionIn"]["duration"] = json!(10.);
    let project = convert(document, &bindings);
    let CompositionTrack::Video { transitions, .. } =
        &project.compositions[&project.root].content.tracks[0]
    else {
        panic!()
    };
    assert_eq!(
        transitions[0].timeline.unwrap().source_span().unwrap(),
        (254016000000, 508032000000)
    );
    let paths = BTreeMap::from([
        ("a".into(), "/portable/a.mov".into()),
        ("b".into(), "/portable/b.mov".into()),
    ]);
    let output = export_with_paths(&project, &paths).unwrap();
    assert_eq!(
        output["timelines"][0]["clips"][1]["transitionIn"]["duration"],
        10.
    );
    for mode in ["reverse", "shift", "custom", "endpoint"] {
        let mut changed = serde_json::to_value(&project).unwrap();
        let transition =
            &mut changed["compositions"][&project.root]["content"]["tracks"][0]["transitions"][0];
        match mode {
            "reverse" => transition["reverse"] = json!(true),
            "shift" => transition["timeline"]["startTick"] = json!("254016000001"),
            "custom" => transition["kernel"]["params"]["turns"] = json!({"Float":1.25}),
            _ => transition["fromClipId"] = Value::Null,
        };
        let changed = serde_json::from_value(changed).unwrap();
        assert!(export_with_paths(&changed, &paths).is_err(), "{mode}");
    }
}

#[test]
fn source_transition_sound_uses_raw_windows_linear_fades_and_exact_source_handles() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{
        nested_audio::plan_audio_tracks, nested_pcm::mix_block_with_video_transitions,
    };
    for head in [0., 0.2, 1.] {
        let (mut document, mut bindings) = input("concat.dissolve", head);
        for source in bindings.values_mut() {
            source.has_audio = true;
            source.audio_channels = Some(2);
        }
        let clips = document["timelines"][0]["clips"].as_array_mut().unwrap();
        clips[0]["volume"] = json!(0.8);
        clips[0]["fadeIn"] = json!(0.2);
        clips[0]["fadeOut"] = json!(0.4);
        clips[1]["volume"] = json!(0.6);
        clips[1]["fadeIn"] = json!(0.3);
        clips[1]["fadeOut"] = json!(0.2);
        let project = convert(document, &bindings);
        let layers = plan_audio_tracks(&project, &project.root, 0..4000000).unwrap();
        assert_eq!(layers.len(), 2);
        assert!(layers.iter().all(|layer| layer.video_transitions.is_none()));
        let lead = (head.min(0.5) * 1e6).round() as u64;
        let begin = 2000000 - lead;
        let end = begin + 1000000;
        let a = layers
            .iter()
            .find(|layer| layer.source_id().as_str() == "a")
            .unwrap();
        let b = layers
            .iter()
            .find(|layer| layer.source_id().as_str() == "b")
            .unwrap();
        assert_eq!(a.root_range_us, 0..end);
        assert_eq!(b.root_range_us, begin..4000000);
        assert_eq!(a.clip.fade_out_ticks, 1000000);
        assert_eq!(b.clip.fade_in_ticks, 1000000);
        let paths = BTreeMap::from([
            ("a".into(), "/portable/a.mov".into()),
            ("b".into(), "/portable/b.mov".into()),
        ]);
        let restored = convert(export_with_paths(&project, &paths).unwrap(), &bindings);
        let restored_layers = plan_audio_tracks(&restored, &restored.root, 0..4000000).unwrap();
        for start in [
            0,
            4800,
            begin * 48000 / 1000000 - 4,
            96000 - 4,
            end * 48000 / 1000000 - 4,
            182400,
        ] {
            let mut reads = vec![];
            let actual = mix_block_with_video_transitions(
                &layers,
                start,
                32,
                48000,
                &AtomicBool::new(false),
                |id, time| {
                    reads.push((id.as_str().to_owned(), time));
                    Ok((
                        2,
                        if id.as_str() == "a" {
                            [0.25, 0.5]
                        } else {
                            [0.75, -0.25]
                        },
                    ))
                },
            )
            .unwrap();
            let roundtrip = mix_block_with_video_transitions(
                &restored_layers,
                start,
                32,
                48000,
                &AtomicBool::new(false),
                |id, _| {
                    Ok((
                        2,
                        if id.as_str() == "a" {
                            [0.25, 0.5]
                        } else {
                            [0.75, -0.25]
                        },
                    ))
                },
            )
            .unwrap();
            assert_eq!(actual, roundtrip);
            let mut expected_reads = vec![];
            for (index, sample) in actual.iter().enumerate() {
                let time = (start + index as u64) * 1000000 / 48000;
                let ga = if time < end {
                    0.8 * (time as f64 / 200000.).min(1.) * ((end - time) as f64 / 1000000.).min(1.)
                } else {
                    0.
                };
                let gb = if time >= begin && time < 4000000 {
                    0.6 * ((time - begin) as f64 / 1000000.).min(1.)
                        * ((4000000 - time) as f64 / 200000.).min(1.)
                } else {
                    0.
                };
                if ga > 0. {
                    expected_reads.push(("a".to_owned(), 1000000 + time));
                }
                if gb > 0. {
                    expected_reads.push((
                        "b".to_owned(),
                        u64::try_from((head * 1e6).round() as i128 + time as i128 - 2000000)
                            .unwrap(),
                    ));
                }
                assert_eq!(
                    *sample,
                    [ga * 0.25 + gb * 0.75, ga * 0.5 - gb * 0.25],
                    "head={head}, time={time}"
                );
            }
            assert_eq!(
                reads, expected_reads,
                "Exact source PCM decode clocks, head={head}"
            );
        }
        let mut invalid = serde_json::to_value(&project).unwrap();
        invalid["compositions"][&project.root]["content"]["tracks"][0]["transitions"][0]
            ["concatOverlap"]["leadTicks"] = json!(123);
        let invalid = serde_json::from_value::<
            video_kadr_backend::domain::nested_composition::NestedCompositionProject,
        >(invalid)
        .unwrap();
        assert!(invalid.dependency_order().is_err());
    }
}

#[test]
fn held_outgoing_tail_uses_last_complete_picture_and_stops_its_sound_at_cut() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{
        flat_surface, group_surface::GroupSurface, nested_audio::plan_audio_tracks,
        nested_pcm::mix_block_with_video_transitions,
    };
    for name in [
        "dissolve",
        "fade-black",
        "fade-white",
        "linear-wipe",
        "clock-wipe",
        "iris",
        "shape-wipe",
        "push",
        "slide",
        "rgb-split-wipe",
        "spin",
        "swirl",
        "blur-dissolve",
        "whip-pan",
        "zoom-transition",
        "light-leak-flash",
        "light-leak-transition",
        "zoom-blur-transition",
        "film-burn",
        "cube",
        "glitch-transition",
        "glitch-warp",
    ] {
        for head in [0., 0.2, 1.] {
            let (document, mut bindings) = input(&format!("concat.{name}"), head);
            bindings.get_mut("a").unwrap().duration_ticks = 3000000;
            for source in bindings.values_mut() {
                source.has_audio = true;
                source.audio_channels = Some(2);
            }
            let project = convert(document, &bindings);
            let content = &project.compositions[&project.root].content;
            let CompositionTrack::Video { transitions, .. } = &content.tracks[0] else {
                panic!()
            };
            assert!(transitions[0].concat_overlap.unwrap().hold_outgoing);
            if let Ok(path) = std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE") {
                let text = std::fs::read_to_string(path).unwrap();
                let row = text
                    .lines()
                    .map(|line| {
                        line.split_whitespace()
                            .map(|value| value.parse::<f64>().unwrap())
                            .collect::<Vec<_>>()
                    })
                    .find(|row| row[0] == head && row[4] == 0.)
                    .unwrap();
                assert_eq!((row[5] * 1e6).round() as u64, 2960000);
            }
            let paths = BTreeMap::from([
                ("a".into(), "/portable/a.mov".into()),
                ("b".into(), "/portable/b.mov".into()),
            ]);
            let restored = convert(export_with_paths(&project, &paths).unwrap(), &bindings);
            for time in [1960000u64, 2000000, 2040000, 2200000, 2480000] {
                let picture = |id: &str, clock: u64| GroupSurface {
                    width: 16,
                    height: 16,
                    rgba: if id == "a" {
                        [(clock / 40000) as u8, 64, 0, 255].repeat(256)
                    } else {
                        [0, 64, (clock / 40000) as u8, 255].repeat(256)
                    },
                };
                let mut seen = vec![];
                let mut decoder =
                    |id: &video_kadr_backend::domain::composition::SourceId, clock: u64, _| {
                        seen.push((id.as_str().to_owned(), clock));
                        Ok(picture(id.as_str(), clock))
                    };
                let frame = flat_surface::render_media_layers(
                    content,
                    time,
                    &mut || Ok(()),
                    None,
                    Some(&mut decoder),
                )
                .unwrap()
                .unwrap();
                let mut decoder =
                    |id: &video_kadr_backend::domain::composition::SourceId, clock: u64, _| {
                        Ok(picture(id.as_str(), clock))
                    };
                let again = flat_surface::render_media_layers(
                    &restored.compositions[&restored.root].content,
                    time,
                    &mut || Ok(()),
                    None,
                    Some(&mut decoder),
                )
                .unwrap()
                .unwrap();
                assert_eq!(frame, again);
                let outgoing = seen.iter().find(|(id, _)| id == "a").unwrap().1;
                assert_eq!(
                    outgoing,
                    if time < 2000000 {
                        1000000 + time
                    } else {
                        2960000
                    },
                    "{name}, head={head}, time={time}"
                );
            }
            let layers = plan_audio_tracks(&project, &project.root, 0..4000000).unwrap();
            let a = layers
                .iter()
                .find(|layer| layer.source_id().as_str() == "a")
                .unwrap();
            assert_eq!(a.root_range_us, 0..2000000);
            assert_eq!(a.clip.fade_out_ticks, 0);
            for time in [1960000u64, 2000000, 2200000] {
                let actual = mix_block_with_video_transitions(
                    &layers,
                    time * 48000 / 1000000,
                    1,
                    48000,
                    &AtomicBool::new(false),
                    |id, _| {
                        Ok((
                            2,
                            if id.as_str() == "a" {
                                [0.25, 0.25]
                            } else {
                                [0.5, 0.5]
                            },
                        ))
                    },
                )
                .unwrap();
                let lead = (head.min(0.5) * 1e6).round() as u64;
                let incoming = if time >= 2000000 - lead {
                    ((time - (2000000 - lead)) as f64 / 1e6).min(1.) * 0.5
                } else {
                    0.
                };
                assert_eq!(
                    actual[0],
                    [if time < 2000000 {
                        0.25 + incoming
                    } else {
                        incoming
                    }; 2]
                );
            }
            let mut forged = serde_json::to_value(&project).unwrap();
            forged["compositions"][&project.root]["content"]["tracks"][0]["transitions"][0]
                ["concatOverlap"]["holdOutgoing"] = json!(false);
            assert!(serde_json::from_value::<
                video_kadr_backend::domain::nested_composition::NestedCompositionProject,
            >(forged)
            .unwrap()
            .dependency_order()
            .is_err());
        }
    }
}

#[test]
fn export_source_range_includes_transition_handles_with_nested_windows_and_pruning() {
    use std::collections::BTreeSet;
    for (held, legacy) in [(false, false), (true, false), (false, true)] {
        let (document, mut bindings) = input("concat.dissolve", 1.);
        if held {
            bindings.get_mut("a").unwrap().duration_ticks = 3000000;
        }
        let project = convert(document, &bindings);
        let project = if legacy {
            let mut wire = serde_json::to_value(&project).unwrap();
            let transition =
                &mut wire["compositions"][&project.root]["content"]["tracks"][0]["transitions"][0];
            transition.as_object_mut().unwrap().remove("concatOverlap");
            transition.as_object_mut().unwrap().remove("timeline");
            transition["kind"] = json!("dissolve");
            transition["durationTicks"] = json!(1000000);
            serde_json::from_value(wire).unwrap()
        } else {
            project
        };
        let ids = |range| {
            project
                .range_image_sources(&project.root, range, &BTreeSet::new())
                .unwrap()
                .into_iter()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(1000000..1100000), ["a"]);
        assert_eq!(ids(1520000..1560000), ["a", "b"]);
        assert_eq!(ids(2200000..2300000), ["a", "b"]);
        assert_eq!(ids(2520000..2600000), ["b"]);
        assert_eq!(ids(3500000..3600000), ["b"]);
        for mode in ["hidden", "disabled"] {
            let mut wire = serde_json::to_value(&project).unwrap();
            let track = &mut wire["compositions"][&project.root]["content"]["tracks"][0];
            if mode == "hidden" {
                track["hidden"] = json!(true);
            } else {
                track["clips"][0]["enabled"] = json!(false);
            }
            let changed = serde_json::from_value::<
                video_kadr_backend::domain::nested_composition::NestedCompositionProject,
            >(wire)
            .unwrap();
            let ids = changed
                .range_image_sources(&changed.root, 2200000..2300000, &BTreeSet::new())
                .unwrap();
            if mode == "hidden" {
                assert!(ids.is_empty());
            } else {
                assert_eq!(ids.iter().map(|id| id.as_str()).collect::<Vec<_>>(), ["b"]);
            }
        }
        let mut nested = serde_json::to_value(&project).unwrap();
        let child = project.root.clone();
        let canvas = nested["compositions"][&child]["content"]["canvas"].clone();
        nested["root"] = json!("outer");
        nested["timelineOrder"]
            .as_array_mut()
            .unwrap()
            .push(json!("outer"));
        nested["compositions"]["outer"] = json!({"content":{"schemaVersion":1,"timeBase":1000000,"canvas":canvas,"sources":{},"tracks":[]},"instances":[{"id":"instance","timing":{"composition":child,"timelineStartUs":4000000,"sourceInUs":0,"sourceOutUs":4000000},"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]});
        let nested = serde_json::from_value::<
            video_kadr_backend::domain::nested_composition::NestedCompositionProject,
        >(nested)
        .unwrap();
        assert_eq!(
            nested
                .range_image_sources("outer", 6200000..6300000, &BTreeSet::new())
                .unwrap()
                .len(),
            2
        );
        assert!(nested
            .range_image_sources("outer", 6200000..6300000, &BTreeSet::from([child]))
            .unwrap()
            .is_empty());
    }
}

#[test]
fn chained_source_transitions_resolve_in_document_order_and_keep_extended_middle_sound() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{
        flat_surface, group_surface::GroupSurface, nested_audio::plan_audio_tracks,
        nested_pcm::mix_block_with_video_transitions,
    };
    for name in [
        "dissolve",
        "fade-black",
        "fade-white",
        "linear-wipe",
        "clock-wipe",
        "iris",
        "shape-wipe",
        "push",
        "slide",
        "rgb-split-wipe",
        "spin",
        "swirl",
        "blur-dissolve",
        "whip-pan",
        "zoom-transition",
        "light-leak-flash",
        "light-leak-transition",
        "zoom-blur-transition",
        "film-burn",
        "cube",
        "glitch-transition",
        "glitch-warp",
    ] {
        for reversed in [false, true] {
            for held in [false, true] {
                let (mut document, mut bindings) = input(&format!("concat.{name}"), 1.);
                document["media"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));
                let clips = document["timelines"][0]["clips"].as_array_mut().unwrap();
                clips.push(json!({"id":"tail","trackId":"v","kind":"video","mediaId":"c","start":4,"duration":6,"sourceStart":2,"transitionIn":{"id":format!("concat.{name}"),"duration":3}}));
                if reversed {
                    clips.swap(1, 2);
                }
                bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":20000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2})).unwrap());
                for source in bindings.values_mut() {
                    source.has_audio = true;
                    source.audio_channels = Some(2);
                }
                if held {
                    bindings.get_mut("b").unwrap().duration_ticks = 3000000;
                }
                let project = convert(document, &bindings);
                let content = &project.compositions[&project.root].content;
                let CompositionTrack::Video { transitions, .. } = &content.tracks[0] else {
                    panic!()
                };
                assert_eq!(transitions.len(), 2);
                let second = transitions
                    .iter()
                    .find(|t| t.to_clip_id.as_ref().unwrap().as_str() == "tail")
                    .unwrap();
                let length = if reversed { 2000000 } else { 2500000 };
                assert_eq!(second.concat_overlap.unwrap().duration_ticks, length);
                assert_eq!(
                    second.concat_overlap.unwrap().authored_duration_ticks,
                    Some(3000000)
                );
                assert_eq!(second.concat_overlap.unwrap().hold_outgoing, held);
                let layers = plan_audio_tracks(&project, &project.root, 0..10000000).unwrap();
                assert_eq!(layers.len(), 3);
                if let Ok(path) = std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE") {
                    let text = std::fs::read_to_string(path).unwrap();
                    let rows = text
                        .lines()
                        .map(|line| {
                            line.split_whitespace()
                                .map(|v| v.parse::<f64>().unwrap())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    for (step, t) in transitions.iter().enumerate() {
                        let row = rows
                            .iter()
                            .find(|r| {
                                r[0] == -1.
                                    && r[1] == f64::from(reversed)
                                    && r[2] == f64::from(held)
                                    && r[3] == step as f64
                            })
                            .unwrap();
                        let span = t.timeline.unwrap().source_span().unwrap();
                        assert_eq!(
                            span,
                            (
                                ((row[5] * 25.).round() as i64) * 10160640000,
                                ((row[6] * 25.).round() as i64) * 10160640000
                            )
                        );
                    }
                    for (index, id) in ["a", "b", "c"].iter().enumerate() {
                        let row = rows
                            .iter()
                            .find(|r| {
                                r[0] == -2.
                                    && r[1] == f64::from(reversed)
                                    && r[2] == f64::from(held)
                                    && r[3] == index as f64
                            })
                            .unwrap();
                        let layer = layers
                            .iter()
                            .find(|layer| layer.source_id().as_str() == *id)
                            .unwrap();
                        assert_eq!(
                            layer.clip.placement.timeline_start_tick,
                            (row[4] * 1e6).round() as u64
                        );
                        assert_eq!(
                            layer.clip.placement.timeline_duration_ticks().unwrap(),
                            (row[5] * 1e6).round() as u64
                        );
                        assert_eq!(
                            layer.clip.placement.source_in_tick,
                            (row[6] * 1e6).round() as u64
                        );
                        assert_eq!(layer.clip.fade_in_ticks, (row[7] * 1e6).round() as u64);
                        assert_eq!(layer.clip.fade_out_ticks, (row[8] * 1e6).round() as u64);
                    }
                }
                let paths = BTreeMap::from([
                    ("a".into(), "/portable/a.mov".into()),
                    ("b".into(), "/portable/b.mov".into()),
                    ("c".into(), "/portable/c.mov".into()),
                ]);
                let output = export_with_paths(&project, &paths).unwrap();
                if name == "dissolve" && reversed && !held {
                    if let Ok(path) = std::env::var("CONCAT_EXPORTED_CHAIN_OUTPUT") {
                        std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
                    }
                    if let Ok(path) = std::env::var("CONCAT_REWRITTEN_CHAIN_INPUT") {
                        let document =
                            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                        let original = convert(document, &bindings);
                        assert_eq!(
                            content.tracks, original.compositions[&original.root].content.tracks,
                            "Original reader/writer changed sequential transition order"
                        );
                    }
                }

                let restored = convert(output, &bindings);
                assert_eq!(
                    content.tracks, restored.compositions[&restored.root].content.tracks,
                    "{name}/{reversed}/{held}"
                );
                let restored_layers =
                    plan_audio_tracks(&restored, &restored.root, 0..10000000).unwrap();
                for time in [1600000u64, 2200000, 3200000, 4200000, 4800000] {
                    let mut decode =
                        |id: &video_kadr_backend::domain::composition::SourceId, clock: u64, _| {
                            Ok(GroupSurface {
                                width: 16,
                                height: 16,
                                rgba: [
                                    if id.as_str() == "a" {
                                        (clock / 40000 % 200) as u8
                                    } else {
                                        0
                                    },
                                    if id.as_str() == "b" {
                                        (clock / 40000 % 200) as u8
                                    } else {
                                        0
                                    },
                                    if id.as_str() == "c" {
                                        (clock / 40000 % 200) as u8
                                    } else {
                                        0
                                    },
                                    255,
                                ]
                                .repeat(256),
                            })
                        };
                    let frame = flat_surface::render_media_layers(
                        content,
                        time,
                        &mut || Ok(()),
                        None,
                        Some(&mut decode),
                    )
                    .unwrap();
                    let again = flat_surface::render_media_layers(
                        &restored.compositions[&restored.root].content,
                        time,
                        &mut || Ok(()),
                        None,
                        Some(&mut decode),
                    )
                    .unwrap();
                    assert_eq!(frame, again);
                    let source = |id: &video_kadr_backend::domain::composition::SourceId, _| {
                        Ok((
                            2,
                            [if id.as_str() == "a" {
                                0.25
                            } else if id.as_str() == "b" {
                                -0.125
                            } else {
                                0.5
                            }; 2],
                        ))
                    };
                    let actual = mix_block_with_video_transitions(
                        &layers,
                        time * 48000 / 1000000,
                        16,
                        48000,
                        &AtomicBool::new(false),
                        source,
                    )
                    .unwrap();
                    let again = mix_block_with_video_transitions(
                        &restored_layers,
                        time * 48000 / 1000000,
                        16,
                        48000,
                        &AtomicBool::new(false),
                        source,
                    )
                    .unwrap();
                    assert_eq!(actual, again);
                }
            }
        }
    }
}

#[test]
fn branching_source_ownership_still_refuses_without_partial_import() {
    let(mut document,mut bindings)=input("concat.dissolve",1.);
    document["media"].as_array_mut().unwrap().push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));
    bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false})).unwrap());
    document["timelines"][0]["clips"].as_array_mut().unwrap().push(json!({"id":"tail","trackId":"v","kind":"video","mediaId":"c","start":2,"duration":2,"sourceStart":1,"transitionIn":{"id":"concat.dissolve","duration":2}}));
    let report=import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings})).unwrap()).unwrap();
    assert_eq!(report.status,"needs_attention");assert!(report.project.is_none());assert_eq!(report.source_document,document);
    assert!(report.diagnostics.iter().any(|message|message.contains("Branching")||message.contains("overlapping visual")));
}

#[test]
fn source_motion_and_volume_keys_keep_the_original_extended_video_and_audio_clocks() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{
        concat_clip_animation::{ClipAnimation, Pose},
        concat_effect_upgrade::ParamKey,
        flat_surface,
        group_surface::GroupSurface,
        nested_audio::plan_audio_tracks,
        nested_pcm::mix_block_with_video_transitions,
    };
    let properties = [
        ("scale", 0.6, 1.1),
        ("offsetX", -0.2, 0.25),
        ("offsetY", 0.15, -0.1),
        ("rotation", -25., 65.),
        ("opacity", 0.3, 0.9),
        ("volume", 0.2, 0.8),
    ];
    let oracle = std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE")
        .ok()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(|line| {
                    line.split_whitespace()
                        .map(|v| v.parse::<f64>().unwrap())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        });
    for eased in [false, true] {
        let ease = if eased {
            json!([0.42, -0.5, 0.58, 1.5])
        } else {
            json!("linear")
        };
        for name in [
            "dissolve",
            "fade-black",
            "fade-white",
            "linear-wipe",
            "clock-wipe",
            "iris",
            "shape-wipe",
            "push",
            "slide",
            "rgb-split-wipe",
            "spin",
            "swirl",
            "blur-dissolve",
            "whip-pan",
            "zoom-transition",
            "light-leak-flash",
            "light-leak-transition",
            "zoom-blur-transition",
            "film-burn",
            "cube",
            "glitch-transition",
            "glitch-warp",
        ] {
            for held in [false, true] {
                for head in [0., 0.2, 1.] {
                    let (mut document, mut bindings) = input(&format!("concat.{name}"), head);
                    for clip in document["timelines"][0]["clips"].as_array_mut().unwrap() {
                        clip["keys"] = json!(properties
                            .iter()
                            .flat_map(|(property, first, last)| [
                                json!({"property":property,"at":0,"value":first,"ease":ease}),
                                json!({"property":property,"at":1,"value":last,"ease":ease})
                            ])
                            .collect::<Vec<_>>());
                    }
                    for source in bindings.values_mut() {
                        source.has_audio = true;
                        source.audio_channels = Some(2);
                    }
                    if held {
                        bindings.get_mut("a").unwrap().duration_ticks = 3000000;
                    }
                    let project = convert(document, &bindings);
                    let content = &project.compositions[&project.root].content;
                    let CompositionTrack::Video {
                        clips, transitions, ..
                    } = &content.tracks[0]
                    else {
                        panic!()
                    };
                    let paths = BTreeMap::from([
                        ("a".into(), "/portable/a.mov".into()),
                        ("b".into(), "/portable/b.mov".into()),
                    ]);
                    let output = export_with_paths(&project, &paths).unwrap();
                    let restored = if name == "dissolve" && held && head == 1. && !eased {
                        if let Ok(path) = std::env::var("CONCAT_EXPORTED_KEYED_TRANSITION_OUTPUT") {
                            std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
                        }
                        if let Ok(path) = std::env::var("CONCAT_REWRITTEN_KEYED_TRANSITION_INPUT") {
                            convert(
                                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),
                                &bindings,
                            )
                        } else {
                            convert(output, &bindings)
                        }
                    } else {
                        convert(output, &bindings)
                    };

                    assert_eq!(
                        content.tracks,
                        restored.compositions[&restored.root].content.tracks
                    );
                    let lead = head.min(0.5);
                    let mut expected_runs = vec![];
                    let mut clocks = vec![];
                    for index in 0..2 {
                        let (start, duration, source, fade_in, fade_out) = if index == 0 {
                            (
                                0.,
                                if held { 2. } else { 3. - lead },
                                1.,
                                0.,
                                if held { 0. } else { 1. },
                            )
                        } else {
                            (2. - lead, 2. + lead, head - lead, 1., 0.)
                        };
                        let mut runs = BTreeMap::new();
                        for (property_index, (property, first, last)) in
                            properties.iter().enumerate()
                        {
                            let mut keys = vec![];
                            for (at, value) in [(0., *first), (1., *last)] {
                                let expected_at =
                                    (at * 2. + if index == 1 { lead } else { 0. }) / duration;
                                let key = if let Some(rows) = &oracle {
                                    let row = rows
                                        .iter()
                                        .find(|row| {
                                            row[0] == -3.
                                                && row[1] == f64::from(held)
                                                && row[2] == head
                                                && row[3] == index as f64
                                                && row[4] == property_index as f64
                                                && row[6] == value
                                        })
                                        .unwrap();
                                    assert_eq!(
                                        &row[7..12],
                                        &[start, duration, source, fade_in, fade_out]
                                    );
                                    serde_json::from_value::<ParamKey>(
                                        json!({"at":row[5],"value":value,"ease":ease}),
                                    )
                                    .unwrap()
                                } else {
                                    serde_json::from_value(
                                        json!({"at":expected_at,"value":value,"ease":ease}),
                                    )
                                    .unwrap()
                                };
                                keys.push(key);
                            }
                            runs.insert(property.to_string(), keys);
                        }
                        expected_runs.push(runs);
                        clocks.push((start, duration, fade_in, fade_out));
                    }
                    let layers = plan_audio_tracks(&project, &project.root, 0..4000000).unwrap();
                    for time in [
                        400000u64, 1560000, 1960000, 2000000, 2200000, 2480000, 3200000, 3960000,
                    ] {
                        let mut poses = vec![];
                        for (index, clip) in clips.iter().enumerate() {
                            let mut runs = expected_runs[index].clone();
                            runs.remove("volume");
                            let (start, duration, _, _) = clocks[index];
                            let progress = if held && index == 0 && time >= 2000000 {
                                1.
                            } else {
                                (time as f64 / 1e6 - (start * 25.).round() / 25.)
                                    / ((duration * 25.).round() / 25.)
                            };
                            let pose = if held && index == 0 && time >= 2000000 {
                                Pose {
                                    scale_x: 1.1,
                                    scale_y: 1.1,
                                    x: 0.25 * 16.,
                                    y: -0.1 * 16.,
                                    rotation_degrees: 65.,
                                    opacity: 0.9,
                                }
                            } else {
                                ClipAnimation::new(&runs)
                                    .unwrap()
                                    .sample_progress(
                                        progress,
                                        clip.transform.concat_animation.as_ref().unwrap().rest,
                                        [1., 1.],
                                        [16., 16.],
                                    )
                                    .unwrap()
                            };
                            let actual =
                            video_kadr_backend::domain::concat_project_transitions::source_pose(
                                clip,
                                clips,
                                transitions,
                                &content.canvas,
                                time as i128 * 254016,
                                held && index == 0 && time >= 2000000,
                            )
                            .unwrap()
                            .unwrap();
                            for (a, b) in [
                                actual.x,
                                actual.y,
                                actual.scale_x,
                                actual.scale_y,
                                actual.rotation_degrees,
                                actual.opacity,
                            ]
                            .into_iter()
                            .zip([
                                pose.x,
                                pose.y,
                                pose.scale_x,
                                pose.scale_y,
                                pose.rotation_degrees,
                                pose.opacity,
                            ]) {
                                assert!(
                                    (a - b).abs() < 1e-12,
                                    "source pose {name}/{held}/{head}/{time}: {a}/{b}"
                                );
                            }
                            poses.push(pose);
                        }
                        let mut frozen = serde_json::to_value(&project).unwrap();
                        for (index, clip) in frozen["compositions"][&project.root]["content"]
                            ["tracks"][0]["clips"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .enumerate()
                        {
                            let pose = poses[index];
                            clip["transform"]
                                .as_object_mut()
                                .unwrap()
                                .remove("concatAnimation");
                            for (channel, value) in [
                                ("x", pose.x),
                                ("y", pose.y),
                                ("scaleX", pose.scale_x),
                                ("scaleY", pose.scale_y),
                                ("rotationDegrees", pose.rotation_degrees),
                            ] {
                                clip["transform"][channel] =
                                    json!({"mode":"constant","value":value});
                            }
                            clip["opacity"] = json!({"mode":"constant","value":pose.opacity});
                        }
                        let frozen = serde_json::from_value::<
                        video_kadr_backend::domain::nested_composition::NestedCompositionProject,
                    >(frozen)
                    .unwrap();
                        let mut decoder =
                            |id: &video_kadr_backend::domain::composition::SourceId,
                             clock: u64,
                             _| {
                                Ok(GroupSurface {
                                    width: 16,
                                    height: 16,
                                    rgba: if id.as_str() == "a" {
                                        [(clock / 40000 % 180) as u8, 30, 90, 255].repeat(256)
                                    } else {
                                        [90, 30, (clock / 40000 % 180) as u8, 255].repeat(256)
                                    },
                                })
                            };
                        let actual = flat_surface::render_media_layers(
                            content,
                            time,
                            &mut || Ok(()),
                            None,
                            Some(&mut decoder),
                        )
                        .unwrap();
                        let expected = flat_surface::render_media_layers(
                            &frozen.compositions[&frozen.root].content,
                            time,
                            &mut || Ok(()),
                            None,
                            Some(&mut decoder),
                        )
                        .unwrap();
                        assert_eq!(
                            actual, expected,
                            "original keyed render {name}/{held}/{head}/{time}"
                        );
                        let actual = mix_block_with_video_transitions(
                            &layers,
                            time * 48000 / 1000000,
                            1,
                            48000,
                            &AtomicBool::new(false),
                            |id, _| Ok((2, [if id.as_str() == "a" { 0.25 } else { -0.125 }; 2])),
                        )
                        .unwrap();
                        let mut expected = 0.;
                        for index in 0..2 {
                            let (start, duration, fade_in, fade_out) = clocks[index];
                            let local = time as f64 / 1e6 - start;
                            if local >= 0. && local < duration {
                                let gain =
                                video_kadr_backend::domain::concat_animation::PreparedTrack::new(
                                    &expected_runs[index]["volume"],
                                )
                                .unwrap()
                                .value_at(local / duration, 1.)
                                .unwrap();
                                let fi = if fade_in == 0. {
                                    1.
                                } else {
                                    (local / fade_in).min(1.)
                                };
                                let fo = if fade_out == 0. {
                                    1.
                                } else {
                                    ((duration - local) / fade_out).min(1.)
                                };
                                expected += gain * fi * fo * if index == 0 { 0.25 } else { -0.125 };
                            }
                        }
                        assert!(
                            (actual[0][0] - expected).abs() < 1e-12,
                            "source keyed audio {name}/{held}/{head}/{time}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn source_filter_keys_follow_extended_windows_and_restart_on_held_copies() {
    use video_kadr_backend::domain::{
        concat_animation::PreparedTrack,
        concat_effect_upgrade::{native_effect, AppliedFilter},
        flat_surface,
        group_surface::GroupSurface,
    };
    let scenes = vec![
        json!([{"id":"concat.exposure","params":{"stops":0},"keys":{"stops":[{"at":0,"value":-0.5},{"at":1,"value":1.5}]}},{"id":"concat.contrast","params":{"amount":20}}]),
        json!([{"id":"concat.saturation","params":{"amount":0},"keys":{"amount":[{"at":0,"value":-20},{"at":1,"value":40}]}},{"id":"concat.white-balance","params":{"temperature":6500}}]),
        json!([{"id":"concat.gaussian-blur","params":{"radius":1},"keys":{"radius":[{"at":0,"value":1},{"at":1,"value":3}]}}]),
        json!([{"id":"concat.fisheye","params":{"strength":20},"keys":{"strength":[{"at":0,"value":20},{"at":1,"value":60}]}}]),
        json!([{"id":"concat.film-grain","params":{"amount":12},"keys":{"amount":[{"at":0,"value":2},{"at":1,"value":30}]}},{"id":"concat.film"}]),
        json!([{"id":"concat.color-wheels","keys":{"lift.x":[{"at":0,"value":-0.1},{"at":1,"value":0.2}],"gamma.m":[{"at":0,"value":0},{"at":1,"value":0.2}]}}]),
        json!([{"id":"concat.adjust","params":{"brightness":10},"keys":{"lift.x":[{"at":0,"value":-0.1},{"at":1,"value":0.2}]}}]),
        json!([{"id":"concat.glow","params":{"amount":10},"keys":{"amount":[{"at":0,"value":10},{"at":1,"value":20}]}},{"id":"concat.halftone","params":{"size":6},"keys":{"size":[{"at":0,"value":6},{"at":1,"value":10}]}}]),
    ];
    let oracle = std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE")
        .ok()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(|line| {
                    line.split_whitespace()
                        .map(|v| v.parse::<f64>().unwrap())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        });
    for name in [
        "dissolve",
        "fade-black",
        "fade-white",
        "linear-wipe",
        "clock-wipe",
        "iris",
        "shape-wipe",
        "push",
        "slide",
        "rgb-split-wipe",
        "spin",
        "swirl",
        "blur-dissolve",
        "whip-pan",
        "zoom-transition",
        "light-leak-flash",
        "light-leak-transition",
        "zoom-blur-transition",
        "film-burn",
        "cube",
        "glitch-transition",
        "glitch-warp",
    ] {
        for held in [false, true] {
            for head in [0., 1.] {
                for (scene_index, effects) in scenes.iter().enumerate() {
                    let (mut document, mut bindings) = input(&format!("concat.{name}"), head);
                    for clip in document["timelines"][0]["clips"].as_array_mut().unwrap() {
                        clip["videoEffects"] = effects.clone();
                    }
                    if held {
                        bindings.get_mut("a").unwrap().duration_ticks = 3000000;
                    }
                    let project = convert(document, &bindings);
                    let content = &project.compositions[&project.root].content;
                    let CompositionTrack::Video {
                        clips, transitions, ..
                    } = &content.tracks[0]
                    else {
                        panic!()
                    };
                    let before = serde_json::to_value(&project).unwrap();
                    video_kadr_backend::domain::transition_export::validate(content).unwrap();
                    let paths = BTreeMap::from([
                        ("a".into(), "/portable/a.mov".into()),
                        ("b".into(), "/portable/b.mov".into()),
                    ]);
                    let output = export_with_paths(&project, &paths).unwrap();
                    let restored = if name == "dissolve" && held && head == 1. && scene_index == 0 {
                        if let Ok(path) = std::env::var("CONCAT_EXPORTED_FILTER_TRANSITION_OUTPUT")
                        {
                            std::fs::write(path, serde_json::to_vec(&output).unwrap()).unwrap();
                        }
                        if let Ok(path) = std::env::var("CONCAT_REWRITTEN_FILTER_TRANSITION_INPUT")
                        {
                            convert(
                                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),
                                &bindings,
                            )
                        } else {
                            convert(output, &bindings)
                        }
                    } else {
                        convert(output, &bindings)
                    };

                    assert_eq!(
                        content.tracks,
                        restored.compositions[&restored.root].content.tracks
                    );
                    for time in [400000u64, 1960000, 2040000, 2480000, 3200000] {
                        let mut reference = before.clone();
                        for (index, clip) in clips.iter().enumerate() {
                            let held_copy = held && index == 0 && time >= 2000000;
                            let lead = head.min(0.5);
                            let (mut start, mut duration) = if index == 0 {
                                if held_copy {
                                    (2., 1. - lead)
                                } else {
                                    (0., if held { 2. } else { 3. - lead })
                                }
                            } else {
                                (2. - lead, 2. + lead)
                            };
                            if let Some(rows) = &oracle {
                                if !held_copy {
                                    let row = rows
                                        .iter()
                                        .find(|row| {
                                            row[0] == -3.
                                                && row[1] == f64::from(held)
                                                && row[2] == head
                                                && row[3] == index as f64
                                        })
                                        .unwrap();
                                    start = row[7];
                                    duration = row[8];
                                }
                            }
                            let start_frame = (start * 25.).round();
                            let duration_frames = (duration * 25.).round();
                            let progress = ((time as f64 / 40000. - start_frame) / duration_frames)
                                .clamp(0., 1.);
                            let clock=video_kadr_backend::domain::concat_project_transitions::effect_clock(clip,clips,transitions,&content.canvas,time as i128*254016,held_copy).unwrap().unwrap();
                            assert!((clock.progress() - progress).abs() < 1e-14);
                            assert_eq!(clock.timeline_tick, time as i64 * 254016);
                            let mut frozen = vec![];
                            for entry in effects.as_array().unwrap() {
                                let mut link: AppliedFilter =
                                    serde_json::from_value(entry.clone()).unwrap();
                                for (name, keys) in &link.keys {
                                    let value = PreparedTrack::new(keys)
                                        .unwrap()
                                        .value_at(
                                            progress,
                                            link.params.get(name).copied().unwrap_or(0.),
                                        )
                                        .unwrap();
                                    link.params.insert(name.clone(), value);
                                }
                                link.keys.clear();
                                frozen.push(
                                    serde_json::to_value(native_effect(&link).unwrap().unwrap())
                                        .unwrap(),
                                );
                            }
                            reference["compositions"][&project.root]["content"]["tracks"][0]
                                ["clips"][index]["effects"] = json!(frozen);
                        }
                        let reference=serde_json::from_value::<video_kadr_backend::domain::nested_composition::NestedCompositionProject>(reference).unwrap();
                        let mut decode =
                            |id: &video_kadr_backend::domain::composition::SourceId,
                             clock: u64,
                             _| {
                                let mut rgba = vec![];
                                for y in 0..16 {
                                    for x in 0..16 {
                                        rgba.extend_from_slice(&[
                                            if id.as_str() == "a" {
                                                (clock / 40000 % 160) as u8 + 40
                                            } else {
                                                (x * 12) as u8
                                            },
                                            (y * 13) as u8,
                                            if id.as_str() == "a" {
                                                (x * 10) as u8
                                            } else {
                                                (clock / 40000 % 160) as u8 + 20
                                            },
                                            255,
                                        ]);
                                    }
                                }
                                Ok(GroupSurface {
                                    width: 16,
                                    height: 16,
                                    rgba,
                                })
                            };
                        let actual = flat_surface::render_media_layers(
                            content,
                            time,
                            &mut || Ok(()),
                            None,
                            Some(&mut decode),
                        )
                        .unwrap();
                        let expected = flat_surface::render_media_layers(
                            &reference.compositions[&reference.root].content,
                            time,
                            &mut || Ok(()),
                            None,
                            Some(&mut decode),
                        )
                        .unwrap();
                        assert_eq!(
                            actual, expected,
                            "source filter clock {name}/{held}/{head}/{scene_index}/{time}"
                        );
                    }
                    assert_eq!(serde_json::to_value(&project).unwrap(), before);
                }
            }
        }
    }
}

#[test]
fn source_transition_filters_preserve_bypass_and_refuse_unqualified_native_effects() {
    let (mut document, bindings) = input("concat.dissolve", 1.);
    document["timelines"][0]["clips"][0]["videoEffects"] = json!([{"id":"unknown.source.package","enabled":false,"params":{"at":0.1,"value":0.2}},{"id":"concat.exposure","enabled":false,"params":{"stops":2}}]);
    let project = convert(document, &bindings);
    video_kadr_backend::domain::transition_export::validate(
        &project.compositions[&project.root].content,
    )
    .unwrap();
    let mut changed = serde_json::to_value(&project).unwrap();
    changed["compositions"][&project.root]["content"]["tracks"][0]["clips"][0]["effects"] =
        json!([{"kind":"noise","parameters":{"amount":10}}]);
    let changed = serde_json::from_value::<
        video_kadr_backend::domain::nested_composition::NestedCompositionProject,
    >(changed)
    .unwrap();
    assert!(changed.dependency_order().is_err());
}

#[test]
fn source_transition_stages_preserve_lower_stack_and_force_incoming_layer_opacity() {
    use video_kadr_backend::domain::{flat_surface, group_surface::GroupSurface};
    let (mut document, bindings) = input("concat.dissolve", 1.);
    document["timelines"][0]["tracks"] = json!([{"id":"under"},{"id":"v"}]);
    let clips = document["timelines"][0]["clips"].as_array_mut().unwrap();
    clips[0]["opacity"] = json!(0.5);
    clips[1]["opacity"] = json!(0.25);
    clips[1]["offsetX"] = json!(0.5);
    clips.push(json!({"id":"under","trackId":"under","kind":"shape","start":0,"duration":4,"shape":{"kind":"square","size":1,"fill":"#00FF00","strokeWidth":0}}));
    let project = convert(document, &bindings);
    let content = &project.compositions[&project.root].content;
    let mut decoder = |id: &video_kadr_backend::domain::composition::SourceId, _, _| {
        Ok(GroupSurface {
            width: 16,
            height: 16,
            rgba: if id.as_str() == "a" {
                [255, 0, 0, 255].repeat(256)
            } else {
                [0, 0, 255, 255].repeat(256)
            },
        })
    };
    let before = flat_surface::render_media_layers(
        content,
        1000000,
        &mut || Ok(()),
        None,
        Some(&mut decoder),
    )
    .unwrap()
    .unwrap();
    let frame = flat_surface::render_media_layers(
        content,
        2000000,
        &mut || Ok(()),
        None,
        Some(&mut decoder),
    )
    .unwrap()
    .unwrap();
    let left = (8 * 16 + 4) * 4;
    let right = (8 * 16 + 12) * 4;
    assert_eq!(
        &frame.rgba[left..left + 4],
        &before.rgba[left..left + 4],
        "incoming image holes preserve outgoing stack"
    );
    let display = |v: f64| {
        v.powf(1. / 2.4)
    };
    for (channel, value) in [0.26, 0.26, 0.48].into_iter().enumerate() {
        let expected = (display(value) * 255.).round() as i16;
        assert!(
            (frame.rgba[right + channel] as i16 - expected).abs() <= 1,
            "original source stage channel {channel}: {}/{}",
            frame.rgba[right + channel],
            expected
        );
    }
    assert_eq!(frame.rgba[right + 3], 255);
}

#[test]
fn source_lane_population_matches_original_selector_and_separate_frame_rounding() {
    use video_kadr_backend::domain::{concat_project_transitions::visual_layers,flat_surface,group_surface::GroupSurface};
    let oracle=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap());
    let rows=oracle.as_ref().map(|text|text.lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-5.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(!rows.is_empty(),"original lane oracle must contain selector records");}
    for held in [false,true]{for head in [0.,1.]{for requested in [0.9,1.,1.1]{
        let(mut document,mut bindings)=input("concat.dissolve",head);
        document["timelines"][0]["clips"][1]["transitionIn"]["duration"]=json!(requested);
        document["timelines"][0]["clips"][0]["opacity"]=json!(0.5);
        if held{bindings.get_mut("a").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        for frame in 35..82{
            let exact=frame as i128*10160640000;
            let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
            if let Some(rows)=&rows{
                let expected=rows.iter().filter(|row|row[1]==f64::from(held)&&row[2]==head&&row[3]==requested&&row[4]==frame as f64).collect::<Vec<_>>();
                assert_eq!(layers.len(),expected.len(),"population {held}/{head}/{requested}/{frame}");
                for(layer,row)in layers.iter().zip(expected){
                    assert_eq!((layer.lane,layer.clip_index,layer.held,layer.source_tick),(row[5]as usize,row[6]as usize,row[7]==1.,row[8]as i128*10160640000),"original lane {held}/{head}/{requested}/{frame}");
                }
            }
        }
        // The raw hold is appended on the outgoing lane and replaces any
        // original clip whose separately quantized end overlaps its start.
        let layers=visual_layers(clips,transitions,&content.canvas,50*10160640000).unwrap().unwrap();
        assert_eq!(layers.iter().filter(|layer|layer.lane==0).count(),1);
        assert_eq!(layers.iter().find(|layer|layer.lane==0).unwrap().held,held);
        // Check real stage pixels after the source shader's lifetime. Incoming
        // geometry has a hole, so a surviving extended outgoing layer is visible.
        let(mut document,mut bindings)=input("concat.dissolve",head);
        document["timelines"][0]["clips"][1]["transitionIn"]["duration"]=json!(requested);
        document["timelines"][0]["clips"][1]["offsetX"]=json!(0.5);
        if held{bindings.get_mut("a").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        let(start,duration)=transitions[0].timeline.unwrap().source_span().unwrap();
        let end=start as i128+duration as i128;
        let plan=visual_layers(clips,transitions,&content.canvas,end).unwrap().unwrap();
        let expected_red=plan.iter().any(|layer|layer.lane==0);
        let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:if id.as_str()=="a"{[255,0,0,255].repeat(256)}else{[0,0,255,255].repeat(256)}});
        let frame=flat_surface::render_media_layers(content,(end/254016)as u64,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
        let at=(8*16+4)*4;assert_eq!(frame.rgba[at],if expected_red{255}else{0},"source tail visibility {held}/{head}/{requested}");
    }}}
}

#[test]
fn source_chained_lane_selection_keeps_document_order_and_authorizes_actual_layers() {
    use video_kadr_backend::domain::{concat_project_transitions::visual_layers,flat_surface,group_surface::GroupSurface};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-6.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(!rows.is_empty(),"original chain lane records required");}
    for reversed in [false,true]{for held in [false,true]{
        let(mut document,mut bindings)=input("concat.dissolve",1.);
        document["media"].as_array_mut().unwrap().push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));
        let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();
        clips.push(json!({"id":"tail","trackId":"v","kind":"video","mediaId":"c","start":4,"duration":6,"sourceStart":2,"transitionIn":{"id":"concat.dissolve","duration":3}}));
        if reversed{clips.swap(1,2);}
        bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":20000000,"width":16,"height":16,"hasAudio":false})).unwrap());
        if held{bindings.get_mut("b").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        for frame in 35..170{
            let layers=visual_layers(clips,transitions,&content.canvas,frame as i128*10160640000).unwrap().unwrap();
            if let Some(rows)=&rows{
                let expected=rows.iter().filter(|row|row[1]==f64::from(reversed)&&row[2]==f64::from(held)&&row[3]==frame as f64).collect::<Vec<_>>();
                assert_eq!(layers.len(),expected.len(),"chain population {reversed}/{held}/{frame}");
                for(layer,row)in layers.iter().zip(expected){
                    let canonical=["a","b","c"].iter().position(|id|*id==clips[layer.clip_index].source_id.as_str()).unwrap();
                    assert_eq!((layer.lane,canonical,layer.held,layer.source_tick),(row[4]as usize,row[5]as usize,row[6]==1.,row[7]as i128*10160640000),"chain source oracle {reversed}/{held}/{frame}");
                }
            }
            let expected=layers.iter().map(|layer|clips[layer.clip_index].source_id.clone()).collect::<std::collections::BTreeSet<_>>();
            let time=frame*40000;
            assert_eq!(project.frame_image_sources(&project.root,time,&Default::default()).unwrap(),expected,"focused source authorization {reversed}/{held}/{frame}");
            let range=project.range_image_sources(&project.root,time..time+1,&Default::default()).unwrap();
            assert!(expected.is_subset(&range),"range authorization includes every decoded layer");
        }
        if reversed&&!held{
            // Original source order a,c,b leaves b as the last active picture
            // on lane one during the second shader. It replaces c on that lane.
            let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:match id.as_str(){"a"=>[255,0,0,255],"b"=>[0,0,255,255],_=>[255,255,255,255]}.repeat(256)});
            let frame=flat_surface::render_media_layers(content,3500000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            assert_eq!(&frame.rgba[0..2],&[0,0]);
            let expected=(0.25f64.powf(1./2.4)*255.).round()as i16;
            assert!((frame.rgba[2]as i16-expected).abs()<=1,"source last-added lane pixel");
        }
    }}
}

#[test]
fn overlapping_source_shaders_follow_first_span_priority_and_original_stages() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{flat_surface,group_surface::GroupSurface,nested_audio::plan_audio_tracks,nested_pcm::mix_block_with_video_transitions};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|matches!(row[0] as i32,-7|-8|-9|-10)).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(rows.iter().any(|row|row[0]==-7.)&&rows.iter().any(|row|row[0]==-9.),"original overlapping span and stage records required");}
    let mut overlapping_frames=0;
    for reversed in [false,true]{for held in [false,true]{
        let(mut document,mut bindings)=input("concat.fade-black",1.);
        document["media"].as_array_mut().unwrap().push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));
        let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();
        clips[1]["duration"]=json!(0.4);clips[1]["transitionIn"]["duration"]=json!(2.);
        clips.push(json!({"id":"tail","trackId":"v","kind":"video","mediaId":"c","start":2.4,"duration":2,"sourceStart":1,"transitionIn":{"id":"concat.dissolve","duration":2}}));
        if reversed{clips.swap(1,2);}
        bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2})).unwrap());
        for source in bindings.values_mut(){source.has_audio=true;source.audio_channels=Some(2);}
        if held{bindings.get_mut("b").unwrap().duration_ticks=1440000;}
        let mut keyed_document=document.clone();
        for clip in keyed_document["timelines"][0]["clips"].as_array_mut().unwrap(){if clip["mediaId"]=="b"{clip["keys"]=json!([{"property":"opacity","at":0,"value":0.2,"ease":"linear"},{"property":"opacity","at":1,"value":0.8,"ease":"linear"}]);}}
        let keyed=convert(keyed_document,&bindings);
        let keyed_content=&keyed.compositions[&keyed.root].content;
        let CompositionTrack::Video{clips:keyed_clips,transitions:keyed_transitions,..}=&keyed_content.tracks[0]else{panic!()};
        let keyed_clip=keyed_clips.iter().find(|clip|clip.source_id.as_str()=="b").unwrap();
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();
        let content=&project.compositions[&project.root].content;
        video_kadr_backend::domain::transition_export::validate(content).unwrap();
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into()),("c".into(),"/portable/c.mov".into())])).unwrap();
        let restored=convert(output.clone(),&bindings);
        assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        if reversed&&!held{
            if let Ok(path)=std::env::var("CONCAT_EXPORTED_OVERLAPPING_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
            if let Ok(path)=std::env::var("CONCAT_REWRITTEN_OVERLAPPING_INPUT"){
                let original=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);
                assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&original.compositions[&original.root].content.tracks).unwrap());
            }
        }
        let audio=plan_audio_tracks(&project,&project.root,0..4400000).unwrap();
        assert!(audio.iter().all(|layer|layer.video_transitions.is_none()),"source audio retains linear clip fades through shader overlap");
        for frame in 35..82{
            let time=frame*40000;let exact=time as i128*254016;
            let active=transitions.iter().filter(|t|{let(start,duration)=t.timeline.unwrap().source_span().unwrap();exact>=start as i128&&exact<start as i128+duration as i128}).collect::<Vec<_>>();
            if active.len()>1{overlapping_frames+=1;}
            let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:match id.as_str(){"a"=>[255,0,0,255],"b"=>[0,255,0,255],_=>[0,0,255,255]}.repeat(256)});
            let actual=flat_surface::render_media_layers(content,time,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            if let Some(rows)=&rows{
                let chosen=rows.iter().find(|row|row[0]==-7.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)&&row[3]==frame as f64).unwrap();
                if chosen[4]<0.{assert!(active.is_empty());}else{
                    let transition=active[0];let id=transition.to_clip_id.as_ref().unwrap();
                    let canonical=if clips.iter().find(|clip|&clip.id==id).unwrap().source_id.as_str()=="b"{1.}else{2.};assert_eq!(canonical,chosen[4]);
                    let(start,duration)=transition.timeline.unwrap().source_span().unwrap();assert_eq!((start,duration),(chosen[5]as i64*10160640000,(chosen[6]-chosen[5])as i64*10160640000));
                }
                for row in rows.iter().filter(|row|row[0]==-8.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)){
                    let clip=clips.iter().find(|clip|clip.source_id.as_str()==["a","b","c"][row[3]as usize]).unwrap();
                    let clock=video_kadr_backend::domain::concat_project_transitions::effect_clock(clip,clips,transitions,&content.canvas,exact,false).unwrap().unwrap();
                    assert_eq!((clock.start_tick,clock.duration_tick),(((row[4]*25.).round()as i64)*10160640000,((row[5]*25.).round()as i64)*10160640000),"raw sequential filter window");
                }
                let window=rows.iter().find(|row|row[0]==-8.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)&&row[3]==1.).unwrap();
                let keys=rows.iter().filter(|row|row[0]==-10.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)).collect::<Vec<_>>();assert_eq!(keys.len(),2);
                let progress=(frame as f64-(window[4]*25.).round())/(window[5]*25.).round();
                let expected=keys[0][4]+(keys[1][4]-keys[0][4])*((progress-keys[0][3])/(keys[1][3]-keys[0][3])).clamp(0.,1.);
                let pose=video_kadr_backend::domain::concat_project_transitions::source_pose(keyed_clip,keyed_clips,keyed_transitions,&keyed_content.canvas,exact,false).unwrap().unwrap();
                assert!((pose.opacity-expected).abs()<1e-12,"source keyed overlap clock {reversed}/{held}/{frame}: {}/{}",pose.opacity,expected);
                // The stage memberships and incoming opacity overrides below
                // come from the unchanged original transition_stages function.
                let compose=|mut base:[f64;3],stage:f64|{for row in rows.iter().filter(|row|row[0]==-9.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)&&row[3]==frame as f64&&row[4]==stage){let alpha=row[7];for channel in 0..3{base[channel]=base[channel]*(1.-alpha)+if channel==row[5]as usize{alpha}else{0.};}}base};
                let mut expected=if chosen[4]<0.{compose([0.;3],3.)}else{
                    let from=compose([0.;3],0.);let to=compose([0.;3],1.);let progress=(frame as f64-chosen[5])/(chosen[6]-chosen[5]);
                    if chosen[4]==1.{let base=if progress>=0.5{to}else{from};base.map(|v|v*(2.*progress-1.).abs().powf(2.4))}else{std::array::from_fn(|k|from[k]*(1.-progress)+to[k]*progress)}
                };
                if chosen[4]>=0.{expected=compose(expected,2.);}
                for pixel in actual.rgba.chunks_exact(4){for channel in 0..3{let want=(expected[channel].clamp(0.,1.).powf(1./2.4)*255.).round()as i16;assert!((pixel[channel]as i16-want).abs()<=1,"original overlap stage {reversed}/{held}/{frame}/{channel}: {}/{}",pixel[channel],want);}assert_eq!(pixel[3],255);}
                let samples=mix_block_with_video_transitions(&audio,time*48000/1000000,16,48000,&AtomicBool::new(false),|id,_|Ok((2,[match id.as_str(){"a"=>0.25,"b"=>-0.125,_=>0.5};2]))).unwrap();
                for(index,sample)in samples.iter().enumerate(){let tick=time+index as u64*1000000/48000;let mut expected=0.;for row in rows.iter().filter(|row|row[0]==-8.&&row[1]==f64::from(reversed)&&row[2]==f64::from(held)){let begin=(row[4]*1e6).round()as u64;let duration=(row[5]*1e6).round()as u64;if tick<begin||tick>=begin+duration{continue;}let local=(tick-begin)as f64;let gain_in=if row[7]>0.{(local/(row[7]*1e6)).min(1.)}else{1.};let gain_out=if row[8]>0.{((duration as f64-local)/(row[8]*1e6)).min(1.)}else{1.};expected+=[0.25,-0.125,0.5][row[3]as usize]*gain_in*gain_out;}
                    assert!((sample[0]-expected).abs()<1e-8&&sample[0]==sample[1],"original overlap sound {reversed}/{held}/{frame}/{index}: {}/{expected}",sample[0]);}
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
    }}
    assert!(overlapping_frames>0,"qualification must actually enter simultaneous shader windows");
}

#[test]
fn live_source_treatments_select_dissolve_fallback_even_at_zero_strength() {
    use video_kadr_backend::domain::{flat_surface,group_surface::GroupSurface,concat_treatment};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-11.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert_eq!(rows.len(),360,"original treatment clock/filter gate records required");}
    for above in [false,true]{for mode in 0..10{
        let(mut document,bindings)=input("concat.fade-black",1.);
        document["timelines"][0]["tracks"]=if above{json!([{"id":"v"},{"id":"look","visible":mode!=6}])}else{json!([{"id":"look","visible":mode!=6},{"id":"v"}])};
        let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();
        clips[0]["opacity"]=json!(0.5);clips[1]["opacity"]=json!(0.25);
        if mode!=0{
            let effects=if mode==5{json!([])}else{json!([{"id":if mode==8{"unknown.plugin"}else{"concat.exposure"},"enabled":mode!=4&&mode!=8,"params":{"stops":if mode==2{2}else{0}}}])};
            clips.push(json!({"id":"look","trackId":"look","kind":"layer","start":1.53,"duration":if mode==9{0.01}else{0.9},"opacity":if mode==2{0.}else if mode==3{0.8}else{1.},"fadeIn":if mode==3{1.}else{0.},"fadeOut":if mode==3{0.4}else{0.},"videoEffects":effects}));
        }
        let mut project=convert(document,&bindings);
        if mode==7{for track in &mut project.compositions.get_mut(&project.root.clone()).unwrap().content.tracks{if let CompositionTrack::Graphic{clips,..}=track{clips[0].enabled=false;}}}
        let content=&project.compositions[&project.root].content;
        let before=serde_json::to_value(&project).unwrap();
        for frame in 35..71{
            let exact=frame as i128*10160640000;let time=frame*40000;
            let live=matches!(mode,1|2|3)&&frame>=38&&frame<61;
            let row=rows.as_ref().map(|rows|rows.iter().find(|row|row[1]==mode as f64&&row[2]==frame as f64).unwrap());
            if let Some(row)=row{assert_eq!(live,row[3]==1.);}
            for track in &content.tracks{if let CompositionTrack::Graphic{clips,hidden,..}=track{let clip=&clips[0];if let Some(clock)=concat_treatment::sample_clock(clip,&content.canvas,exact).unwrap(){
                let treatment=clip.concat_treatment.as_ref().unwrap();
                let visible=!*hidden&&concat_treatment::has_shader(&clip.effects);
                assert_eq!(visible,live);
                if visible{if let Some(row)=row{assert!((concat_treatment::sample_strength(treatment,clock).unwrap()-row[4]).abs()<1e-7);assert!((clock.progress()-row[5]).abs()<1e-13);}}
            }else{assert!(!live);}}}
            let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:if id.as_str()=="a"{[255,0,0,255].repeat(256)}else{[0,0,255,255].repeat(256)}});
            let actual=flat_surface::render_media_layers(content,time,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            let expected=if frame<38{[0.5,0.,0.]}else if frame>=63{[0.,0.,0.25]}else{
                let progress=(frame as f64-38.)/25.;
                if live{[0.5*(1.-0.25*progress),0.,0.25*progress]}else{let fade=(2.*progress-1.).abs().powf(2.4);if progress>=0.5{[0.,0.,fade]}else{[0.5*fade,0.,0.]}}
            };
            for pixel in actual.rgba.chunks_exact(4){for channel in 0..3{let expected=(expected[channel].powf(1./2.4)*255.).round()as i16;assert!((pixel[channel]as i16-expected).abs()<=1,"source treatment fallback {above}/{mode}/{frame}/{channel}: {}/{expected}",pixel[channel]);}assert_eq!(pixel[3],255);}
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
        if mode!=7{
            let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())])).unwrap();
            let restored=convert(output.clone(),&bindings);
            assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
            if above&&mode==3{
                if let Ok(path)=std::env::var("CONCAT_EXPORTED_TREATED_TRANSITION_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
                if let Ok(path)=std::env::var("CONCAT_REWRITTEN_TREATED_TRANSITION_INPUT"){
                    let source=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);
                    assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&source.compositions[&source.root].content.tracks).unwrap());
                }
            }
        }
    }}
}

#[test]
fn legacy_cross_fade_keeps_source_opacity_and_never_uses_packaged_shader() {
    use video_kadr_backend::domain::{concat_project_transitions::{visual_layers,SourceLegacy},flat_surface,group_surface::GroupSurface};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-12.||row[0]==-13.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(rows.iter().any(|row|row[0]==-13.),"original full legacy resolver records required");}
    for held in [false,true]{for head in [0.,0.2,1.]{for requested in [0.9,1.,1.1]{
        let(mut document,mut bindings)=input("cross-fade",head);
        document["timelines"][0]["clips"][0]["opacity"]=json!(0.5);
        document["timelines"][0]["clips"][1]["opacity"]=json!(0.25);
        document["timelines"][0]["clips"][1]["transitionIn"]["duration"]=json!(requested);
        if held{bindings.get_mut("a").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();
        let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        assert_eq!(transitions[0].concat_overlap.unwrap().legacy,Some(SourceLegacy::CrossFade));
        video_kadr_backend::domain::transition_export::validate(content).unwrap();
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())])).unwrap();
        assert_eq!(output["timelines"][0]["clips"][1]["transitionIn"]["id"],"cross-fade");
        let restored=convert(output.clone(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        if held&&head==1.&&requested==1.{
            if let Ok(path)=std::env::var("CONCAT_EXPORTED_LEGACY_CROSS_FADE_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
            if let Ok(path)=std::env::var("CONCAT_REWRITTEN_LEGACY_CROSS_FADE_INPUT"){
                let original=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);
                assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&original.compositions[&original.root].content.tracks).unwrap());
            }
            let mut forged=before.clone();forged["compositions"][&project.root]["content"]["tracks"][0]["transitions"][0]["kind"]=json!("concat_fade_black");
            let forged:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(forged).unwrap();assert!(forged.dependency_order().is_err(),"legacy ownership cannot be attached to another kernel");
        }
        for frame in 35..88{
            let exact=frame as i128*10160640000;let time=frame*40000;
            let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
            let mut reads=vec![];
            let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,time:u64,_|{reads.push((id.as_str().to_owned(),time));Ok(GroupSurface{width:16,height:16,rgba:if id.as_str()=="a"{[255,0,0,255].repeat(256)}else{[0,0,255,255].repeat(256)}})};
            let actual=flat_surface::render_media_layers(content,time,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            if let Some(rows)=&rows{
                let expected=rows.iter().filter(|row|row[0]==-13.&&row[1]==f64::from(held)&&row[2]==head&&row[3]==requested&&row[4]==frame as f64).collect::<Vec<_>>();
                assert_eq!(layers.len(),expected.len());
                let mut colour=[0.;3];let mut clocks=vec![];
                for(layer,row)in layers.iter().zip(expected){
                    assert_eq!((layer.lane,layer.clip_index,layer.held,layer.source_tick),(row[5]as usize,row[6]as usize,row[7]==1.,row[8]as i128*10160640000));
                    clocks.push((["a","b"][row[6]as usize].to_owned(),row[8]as u64*40000));
                    let alpha=row[9];for channel in 0..3{colour[channel]=colour[channel]*(1.-alpha)+if channel==if row[6]==0.{0}else{2}{alpha}else{0.};}
                }
                assert_eq!(reads,clocks,"legacy source reads {held}/{head}/{requested}/{frame}");
                for pixel in actual.rgba.chunks_exact(4){for channel in 0..3{let expected=(colour[channel].powf(1./2.4)*255.).round()as i16;assert!((pixel[channel]as i16-expected).abs()<=1,"original legacy opacity {held}/{head}/{requested}/{frame}/{channel}: {}/{expected}",pixel[channel]);}assert_eq!(pixel[3],255);}
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
    }}}
}

#[test]
fn legacy_push_zoom_match_original_generated_and_conflicting_key_rides() {
    use video_kadr_backend::domain::concat_project_transitions::{visual_layers,source_pose};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-14.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(!rows.is_empty(),"original legacy resolver/engine pose records required");}
    for(kind,name,property)in [(0,"push","offsetX"),(1,"zoom","scale")]{for held in [false,true]{for head in [0.,1.]{for keyed in 0..3{for rest in 0..3{
        let(mut document,mut bindings)=input(name,head);
        for clip in document["timelines"][0]["clips"].as_array_mut().unwrap(){
            clip["scale"]=json!(if rest==2{8.}else if rest==1{1.7}else{1.});clip["offsetX"]=json!(if rest==2{-0.4}else if rest==1{0.2}else{0.});clip["opacity"]=json!(0.65);
            if rest>0{clip["stretchX"]=json!(1.2);clip["stretchY"]=json!(0.8);}
        }
        if keyed>0{document["timelines"][0]["clips"][keyed-1]["keys"]=json!([{"property":property,"at":0,"value":if property=="scale"{0.8}else{-0.1},"ease":"linear"},{"property":property,"at":1,"value":if property=="scale"{1.2}else{0.2},"ease":"linear"}]);}
        if held{bindings.get_mut("a").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        assert!(clips.iter().all(|clip|clip.transform.concat_animation.is_some()),"static source rests must survive native ownership");
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())])).unwrap();
        assert_eq!(output["timelines"][0]["clips"][1]["transitionIn"]["id"],name);
        let restored=convert(output.clone(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        if !held&&head==1.&&keyed==0&&rest==1{
            let variable=if kind==0{"CONCAT_EXPORTED_LEGACY_PUSH_OUTPUT"}else{"CONCAT_EXPORTED_LEGACY_ZOOM_OUTPUT"};
            if let Ok(path)=std::env::var(variable){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
            let variable=if kind==0{"CONCAT_REWRITTEN_LEGACY_PUSH_INPUT"}else{"CONCAT_REWRITTEN_LEGACY_ZOOM_INPUT"};
            if let Ok(path)=std::env::var(variable){let original=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&original.compositions[&original.root].content.tracks).unwrap());}
        }
        for frame in [35,38,40,45,50,53,55,60,63,70,85,100,108,120,130]{
            let exact=frame as i128*10160640000;let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
            if let Some(rows)=&rows{
                let expected=rows.iter().filter(|row|row[1]==kind as f64&&row[2]==f64::from(held)&&row[3]==head&&row[4]==keyed as f64&&row[5]==rest as f64&&row[6]==frame as f64).collect::<Vec<_>>();
                assert_eq!(layers.len(),expected.len(),"legacy scene population {name}/{held}/{head}/{keyed}/{rest}/{frame}");
                for(layer,row)in layers.iter().zip(expected){
                    assert_eq!((layer.lane,layer.clip_index,layer.held,layer.source_tick),(row[7]as usize,row[8]as usize,row[9]==1.,row[16]as i128*10160640000));
                    assert!((layer.ramp as f64-row[15]).abs()<1e-7);
                    let pose=source_pose(&clips[layer.clip_index],clips,transitions,&content.canvas,exact,layer.held).unwrap().unwrap();
                    for(actual,expected)in [pose.scale_x,pose.scale_y,pose.x,pose.y,pose.opacity].into_iter().zip(&row[10..15]){
                        assert!((actual-expected).abs()<1e-7,"original legacy pose {name}/{held}/{head}/{keyed}/{rest}/{frame}: {actual}/{expected}");
                    }
                }
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
    }}}}}
}

#[test]
fn legacy_motion_chains_preserve_prior_rides_and_creation_time_held_pose() {
    use video_kadr_backend::domain::concat_project_transitions::{visual_layers,source_pose};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-15.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert!(!rows.is_empty(),"original motion chain pose records required");}
    for first in 0..2{for second in 0..2{for reversed in [false,true]{for held in [false,true]{
        let(mut document,mut bindings)=input(if first==0{"push"}else{"zoom"},1.);
        document["media"].as_array_mut().unwrap().push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));
        let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();clips.push(json!({"id":"tail","trackId":"v","mediaId":"c","kind":"video","start":4,"duration":2,"sourceStart":1,"transitionIn":{"id":if second==0{"push"}else{"zoom"},"duration":1}}));
        for clip in clips.iter_mut(){clip["scale"]=json!(1.7);clip["offsetX"]=json!(0.2);clip["opacity"]=json!(0.65);clip["stretchX"]=json!(1.2);clip["stretchY"]=json!(0.8);}
        if reversed{clips.swap(1,2);}
        bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false})).unwrap());
        if held{bindings.get_mut("b").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into()),("c".into(),"/portable/c.mov".into())])).unwrap();
        let restored=convert(output,&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        for frame in [35,38,40,45,50,53,55,60,63,70,85,100,108,120,130]{
            let exact=frame as i128*10160640000;let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
            if let Some(rows)=&rows{
                let expected=rows.iter().filter(|row|row[1]==first as f64&&row[2]==second as f64&&row[3]==f64::from(reversed)&&row[4]==f64::from(held)&&row[5]==frame as f64).collect::<Vec<_>>();assert_eq!(layers.len(),expected.len());
                for(layer,row)in layers.iter().zip(expected){let index=["a","b","c"].iter().position(|id|*id==clips[layer.clip_index].source_id.as_str()).unwrap();
                    assert_eq!((layer.lane,index,layer.held,layer.source_tick),(row[6]as usize,row[7]as usize,row[8]==1.,row[15]as i128*10160640000));assert!((layer.ramp as f64-row[14]).abs()<1e-7);
                    let pose=source_pose(&clips[layer.clip_index],clips,transitions,&content.canvas,exact,layer.held).unwrap().unwrap();
                    for(actual,expected)in [pose.scale_x,pose.scale_y,pose.x,pose.y,pose.opacity].into_iter().zip(&row[9..14]){assert!((actual-expected).abs()<1e-7,"original chained pose {first}/{second}/{reversed}/{held}/{frame}: {actual}/{expected}");}
                }
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
    }}}}
}

#[test]
fn legacy_wipes_use_original_first_frame_and_picture_space_edges() {
    use video_kadr_backend::domain::{concat_project_transitions::source_wipe,flat_surface,group_surface::GroupSurface};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-16.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert_eq!(rows.len(),1696,"original wipe resolver/shading records required");}
    for(direction,name)in [(0,"wipe-left"),(1,"wipe-right")]{for held in [false,true]{for head in [0.,1.]{for requested in [0.9,1.1]{for keyed in [false,true]{
      for(geometry,(crop,flip,rotation,offset))in [(false,false,0,0.),(true,false,0,0.25),(true,true,90,0.),(false,true,90,-0.25)].into_iter().enumerate(){
        let(mut document,mut bindings)=input(name,head);let authored=document["timelines"][0]["clips"].as_array_mut().unwrap();
        authored[0]["opacity"]=json!(0.5);authored[1]["opacity"]=json!(0.25);authored[1]["transitionIn"]["duration"]=json!(requested);
        authored[1]["offsetX"]=json!(offset);authored[1]["rotation"]=json!(rotation);authored[1]["flipH"]=json!(flip);
        if crop{authored[1]["crop"]=json!({"left":0.25,"right":0.25});}
        if keyed{authored[1]["keys"]=json!([{"property":"opacity","at":0,"value":0.2,"ease":"linear"},{"property":"opacity","at":1,"value":0.8,"ease":"linear"}]);}
        if held{bindings.get_mut("a").unwrap().duration_ticks=3000000;}
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};let incoming=clips.iter().find(|clip|clip.source_id.as_str()=="b").unwrap();
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())])).unwrap();
        assert_eq!(output["timelines"][0]["clips"][1]["transitionIn"]["id"],name);
        let restored=convert(output.clone(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        if held&&head==1.&&requested==1.1&&keyed&&geometry==2{
            let variable=if direction==0{"CONCAT_EXPORTED_LEGACY_WIPE_LEFT_OUTPUT"}else{"CONCAT_EXPORTED_LEGACY_WIPE_RIGHT_OUTPUT"};if let Ok(path)=std::env::var(variable){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
            let variable=if direction==0{"CONCAT_REWRITTEN_LEGACY_WIPE_LEFT_INPUT"}else{"CONCAT_REWRITTEN_LEGACY_WIPE_RIGHT_INPUT"};if let Ok(path)=std::env::var(variable){let original=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&original.compositions[&original.root].content.tracks).unwrap());}
        }
        for frame in 35..88{
            let exact=frame as i128*10160640000;let wipe=source_wipe(incoming,clips,transitions,&content.canvas,exact,false).unwrap();
            assert!(source_wipe(incoming,clips,transitions,&content.canvas,exact,true).unwrap().is_none(),"held copies clear original transition shapes");
            let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:if id.as_str()=="a"{[255,0,0,255].repeat(256)}else{(0..256).flat_map(|pixel|[0,(64+8*(pixel%16))as u8,255,255]).collect()}});
            let actual=flat_surface::render_media_layers(content,frame*40000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            if let Some(rows)=&rows{
                let row=rows.iter().find(|row|row[1]==direction as f64&&row[2]==f64::from(held)&&row[3]==head&&row[4]==requested&&row[5]==f64::from(keyed)&&row[6]==frame as f64).unwrap();
                assert_eq!(wipe.is_some(),row[13]==1.,"original wipe lifetime {name}/{held}/{head}/{requested}/{keyed}/{frame}");
                for probe in 0..18{let u=if probe<16{(probe as f32+0.5)/16.}else{(probe-16)as f32};assert_eq!(wipe.map_or(true,|wipe|wipe.keeps(u)),(row[12]as u32&(1<<probe))!=0,"original Shading::keeps boundary {name}/{frame}/{probe}");}
                // Independent source reference geometry at integer quarter turns:
                // inverse placement first, then the clip-local wipe and crop/flip.
                let width=if crop{8.}else{16.};let centre_x=8.+offset*16.;
                for y in 0..16{for x in 0..16{
                    let mut expected=[if row[14]==1.{0.5}else{0.},0.,0.];
                    let(dx,dy)=(x as f64+0.5-centre_x,y as f64+0.5-8.);
                    let(px,py)=if rotation==0{(dx+width/2.,dy+8.)}else{(dy+width/2.,-dx+8.)};
                    let u=(px/width)as f32;
                    if row[15]==1.&&px>=0.&&px<=width&&py>=0.&&py<=16.&&(u as f64)<row[9]&&(u as f64)>=row[10]{
                        let column=(px-0.5).round()as usize;let column=if flip{width as usize-1-column}else{column};let source_column=column+if crop{4}else{0};
                        let colour=[0.,((64+8*source_column)as f64/255.).powf(2.4),1.];let alpha=row[11];for channel in 0..3{expected[channel]=expected[channel]*(1.-alpha)+colour[channel]*alpha;}
                    }
                    let at=(y*16+x)*4;for channel in 0..3{let expected=(expected[channel].powf(1./2.4)*255.).round()as i16;assert!((actual.rgba[at+channel]as i16-expected).abs()<=1,"original picture wipe {name}/{held}/{head}/{requested}/{keyed}/{geometry}/{frame}/{x}/{y}/{channel}: {}/{expected}",actual.rgba[at+channel]);}assert_eq!(actual.rgba[at+3],255);
                }}
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
      }
    }}}}}
}

#[test]
fn legacy_colour_fades_match_original_linear_light_caps_and_leave_audio_unextended() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::domain::{concat_project_transitions::source_colour,flat_surface,group_surface::GroupSurface,nested_audio::plan_audio_tracks,nested_pcm::mix_block_with_video_transitions};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-17.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert_eq!(rows.len(),3552,"original colour resolver/shading records required");}
    for white in [false,true]{for requested in [0.01,0.9,1.1,10.]{for short in [false,true]{for head in [0.,1.]{
        let name=if white{"fade-white"}else{"fade-black"};let length=if short{0.4}else{2.};let(mut document,mut bindings)=input(name,head);
        document["timelines"][0]["tracks"]=json!([{"id":"under"},{"id":"v"}]);let authored=document["timelines"][0]["clips"].as_array_mut().unwrap();
        authored[0]["duration"]=json!(length);authored[0]["opacity"]=json!(0.5);authored[1]["start"]=json!(length);authored[1]["opacity"]=json!(0.25);authored[1]["transitionIn"]["duration"]=json!(requested);
        authored.push(json!({"id":"under","trackId":"under","kind":"shape","start":0,"duration":4.4,"shape":{"kind":"square","size":1,"fill":"#00FF00","strokeWidth":0}}));
        bindings.get_mut("a").unwrap().duration_ticks=((1.+length)*1e6)as u64;for source in bindings.values_mut(){source.has_audio=true;source.audio_channels=Some(2);}
        let project=convert(document,&bindings);let before=serde_json::to_value(&project).unwrap();let content=&project.compositions[&project.root].content;let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
        let marker=transitions[0].concat_overlap.unwrap();assert_eq!(marker.lead_ticks,0);assert!(!marker.hold_outgoing);assert_eq!(marker.duration_ticks,(requested*1e6)as u64);
        let layers=plan_audio_tracks(&project,&project.root,0..4400000).unwrap();assert_eq!(layers.len(),2);for layer in &layers{let original=clips.iter().find(|clip|clip.id==layer.clip.id).unwrap();assert_eq!(layer.clip.placement,original.placement);assert_eq!((layer.clip.fade_in_ticks,layer.clip.fade_out_ticks),(0,0));assert!(layer.video_transitions.is_none());}
        if requested==10.&&!short{let expected=std::collections::BTreeSet::from([video_kadr_backend::domain::composition::SourceId::parse("b").unwrap()]);assert_eq!(project.range_image_sources(&project.root,3500000..3600000,&Default::default()).unwrap(),expected);}
        let output=export_with_paths(&project,&BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())])).unwrap();
        let restored=convert(output.clone(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        if requested==1.1&&!short&&head==1.{let variable=if white{"CONCAT_EXPORTED_LEGACY_FADE_WHITE_OUTPUT"}else{"CONCAT_EXPORTED_LEGACY_FADE_BLACK_OUTPUT"};if let Ok(path)=std::env::var(variable){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}let variable=if white{"CONCAT_REWRITTEN_LEGACY_FADE_WHITE_INPUT"}else{"CONCAT_REWRITTEN_LEGACY_FADE_BLACK_INPUT"};if let Ok(path)=std::env::var(variable){let original=convert(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&original.compositions[&original.root].content.tracks).unwrap());}}
        for frame in 0..111{
            let exact=frame as i128*10160640000;let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:if id.as_str()=="a"{[255,0,0,255].repeat(256)}else{[0,0,255,255].repeat(256)}});
            let actual=flat_surface::render_media_layers(content,frame*40000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();
            if let Some(rows)=&rows{let row=rows.iter().find(|row|row[1]==f64::from(white)&&row[2]==requested&&row[3]==f64::from(short)&&row[4]==head&&row[5]==frame as f64).unwrap();let mut expected=[0.,if frame<110{1.}else{0.},0.];
                if row[6]>=0.{let clip=&clips[row[6]as usize];let shade=source_colour(clip,clips,transitions,&content.canvas,exact,false).unwrap();let(scale,offset)=shade.map_or((1.,0.),|shade|(shade.scale,shade.offset));assert!((scale as f64-row[9]).abs()<1e-7&&(offset as f64-row[10]).abs()<1e-7);assert!(source_colour(clip,clips,transitions,&content.canvas,exact,true).unwrap().is_none());let input=if row[6]==0.{[1.,0.,0.]}else{[0.,0.,1.]};for channel in 0..3{expected[channel]=expected[channel]*(1.-row[11])+(input[channel]*row[9]+row[10])*row[11];}}
                for pixel in actual.rgba.chunks_exact(4){for channel in 0..3{let expected=(expected[channel].powf(1./2.4)*255.).round()as i16;assert!((pixel[channel]as i16-expected).abs()<=1,"original per-picture colour {white}/{requested}/{short}/{head}/{frame}/{channel}: {}/{expected}",pixel[channel]);}assert_eq!(pixel[3],255);}
                let samples=mix_block_with_video_transitions(&layers,frame*1920,1,48000,&AtomicBool::new(false),|id,_|Ok((2,[if id.as_str()=="a"{0.25}else{-0.125};2]))).unwrap();let sound=if row[6]==0.{0.25}else if row[6]==1.{-0.125}else{0.};assert_eq!(samples,vec![[sound;2]],"colour fades must not crossfade sound");
            }
        }
        assert_eq!(serde_json::to_value(&project).unwrap(),before);
    }}}}
}

#[test]
fn legacy_colour_shapes_fold_in_document_order_and_do_not_raise_packaged_lanes() {
    use video_kadr_backend::domain::{flat_surface,group_surface::GroupSurface,concat_project_transitions::transition_lane};
    let rows=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").ok().map(|path|std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-18.||row[0]==-19.).collect::<Vec<_>>());
    if let Some(rows)=&rows{assert_eq!(rows.iter().filter(|row|row[0]==-18.).count(),888);}
    let scene=|first:&str,second:&str,reversed:bool|{
        let(mut document,mut bindings)=input(first,1.);document["media"].as_array_mut().unwrap().push(json!({"id":"c","kind":"video","path":"/missing/c.mov"}));document["timelines"][0]["tracks"]=json!([{"id":"under"},{"id":"v"}]);
        let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();clips[0]["opacity"]=json!(0.5);clips[1]["opacity"]=json!(0.25);clips[1]["duration"]=json!(0.4);clips[1]["transitionIn"]["duration"]=json!(2);
        clips.push(json!({"id":"tail","trackId":"v","kind":"video","mediaId":"c","start":2.4,"duration":2,"sourceStart":1,"opacity":0.75,"transitionIn":{"id":second,"duration":2}}));if reversed{clips.swap(1,2);}clips.push(json!({"id":"under","trackId":"under","kind":"shape","start":0,"duration":4.4,"shape":{"kind":"square","size":1,"fill":"#00FF00","strokeWidth":0}}));
        bindings.insert("c".into(),serde_json::from_value(json!({"id":"c","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false})).unwrap());(document,bindings)
    };
    for first in [false,true]{for second in [false,true]{for reversed in [false,true]{let(document,bindings)=scene(if first{"fade-white"}else{"fade-black"},if second{"fade-white"}else{"fade-black"},reversed);let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;
        for frame in 0..111{let mut decode=|id:&video_kadr_backend::domain::composition::SourceId,_,_|Ok(GroupSurface{width:16,height:16,rgba:match id.as_str(){"a"=>[255,0,0,255],"b"=>[0,0,255,255],_=>[255,0,255,255]}.repeat(256)});let actual=flat_surface::render_media_layers(content,frame*40000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();if let Some(rows)=&rows{let row=rows.iter().find(|row|row[0]==-18.&&row[1]==f64::from(first)&&row[2]==f64::from(second)&&row[3]==f64::from(reversed)&&row[4]==frame as f64).unwrap();let mut expected=[0.,if frame<110{1.}else{0.},0.];if row[5]>=0.{let input=match row[5]as usize{0=>[1.,0.,0.],1=>[0.,0.,1.],_=>[1.,0.,1.]};for channel in 0..3{expected[channel]=expected[channel]*(1.-row[8])+(input[channel]*row[6]+row[7])*row[8];}}for pixel in actual.rgba.chunks_exact(4){for channel in 0..3{let want=(expected[channel].powf(1./2.4)*255.).round()as i16;assert!((pixel[channel]as i16-want).abs()<=1,"ordered colour shapes {first}/{second}/{reversed}/{frame}: {}/{want}",pixel[channel]);}}}}
    }}}
    for colour_first in [false,true]{let(mut document,bindings)=scene(if colour_first{"fade-white"}else{"concat.dissolve"},if colour_first{"concat.dissolve"}else{"fade-black"},false);for clip in document["timelines"][0]["clips"].as_array_mut().unwrap(){if clip.get("transitionIn").is_some(){clip["transitionIn"]["duration"]=json!(1);}}
        let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;let CompositionTrack::Video{transitions,..}=&content.tracks[0]else{panic!()};let package=transitions.iter().find(|t|t.concat_overlap.unwrap().legacy.is_none()).unwrap();if let Some(rows)=&rows{let row=rows.iter().find(|row|row[0]==-19.&&row[1]==f64::from(colour_first)).unwrap();assert_eq!(transition_lane(transitions,package).unwrap(),row[2]as usize);assert_eq!(package.timeline.unwrap().source_span().unwrap(),(row[3]as i64*10160640000,(row[4]-row[3])as i64*10160640000));}
    }
}

#[test]
fn retimed_colour_preserves_authored_duration_and_six_decimal_source_rate() {
    use video_kadr_backend::domain::concat_project_transitions::visual_layers;
    for name in ["fade-black","fade-white"] {
        for speed in [0.123456789_f64,0.5,1.234567891,2.] {
            let (mut document,bindings)=input(name,1.);
            for clip in document["timelines"][0]["clips"].as_array_mut().unwrap(){clip["speed"]=json!(speed);}
            let project=convert(document,&bindings);
            let content=&project.compositions[&project.root].content;
            let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0] else{panic!()};
            for clip in clips {assert_eq!(clip.placement.timeline_duration_ticks().unwrap(),2_000_000);}
            for frame in 0..100i128 {
                let layers=visual_layers(clips,transitions,&content.canvas,frame*10_160_640_000).unwrap().unwrap();
                assert_eq!(layers.len(),1);
                let local=frame%50;
                // Original Rational::approximate(speed), then source_start + local * speed.
                let numerator=(speed*1_000_000.).round() as i128;
                assert_eq!(layers[0].source_tick,254_016_000_000+local*10_160_640_000*numerator/1_000_000);
            }
            let paths=BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())]);
            let restored=convert(export_with_paths(&project,&paths).unwrap(),&bindings);
            assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        }
    }
}

#[test]
#[ignore = "requires generated unchanged Concat resolver/selector oracle"]
fn offgrid_colour_windows_match_original_selector_source_and_shading() {
    use video_kadr_backend::domain::concat_project_transitions::{visual_layers,source_colour};
    let path=std::env::var("CONCAT_TRANSITION_PLACEMENT_ORACLE").expect("run scripts/qualify-concat-transition-placement.py first");
    let rows=std::fs::read_to_string(path).unwrap().lines().map(|line|line.split_whitespace().map(|v|v.parse::<f64>().unwrap()).collect::<Vec<_>>()).filter(|row|row[0]==-20.).collect::<Vec<_>>();
    assert_eq!(rows.len(),2*4*7*3*105);
    for white in [false,true]{for start in [0.,0.019,0.02,0.021]{for length in [0.001,0.019,0.02,0.021,2.019,2.02,2.021]{for source_start in [0.019,0.02,0.021]{
        let(mut document,bindings)=input(if white{"fade-white"}else{"fade-black"},source_start);
        let authored=document["timelines"][0]["clips"].as_array_mut().unwrap();
        authored[0]["start"]=json!(start);authored[0]["duration"]=json!(length);authored[0]["sourceStart"]=json!(source_start);authored[1]["start"]=json!(start+length);
        let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;
        let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0] else{panic!()};
        assert_eq!(clips[0].placement.timeline_start_tick,(start*1e6_f64).round()as u64);
        assert_eq!(clips[0].placement.timeline_duration_ticks().unwrap(),(length*1e6_f64).round()as u64);
        for row in rows.iter().filter(|r|r[1]==white as u8 as f64&&r[2]==start&&r[3]==length&&r[4]==source_start){
            let exact=row[5]as i128*10_160_640_000;
            let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
            if row[6]<0.{assert!(layers.is_empty(),"{row:?}");continue;}
            assert_eq!(layers.len(),1,"{row:?}");let layer=&layers[0];assert_eq!(layer.clip_index,row[6]as usize,"{row:?}");assert_eq!(layer.source_tick,row[7]as i128*10_160_640_000,"{row:?}");
            let colour=source_colour(&clips[layer.clip_index],clips,transitions,&content.canvas,exact,false).unwrap();
            let(scale,offset)=colour.map(|c|(c.scale,c.offset)).unwrap_or((1.,0.));assert!((scale as f64-row[8]).abs()<1e-6,"{row:?}");assert!((offset as f64-row[9]).abs()<1e-6,"{row:?}");
        }
        let paths=BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())]);
        let restored=convert(export_with_paths(&project,&paths).unwrap(),&bindings);
        assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
    }}}}
}

#[test]
fn touching_cut_roundoff_does_not_admit_real_microsecond_overlap() {
    let(mut document,bindings)=input("fade-black",1.);
    document["timelines"][0]["clips"][0]["start"]=json!(0.021);
    document["timelines"][0]["clips"][0]["duration"]=json!(2.02);
    document["timelines"][0]["clips"][1]["start"]=json!(2.040999);
    let report=import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings})).unwrap()).unwrap();
    assert_eq!(report.status,"needs_attention");assert!(report.project.is_none());
    assert!(report.diagnostics.iter().any(|d|d.contains("overlapping visual clips")),"{:?}",report.diagnostics);
}

#[test]
#[ignore = "requires unchanged Concat Rational/FrameRate/quantise oracle"]
fn fractional_rate_half_ties_follow_original_float_operation_order() {
    use video_kadr_backend::domain::concat_project_transitions::visual_layers;
    let path=std::env::var("CONCAT_FRAME_CLOCK_ORACLE").expect("run scripts/qualify-concat-frame-clock.py first");
    let records=std::fs::read_to_string(path).unwrap();let rows=records.lines().collect::<Vec<_>>();assert_eq!(rows.len(),54);
    for row in rows {
        let fields=row.split_whitespace().collect::<Vec<_>>();let num=fields[0].parse::<u64>().unwrap();let den=fields[1].parse::<u64>().unwrap();let seconds=fields[2].parse::<f64>().unwrap();let quantised=fields[3].parse::<i128>().unwrap();let cut=fields[4].parse::<i128>().unwrap();let duration=fields[5].parse::<i128>().unwrap();
        let unit=254_016_000_000i128*den as i128/num as i128;
        for family in ["fade-black","fade-white"] {
            let(mut document,bindings)=input(family,seconds);document["timelines"][0]["video"]["rateNum"]=json!(num);document["timelines"][0]["video"]["rateDen"]=json!(den);
            let authored=document["timelines"][0]["clips"].as_array_mut().unwrap();authored[0]["start"]=json!(seconds);authored[0]["duration"]=json!(seconds);authored[0]["sourceStart"]=json!(seconds);authored[1]["start"]=json!(seconds+seconds);
            let project=convert(document,&bindings);let content=&project.compositions[&project.root].content;let CompositionTrack::Video{clips,transitions,..}=&content.tracks[0]else{panic!()};
            for frame in 0..500i128 {
                let exact=frame*unit;
                let expected=if exact>=cut&&exact<cut+duration{Some((1,quantised+exact-cut))}else if exact>=quantised&&exact<quantised*2{Some((0,exact))}else{None};
                let layers=visual_layers(clips,transitions,&content.canvas,exact).unwrap().unwrap();
                if let Some((index,source))=expected{assert_eq!(layers.len(),1,"{family}/{row}/{frame}");assert_eq!(layers[0].clip_index,index,"{family}/{row}/{frame}");assert_eq!(layers[0].source_tick,source,"{family}/{row}/{frame}");}else{assert!(layers.is_empty(),"{family}/{row}/{frame}");}
            }
            let paths=BTreeMap::from([("a".into(),"/portable/a.mov".into()),("b".into(),"/portable/b.mov".into())]);let restored=convert(export_with_paths(&project,&paths).unwrap(),&bindings);assert_eq!(serde_json::to_value(&content.tracks).unwrap(),serde_json::to_value(&restored.compositions[&restored.root].content.tracks).unwrap());
        }
    }
}

#[test]
fn retained_curve_is_persisted_but_not_admitted_before_audio_integration(){
    let(document,bindings)=input("fade-black",1.);let mut project=convert(document,&bindings);
    let content=&mut project.compositions.get_mut(&project.root).unwrap().content;
    let CompositionTrack::Video{clips,..}=&mut content.tracks[0]else{panic!()};
    clips[0].placement.concat_retime=Some(video_kadr_backend::domain::concat_retime::SourceClock{points:vec![(0.,1.),(1.,3.)],duration_ticks:2000000,rate_num:25,rate_den:1,document_clock:None});
    clips[0].placement.source_out_tick=clips[0].placement.source_in_tick+4000000;
    assert!(content.validate_native_surface().is_err());
    let wire=serde_json::to_value(&project).unwrap();
    let restored:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(wire.clone()).unwrap();assert_eq!(serde_json::to_value(restored).unwrap(),wire);
}

#[test]
fn source_gain_keys_override_zero_constant_without_discarding_audio_layer(){
    let(mut document,mut bindings)=input("fade-black",1.);
    for source in bindings.values_mut(){source.has_audio=true;source.audio_channels=Some(2);}
    document["timelines"][0]["clips"][0]["volume"]=json!(0);
    document["timelines"][0]["clips"][0]["keys"]=json!([{"property":"volume","at":0,"value":0.2},{"property":"volume","at":1,"value":0.8}]);
    let project=convert(document,&bindings);
    let layers=video_kadr_backend::domain::nested_audio::plan_audio_tracks(&project,&project.root,0..1000000).unwrap();
    assert_eq!(layers.len(),1,"source gain keys supersede the zero scalar volume");
    let sample=layers[0].sample(500000).unwrap().unwrap();assert!((sample.gain-0.35).abs()<1e-12);
    let mut silent=project.clone();let CompositionTrack::Video{clips,..}=&mut silent.compositions.get_mut(&silent.root).unwrap().content.tracks[0]else{panic!()};clips[0].concat_gain_keys.clear();
    assert!(video_kadr_backend::domain::nested_audio::plan_audio_tracks(&silent,&silent.root,0..1000000).unwrap().is_empty());
}
