//! End-to-end render test against the real ffmpeg binary. It generates a tiny
//! test clip with lavfi, runs it through the actual `build_ffmpeg_args` +
//! process-policy render pipeline, and probes the output. Skipped (not failed) when
//! ffmpeg/ffprobe are not on PATH, so `cargo test` stays green without them;
//! CI installs ffmpeg so this runs for real there.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use video_editor_backend::model::EditRequest;
use video_editor_backend::process_control::ProcessRuntime;
use video_editor_backend::tools::{build_ffmpeg_args, check_tool, probe_video, run_ffmpeg, Done};

async fn tools_available(runtime: &ProcessRuntime) -> bool {
    check_tool(runtime, "ffmpeg", "-version").await.0
        && check_tool(runtime, "ffprobe", "-version").await.0
}

#[tokio::test]
async fn real_render_trim_scale_grayscale() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real_render_trim_scale_grayscale: ffmpeg/ffprobe not on PATH");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src.mp4");
    let output = dir.path().join("out.mp4");

    // Generate a 1s 320x240 test clip with a sine audio track.
    let gen = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=15:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-shortest",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(
        gen.status.success(),
        "failed to generate test clip: {}",
        String::from_utf8_lossy(&gen.stderr)
    );

    let req: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "trim": { "start": 0.0, "end": 0.5 },
        "scale": { "w": 160, "h": -2 },
        "filter": "grayscale",
        "mute": true
    }))
    .unwrap();

    let probe = probe_video(&runtime, &input).await.unwrap();
    let args = build_ffmpeg_args(&input, &output, &req, probe.duration);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(&runtime, &args, 0.5, &tx, &token, Duration::from_secs(60))
        .await
        .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));

    let meta = tokio::fs::metadata(&output).await.unwrap();
    assert!(meta.len() > 0, "output file should be non-empty");

    let out = probe_video(&runtime, &output).await.unwrap();
    assert!(out.duration > 0.0, "output should have a positive duration");
    assert_eq!(out.width, 160, "scale width should be applied");
}

#[tokio::test]
async fn real_render_denoise_sharpen_grain_look() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real_render_denoise_sharpen_grain_look: ffmpeg/ffprobe not on PATH");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src.mp4");
    let output = dir.path().join("out.mp4");

    let gen = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=15:duration=1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(gen.status.success());

    // Exercise the new effects through the real binary so a bad filter string fails.
    let req: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "denoise": true,
        "sharpen": 1.2,
        "grain": 15.0,
        "filter": "teal-orange",
        "mute": true
    }))
    .unwrap();
    let probe = probe_video(&runtime, &input).await.unwrap();
    let args = build_ffmpeg_args(&input, &output, &req, probe.duration);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(&runtime, &args, 1.0, &tx, &token, Duration::from_secs(60))
        .await
        .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    assert!(tokio::fs::metadata(&output).await.unwrap().len() > 0);
}

#[tokio::test]
async fn real_render_prores_with_audio_cleanup() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real_render_prores_with_audio_cleanup: ffmpeg/ffprobe not on PATH");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src.mp4");
    let output = dir.path().join("out.mov");

    let gen = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=15:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-shortest",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(gen.status.success());

    // ProRes export with loudnorm + highpass exercises prores_ks + the audio chain.
    let req: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "format": "prores",
        "normalizeAudio": true,
        "highpass": true
    }))
    .unwrap();
    let probe = probe_video(&runtime, &input).await.unwrap();
    let args = build_ffmpeg_args(&input, &output, &req, probe.duration);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(&runtime, &args, 1.0, &tx, &token, Duration::from_secs(60))
        .await
        .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    let out = probe_video(&runtime, &output).await.unwrap();
    assert!(out.duration > 0.0);
    assert_eq!(out.vcodec.as_deref(), Some("prores"));
}
