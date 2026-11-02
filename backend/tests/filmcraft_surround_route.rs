use filmcraft_audio_dsp::channels::{self, Layout, Mixdown, Pan51};
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceSequence,
    filmcraft_stereo_graph::SourceStereoGraph,
    filmcraft_surround_route::{SourceSurroundRoute, SourceSurroundSend},
};
fn bits(bus: &[Vec<f32>]) -> Vec<Vec<u32>> {
    bus.iter()
        .map(|c| c.iter().map(|v| v.to_bits()).collect())
        .collect()
}
fn signal(channels: usize, n: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|c| {
            (0..n)
                .map(|i| ((i * 13 + c * 7) % 47) as f32 / 64. - 0.25)
                .collect()
        })
        .collect()
}

#[test]
fn surround_output_matches_original_matrices_at_all_speaker_positions_and_lfe_gains() {
    for (name, layout, width) in [
        ("Mono", Layout::Mono, 2),
        ("Stereo", Layout::Stereo, 2),
        ("Surround51", Layout::Surround51, 6),
    ] {
        let bus = signal(width, 31);
        let original = bus.clone();
        for x in [-100., -37., 0., 51., 100.] {
            for y in [-100., -23., 0., 77., 100.] {
                for center in [0., 37., 100.] {
                    for lfe in [-96., -12., 0., 6.] {
                        let track = json!({"channels":name,"mixer":{"lanes":{"pan51.x":{"value":{"Float":x}},"pan51.y":{"value":{"Float":y}},"pan51.center":{"value":{"Float":center}},"pan51.lfe":{"value":{"Float":lfe}}}}});
                        let route = SourceSurroundRoute::read(&track).unwrap();
                        let route: SourceSurroundRoute =
                            serde_json::from_value(serde_json::to_value(route).unwrap()).unwrap();
                        let refs = if layout == Layout::Mono {
                            vec![bus[0].as_slice()]
                        } else {
                            bus.iter().map(Vec::as_slice).collect()
                        };
                        let gain = if lfe <= -96. {
                            0.
                        } else {
                            10f64.powf(lfe / 20.) as f32
                        };
                        let matrix = channels::pan51_matrix(
                            layout,
                            Pan51 {
                                x: (x / 100.) as f32,
                                y: (y / 100.) as f32,
                                center: (center / 100.) as f32,
                                lfe: gain,
                            },
                        );
                        let expected = channels::apply_matrix(&matrix, &refs);
                        let actual = route.output(&bus, 6, -101, 44117).unwrap();
                        assert_eq!(
                            bits(&actual),
                            bits(&expected),
                            "{name} {x}/{y}/{center}/{lfe}"
                        );
                    }
                }
            }
        }
        assert_eq!(bus, original);
    }
    let bus = signal(6, 31);
    let route = SourceSurroundRoute::read(&json!({"channels":"Surround51"})).unwrap();
    assert_eq!(bits(&route.output(&bus, 6, 0, 48000).unwrap()), bits(&bus));
    let mut lfe = vec![vec![0.; 31]; 6];
    lfe[3].fill(1.);
    assert!(route
        .output(&lfe, 2, 0, 48000)
        .unwrap()
        .iter()
        .flatten()
        .all(|v| *v == 0.));
}

#[test]
fn animated_panner_is_sample_accurate_chunk_invariant_and_respects_automation_modes() {
    let tick = 254_016_000_000i64 / 1000;
    let bus = signal(2, 97);
    for mode in ["Read", "Off", "Write", "Touch", "Latch"] {
        let track = json!({"channels":"Stereo","mixer":{"mode":mode,"lanes":{"pan51.x":{"value":{"Float":33.},"keyframes":[{"time":0,"value":{"Float":-100.},"interp":"Linear"},{"time":tick,"value":{"Float":100.},"interp":"Linear"}]},"pan51.center":{"value":{"Float":70.}},"pan51.lfe":{"value":{"Float":-9.}}}}});
        let route = SourceSurroundRoute::read(&track).unwrap();
        let whole = route.output(&bus, 6, -13, 48000).unwrap();
        let mut split = vec![Vec::new(); 6];
        let mut at = 0;
        for n in [1, 3, 7, 33, 53] {
            let slice = bus
                .iter()
                .map(|c| c[at..at + n].to_vec())
                .collect::<Vec<_>>();
            let part = route.output(&slice, 6, -13 + at as i64, 48000).unwrap();
            for (c, p) in split.iter_mut().zip(part) {
                c.extend(p);
            }
            at += n;
        }
        assert_eq!(bits(&whole), bits(&split), "chunks {mode}");
        for i in 0..97 {
            let x = if mode == "Off" {
                33.
            } else {
                -100. + 200. * ((i as f64 - 13.) / 48.).clamp(0., 1.)
            };
            let m = channels::pan51_matrix(
                Layout::Stereo,
                Pan51 {
                    x: (x / 100.) as f32,
                    y: 1.,
                    center: 0.7,
                    lfe: 10f64.powf(-9. / 20.) as f32,
                },
            );
            for (o, row) in m.iter().enumerate() {
                let expected = row.iter().zip(&bus).fold(0f32, |v, (g, c)| v + g * c[i]);
                assert!(
                    (whole[o][i] - expected).abs() < 2e-7,
                    "animated {mode}/{o}/{i}"
                );
            }
        }
    }
}

#[test]
fn sends_use_width_conversion_instead_of_the_strip_panner() {
    let mono = json!({"channels":"Mono","mixer":{"lanes":{"pan51.y":{"value":{"Float":-100.}}},"sends":[{"target":18446744073709551614u64,"level_db":0.,"pan":-100.,"pre_fader":true}]}});
    let bus = vec![vec![0.25; 31]; 2];
    let route = SourceSurroundRoute::read(&mono).unwrap();
    let send = SourceSurroundSend::read(&mono, 0).unwrap().unwrap();
    let send: SourceSurroundSend =
        serde_json::from_value(serde_json::to_value(send).unwrap()).unwrap();
    assert_eq!(send.target, u64::MAX - 1);
    assert!(send.pre_fader);
    let output = route.output(&bus, 6, 0, 48000).unwrap();
    let sent = send.render(&bus, 6, 0, 48000).unwrap();
    assert!(output[2].iter().all(|v| *v == 0.));
    assert!(output[4].iter().any(|v| *v > 0.));
    for c in 0..6 {
        assert!(sent[c].iter().all(|v| *v == if c == 2 { 0.25 } else { 0. }));
    }
    let bus = signal(6, 31);
    let track =
        json!({"channels":"Surround51","mixer":{"sends":[{"target":42,"level_db":-6.,"pan":0.}]}});
    let send = SourceSurroundSend::read(&track, 0).unwrap().unwrap();
    let gain = 10f64.powf(-6. / 20.) as f32;
    let scaled = bus
        .iter()
        .map(|c| c.iter().map(|s| s * gain).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut expected = channels::convert(
        &scaled.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        Layout::Surround51,
        Layout::Stereo,
        Mixdown::FrontRear,
    );
    // Original send balance still runs at centre: retain its f32 sin/cos
    // rounding rather than assuming mathematical unity.
    let angle = std::f32::consts::FRAC_PI_4;
    for (channel, gain) in expected.iter_mut().zip([
        (angle.cos() * std::f32::consts::SQRT_2).min(1.),
        (angle.sin() * std::f32::consts::SQRT_2).min(1.),
    ]) {
        for sample in channel {
            *sample *= gain;
        }
    }
    assert_eq!(
        bits(&send.render(&bus, 2, -31, 48000).unwrap()),
        bits(&expected)
    );
    let muted = json!({"channels":"Surround51","mixer":{"sends":[{"target":42,"muted":true}]}});
    assert!(SourceSurroundSend::read(&muted, 0)
        .unwrap()
        .unwrap()
        .render(&bus, 2, 0, 48000)
        .unwrap()
        .iter()
        .flatten()
        .all(|v| *v == 0.));
}

#[test]
fn invalid_routing_requests_are_rejected_without_mutating_input() {
    let route = SourceSurroundRoute::read(&json!({"channels":"Surround51"})).unwrap();
    let bus = signal(6, 31);
    let before = bits(&bus);
    assert!(route.output(&bus, 1, 0, 48000).is_err());
    assert!(route.output(&bus, 6, 0, 0).is_err());
    assert!(route.output(&bus, 6, i64::MAX, 48000).is_err());
    assert!(route.output(&bus[..2], 6, 0, 48000).is_err());
    let mut ragged = bus.clone();
    ragged[5].pop();
    assert!(route.output(&ragged, 6, 0, 48000).is_err());
    assert_eq!(bits(&bus), before);
    assert!(SourceSurroundRoute::read(&json!({"channels":"unknown"})).is_err());
    assert!(SourceSurroundSend::read(&json!({}), 5).is_err());
    assert!(SourceSurroundSend::read(&json!({}), 0).unwrap().is_none());
}

#[test]
fn shared_send_stage_preserves_original_stereo_solo_submix_and_automation_pcm() {
    for text in [
        include_str!("fixtures/filmcraft-stereo-graph.json"),
        include_str!("fixtures/filmcraft-stereo-send-graph.json"),
    ] {
        let oracle: Value = serde_json::from_str(text).unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 432);
        for (i, case) in cases.iter().enumerate() {
            let doc = SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                .unwrap();
            let index = ProjectIndex::new(&doc).unwrap();
            let seq = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            let graph = SourceStereoGraph::compile(&seq).unwrap();
            let graph: SourceStereoGraph =
                serde_json::from_value(serde_json::to_value(graph).unwrap()).unwrap();
            let out = graph
                .render(
                    case["start"].as_i64().unwrap(),
                    31,
                    case["rate"].as_u64().unwrap() as u32,
                    |id| {
                        let input = case["inputs"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|v| v["id"].as_u64() == Some(id))
                            .unwrap();
                        let raw: Vec<Vec<u32>> =
                            serde_json::from_value(input["bits"].clone()).unwrap();
                        Ok(raw
                            .into_iter()
                            .map(|c| c.into_iter().map(f32::from_bits).collect())
                            .collect())
                    },
                )
                .unwrap();
            assert!(
                serde_json::to_value(bits(&out)).unwrap() == case["bits"],
                "original stereo graph {i}"
            );
        }
    }
}

#[test]
fn shared_send_stage_preserves_original_insert_latency_and_seek_runtime_pcm() {
    use video_kadr_backend::domain::filmcraft_stereo_graph::SourceStereoGraphRuntime;
    for text in [
        include_str!("fixtures/filmcraft-stereo-insert-graph.json"),
        include_str!("fixtures/filmcraft-stereo-graph-seek.json"),
    ] {
        let oracle: Value = serde_json::from_str(text).unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 330);
        for case in cases {
            let doc = SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                .unwrap();
            let index = ProjectIndex::new(&doc).unwrap();
            let seq = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            let graph = SourceStereoGraph::compile(&seq).unwrap();
            let mut runtime = SourceStereoGraphRuntime::new(graph, 48000).unwrap();
            let requests = case
                .get("requests")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| {
                    vec![json!({"start":case["start"],"frames":257,"bits":case["bits"]})]
                });
            for request in requests {
                let out = runtime
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        |id, start, frames| {
                            let side = match id {
                                10 => 0,
                                11 => 1,
                                _ => panic!("unexpected source {id}"),
                            };
                            Ok((0..2)
                                .map(|channel| {
                                    (0..frames)
                                        .map(|i| {
                                            let n = start + i as i64;
                                            if !(0..480000).contains(&n) {
                                                0.
                                            } else if channel == 0 {
                                                (((n + side * 7) % 31) - 15) as f32 / 32.
                                            } else {
                                                (((n + side * 11) % 23) - 7) as f32 / 64.
                                            }
                                        })
                                        .collect()
                                })
                                .collect())
                        },
                    )
                    .unwrap();
                assert!(
                    serde_json::to_value(bits(&out)).unwrap() == request["bits"],
                    "original insert/seek {}/{}/{} at {}",
                    case["effect"],
                    case["host"],
                    case["post"],
                    request["start"]
                );
            }
        }
    }
}
