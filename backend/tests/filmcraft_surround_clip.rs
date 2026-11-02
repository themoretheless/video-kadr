use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_clip_audio::{SourceTrackAudio, SourceTrackAudioRuntime},
    filmcraft_sequence_source::{SourceTrack, SourceTrackItem},
    filmcraft_surround_route::SourceSurroundRoute,
};
const T: i64 = 254_016_000_000;
fn plan(raw: &Value) -> SourceTrackAudio {
    let track = SourceTrack {
        id: raw["id"].as_u64().unwrap(),
        raw,
        items: raw["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(SourceTrackItem::read)
            .collect::<anyhow::Result<_>>()
            .unwrap(),
    };
    let plan = SourceTrackAudio::compile(&track).unwrap();
    serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap()
}
fn base() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-clip-audio-input.json")).unwrap();
    let mut track = fixture["cases"][0]["track"].clone();
    track["items"].as_array_mut().unwrap().truncate(1);
    track["channels"] = json!("Surround51");
    track["transitions"] = json!([]);
    let clip = &mut track["items"][0];
    clip["start"] = json!(0);
    clip["source_in"] = json!(0);
    clip["duration"] = json!(T * 2);
    clip["speed"] = json!(1.);
    clip["reverse"] = json!(false);
    clip["gain_db"] = json!(0.);
    clip["effects"] = json!([]);
    clip["source_channels"] = json!([]);
    track
}
fn signal(c: usize, n: i64) -> f32 {
    if n < 0 {
        0.
    } else {
        (((n * 13 + c as i64 * 7) % 47) - 17) as f32 / 128.
    }
}
fn read(channels: usize, start: i64, frames: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|c| (0..frames).map(|i| signal(c, start + i as i64)).collect())
        .collect()
}
fn bits(bus: &[Vec<f32>]) -> Vec<Vec<u32>> {
    bus.iter()
        .map(|c| c.iter().map(|s| s.to_bits()).collect())
        .collect()
}

#[test]
fn surround_source_channels_match_original_selection_and_front_upmix() {
    let template = base();
    for available in [1usize, 2, 3, 5, 6, 8, 16] {
        for picks in [
            vec![],
            vec![0],
            vec![1],
            vec![2, 0],
            vec![0, 1, 2],
            vec![5, 4, 3, 2, 1, 0],
            vec![15, 99, 1, 0, 2, 3, 4],
        ] {
            let mut track = template.clone();
            track["items"][0]["source_channels"] = json!(picks);
            let native = plan(&track);
            let ok = |c: usize| if c < available { c } else { 0 };
            let selected = match picks.len() {
                0 => {
                    if available == 1 {
                        vec![0]
                    } else if available < 6 {
                        vec![0, 1]
                    } else {
                        (0..6).collect()
                    }
                }
                1 => vec![ok(picks[0])],
                2..=5 => vec![ok(picks[0]), ok(picks[1])],
                _ => picks.iter().take(6).map(|c| ok(*c)).collect(),
            };
            for rate in [44100, 44117, 48000, 96000] {
                for start in [-5i64, 0, 17] {
                    let out = native
                        .render(start, 97, rate, |_, at, n, _| {
                            Ok(Some(read(available, at, n)))
                        })
                        .unwrap();
                    let mut expected = vec![vec![0.; 97]; 6];
                    for i in 0..97 {
                        let at = start + i as i64;
                        if at < 0 {
                            continue;
                        }
                        match selected.len() {
                            1 => expected[2][i] = signal(selected[0], at),
                            2 => {
                                expected[0][i] = signal(selected[0], at);
                                expected[1][i] = signal(selected[1], at);
                            }
                            _ => {
                                for c in 0..6 {
                                    expected[c][i] = signal(selected[c], at);
                                }
                            }
                        }
                    }
                    assert!(
                        bits(&out) == bits(&expected),
                        "selection {available}/{picks:?}/{rate}/{start}"
                    );
                    let routed = SourceSurroundRoute::read(&track)
                        .unwrap()
                        .output(&out, 6, start, rate)
                        .unwrap();
                    assert!(
                        bits(&routed) == bits(&out),
                        "default surround routing must preserve clip channels"
                    );
                }
            }
        }
    }
}

#[test]
fn surround_intrinsics_ignore_clip_panner_and_only_scale_channel_volume_on_left_right() {
    let mut track = base();
    track["items"][0]["gain_db"] = json!(-3.25);
    track["items"][0]["effects"] = json!([
        {"effect":"volume","enabled":true,"params":{"level":{"value":{"Float":-13.25}}}},
        {"effect":"channel_volume","enabled":true,"params":{"left":{"value":{"Float":-5.5}},"right":{"value":{"Float":1.25}}}},
        {"effect":"panner","enabled":true,"params":{"balance":{"value":{"Float":-100.}}}}
    ]);
    let native = plan(&track);
    let out = native
        .render(17, 97, 48000, |_, at, n, _| Ok(Some(read(6, at, n))))
        .unwrap();
    let gain = |db: f64| 10f64.powf(db / 20.) as f32;
    let g = gain(-3.25) * gain(-13.25);
    for c in 0..6 {
        let gc = g * match c {
            0 => gain(-5.5),
            1 => gain(1.25),
            _ => 1.,
        };
        for i in 0..97 {
            assert_eq!(
                out[c][i].to_bits(),
                (0. + signal(c, 17 + i as i64) * gc).to_bits(),
                "intrinsic {c}/{i}"
            );
        }
    }
    let mut muted = track.clone();
    muted["items"][0]["essential"] = json!({"mute":true});
    assert!(plan(&muted)
        .render(0, 97, 48000, |_, _, _, _| panic!(
            "muted clip must not decode"
        ))
        .unwrap()
        .iter()
        .flatten()
        .all(|v| *v == 0.));
}

#[test]
fn surround_clip_dsp_and_continuation_process_all_six_channels() {
    let mut track = base();
    track["items"][0]["effects"] = json!([{"effect":"invert_a","enabled":true,"params":{}}]);
    let mut runtime = SourceTrackAudioRuntime::new(plan(&track));
    for (start, n) in [(17, 97), (114, 29), (11, 31), (-5, 31)] {
        let out = runtime
            .render(start, n, 48000, |_, at, n, _| Ok(Some(read(6, at, n))))
            .unwrap();
        for c in 0..6 {
            for i in 0..n {
                let at = start + i as i64;
                let expected = if at < 0 { 0. } else { 0. - signal(c, at) };
                assert_eq!(
                    out[c][i].to_bits(),
                    expected.to_bits(),
                    "six-channel clip DSP {c}/{at}"
                );
            }
        }
    }
}

#[test]
fn surround_transitions_apply_original_gains_to_every_channel() {
    let mut track = base();
    let mut second = track["items"][0].clone();
    second["id"] = json!(101);
    second["item"] = json!(2);
    track["items"].as_array_mut().unwrap().push(second);
    for effect in ["constant_gain", "constant_power", "exponential_fade"] {
        track["transitions"] = json!([{"start":32*(T/48000),"duration":64*(T/48000),"from":100,"to":101,"effect":{"effect":effect}}]);
        let native = plan(&track);
        let out = native
            .render(0, 128, 48000, |id, _, n, _| {
                Ok(Some(
                    (0..6)
                        .map(|c| vec![(c + 1) as f32 * if id == 1 { 0.01 } else { 0.02 }; n])
                        .collect(),
                ))
            })
            .unwrap();
        for c in 0..6 {
            for i in 0..128 {
                let a = (c + 1) as f32 * 0.01;
                let b = (c + 1) as f32 * 0.02;
                let expected = if (32..96).contains(&i) {
                    let p = (i - 32) as f32 / 64.;
                    let (ga, gb) = match effect {
                        "constant_gain" => (1. - p, p),
                        "exponential_fade" => ((1. - p).powi(3), 1. - (1. - p).powi(3)),
                        _ => (
                            (p * std::f32::consts::FRAC_PI_2).cos(),
                            (p * std::f32::consts::FRAC_PI_2).sin(),
                        ),
                    };
                    0. + a * ga + b * gb
                } else {
                    a + b
                };
                assert_eq!(
                    out[c][i].to_bits(),
                    expected.to_bits(),
                    "transition {effect}/{c}/{i}"
                );
            }
        }
    }
}

#[test]
fn surround_speed_reverse_and_legacy_stereo_plan_retain_source_clocks() {
    let mut track = base();
    track["items"][0]["source_in"] = json!(T);
    for speed in [0.5f64, 1., 1.5, 2.] {
        for reverse in [false, true] {
            track["items"][0]["speed"] = json!(speed);
            track["items"][0]["reverse"] = json!(reverse);
            let native = plan(&track);
            let mut reads = Vec::new();
            let out = native
                .render(0, 97, 48000, |_, at, n, _| {
                    reads.push((at, n));
                    Ok(Some(read(6, at, n)))
                })
                .unwrap();
            assert_eq!(reads.len(), 1);
            let (start, span) = reads[0];
            for c in 0..6 {
                for i in 0..97 {
                    let expected = if speed == 1. && !reverse {
                        signal(c, start + i as i64)
                    } else {
                        let pos = if reverse {
                            span as f64 - 1. - i as f64 * speed
                        } else {
                            i as f64 * speed
                        };
                        let at = pos.floor().max(0.) as i64;
                        let fraction = (pos - at as f64) as f32;
                        let a = signal(c, start + at);
                        let b = signal(c, start + at + 1);
                        a + (b - a) * fraction
                    };
                    assert_eq!(
                        out[c][i].to_bits(),
                        expected.to_bits(),
                        "retiming {speed}/{reverse}/{c}/{i}"
                    );
                }
            }
        }
    }
    track["channels"] = json!("Stereo");
    let native = plan(&track);
    let mut serialized = serde_json::to_value(native).unwrap();
    serialized.as_object_mut().unwrap().remove("width");
    let legacy: SourceTrackAudio = serde_json::from_value(serialized).unwrap();
    assert_eq!(
        legacy
            .render(0, 97, 48000, |_, at, n, _| Ok(Some(read(6, at, n))))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn existing_original_stereo_clip_and_effect_pcm_remain_unchanged() {
    for (text, channels, count) in [
        (
            include_str!("fixtures/filmcraft-clip-audio-input.json"),
            4,
            1080,
        ),
        (include_str!("fixtures/filmcraft-clip-effects.json"), 2, 440),
    ] {
        let oracle: Value = serde_json::from_str(text).unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), count);
        for (index, case) in cases.iter().enumerate() {
            let mut native = SourceTrackAudioRuntime::new(plan(&case["track"]));
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                let out = native
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        case["rate"].as_u64().unwrap() as u32,
                        |id, start, n, _| {
                            assert_eq!(Some(id), case["source"].as_u64());
                            Ok(Some(
                                (0..channels)
                                    .map(|c| {
                                        (0..n)
                                            .map(|i| {
                                                let n = start + i as i64;
                                                if n < 0 {
                                                    0.
                                                } else {
                                                    match c {
                                                        0 => ((n % 31) - 15) as f32 / 32.,
                                                        1 => ((n % 23) - 7) as f32 / 64.,
                                                        2 => ((n % 17) - 5) as f32 / 16.,
                                                        _ => ((n % 19) - 13) as f32 / 128.,
                                                    }
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
                    serde_json::to_value(bits(&out)).unwrap() == request["bits"],
                    "stereo original {index}/{request_index}"
                );
            }
        }
    }
}
