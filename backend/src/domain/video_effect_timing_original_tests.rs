use super::*;

#[test]
fn persisted_source_effect_timing_matches_original_126_reverse_speed_hold_cases() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-effect-frame-clock.json"
    ))
    .unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 126);
    for case in cases {
        let input = &case["input"];
        let source_in = input["source_in"].as_i64().unwrap();
        let duration = input["duration"].as_i64().unwrap();
        let speed = input["speed"].as_f64().unwrap();
        let reverse = input["reverse"].as_bool().unwrap();
        let source_out = source_in + (duration as f64 * speed.abs()).round() as i64;
        let playback = if let Some(held) = input["frame_hold"].as_i64() {
            serde_json::json!({"mode":"freeze","sourceTick":held / 254016})
        } else {
            serde_json::json!({"mode":if reverse {"reverse"}else{"forward"}})
        };
        let clip: VideoClip = serde_json::from_value(serde_json::json!({
            "id":"clip","sourceId":"source","sourceAudioEnabled":false,
            "placement":{"timelineStartTick":input["timeline_start"].as_i64().unwrap()/254016,"sourceInTick":source_in/254016,"sourceOutTick":source_out/254016,"timelineDurationTick":duration/254016,"speed":speed},
            "playbackMode":playback,"effectTiming":{"mode":"filmcraft_source","holdFilters":input["hold_filters"],"reverseWhileFrozen":reverse},
            "transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"
        })).unwrap();
        let saved: VideoClip =
            serde_json::from_value(serde_json::to_value(&clip).unwrap()).unwrap();
        assert_eq!(
            sample(&saved, case["at"].as_i64().unwrap() as i128).unwrap(),
            Some(EffectFrameClock {
                frame: case["frame"].as_i64().unwrap(),
                parameters: case["parameters"].as_i64().unwrap(),
                generator: case["generator"].as_i64().unwrap(),
            }),
            "{case}"
        );
        assert!(sample(&saved, i128::MAX).is_err());
        let mut legacy = saved;
        legacy.effect_timing = None;
        assert!(serde_json::to_value(&legacy)
            .unwrap()
            .get("effectTiming")
            .is_none());
        if let Some(clocks) = sample(&legacy, case["at"].as_i64().unwrap() as i128).unwrap() {
            assert_eq!(clocks.parameters, clocks.generator);
        }
    }
}
