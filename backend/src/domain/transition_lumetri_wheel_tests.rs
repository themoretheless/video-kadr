// Persisted Filmcraft media-time and Hold Filters through the actual transition-side renderer.

use super::*;
fn picture(side: u64, time: u64) -> GroupSurface {
    let q = time / 100000;
    let rgba = (0..16u64)
        .flat_map(|i| {
            let (x, y) = (i % 4, i / 4);
            [
                ((11 + x * 37 + y * 19 + q * 13 + side * 71) % 256) as u8,
                ((29 + x * 17 + y * 43 + q * 23 + side * 31) % 256) as u8,
                ((47 + x * 53 + y * 7 + q * 41 + side * 89) % 256) as u8,
                [64, 128, 192, 255][((x + y + q + side) % 4) as usize],
            ]
        })
        .collect();
    GroupSurface {
        width: 4,
        height: 4,
        rgba,
    }
}

#[test]
fn all_3_lumetri_wheel_animations_match_original_108_frames() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-transition-lumetri-wheel.json"
    ))
    .unwrap();
    assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    assert_eq!(oracle["independentSourceIds"], true);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 108);
    assert_eq!(oracle["definitions"], 3);
    assert_eq!(oracle["unsupportedParameters"], serde_json::json!([]));
    let adapters: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-video-default-adapters.json"
    ))
    .unwrap();
    let mut failures = std::collections::BTreeMap::<String, (usize, String)>::new();
    for case in cases {
        let mode = case["mode"].as_u64().unwrap();
        let mut composition = Composition::new(Default::default());
        composition.canvas.fps_milli = 23976;
        composition.canvas.fps_rate = Some(super::super::composition::FrameRateRatio {
            numerator: 24000,
            denominator: 1001,
        });
        composition.canvas.width = 4;
        composition.canvas.height = 4;
        composition.canvas.background.alpha = 0.;
        composition.sources=serde_json::from_value(serde_json::json!({"red":{"id":"red","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false},"blue":{"id":"blue","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false}})).unwrap();
        let kind = "procedural";
        let kernel = serde_json::json!({"effect":"cross_dissolve","params":{}});
        let mut effect = adapters["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["source"] == case["effect"])
            .unwrap()["native"]
            .clone();
        effect["parameters"]["secondary"] =
            serde_json::json!({"enabled":true,"temperature":10.,"saturation":120.,"hueShift":15.});
        effect["parameters"]["grade"] = serde_json::json!({"look":2});
        let native: super::super::composition::VideoEffect =
            serde_json::from_value(effect).unwrap();
        let native = super::super::filmcraft_video_project::with_source_point_animation(
            native,
            case["parameter"].as_str().unwrap(),
            &case["sourceParameter"],
        )
        .unwrap();
        let hold = case["hold"].as_u64().unwrap();
        let partition =
            super::super::filmcraft_video_project::partition_adapted_effects(vec![native]).unwrap();
        let is_layer = matches!(
            case["effect"].as_str(),
            Some("graphic_shape" | "graphic_text")
        );
        assert_eq!(
            partition.graphic.layers.len(),
            usize::from(is_layer),
            "graphic layer must be retained"
        );
        assert_eq!(partition.media_effects.len(), usize::from(!is_layer));
        let effects = serde_json::to_value(&partition.media_effects).unwrap();
        let transitions = vec![
            serde_json::json!({"id":"first","fromClipId":"a","toClipId":"b","timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind,"kernel":kernel,"reverse":case["reverse"]}),
        ];
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","transitions":transitions,"clips":[{"id":"a","sourceId":"red","playbackMode":if hold>0&&mode==0{serde_json::json!({"mode":"freeze","sourceTick":25000000})}else{serde_json::json!({"mode":"forward"})},"placement":{"timelineStartTick":0,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"effectTiming":{"mode":"filmcraft_source","holdFilters":hold==2,"reverseWhileFrozen":false},"sourceAudioEnabled":false,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":if mode==0{effects.clone()}else{serde_json::json!([])},"enabled":true},{"id":"b","sourceId":"blue","playbackMode":if hold>0&&mode==1{serde_json::json!({"mode":"freeze","sourceTick":25000000})}else{serde_json::json!({"mode":"forward"})},"placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"effectTiming":{"mode":"filmcraft_source","holdFilters":hold==2,"reverseWhileFrozen":false},"sourceAudioEnabled":false,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":if mode==1{effects.clone()}else{serde_json::json!([])},"enabled":true}]}])).unwrap();
        composition.metadata = Some(super::super::metadata_burnin::CompositionMetadata {
            markers: Default::default(),
            media_versions: Default::default(),
            clip_links: Default::default(),
            concat_lane_groups: Default::default(),
            sequence_name: "s".into(),
            clip_names: [("a", "matte"), ("b", "matte")]
                .into_iter()
                .map(|(id, name)| {
                    (
                        super::super::composition::CompositionClipId::parse(id).unwrap(),
                        name.into(),
                    )
                })
                .collect(),
            source_names: [("red", "matte"), ("blue", "matte")]
                .into_iter()
                .map(|(id, name)| {
                    (
                        super::super::composition::SourceId::parse(id).unwrap(),
                        name.into(),
                    )
                })
                .collect(),
            drop_frame: false,
            sequence_start_frame: 0,
        });
        let exact = case["at"].as_str().unwrap().parse::<i128>().unwrap();
        let time = (exact / 254016) as u64;
        composition = serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();
        let CompositionTrack::Video { clips, .. } = &composition.tracks[0] else {
            unreachable!()
        };
        let clocks = super::super::video_effect_timing::sample(&clips[mode as usize], exact)
            .unwrap()
            .unwrap();
        let parse = |field: &str| case[field].as_str().unwrap().parse::<i64>().unwrap();
        assert_eq!(
            clocks,
            super::super::effect_frame_clock::EffectFrameClock {
                frame: parse("sourceTime"),
                parameters: parse("effectTime"),
                generator: parse("localTime")
            }
        );
        let mut decode = |id: &super::super::composition::SourceId, time: u64, _: bool| {
            Ok(picture(u64::from(id.as_str() != "red"), time))
        };
        let mut working = None;
        let result = render_media_layers_context(
            &composition,
            time,
            &mut || Ok(()),
            None,
            Some(&mut decode),
            &[],
            0,
            None,
            Some(&mut working),
            Some(exact),
        );
        let error = match result {
            Err(error) => Some(format!("render error: {error:#}")),
            Ok(None) => Some("missing transition output".into()),
            Ok(Some(_)) => {
                let bits: Vec<u32> = working
                    .unwrap()
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|value| value.to_bits())
                    .collect();
                let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
                if bits == expected {
                    None
                } else {
                    Some(format!(
                        "pixel mismatch at {} side{} reverse{} hold{} first_difference{:?}",
                        case["at"],
                        mode,
                        case["reverse"],
                        hold,
                        bits.iter()
                            .zip(&expected)
                            .enumerate()
                            .find(|(_, (a, b))| a != b)
                    ))
                }
            }
        };
        if let Some(error) = error {
            let entry = failures
                .entry(case["parameter"].as_str().unwrap().to_string())
                .or_insert((0, error));
            entry.0 += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "3Lumetri wheel scene parity failures: {failures:#?}"
    );
}
