use std::{path::Path, process::Command, sync::atomic::AtomicBool, time::Duration};
use video_kadr_backend::domain::{
    concat_retime_edit::Clock, group_media::decode_source_frame_policy, video_clock::FrameTime,
};
fn movie(dir: &Path, rate: &str) -> std::path::PathBuf {
    let mut bytes = vec![];
    for colour in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
        for _ in 0..256 {
            bytes.extend_from_slice(&colour);
        }
    }
    let raw = dir.join("colours.rgba");
    let output = dir.join("colours.nut");
    std::fs::write(&raw, bytes).unwrap();
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "16x16",
            "-framerate",
            if rate == "10000000" { "25" } else { rate },
            "-i",
        ])
        .arg(&raw);
    if rate == "10000000" {
        command.args([
            "-vf",
            "settb=expr=1/10000000,setpts=N",
            "-enc_time_base",
            "1:10000000",
        ]);
    }
    let status = command
        .args([
            "-c:v",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-fps_mode",
            "passthrough",
            "-f",
            "nut",
        ])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    output
}
fn pixel(input: &Path, time: FrameTime) -> Vec<u8> {
    let frame = decode_source_frame_policy(
        Path::new("ffmpeg"),
        input,
        time,
        16,
        16,
        Duration::from_secs(10),
        &AtomicBool::new(false),
        true,
    )
    .unwrap();
    frame.rgba[..4].to_vec()
}
#[test]
fn fractional_frame_boundaries_keep_the_correct_decoded_picture() {
    let dir = tempfile::tempdir().unwrap();
    let input = movie(dir.path(), "30000/1001");
    let clock = Clock {
        start: 0.,
        duration: 0.1,
        source_start: 0.,
        speed: 1.,
        speed_curve: None,
    };
    let placement = clock.placement(30000, 1001).unwrap();
    let owner = placement.concat_retime.as_ref().unwrap();
    let frame = 254016000000i128 * 1001 / 30000;
    for (index, expected) in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
        .into_iter()
        .enumerate()
    {
        let source = owner.sample(0, 0, index as i128 * frame).unwrap().unwrap();
        assert_eq!(
            pixel(&input, FrameTime::SourceTicks(source)),
            expected,
            "frame {index}"
        );
        if index > 0 {
            let before = pixel(&input, FrameTime::SourceTicks(source - 1));
            assert_ne!(before, expected, "half-open source boundary frame {index}");
            let lossy = pixel(&input, FrameTime::Microseconds(source as u64 / 254016));
            assert_ne!(
                lossy, expected,
                "premature microsecond rounding must detect a different frame"
            );
        }
    }
}
#[test]
fn sub_microsecond_video_pts_are_selected_without_a_float_tolerance() {
    let dir = tempfile::tempdir().unwrap();
    let input = movie(dir.path(), "10000000");
    for (tick, expected) in [
        (25401, [255, 0, 0, 255]),
        (25402, [0, 255, 0, 255]),
        (50803, [0, 255, 0, 255]),
        (50804, [0, 0, 255, 255]),
    ] {
        assert_eq!(
            pixel(&input, FrameTime::SourceTicks(tick)),
            expected,
            "tick {tick}"
        );
    }
}
#[test]
fn exact_frame_times_refuse_lossy_adapters_and_invalid_pts_bases() {
    assert!(FrameTime::SourceTicks(1).legacy_microseconds().is_err());
    assert_eq!(
        FrameTime::SourceTicks(254016)
            .legacy_microseconds()
            .unwrap(),
        1
    );
    assert!(FrameTime::SourceTicks(-1).microseconds().is_err());
    assert!(FrameTime::SourceTicks(1).last_presented_pts(0, 1).is_err());
    assert_eq!(
        FrameTime::SourceTicks(8475667200)
            .last_presented_pts(1001, 30000)
            .unwrap(),
        1
    );
    assert_eq!(
        FrameTime::SourceTicks(8475667199)
            .last_presented_pts(1001, 30000)
            .unwrap(),
        0
    );
}

#[test]
fn retained_clocks_require_native_video_even_without_effects_or_pose_keys() {
    use serde_json::json;
    use video_kadr_backend::domain::{
        composition::{Composition, TransformSpec},
        flat_surface,
    };
    let clock = Clock {
        start: 0.,
        duration: 1.,
        source_start: 0.,
        speed: 1.,
        speed_curve: None,
    };
    let mut wire = serde_json::to_value(Composition::new(Default::default())).unwrap();
    wire["sources"] = json!({"raw":{"id":"raw","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}});
    wire["tracks"] = json!([{"kind":"video","id":"v","name":"Video","clips":[{"id":"c","sourceId":"raw","placement":clock.placement(25,1).unwrap(),"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false}]}]);
    let retained: Composition = serde_json::from_value(wire.clone()).unwrap();
    assert!(flat_surface::requires_native_video(&retained));
    assert!(
        retained.validate_native_surface().is_err(),
        "production admission remains closed"
    );
    wire["tracks"][0]["clips"][0]["placement"]
        .as_object_mut()
        .unwrap()
        .remove("concatRetime");
    let ordinary: Composition = serde_json::from_value(wire).unwrap();
    assert!(!flat_surface::requires_native_video(&ordinary));
}
