use filmcraft_audio_dsp::channels::{convert_owned, Layout, Mixdown};
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_project_audio::SourceProjectAudioRuntime,
    filmcraft_project_file::SourceProjectDocument, filmcraft_surround_route::SourceSurroundRoute,
};
const T: i64 = 254_016_000_000;
fn seq(doc: &mut Value, id: u64) -> &mut Value {
    &mut doc["project"]["items"][id.to_string()]["kind"]["Sequence"]
}
fn base() -> Value {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-project-audio.json")).unwrap();
    let mut doc = oracle["cases"][0]["document"].clone();
    for id in [7, 9] {
        let s = seq(&mut doc, id);
        s["settings"]["audio_master"] = json!("Surround51");
        s["settings"]["sample_rate"] = json!(48000);
        s["master_volume_db"] = json!(0.);
        s["master_effects"] = json!([]);
        s["master_mixer"] = json!({"mode":"Read"});
        s["submix_tracks"] = json!([]);
        s["multicam"] = Value::Null;
        s["audio_tracks"].as_array_mut().unwrap().truncate(1);
        let tr = &mut s["audio_tracks"][0];
        tr["channels"] = json!("Surround51");
        tr["volume_db"] = json!(0.);
        tr["pan"] = json!(0.);
        tr["muted"] = json!(false);
        tr["solo"] = json!(false);
        tr["effects"] = json!([]);
        tr["transitions"] = json!([]);
        tr["mixer"] = json!({"mode":"Read","input_map":"Stereo"});
        tr["items"].as_array_mut().unwrap().truncate(1);
        let clip = &mut tr["items"][0];
        clip["start"] = json!(0);
        clip["source_in"] = json!(0);
        clip["duration"] = json!(T * 2);
        clip["speed"] = json!(1.);
        clip["reverse"] = json!(false);
        clip["frame_hold"] = Value::Null;
        clip["gain_db"] = json!(0.);
        clip["effects"] = json!([]);
        clip["source_channels"] = json!([]);
    }
    let m = &mut doc["project"]["items"]["1"]["kind"]["Media"];
    m["media"] = json!({"File":{"path":"/untrusted/six.wav"}});
    m["info"]["kind"] = json!("AudioOnly");
    m["info"]["video"] = Value::Null;
    m["info"]["duration"] = json!(T * 2);
    m["info"]["audio"] =
        json!({"channels":6,"sample_rate":48000,"bits_per_sample":32,"codec":"pcm_f32le"});
    doc
}
fn runtime(doc: &Value) -> SourceProjectAudioRuntime {
    SourceProjectAudioRuntime::new(
        SourceProjectDocument::read(&serde_json::to_vec(doc).unwrap()).unwrap(),
    )
    .unwrap()
}
fn signal(c: usize, n: i64) -> f32 {
    if n < 0 {
        0.
    } else {
        (((n * 13 + c as i64 * 7) % 47) - 17) as f32 / 128.
    }
}
fn source(id: u64, start: i64, n: usize, _rate: u32) -> anyhow::Result<Option<Vec<Vec<f32>>>> {
    assert_eq!(id, 1);
    Ok(Some(
        (0..6)
            .map(|c| (0..n).map(|i| signal(c, start + i as i64)).collect())
            .collect(),
    ))
}
fn bits(v: &[Vec<f32>]) -> Vec<Vec<u32>> {
    v.iter()
        .map(|c| c.iter().map(|s| s.to_bits()).collect())
        .collect()
}
// Independent source nested_mix ordering: mix/convert at the child's rate,
// interpolate in parent sample coordinates, route parent, then convert export.
fn reference(doc: &Value, start: i64, n: usize, channels: usize) -> Vec<Vec<f32>> {
    let child = &doc["project"]["items"]["7"]["kind"]["Sequence"];
    let parent = &doc["project"]["items"]["9"]["kind"]["Sequence"];
    let own = child["settings"]["sample_rate"].as_u64().unwrap() as f64;
    let rate = parent["settings"]["sample_rate"].as_u64().unwrap() as f64;
    let w = if parent["audio_tracks"][0]["channels"] == "Surround51" {
        6
    } else {
        2
    };
    let cw = if child["settings"]["audio_master"] == "Surround51" {
        6
    } else {
        2
    };
    let rw = if parent["settings"]["audio_master"] == "Surround51" {
        6
    } else {
        2
    };
    let child_route = SourceSurroundRoute::read(&child["audio_tracks"][0]).unwrap();
    let mut nested = vec![vec![0.; n]; w];
    let at_child = |at: i64| {
        let input = (0..6).map(|c| vec![signal(c, at)]).collect::<Vec<_>>();
        let mix = child_route.output(&input, cw, at, own as u32).unwrap();
        convert_owned(mix, Layout::from_channels(w), Mixdown::FrontRear)
    };
    let first = ((start.max(0) as f64) * own / rate).floor();
    for i in 0..n {
        let at = start + i as i64;
        if at < 0 {
            continue;
        }
        let position = at as f64 * own / rate - first;
        let index = position.floor();
        let fraction = (position - index) as f32;
        let a = at_child((first + index) as i64);
        let b = at_child((first + index + 1.) as i64);
        for c in 0..w {
            nested[c][i] = a[c][0] + (b[c][0] - a[c][0]) * fraction;
        }
    }
    let mix = SourceSurroundRoute::read(&parent["audio_tracks"][0])
        .unwrap()
        .output(&nested, rw, start, rate as u32)
        .unwrap();
    let bus = convert_owned(
        mix,
        if channels == 6 {
            Layout::Surround51
        } else {
            Layout::Stereo
        },
        Mixdown::FrontRear,
    );
    if channels == 1 {
        vec![bus[0]
            .iter()
            .zip(&bus[1])
            .map(|(l, r)| 0.5 * (l + r))
            .collect()]
    } else {
        bus
    }
}

#[test]
fn nested_rates_and_layouts_convert_before_resampling_and_preserve_export_channel_order() {
    let template = base();
    for own in [44100, 48000, 96000] {
        for rate in [44100, 48000] {
            for child_master in ["Stereo", "Surround51"] {
                for parent_track in ["Stereo", "Surround51"] {
                    for root_master in ["Stereo", "Surround51"] {
                        let mut doc = template.clone();
                        seq(&mut doc, 7)["settings"]["sample_rate"] = json!(own);
                        seq(&mut doc, 7)["settings"]["audio_master"] = json!(child_master);
                        seq(&mut doc, 9)["settings"]["sample_rate"] = json!(rate);
                        seq(&mut doc, 9)["settings"]["audio_master"] = json!(root_master);
                        seq(&mut doc, 9)["audio_tracks"][0]["channels"] = json!(parent_track);
                        for channels in [1, 2, 6] {
                            for start in [-5, 17] {
                                let actual = runtime(&doc)
                                    .render_media_channels(9, start, 97, channels, source)
                                    .unwrap();
                                let expected = reference(&doc, start, 97, channels);
                                assert!(bits(&actual)==bits(&expected),"nested {own}/{rate}/{child_master}/{parent_track}/{root_master}/{channels}/{start}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn output_layout_switches_do_not_poison_native_graph_cache() {
    let mut doc = base();
    seq(&mut doc, 7)["settings"]["sample_rate"] = json!(44100);
    let mut cached = runtime(&doc);
    for (start, channels) in [(0, 2), (97, 6), (194, 1), (0, 6), (-5, 2), (97, 6)] {
        let actual = cached
            .render_media_channels(9, start, 97, channels, source)
            .unwrap();
        assert!(
            bits(&actual) == bits(&reference(&doc, start, 97, channels)),
            "cache {start}/{channels}"
        );
    }
}

#[test]
fn nested_surround_clip_effects_and_recursion_limit_preserve_six_channel_shapes() {
    let mut doc = base();
    seq(&mut doc, 7)["settings"]["sample_rate"] = json!(44100);
    let expected = reference(&doc, 17, 97, 6);
    seq(&mut doc, 9)["audio_tracks"][0]["items"][0]["effects"] =
        json!([{"effect":"invert_a","enabled":true,"params":{}}]);
    let actual = runtime(&doc)
        .render_media_channels(9, 17, 97, 6, source)
        .unwrap();
    for c in 0..6 {
        for i in 0..97 {
            let v = if expected[c][i] == 0. {
                0.
            } else {
                -expected[c][i]
            };
            assert_eq!(actual[c][i].to_bits(), v.to_bits(), "nested insert {c}/{i}");
        }
    }
    let mut doc = base();
    seq(&mut doc, 9)["audio_tracks"][0]["items"][0]["item"] = json!(9);
    let bus = runtime(&doc)
        .render_media_channels(9, 0, 97, 6, |_, _, _, _| {
            panic!("recursive sequence must not decode files")
        })
        .unwrap();
    assert_eq!(bus.len(), 6);
    assert!(bus
        .iter()
        .all(|c| c.len() == 97 && c.iter().all(|s| *s == 0.)));
}

#[test]
fn nested_remix_reads_keep_parent_width_across_piece_boundaries() {
    let mut doc = base();
    let p = filmcraft_audio_dsp::remix::Plan {
        pieces: vec![
            filmcraft_audio_dsp::remix::Piece { src: 0, len: T / 4 },
            filmcraft_audio_dsp::remix::Piece {
                src: T / 2,
                len: T / 4,
            },
        ],
        xfade: 0,
    };
    seq(&mut doc, 9)["audio_tracks"][0]["items"][0]["effects"] =
        json!([{"effect":"remix","enabled":true,"params":{"plan":{"value":{"Text":p.to_text()}}}}]);
    let out = runtime(&doc)
        .render_media_channels(9, 11995, 97, 6, source)
        .unwrap();
    for c in 0..6 {
        for i in 0..97 {
            let at = 11995 + i as i64;
            let at = if at < 12000 { at } else { 24000 + at - 12000 };
            assert_eq!(
                out[c][i].to_bits(),
                signal(c, at).to_bits(),
                "nested remix {c}/{i}"
            );
        }
    }
}

#[test]
fn original_stereo_nested_rates_multicam_and_depth_oracles_remain_exact() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-project-audio.json")).unwrap();
    assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 652);
    for (case_index, case) in cases.iter().enumerate() {
        let mut native = runtime(&case["document"]);
        for (request_index, r) in case["requests"].as_array().unwrap().iter().enumerate() {
            let out = native
                .render(
                    case["root"].as_u64().unwrap(),
                    r["start"].as_i64().unwrap(),
                    r["frames"].as_u64().unwrap() as usize,
                    |id, start, n, _| {
                        assert_eq!(Some(id), case["source"].as_u64());
                        Ok(Some(
                            (0..2)
                                .map(|c| {
                                    (0..n)
                                        .map(|i| {
                                            let n = start + i as i64;
                                            if n < 0 {
                                                0.
                                            } else if c == 0 {
                                                ((n % 31) - 15) as f32 / 32.
                                            } else {
                                                ((n % 23) - 7) as f32 / 64.
                                            }
                                        })
                                        .collect()
                                })
                                .collect(),
                        ))
                    },
                )
                .unwrap();
            assert!(
                serde_json::to_value(bits(&out)).unwrap() == r["bits"],
                "original project {case_index}/{request_index}"
            );
        }
    }
}

#[test]
fn surround_multicam_modes_keep_selected_cameras_and_audio_only_tracks() {
    let template = base();
    for mode in ["Camera1", "AllCameras", "SwitchAudio"] {
        for angle in [0, 1] {
            let mut doc = template.clone();
            let child = seq(&mut doc, 7);
            let first = child["audio_tracks"][0].clone();
            child["audio_tracks"] = json!((0..3)
                .map(|i| {
                    let mut t = first.clone();
                    t["id"] = json!(4 + i);
                    t["items"][0]["id"] = json!(100 + i);
                    t["volume_db"] = json!([-12., -6., -18.][i]);
                    t
                })
                .collect::<Vec<_>>());
            child["multicam"] = json!({"audio":mode,"cameras":[{"name":"A","video_track":2,"audio_tracks":[4]},{"name":"B","video_track":3,"audio_tracks":[5]},{"name":"Audio","video_track":null,"audio_tracks":[6]}]});
            seq(&mut doc, 9)["audio_tracks"][0]["items"][0]["multicam"] =
                json!({"enabled":true,"angle":angle});
            let out = runtime(&doc)
                .render_media_channels(9, 17, 97, 6, source)
                .unwrap();
            for c in 0..6 {
                for i in 0..97 {
                    let mut expected = 0f32;
                    for (a, db) in [-12f64, -6., -18.].into_iter().enumerate() {
                        let active = a == 2
                            || mode == "AllCameras"
                            || a == if mode == "SwitchAudio" { angle } else { 0 };
                        if active {
                            expected += signal(c, 17 + i as i64) * 10f64.powf(db / 20.) as f32;
                        }
                    }
                    assert_eq!(
                        out[c][i].to_bits(),
                        expected.to_bits(),
                        "surround multicam {mode}/{angle}/{c}/{i}"
                    );
                }
            }
        }
    }
}
