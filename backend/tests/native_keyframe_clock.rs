use serde_json::json;
use video_kadr_backend::domain::{keyframes::KeyframeTrack, native_keyframe_clock::Clock};
fn track() -> KeyframeTrack<f64> {
    serde_json::from_value(json!({"timeBase":24,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":240,"value":240.}]})).unwrap()
}
#[test]
fn fractional_tail_phase_is_not_an_integer_key_shift() {
    let track = track();
    let clock = Clock::identity()
        .shifted(100000)
        .unwrap()
        .prepare(&track)
        .unwrap();
    assert_eq!(clock.sample_microseconds(10000).unwrap(), 3.);
    let mut rounded = track.clone();
    for key in &mut rounded.keyframes {
        key.tick -= 2;
    }
    assert_eq!(rounded.sample_seconds(0.01).unwrap(), 2.);
    for us in [-200000, -100000, -37500, 0, 1, 10000, 62500, 1000000] {
        assert_eq!(
            clock.sample_microseconds(us).unwrap().to_bits(),
            track
                .sample_seconds((us + 100000) as f64 / 1e6)
                .unwrap()
                .to_bits()
        );
    }
}
#[test]
fn fractional_hold_rate_does_not_floor_the_time_base() {
    let track = track();
    let clock = Clock::identity().retimed(4123456, 1000000).unwrap();
    assert_eq!(clock.parts(), (64429, 15625, 0));
    assert_eq!(
        clock
            .prepare(&track)
            .unwrap()
            .sample_microseconds(300000)
            .unwrap(),
        30.
    );
    let mut rounded = track.clone();
    rounded.time_base = 98;
    assert_eq!(rounded.sample_seconds(0.3).unwrap(), 29.);
    let composed = Clock::identity()
        .shifted(100000)
        .unwrap()
        .retimed(4123456, 1000000)
        .unwrap();
    assert_eq!(composed.parts(), (64429, 15625, 1562500000));
    assert_eq!(
        composed
            .prepare(&track)
            .unwrap()
            .sample_microseconds(300000)
            .unwrap(),
        32.
    );
}
#[test]
fn exact_source_sampling_does_not_round_fractional_tps_or_use_native_grid_policy() {
    let track = track();
    let clock = Clock::identity()
        .shifted(100000)
        .unwrap()
        .prepare(&track)
        .unwrap();
    let tick = 10000 * 254016;
    assert_eq!(
        clock.sample_source_ticks(tick, false).unwrap().to_bits(),
        track.sample_source_ticks(110000 * 254016).to_bits()
    );
    assert_ne!(
        clock.sample_microseconds(10000).unwrap().to_bits(),
        clock.sample_source_ticks(tick, false).unwrap().to_bits()
    );
    let c = Clock::identity()
        .shifted(100000)
        .unwrap()
        .retimed(4123456, 1000000)
        .unwrap();
    let exact = c
        .prepare(&track)
        .unwrap()
        .sample_source_ticks(300000 * 254016, false)
        .unwrap();
    let expected = (0.1 + 0.3 * 4.123456) * 24.;
    assert!((exact - expected).abs() < 1e-12);
    let nearest = ((0.1 + 0.3 * 4.123456) * 254016000000f64).round() as i64;
    assert_ne!(
        exact.to_bits(),
        track.sample_source_ticks(nearest).to_bits()
    );
}
#[test]
fn legacy_original_filmcraft_parameter_bits_remain_unchanged() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-exact-parameter-time.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let origin = case["origin"].as_str().unwrap().parse::<i64>().unwrap() / 254016;
        let track:KeyframeTrack<f64>=serde_json::from_value(json!({"timeBase":1000000,"interpolation":case["nativeInterpolation"],"keyframes":[{"tick":origin,"value":5.},{"tick":origin+1000000,"value":10.}]})).unwrap();
        let at = case["at"].as_str().unwrap().parse::<i64>().unwrap();
        let expected = case["bits"].as_u64().unwrap();
        assert_eq!(track.sample_filmcraft_source_ticks(at).to_bits(), expected);
        assert_eq!(
            Clock::identity()
                .prepare(&track)
                .unwrap()
                .sample_source_ticks(at, true)
                .unwrap()
                .to_bits(),
            expected
        );
    }
}
#[test]
fn clocks_reduce_validate_and_preserve_extrapolation_and_negative_ties() {
    let mut track = track();
    track.extrapolation =
        Some(serde_json::from_value(json!({"before":"loop","after":"ping_pong"})).unwrap());
    let c = Clock::new(2, 2, 200000).unwrap();
    assert_eq!(c.parts(), (1, 1, 100000));
    let prepared = c.prepare(&track).unwrap();
    for us in [-2000000, -1000000, -37500, 0, 10000, 10000000, 20000000] {
        assert_eq!(
            prepared.sample_microseconds(us).unwrap().to_bits(),
            track
                .sample_seconds((us + 100000) as f64 / 1e6)
                .unwrap()
                .to_bits()
        );
    }
    assert!(Clock::new(0, 1, 0).is_err());
    assert!(Clock::new(1, 0, 0).is_err());
    assert!(Clock::new(1, 1000001, 0).is_err());
    assert!(Clock::identity().retimed(0, 1).is_err());
    let mut invalid = track.clone();
    invalid.keyframes.reverse();
    assert!(c.prepare(&invalid).is_err());
    invalid.keyframes.clear();
    assert!(c.prepare(&invalid).is_err());
}

#[test]
fn negative_half_ticks_and_numeric_boundaries_are_explicit() {
    let mut track = track();
    track.extrapolation =
        Some(serde_json::from_value(json!({"before":"continue","after":"hold"})).unwrap());
    let clock = Clock::identity().shifted(-100000).unwrap();
    assert_eq!(clock.retimed(42, 42).unwrap(), clock);
    let prepared = clock.prepare(&track).unwrap();
    assert_eq!(prepared.sample_microseconds(37500).unwrap(), -2.);
    assert_eq!(
        prepared.sample_source_ticks(37500 * 254016, false).unwrap(),
        -1.5
    );
    let extreme = Clock::new(86400000000, 1, 9007199254740991).unwrap();
    let mut wide = track.clone();
    wide.time_base = 100000000;
    let prepared = extreme.prepare(&wide).unwrap();
    assert!(prepared.sample_microseconds(i64::MAX).is_err());
    assert!(prepared.sample_microseconds(i64::MIN).is_err());
    assert!(prepared.sample_source_ticks(i64::MAX, false).is_err());
    assert!(prepared.sample_source_ticks(i64::MIN, true).is_err());
    wide.time_base += 1;
    assert!(extreme.prepare(&wide).is_err());
    assert!(Clock::new(86400000001, 1, 0).is_err());
    assert!(Clock::new(1, 1, 9007199254740992).is_err());
}

#[test]
fn clocked_tracks_roundtrip_and_all_native_sampling_modes_use_the_clock() {
    use video_kadr_backend::domain::{
        animation_sample_time::AnimationSampleTime, keyframes::FfmpegKeyframeAdapter,
    };
    let mut value = serde_json::to_value(track()).unwrap();
    value["sampleClock"] = json!({"rateNum":2,"rateDen":2,"offsetNum":200000});
    let parsed: KeyframeTrack<f64> = serde_json::from_value(value).unwrap();
    let serialized = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        serialized["sampleClock"],
        json!({"rateNum":1,"rateDen":1,"offsetNum":100000})
    );
    let restored: KeyframeTrack<f64> = serde_json::from_value(serialized.clone()).unwrap();
    assert_eq!(parsed, restored);
    assert_eq!(
        AnimationSampleTime::Microseconds(10000)
            .sample_track(&restored)
            .unwrap(),
        3.
    );
    assert_eq!(
        AnimationSampleTime::SignedMicroseconds(10000)
            .sample_track(&restored)
            .unwrap(),
        3.
    );
    assert_eq!(restored.sample_seconds(0.01).unwrap(), 3.);
    assert_eq!(
        restored.sample_source_ticks(10000 * 254016).to_bits(),
        track().sample_source_ticks(110000 * 254016).to_bits()
    );
    assert_eq!(
        restored
            .sample_filmcraft_source_ticks(10000 * 254016)
            .to_bits(),
        track()
            .sample_filmcraft_source_ticks(110000 * 254016)
            .to_bits()
    );
    let expression = FfmpegKeyframeAdapter::new(&restored)
        .expression_shifted("t", 2.)
        .unwrap();
    assert!(expression.contains("0.1"));
    assert!(expression.contains("t-2"));
    for bad in [
        json!({"rateNum":0,"rateDen":1,"offsetNum":0}),
        json!({"rateNum":1,"rateDen":0,"offsetNum":0}),
        json!({"rateNum":1,"rateDen":1,"offsetNum":0,"path":"forged"}),
    ] {
        let mut bad_track = serialized.clone();
        bad_track["sampleClock"] = bad;
        assert!(serde_json::from_value::<KeyframeTrack<f64>>(bad_track).is_err());
    }
}

#[test]
fn serialized_clocked_pose_uses_native_raster_sampling_after_roundtrip() {
    use video_kadr_backend::domain::{
        composition::{Composition, TransformSpec},
        flat_surface,
        group_surface::GroupSurface,
    };
    let mut value = serde_json::to_value(Composition::new(Default::default())).unwrap();
    value["canvas"]["width"] = json!(64);
    value["canvas"]["height"] = json!(48);
    value["sources"] = json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}});
    let mut transform = serde_json::to_value(TransformSpec::default()).unwrap();
    let mut curve = serde_json::to_value(track()).unwrap();
    curve["sampleClock"] = json!({"rateNum":1,"rateDen":1,"offsetNum":100000});
    transform["x"] = json!({"mode":"keyframes","track":curve});
    value["tracks"] = json!([{"kind":"video","id":"v","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.},"transform":transform,"opacity":{"mode":"constant","value":1.},"blendMode":"normal","sourceAudioEnabled":false}]}]);
    let clocked: Composition = serde_json::from_value(value.clone()).unwrap();
    assert!(flat_surface::requires_native_video(&clocked));
    value["tracks"][0]["clips"][0]["transform"]["x"] = json!({"mode":"constant","value":3.});
    let expected: Composition = serde_json::from_value(value).unwrap();
    let render = |c: &Composition| {
        let mut decode = |_: &_, _: u64, _: bool| {
            Ok(GroupSurface {
                width: 16,
                height: 16,
                rgba: [255, 0, 0, 255].repeat(256),
            })
        };
        flat_surface::render_media_layers(c, 10000, &mut || Ok(()), None, Some(&mut decode))
            .unwrap()
            .unwrap()
    };
    assert_eq!(render(&clocked), render(&expected));
    let restored: Composition =
        serde_json::from_value(serde_json::to_value(&clocked).unwrap()).unwrap();
    assert_eq!(render(&restored), render(&expected));
}


#[test]
fn clocked_audio_selects_native_delivery_and_mixes_fractional_gain_pan_after_roundtrip() {
    use std::sync::atomic::AtomicBool;
    use video_kadr_backend::{domain::{composition::{Composition, TransformSpec}, nested_composition::NestedCompositionProject, nested_audio, nested_pcm, audio_pan::equal_power_pan}, tools::composition_native_audio::needs_native_audio};
    let gain = json!({"mode":"keyframes","track":{"timeBase":24,"interpolation":"linear","sampleClock":{"rateNum":1,"rateDen":1,"offsetNum":100000},"keyframes":[{"tick":0,"value":0.25},{"tick":24,"value":1.25}]}});
    let pan = json!({"mode":"keyframes","track":{"timeBase":24,"interpolation":"linear","sampleClock":{"rateNum":1,"rateDen":1,"offsetNum":100000},"keyframes":[{"tick":0,"value":-1.},{"tick":24,"value":1.}]}});
    for video in [false, true] {
        let mut value = serde_json::to_value(Composition::new(Default::default())).unwrap();
        value["sources"] = json!({"source":{"id":"source","kind":if video {"video"} else {"audio"},"durationTicks":1000000,"width":16,"height":16,"hasAudio":true}});
        let placement=json!({"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.});
        value["tracks"] = if video {json!([{"kind":"video","id":"v","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":placement,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","sourceAudioEnabled":true,"audioGain":gain,"audioPan":pan}]}])} else {json!([{"kind":"audio","id":"a","name":"Audio","clips":[{"id":"clip","sourceId":"source","placement":placement,"gain":gain,"pan":pan,"fadeInTicks":0,"fadeOutTicks":0}]}])};
        let composition:Composition=serde_json::from_value(value.clone()).unwrap();
        assert!(needs_native_audio(&composition));
        let track=&mut value["tracks"][0];
        let clip=&mut track["clips"][0];
        for name in if video {["audioGain","audioPan"]}else{["gain","pan"]} {clip[name]["track"].as_object_mut().unwrap().remove("sampleClock");}
        let legacy:Composition=serde_json::from_value(value).unwrap();
        assert!(!needs_native_audio(&legacy));
        let project:NestedCompositionProject=serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}})).unwrap();
        let project:NestedCompositionProject=serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
        let layers=nested_audio::plan_audio_tracks(&project,"root",0..1000000).unwrap();
        assert_eq!(layers.len(),1);
        let start=480u64; // ten milliseconds, before author-grid rounding
        let output=nested_pcm::mix_block(&layers,start,960,48000,&AtomicBool::new(false),|_,_|Ok((1,[1.,0.]))).unwrap();
        let at=|us:u64| ((us+100000) as f64*24./1000000.).round()/24.;
        for (i, frame) in output.iter().enumerate() {
            let us=(start+i as u64)*1000000/48000;
            let grid=us/10000*10000;
            let a=equal_power_pan(-1.+2.*at(grid),1);
            let b=equal_power_pan(-1.+2.*at(grid+10000),1);
            let fraction=(us-grid) as f64/10000.;
            let g=0.25+at(us);
            let expected=[g*(a[0]+(b[0]-a[0])*fraction),g*(a[2]+(b[2]-a[2])*fraction)];
            for channel in 0..2 {assert!((frame[channel]-expected[channel]).abs()<1e-14,"video {video} sample {i} channel {channel}");}
        }
        // Dropping the fractional offset would produce 0.25, rather than 0.375.
        assert_eq!(layers[0].sample(10000).unwrap().unwrap().gain,0.375);
    }
}
