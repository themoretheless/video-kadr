use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    domain::concat_retime_edit::Clock,
    process_control::ProcessRuntime,
    services::concat_freeze_capture::{capture, CapturedStill, Tools},
};
fn movie(dir: &Path, rate: &str, portrait: bool, offset: bool) -> PathBuf {
    textured_movie(dir, rate, portrait, offset, false)
}
fn textured_movie(dir: &Path, rate: &str, portrait: bool, offset: bool, texture: bool) -> PathBuf {
    let (w, h) = if portrait { (16, 28) } else { (16, 16) };
    let mut bytes = vec![];
    for colour in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
        for pixel in 0..w * h {
            let mut c = colour;
            if texture {
                for channel in 0..3 {
                    if c[channel] == 0 {
                        c[channel] = ((pixel * 7 + channel * 3) % 31) as u8;
                    }
                }
            }
            bytes.extend_from_slice(&c);
        }
    }
    let raw = dir.join("frames.rgba");
    std::fs::write(&raw, bytes).unwrap();
    let out = dir.join("frames.nut");
    let mut command = Command::new("/opt/homebrew/bin/ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{w}x{h}"),
            "-framerate",
            rate,
            "-i",
        ])
        .arg(raw);
    if offset {
        command.args([
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=mono:d=4",
            "-vf",
            "setpts=PTS+2/TB",
        ]);
    }
    command.args(["-map", "0:v:0"]);
    if offset {
        command.args(["-map", "1:a:0", "-c:a", "pcm_s16le"]);
    }
    let result = command
        .args([
            "-frames:v",
            "3",
            "-fps_mode",
            "passthrough",
            "-c:v",
            "ffv1",
            "-pix_fmt",
            "bgra",
            "-avoid_negative_ts",
            "disabled",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    out
}
async fn still(input: &Path, us: u64) -> CapturedStill {
    let clock = Clock {
        start: 0.,
        duration: 10.,
        source_start: 0.,
        speed: 1.,
        speed_curve: None,
    };
    capture(
        &ProcessRuntime::local_default(),
        Tools {
            ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe"),
        },
        input,
        &clock.still_capture(us as f64 / 1e6).unwrap(),
        Duration::from_secs(20),
        &CancellationToken::new(),
    )
    .await
    .unwrap()
}
fn dominant(still: &CapturedStill) -> usize {
    let frame = image::load_from_memory(&still.jpeg).unwrap().to_rgb8();
    assert_eq!(frame.dimensions(), (still.width, still.height));
    let p = frame.get_pixel(still.width / 2, still.height / 2).0;
    let index = (0..3).max_by_key(|i| p[*i]).unwrap();
    assert!(p[index] > 240);
    index
}
#[tokio::test]
async fn capture_selects_first_at_or_after_and_only_eof_returns_the_first_picture() {
    for offset in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let input = movie(dir.path(), "10", false, offset);
        for (us, colour, fallback) in [
            (0, 0, false),
            (50_000, 1, false),
            (100_000, 1, false),
            (100_001, 2, false),
            (700_000, 0, true),
        ] {
            let frame = still(&input, us).await;
            assert_eq!(dominant(&frame), colour, "offset={offset} us={us}");
            assert_eq!(frame.used_first_frame, fallback);
        }
    }
}
#[tokio::test]
async fn capture_preserves_portrait_fixed_width_and_fractional_fps_selection() {
    let dir = tempfile::tempdir().unwrap();
    let input = movie(dir.path(), "30000/1001", true, false);
    let first = still(&input, 33366).await;
    assert_eq!((first.width, first.height), (1280, 2240));
    assert_eq!(dominant(&first), 1);
    let next = still(&input, 33367).await;
    assert_eq!(dominant(&next), 2);
}
#[tokio::test]
#[ignore = "requires scripts/qualify-concat-freeze-pixels.py"]
async fn jpeg_packets_match_unchanged_original_decoder_and_encoder() {
    let oracle = std::env::var("CONCAT_FREEZE_PIXEL_ORACLE").expect("build original still oracle");
    let mut cases = 0;
    for (rate, portrait, offset, texture, times) in [
        ("10", false, false, false, vec![0, 50_000, 100_001, 700_000]),
        ("10", false, true, false, vec![0, 50_000, 100_001, 700_000]),
        (
            "30000/1001",
            true,
            false,
            false,
            vec![0, 33366, 33367, 700_000],
        ),
        (
            "30000/1001",
            false,
            false,
            true,
            vec![0, 33366, 33367, 700_000],
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let input = textured_movie(dir.path(), rate, portrait, offset, texture);
        for us in times {
            let ours = still(&input, us).await;
            let jpg = dir.path().join("original.jpg");
            let rgba = dir.path().join("original.rgba");
            let result = Command::new(&oracle)
                .arg(&input)
                .arg(us.to_string())
                .arg(&jpg)
                .arg(&rgba)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let expected = std::fs::read(&jpg).unwrap();
            assert!(ours.jpeg == expected,"original JPEG differs: rate={rate} portrait={portrait} offset={offset} texture={texture} us={us}; native={} reference={} firstDiff={:?}",ours.jpeg.len(),expected.len(),ours.jpeg.iter().zip(&expected).position(|(a,b)|a!=b));
            cases += 1;
        }
    }
    assert_eq!(cases, 16);
}
#[tokio::test]
async fn invalid_cancelled_or_failed_capture_never_publishes_a_picture() {
    let dir = tempfile::tempdir().unwrap();
    let input = movie(dir.path(), "10", false, false);
    let clock = Clock {
        start: 0.,
        duration: 1.,
        source_start: 0.,
        speed: 1.,
        speed_curve: None,
    };
    let plan = clock.still_capture(0.1).unwrap();
    let runtime = ProcessRuntime::local_default();
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(capture(
        &runtime,
        Tools {
            ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe")
        },
        &input,
        &plan,
        Duration::from_secs(10),
        &cancelled
    )
    .await
    .is_err());
    assert!(capture(
        &runtime,
        Tools {
            ffmpeg: Path::new("/usr/bin/false"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe")
        },
        &input,
        &plan,
        Duration::from_secs(10),
        &CancellationToken::new()
    )
    .await
    .is_err());
    assert!(capture(
        &runtime,
        Tools {
            ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe")
        },
        &input,
        &plan,
        Duration::ZERO,
        &CancellationToken::new()
    )
    .await
    .is_err());
}

#[tokio::test]
async fn native_freeze_plan_uses_scalar_capture_even_when_preview_curve_selects_another_picture() {
    use serde_json::json;
    use video_kadr_backend::domain::concat_retime_edit::{plan, Point};
    let dir = tempfile::tempdir().unwrap();
    let input = movie(dir.path(), "10", false, false);
    let clock = Clock {
        start: 0.,
        duration: 1.,
        source_start: 0.,
        speed: 1.,
        speed_curve: Some(vec![
            Point { at: 0., speed: 1. },
            Point { at: 1., speed: 3. },
        ]),
    };
    let result=plan(serde_json::from_value(json!({"clock":clock,"rateNum":25,"rateDen":1,"command":{"kind":"freeze","time":0.1,"duration":1.}})).unwrap()).unwrap();
    let capture_plan = result.capture.unwrap();
    let runtime = ProcessRuntime::local_default();
    let cancel = CancellationToken::new();
    let capture_at = |plan| {
        capture(
            &runtime,
            Tools {
                ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
                ffprobe: Path::new("/opt/homebrew/bin/ffprobe"),
            },
            &input,
            plan,
            Duration::from_secs(20),
            &cancel,
        )
    };
    let frozen = capture_at(&capture_plan).await.unwrap();
    assert_eq!(dominant(&frozen), 1);
    let p = clock.placement(25, 1).unwrap();
    let curve = p
        .concat_retime
        .as_ref()
        .unwrap()
        .sample(0, 0, (0.1 * 254016000000.) as i128)
        .unwrap()
        .unwrap();
    let mut wrong = clock.still_capture(0.1).unwrap();
    wrong.source_seconds = curve as f64 / 254016000000.;
    wrong.seek_time_us = (wrong.source_seconds * 1e6).round() as u64;
    wrong.cache_time_ms = (wrong.source_seconds * 1000.).round() as u64;
    let curved_preview = capture_at(&wrong).await.unwrap();
    assert_eq!(dominant(&curved_preview), 2);
    assert_ne!(frozen.jpeg, curved_preview.jpeg);
}

fn multivideo(dir: &Path, default_second: bool, impaired_first: bool) -> PathBuf {
    let output = dir.join("multi.mkv");
    let result = Command::new("/opt/homebrew/bin/ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=10:d=0.3",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=32x16:r=10:d=0.3",
            "-map",
            "0:v",
            "-map",
            "1:v",
            "-c:v",
            "ffv1",
            "-disposition:v:0",
            if impaired_first {
                "visual_impaired"
            } else if default_second {
                "0"
            } else {
                "default"
            },
            "-disposition:v:1",
            if default_second { "default" } else { "0" },
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    output
}
fn rotated(dir: &Path, degrees: i32) -> PathBuf {
    let source = textured_movie(dir, "10", true, false, true);
    let base = dir.join("base.mp4");
    let output = dir.join("rotated.mp4");
    for (first, args) in [
        (
            true,
            vec![
                "-i".to_owned(),
                source.to_str().unwrap().to_owned(),
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                "0".into(),
                "-pix_fmt".into(),
                "yuv444p".into(),
            ],
        ),
        (
            false,
            vec![
                "-i".into(),
                base.to_str().unwrap().into(),
                "-c".into(),
                "copy".into(),
                "-metadata:s:v:0".into(),
                format!("rotate={degrees}"),
            ],
        ),
    ] {
        let result = Command::new("/opt/homebrew/bin/ffmpeg")
            .args(["-nostdin", "-v", "error"])
            .args(args)
            .arg(if first { &base } else { &output })
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    output
}
fn original_jpeg(oracle: &str, input: &Path, us: u64, dir: &Path) -> Vec<u8> {
    let jpg = dir.join("reference.jpg");
    let raw = dir.join("reference.rgba");
    let result = Command::new(oracle)
        .arg(input)
        .arg(us.to_string())
        .arg(&jpg)
        .arg(raw)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(jpg).unwrap()
}
#[tokio::test]
async fn native_selector_uses_default_stream_and_not_the_first_video() {
    assert!(Path::new(env!("CARGO_BIN_EXE_video-kadr-media-probe")).is_file());
    for (default, impaired, expected, height) in [
        (true, false, 2, 640),
        (false, false, 0, 1280),
        (false, true, 2, 640),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let input = multivideo(dir.path(), default, impaired);
        let frame = still(&input, 50_000).await;
        assert_eq!(dominant(&frame), expected);
        assert_eq!(frame.height, height);
    }
}
#[tokio::test]
#[ignore = "requires scripts/qualify-concat-freeze-pixels.py"]
async fn rotated_and_multivideo_capture_packets_match_original() {
    let oracle = std::env::var("CONCAT_FREEZE_PIXEL_ORACLE").unwrap();
    let mut compared = 0;
    for (default, impaired) in [(true, false), (false, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        let input = multivideo(dir.path(), default, impaired);
        for us in [0, 50_000, 700_000] {
            let actual = still(&input, us).await;
            let expected = original_jpeg(&oracle, &input, us, dir.path());
            assert!(
                actual.jpeg == expected,
                "multivideo default={default} impaired={impaired} us={us}"
            );
            compared += 1;
        }
    }
    for degrees in [90, 180, 270] {
        let dir = tempfile::tempdir().unwrap();
        let input = rotated(dir.path(), degrees);
        for us in [0, 50_000, 700_000] {
            let actual = still(&input, us).await;
            let expected = original_jpeg(&oracle, &input, us, dir.path());
            assert!(
                actual.jpeg == expected,
                "rotation={degrees} us={us}; native={} reference={}",
                actual.jpeg.len(),
                expected.len()
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 18);
}
