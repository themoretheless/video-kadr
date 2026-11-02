use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_export_command_range::{resolve_with_clock, Clock, CommandRange},
    filmcraft_project_file::SourceProjectDocument,
};
fn document() -> SourceProjectDocument {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    for tracks in [
        "video_tracks",
        "audio_tracks",
        "submix_tracks",
        "caption_tracks",
    ] {
        sequence[tracks] = json!([]);
    }
    sequence["mark_in"] = json!(10584000000i64);
    sequence["mark_out"] = json!(21168000000i64);
    SourceProjectDocument::read(&serde_json::to_vec(&document).unwrap()).unwrap()
}
#[test]
fn source_time_precedence_and_exact_decimal_transport() {
    let clock = Clock {
        rate: FrameRate::new(24, 1),
        drop_frame: false,
        playhead: Tick::ZERO,
    };
    assert_eq!(clock.time(&json!({"startTime":7,"startFrame":3,"startSeconds":2,"startTimecode":"00:01:00:00"}),"start").unwrap(),Some(7));
    assert_eq!(
        clock
            .time(
                &json!({"startTime":"not ticks","startFrame":3,"startSeconds":2}),
                "start"
            )
            .unwrap(),
        Some(31752000000)
    );
    assert_eq!(
        clock
            .time(&json!({"startTime":"9007199254740993"}), "start")
            .unwrap(),
        Some(9007199254740993)
    );
    assert_eq!(
        clock.time(&json!({"startFrame":"3"}), "start").unwrap(),
        Some(31752000000)
    );
    assert_eq!(
        clock.time(&json!({"startSeconds":0.5}), "start").unwrap(),
        Some(TICKS_PER_SECOND / 2)
    );
    assert_eq!(
        clock
            .time(&json!({"startTimecode":"bad"}), "start")
            .unwrap(),
        None
    );
}
#[test]
fn original_ntsc_drop_frame_and_relative_playhead_clock() {
    let rate = FrameRate::new(30000, 1001);
    let clock = Clock {
        rate,
        drop_frame: true,
        playhead: rate.tick_of(100),
    };
    for (tc, frame) in [
        ("00:01:00;02", 1800),
        ("00:10:00;00", 17982),
        ("+10", 110),
        ("-10", 90),
        ("000102", 32),
    ] {
        assert_eq!(
            clock.time(&json!({"startTimecode":tc}), "start").unwrap(),
            Some(rate.tick_of(frame).0),
            "{tc}"
        );
    }
    assert_eq!(
        clock.time(&json!({"startFrame":1001}), "start").unwrap(),
        Some(rate.tick_of(1001).0)
    );
}
#[test]
fn source_custom_ranges_are_exclusive_and_range_objects_override_flat_times() {
    let doc = document();
    let clock = Clock {
        rate: FrameRate::new(24, 1),
        drop_frame: false,
        playhead: Tick::ZERO,
    };
    let range = resolve_with_clock(
        &doc,
        6,
        &json!({"startFrame":2,"endFrame":4}),
        &json!({"startFrame":100,"endFrame":200}),
        clock,
    )
    .unwrap();
    assert_eq!(range.bounds().unwrap(), (21168000000, 42336000000));
    let automatic = resolve_with_clock(
        &doc,
        6,
        &Value::Null,
        &json!({"startSeconds":0.0,"endSeconds":0.5}),
        clock,
    )
    .unwrap();
    assert_eq!(automatic.bounds().unwrap(), (0, TICKS_PER_SECOND / 2));
    let inout = resolve_with_clock(
        &doc,
        6,
        &json!("sequence In/Out"),
        &json!({"startFrame":i64::MAX}),
        clock,
    )
    .unwrap();
    assert_eq!(inout.bounds().unwrap(), (10584000000, 31752000000));
    let exact = resolve_with_clock(
        &doc,
        6,
        &json!("custom"),
        &json!({"startTime":"9007199254740993","endTime":"9007199254740994"}),
        clock,
    )
    .unwrap();
    assert_eq!(
        exact.bounds().unwrap(),
        (9007199254740993, 9007199254740994)
    );
}
#[test]
fn missing_reversed_negative_and_overflowing_times_refuse_without_panics() {
    let doc = document();
    let clock = Clock {
        rate: FrameRate::new(24, 1),
        drop_frame: false,
        playhead: Tick::ZERO,
    };
    for params in [
        json!({"startFrame":0}),
        json!({"startFrame":4,"endFrame":2}),
        json!({"startFrame":-1,"endFrame":2}),
        json!({"startTimecode":"+9223372036854775807","endFrame":2}),
        json!({"startSeconds":1e300,"endSeconds":1e301}),
    ] {
        assert!(
            resolve_with_clock(&doc, 6, &json!("custom"), &params, clock).is_err(),
            "{params}"
        );
    }
    assert!(Clock {
        rate: FrameRate { num: 0, den: 1 },
        ..clock
    }
    .time(&json!({"startFrame":0}), "start")
    .is_err());
    let context:CommandRange=serde_json::from_value(json!({"source":"custom","times":{"startTimecode":"+1","endTimecode":"+3"},"contextSequenceId":"6","playheadTime":"105840000000"})).unwrap();
    assert_eq!(
        context.resolve(&doc, 6).unwrap().bounds().unwrap(),
        (116424000000, 137592000000)
    );
}
