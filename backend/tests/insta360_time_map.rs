use video_kadr_backend::domain::{insta360_time_map::parse, insta360_video_clock::VideoClock};
#[path = "support/insta360_time_map.rs"]
mod fixture;
#[test]
fn constant_original_trim_uses_frame_aligned_translation_and_ignores_opaque_word() {
    let video = VideoClock {
        numerator: 100,
        denominator: 1,
        sample_count: 762,
    };
    let map = parse(&fixture::trim(762, 10., 21000.), video).unwrap();
    assert_eq!(map.source_start_seconds, 21.);
    assert_eq!(map.source_frame_offset, 2100);
    assert_eq!(map.clip_frames, 762);
    let rational = VideoClock {
        numerator: 30000,
        denominator: 1001,
        sample_count: 60,
    };
    let step = 1000. * 1001. / 30000.;
    assert_eq!(
        parse(&fixture::trim(60, step, step * 21.), rational)
            .unwrap()
            .source_frame_offset,
        21
    );
}
#[test]
fn unsupported_time_maps_fail_closed_instead_of_guessing_original_capture_clock() {
    let clock = VideoClock {
        numerator: 100,
        denominator: 1,
        sample_count: 4,
    };
    let good = fixture::trim(4, 10., 21000.);
    for at in [0, 4, 8, 12] {
        let mut bad = good.clone();
        bad[at] = 255;
        assert!(parse(&bad, clock).is_err());
    }
    for (at, value) in [
        (24, 1.),
        (32, 29.),
        (40, -1.),
        (48, 21031.),
        (56 + 16, 0.),
        (56 + 24, 21011.),
        (56 + 8, f64::NAN),
    ] {
        let mut bad = good.clone();
        bad[at..at + 8].copy_from_slice(&value.to_le_bytes());
        assert!(parse(&bad, clock).is_err(), "corruption at{at}");
    }
    assert!(parse(&fixture::trim(4, 10., 21001.), clock).is_err());
    assert!(parse(
        &good,
        VideoClock {
            sample_count: 5,
            ..clock
        }
    )
    .is_err());
    for length in 0..good.len() {
        assert!(parse(&good[..length], clock).is_err());
    }
}
