use serde_json::json;
use video_kadr_backend::domain::{
    composition::{TransformSpec, VideoClip},
    concat_retime_edit::{Clock, Point},
    video_clock::FrameTime,
    video_effect_timing::temporal_sample,
};
fn clip(raw: Clock, num: u32, den: u32) -> VideoClip {
    serde_json::from_value(json!({"id":"c","sourceId":"source","placement":raw.placement(num,den).unwrap(),"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false})).unwrap()
}
#[test]
fn neighboring_frames_use_quantized_source_local_time_and_duration() {
    let clip = clip(
        Clock {
            start: 0.02100049,
            duration: 2.0199996,
            source_start: 1.01900049,
            speed: 1.75,
            speed_curve: Some(vec![
                Point { at: 0., speed: 1. },
                Point { at: 1., speed: 3. },
            ]),
        },
        25,
        1,
    );
    let origin = (0.08 * 254016000000.) as i64;
    let sample = temporal_sample(&clip, origin, 0.04).unwrap().unwrap();
    assert_eq!(sample.timeline_tick, (0.12 * 254016000000.) as i64);
    assert_eq!(sample.local_seconds, 0.08);
    assert_eq!(sample.duration_us, 2_000_000);
    assert_eq!(sample.progress, 0.04);
    assert!(matches!(sample.frame_time, FrameTime::SourceTicks(_)));
    assert_ne!(
        sample.local_seconds,
        0.12 - 0.02100049,
        "wire start must not replace the quantized source origin"
    );
    let (start, duration) = clip
        .placement
        .concat_retime
        .as_ref()
        .unwrap()
        .timeline_window_ticks(
            clip.placement.timeline_start_tick,
            clip.placement.source_in_tick,
        )
        .unwrap();
    assert!(temporal_sample(&clip, start as i64, -1. / 254016000000.)
        .unwrap()
        .is_none());
    assert!(temporal_sample(&clip, (start + duration) as i64, 0.)
        .unwrap()
        .is_none());
    for delta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e30] {
        assert!(temporal_sample(&clip, origin, delta).is_err());
    }
    assert!(temporal_sample(&clip, i64::MAX, 1.).is_err());
}
#[test]
#[ignore = "requires original raw clock records from scripts/qualify-concat-retime.py"]
fn neighboring_requests_keep_original_source_frames_at_fractional_rates() {
    let records = std::fs::read_to_string(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).unwrap();
    let mut checked = 0;
    for line in records.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f[0] != "8" {
            continue;
        }
        let index: usize = f[1].parse().unwrap();
        let num: u32 = f[2].parse().unwrap();
        let den: u32 = f[3].parse().unwrap();
        let exact: i64 = f[5].parse().unwrap();
        let expected: i64 = f[6].parse().unwrap();
        let start: i64 = f[7].parse().unwrap();
        let duration: i64 = f[8].parse().unwrap();
        let (raw_start, raw_duration, source_start) = [
            (0.0199996, 2.01900049, 0.0199996),
            (0.65065049, 2.01900049, 1.01900049),
            (0.02100049, 2.0199996, 1.01900049),
        ][index];
        let clip = clip(
            Clock {
                start: raw_start,
                duration: raw_duration,
                source_start,
                speed: 1.75,
                speed_curve: Some(vec![
                    Point { at: 0., speed: 1. },
                    Point { at: 1., speed: 3. },
                ]),
            },
            num,
            den,
        );
        for shift in [-1i64, 0, 1] {
            let sample = temporal_sample(&clip, exact - shift, shift as f64 / 254016000000.)
                .unwrap()
                .unwrap();
            assert_eq!(
                sample.frame_time,
                FrameTime::SourceTicks(expected),
                "{line}"
            );
            assert_eq!(sample.local_seconds, (exact - start) as f64 / 254016000000.);
            assert_eq!(
                sample.progress,
                ((exact - start) as f64 / duration as f64).clamp(0., 1.)
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 477 * 3);
}

#[test]
fn native_temporal_effects_on_source_transitions_remain_explicitly_unadmitted() {
    use video_kadr_backend::domain::{
        composition::CompositionTrack,
        concat_project_import::{import, ImportRequest},
    };
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"document":{"version":1,"media":[{"id":"a","kind":"video","path":"/missing/a.mov"},{"id":"b","kind":"video","path":"/missing/b.mov"}],"timelines":[{"id":"TL","video":{"width":16,"height":16,"rateNum":25,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"out","trackId":"v","mediaId":"a","kind":"video","start":0,"duration":2,"sourceStart":1},{"id":"in","trackId":"v","mediaId":"b","kind":"video","start":2,"duration":2,"sourceStart":1,"transitionIn":{"id":"concat.fade-black","duration":1}}]}]},"mediaBindings":{"a":{"id":"a","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false},"b":{"id":"b","kind":"video","durationTicks":10000000,"width":16,"height":16,"hasAudio":false}}})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let mut content = project.compositions[&project.root].content.clone();
    content.validate_native_surface().unwrap();
    let CompositionTrack::Video { clips, .. } = &mut content.tracks[0] else {
        panic!()
    };
    clips.iter_mut().find(|clip|clip.id.as_str()=="in").unwrap().effects.push(serde_json::from_value(json!({"kind":"echo","parameters":{"time":-0.1,"count":1,"start":1,"decay":1,"operator":6}})).unwrap());
    assert!(
        content.validate_native_surface().is_err(),
        "source transition policy still refuses unqualified native temporal effects"
    );
}

#[test]
fn ordinary_temporal_progress_retains_the_existing_float_operation_order() {
    let original = clip(
        Clock {
            start: 0.,
            duration: 1.234567,
            source_start: 0.,
            speed: 1.,
            speed_curve: None,
        },
        25,
        1,
    );
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["placement"]
        .as_object_mut()
        .unwrap()
        .remove("concatRetime");
    let ordinary: VideoClip = serde_json::from_value(wire).unwrap();
    let sample = temporal_sample(&ordinary, 1, 0.).unwrap().unwrap();
    assert_eq!(sample.progress.to_bits(), 0x3d8c0c7d457c5c0c);
    assert_eq!(sample.frame_time, FrameTime::Microseconds(0));
}
