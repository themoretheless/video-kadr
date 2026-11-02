use video_kadr_backend::domain::video_clock::FrameTime;
#[test]
fn source_end_clamp_is_exact_idempotent_and_does_not_change_in_range_samples() {
    for duration in [1, 250_000, 1_000_000, u64::MAX] {
        for value in [0, 1, 249_999, 250_000, 999_999, 1_000_000, u64::MAX] {
            let actual = FrameTime::Microseconds(value)
                .clamp_video_duration(duration)
                .unwrap();
            assert_eq!(actual, FrameTime::Microseconds(value.min(duration - 1)));
            assert_eq!(actual.clamp_video_duration(duration).unwrap(), actual);
        }
        for value in [0, 1, 254_016 - 1, 254_016, 254_016_000_000, i64::MAX] {
            let actual = FrameTime::SourceTicks(value)
                .clamp_video_duration(duration)
                .unwrap();
            assert_eq!(
                actual,
                FrameTime::SourceTicks(
                    i128::from(value).min(i128::from(duration) * 254_016 - 1) as i64
                )
            );
            assert_eq!(actual.clamp_video_duration(duration).unwrap(), actual);
        }
    }
    assert!(FrameTime::SourceTicks(-1).clamp_video_duration(1).is_err());
    assert!(FrameTime::Microseconds(0).clamp_video_duration(0).is_err());
    // A sub-microsecond tick before the end must not be rounded forward.
    assert_eq!(
        FrameTime::SourceTicks(254_016_000_000 - 1)
            .clamp_video_duration(1_000_000)
            .unwrap(),
        FrameTime::SourceTicks(254_016_000_000 - 1)
    );
}
#[test]
#[ignore = "requires actual FFmpeg/ffprobe; generated lossless four-frame clip"]
fn bounded_held_decoder_returns_the_last_real_frame_beyond_eof_with_video_origins() {
    use std::{path::Path, process::Command, sync::atomic::AtomicBool, time::Duration};
    use video_kadr_backend::domain::group_media::decode_source_frame_policy;
    let temp = tempfile::tempdir().unwrap();
    let raw = temp.path().join("frames.rgba");
    let colors = [
        [16, 32, 48, 255],
        [64, 80, 96, 255],
        [112, 128, 144, 255],
        [160, 176, 192, 255],
    ];
    std::fs::write(
        &raw,
        colors
            .iter()
            .flat_map(|color| color.repeat(4))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    for origin in [0, 3] {
        let video = temp.path().join(format!("origin-{origin}.mkv"));
        let output = Command::new("ffmpeg")
            .args([
                "-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2", "-r", "4",
                "-i",
            ])
            .arg(&raw)
            .args([
                "-vf",
                &format!("setpts=PTS+{origin}/TB"),
                "-copyts",
                "-c:v",
                "ffv1",
            ])
            .arg(&video)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_entries",
                "stream=start_time,nb_read_frames",
                "-of",
                "json",
            ])
            .arg(&video)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let stream = &metadata["streams"][0];
        assert_eq!(
            stream["start_time"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap(),
            f64::from(origin)
        );
        assert_eq!(stream["nb_read_frames"], "4");
        for (time, index) in [
            (0, 0),
            (250_000, 1),
            (750_000, 3),
            (999_999, 3),
            (1_000_000, 3),
            (4_000_000, 3),
            (3_600_000_000, 3),
        ] {
            for input in [
                FrameTime::Microseconds(time),
                FrameTime::SourceTicks(i64::try_from(time).unwrap() * 254_016),
            ] {
                let bounded = input.clamp_video_duration(1_000_000).unwrap();
                let frame = decode_source_frame_policy(
                    Path::new("ffmpeg"),
                    &video,
                    bounded,
                    2,
                    2,
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                    true,
                )
                .unwrap();
                assert_eq!(
                    frame.rgba,
                    colors[index].repeat(4),
                    "origin {origin} input {input:?}"
                );
            }
        }
    }
}
