use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceSequence,
    filmcraft_stereo_graph::{SourceStereoGraph, SourceStereoGraphRuntime},
    filmcraft_surround_route::{SourceSurroundRoute, SourceSurroundSend},
};
const T: i64 = 254_016_000_000;
fn base() -> Value {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-stereo-graph.json")).unwrap();
    let mut doc = oracle["cases"][0]["document"].clone();
    let s = sequence(&mut doc);
    s["settings"]["audio_master"] = json!("Surround51");
    s["settings"]["sample_rate"] = json!(48000);
    s["master_volume_db"] = json!(0.);
    s["master_effects"] = json!([]);
    s["master_mixer"] = json!({"mode":"Read"});
    for key in ["audio_tracks", "submix_tracks"] {
        for tr in s[key].as_array_mut().unwrap() {
            tr["channels"] = json!("Surround51");
            tr["items"] = json!([]);
            tr["effects"] = json!([]);
            tr["transitions"] = json!([]);
            tr["pan"] = json!(0.);
            tr["volume_db"] = json!(0.);
            tr["muted"] = json!(false);
            tr["solo"] = json!(false);
            tr["mixer"] = json!({"mode":"Read","input_map":"Stereo","output":null,"sends":[]});
        }
    }
    doc
}
fn sequence(doc: &mut Value) -> &mut Value {
    &mut doc["project"]["items"]["5"]["kind"]["Sequence"]
}
fn graph(doc: &Value) -> SourceStereoGraph {
    let doc = SourceProjectDocument::read(&serde_json::to_vec(doc).unwrap()).unwrap();
    let index = ProjectIndex::new(&doc).unwrap();
    let sequence = SourceSequence::read(&index, 5).unwrap();
    let graph = SourceStereoGraph::compile(&sequence).unwrap();
    serde_json::from_value(serde_json::to_value(graph).unwrap()).unwrap()
}
fn signal(channels: usize, start: i64, n: usize, id: u64) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|c| {
            (0..n)
                .map(|i| {
                    let x = start + i as i64;
                    if x < 0 {
                        0.
                    } else {
                        ((x * 13 + c as i64 * 7 + id as i64) % 47) as f32 / 128. - 0.125
                    }
                })
                .collect()
        })
        .collect()
}
fn bits(v: &[Vec<f32>]) -> Vec<Vec<u32>> {
    v.iter()
        .map(|c| c.iter().map(|s| s.to_bits()).collect())
        .collect()
}
fn add(a: &mut [Vec<f32>], b: &[Vec<f32>]) {
    for (a, b) in a.iter_mut().zip(b) {
        for (x, y) in a.iter_mut().zip(b) {
            *x += y;
        }
    }
}

#[test]
fn mixed_stereo_and_surround_tracks_sum_into_six_channel_master() {
    let mut doc = base();
    sequence(&mut doc)["audio_tracks"][1]["channels"] = json!("Stereo");
    let graph = graph(&doc);
    assert_eq!(graph.channels(), 6);
    assert!(graph.uses_surround());
    let a = signal(6, 17, 97, 10);
    let b = signal(2, 17, 97, 11);
    let mut expected = a.clone();
    for c in 0..2 {
        for i in 0..97 {
            expected[c][i] += b[c][i];
        }
    }
    let out = graph
        .render(17, 97, 48000, |id| {
            Ok(if id == 10 { a.clone() } else { b.clone() })
        })
        .unwrap();
    assert_eq!(bits(&out), bits(&expected));
    let mut runtime = SourceStereoGraphRuntime::new(graph, 48000).unwrap();
    let out = runtime
        .render(17, 97, |id, start, n| {
            Ok(signal(if id == 10 { 6 } else { 2 }, start, n, id))
        })
        .unwrap();
    assert_eq!(bits(&out), bits(&expected));
}

#[test]
fn mixed_width_submixes_and_prefader_sends_use_original_route_matrices() {
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][1]["muted"] = json!(true);
    s["audio_tracks"][0]["mixer"]["output"] = json!(100);
    s["audio_tracks"][0]["mixer"]["sends"] =
        json!([{"target":101,"pre_fader":true,"level_db":-6.,"pan":0.}]);
    s["submix_tracks"][0]["channels"] = json!("Stereo");
    let source = s["audio_tracks"][0].clone();
    let stereo_submix = s["submix_tracks"][0].clone();
    let surround_submix = s["submix_tracks"][1].clone();
    let input = signal(6, 0, 97, 10);
    let stereo = SourceSurroundRoute::read(&source)
        .unwrap()
        .output(&input, 2, 0, 48000)
        .unwrap();
    let mut expected = SourceSurroundRoute::read(&stereo_submix)
        .unwrap()
        .output(&stereo, 6, 0, 48000)
        .unwrap();
    let send = SourceSurroundSend::read(&source, 0)
        .unwrap()
        .unwrap()
        .render(&input, 6, 0, 48000)
        .unwrap();
    let sent = SourceSurroundRoute::read(&surround_submix)
        .unwrap()
        .output(&send, 6, 0, 48000)
        .unwrap();
    add(&mut expected, &sent);
    let graph = graph(&doc);
    let out = graph
        .render(0, 97, 48000, |id| {
            assert_eq!(id, 10);
            Ok(input.clone())
        })
        .unwrap();
    assert_eq!(bits(&out), bits(&expected));
    let mut runtime = SourceStereoGraphRuntime::new(graph, 48000).unwrap();
    assert_eq!(
        bits(
            &runtime
                .render(0, 97, |id, start, n| {
                    assert_eq!(id, 10);
                    Ok(signal(6, start, n, id))
                })
                .unwrap()
        ),
        bits(&expected)
    );
}

#[test]
fn six_channel_mute_preserves_prefader_sends_and_solo_closure() {
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][1]["muted"] = json!(true);
    s["audio_tracks"][0]["muted"] = json!(true);
    s["audio_tracks"][0]["volume_db"] = json!(-96.);
    s["audio_tracks"][0]["mixer"]["sends"] =
        json!([{"target":101,"pre_fader":true,"level_db":0.,"pan":0.}]);
    let a = signal(6, 0, 97, 10);
    let out = graph(&doc)
        .render(0, 97, 48000, |id| {
            assert_eq!(id, 10);
            Ok(a.clone())
        })
        .unwrap();
    assert_eq!(bits(&out), bits(&a));
    sequence(&mut doc)["audio_tracks"][0]["mixer"]["sends"][0]["pre_fader"] = json!(false);
    let out = graph(&doc)
        .render(0, 97, 48000, |_| {
            panic!("muted post-fader route must not read clips")
        })
        .unwrap();
    assert!(out.iter().flatten().all(|v| *v == 0.));
    let mut doc = base();
    sequence(&mut doc)["audio_tracks"][1]["solo"] = json!(true);
    let b = signal(6, 0, 97, 11);
    let out = graph(&doc)
        .render(0, 97, 48000, |id| {
            assert_eq!(id, 11);
            Ok(b.clone())
        })
        .unwrap();
    assert_eq!(bits(&out), bits(&b));
}

#[test]
fn six_channel_automation_and_input_mapping_follow_content_time() {
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][1]["muted"] = json!(true);
    let strip = &mut s["audio_tracks"][0];
    strip["mixer"]["input_map"] = json!("Left");
    strip["mixer"]["lanes"] = json!({
        "volume":{"value":{"Float":-20.},"keyframes":[{"time":0,"value":{"Float":-6.},"interp":"Linear"},{"time":32*(T/48000),"value":{"Float":0.},"interp":"Linear"}]},
        "mute":{"value":{"Bool":false},"keyframes":[{"time":0,"value":{"Bool":false}},{"time":16*(T/48000),"value":{"Bool":true}},{"time":32*(T/48000),"value":{"Bool":false}}]}
    });
    let mut runtime = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    let out = runtime
        .render(0, 64, |id, _, n| {
            assert_eq!(id, 10);
            Ok((0..6).map(|c| vec![(c + 1) as f32 / 32.; n]).collect())
        })
        .unwrap();
    for c in 0..6 {
        for i in 0..64 {
            let expected = if (16..32).contains(&i) {
                0.
            } else {
                let db = -6. + 6. * (i as f64 / 32.).clamp(0., 1.);
                (c + 1) as f32 / 32. * 10f64.powf(db / 20.) as f32
            };
            assert_eq!(
                out[c][i].to_bits(),
                expected.to_bits(),
                "automation {c}/{i}"
            );
        }
    }
}

fn impulse(id: u64, start: i64, n: usize) -> Vec<Vec<f32>> {
    (0..6)
        .map(|c| {
            (0..n)
                .map(|i| {
                    if [64, 160].contains(&(start + i as i64)) {
                        (c + 1) as f32 * if id == 10 { 0.01 } else { 0.02 }
                    } else {
                        0.
                    }
                })
                .collect()
        })
        .collect()
}
fn limiter() -> Value {
    json!({"effect":"hard_limiter","enabled":true,"params":{"max":{"value":{"Float":0.}},"boost":{"value":{"Float":0.}},"release":{"value":{"Float":100.}}}})
}
#[test]
fn six_channel_delay_compensation_survives_continuation_seek_and_failed_provider() {
    let mut doc = base();
    let mut baseline = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    sequence(&mut doc)["audio_tracks"][0]["effects"] = json!([limiter()]);
    let mut runtime = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    assert!(runtime.latency_samples() > 0);
    for (start, n) in [(0, 128), (128, 97), (-17, 97)] {
        let expected = baseline
            .render(start, n, |id, start, n| Ok(impulse(id, start, n)))
            .unwrap();
        let out = runtime
            .render(start, n, |id, start, n| Ok(impulse(id, start, n)))
            .unwrap();
        assert_eq!(bits(&out), bits(&expected), "PDC {start}");
    }
    assert!(runtime
        .render(0, 97, |_, _, n| Ok(vec![vec![0.; n]; 2]))
        .is_err());
    let expected = baseline
        .render(0, 97, |id, start, n| Ok(impulse(id, start, n)))
        .unwrap();
    let out = runtime
        .render(0, 97, |id, start, n| Ok(impulse(id, start, n)))
        .unwrap();
    assert_eq!(bits(&out), bits(&expected));
}

#[test]
fn mixed_target_width_compensation_preserves_all_master_channels() {
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][0]["mixer"]["output"] = json!(100);
    s["audio_tracks"][1]["mixer"]["output"] = json!(101);
    s["submix_tracks"][0]["channels"] = json!("Stereo");
    let mut baseline = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    sequence(&mut doc)["audio_tracks"][0]["effects"] = json!([limiter()]);
    let mut runtime = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    assert!(runtime.latency_samples() > 0);
    for (start, n) in [(0, 128), (128, 97), (-17, 97)] {
        let expected = baseline
            .render(start, n, |id, start, n| Ok(impulse(id, start, n)))
            .unwrap();
        let out = runtime
            .render(start, n, |id, start, n| Ok(impulse(id, start, n)))
            .unwrap();
        assert_eq!(bits(&out), bits(&expected), "mixed PDC {start}");
    }
}

#[test]
fn mono_fold_reaches_centre_and_legacy_graph_metadata_defaults_to_stereo() {
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][0]["channels"] = json!("Mono");
    s["audio_tracks"][1]["muted"] = json!(true);
    let input = signal(2, 17, 97, 10);
    let out = graph(&doc)
        .render(17, 97, 48000, |id| {
            assert_eq!(id, 10);
            Ok(input.clone())
        })
        .unwrap();
    for c in 0..6 {
        for i in 0..97 {
            let expected = if c == 2 {
                (input[0][i] + input[1][i]) * 0.5
            } else {
                0.
            };
            assert_eq!(out[c][i].to_bits(), expected.to_bits());
        }
    }
    let s = sequence(&mut doc);
    s["settings"]["audio_master"] = json!("Stereo");
    for key in ["audio_tracks", "submix_tracks"] {
        for strip in s[key].as_array_mut().unwrap() {
            strip["channels"] = json!("Stereo");
            strip["pan"] = json!(-37.);
        }
    }
    let current = graph(&doc);
    let mut saved = serde_json::to_value(&current).unwrap();
    saved.as_object_mut().unwrap().remove("master_width");
    for strip in saved["strips"].as_array_mut().unwrap() {
        strip.as_object_mut().unwrap().remove("width");
        strip.as_object_mut().unwrap().remove("output");
    }
    let legacy: SourceStereoGraph = serde_json::from_value(saved).unwrap();
    assert_eq!(legacy.channels(), 2);
    let expected = current
        .render(17, 97, 48000, |_| Ok(input.clone()))
        .unwrap();
    let actual = legacy.render(17, 97, 48000, |_| Ok(input.clone())).unwrap();
    assert_eq!(bits(&actual), bits(&expected));
}

#[test]
fn actual_six_channel_clip_runtime_feeds_surround_graph_without_reordering() {
    use video_kadr_backend::domain::filmcraft_clip_audio::{
        SourceTrackAudio, SourceTrackAudioRuntime,
    };
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-clip-audio-input.json")).unwrap();
    let mut clip = oracle["cases"][0]["track"]["items"][0].clone();
    clip["start"] = json!(0);
    clip["source_in"] = json!(0);
    clip["duration"] = json!(T * 2);
    clip["speed"] = json!(1.);
    clip["reverse"] = json!(false);
    clip["gain_db"] = json!(0.);
    clip["effects"] = json!([]);
    clip["source_channels"] = json!([]);
    let mut doc = base();
    let s = sequence(&mut doc);
    s["audio_tracks"][0]["items"] = json!([clip]);
    s["audio_tracks"][1]["muted"] = json!(true);
    let parsed = SourceProjectDocument::read(&serde_json::to_vec(&doc).unwrap()).unwrap();
    let index = ProjectIndex::new(&parsed).unwrap();
    let seq = SourceSequence::read(&index, 5).unwrap();
    let mut clips =
        SourceTrackAudioRuntime::new(SourceTrackAudio::compile(&seq.audio_tracks[0]).unwrap());
    let mut runtime = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    for (start, n) in [(17, 97), (114, 33), (-5, 97)] {
        let out = runtime
            .render(start, n, |id, start, n| {
                assert_eq!(id, 10);
                clips.render(start, n, 48000, |_, start, n, _| {
                    Ok(Some(signal(6, start, n, 10)))
                })
            })
            .unwrap();
        assert_eq!(
            bits(&out),
            bits(&signal(6, start, n, 10)),
            "clip/graph {start}"
        );
    }
}

#[test]
fn master_pre_and_post_insert_racks_process_all_six_channels() {
    let mut doc = base();
    sequence(&mut doc)["audio_tracks"][1]["muted"] = json!(true);
    sequence(&mut doc)["master_volume_db"] = json!(-6.);
    let mut baseline = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
    let expected = baseline
        .render(17, 97, |id, start, n| {
            assert_eq!(id, 10);
            Ok(signal(6, start, n, id))
        })
        .unwrap()
        .iter()
        .map(|c| c.iter().map(|s| -s).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for post in [false, true] {
        sequence(&mut doc)["master_effects"] =
            json!([{"effect":"invert_a","enabled":true,"post_fader":post,"params":{}}]);
        let mut runtime = SourceStereoGraphRuntime::new(graph(&doc), 48000).unwrap();
        let out = runtime
            .render(17, 97, |id, start, n| {
                assert_eq!(id, 10);
                Ok(signal(6, start, n, id))
            })
            .unwrap();
        assert_eq!(bits(&out), bits(&expected), "master inserts post={post}");
    }
}
