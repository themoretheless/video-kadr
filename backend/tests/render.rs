//! End-to-end render test against the real ffmpeg binary. It generates a tiny
//! test clip with lavfi, runs it through the actual export compiler +
//! process-policy render pipeline, and probes the output. Skipped (not failed) when
//! ffmpeg/ffprobe are not on PATH, so `cargo test` stays green without them;
//! CI installs ffmpeg so this runs for real there.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use video_kadr_backend::config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use video_kadr_backend::domain::artifact_graph::Fingerprint;
use video_kadr_backend::model::EditRequest;
use video_kadr_backend::ports::{
    CompiledExportCommand, ExportCommandCompiler, ExportCompileRequest,
};
use video_kadr_backend::process_control::ProcessRuntime;
use video_kadr_backend::services::render::{
    EditPlan, ExportExecutionProfile, RenderExecution, RenderResources, SourceMediaMetadata,
};
use video_kadr_backend::tools::{
    check_tool, probe_video, run_compiled_ffmpeg, run_ffmpeg, Done, FfmpegExportCompiler,
};

async fn tools_available(runtime: &ProcessRuntime) -> bool {
    check_tool(runtime, "ffmpeg", "-version").await.0
        && check_tool(runtime, "ffprobe", "-version").await.0
}

async fn sample_rgb(path: &std::path::Path, at_seconds: f64) -> [u8; 3] {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at_seconds:.3}"), "-i"])
        .arg(path)
        .args([
            "-vf",
            "scale=1:1:flags=area,format=rgb24",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success() && output.stdout.len() >= 3,
        "failed to sample frame: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    [output.stdout[0], output.stdout[1], output.stdout[2]]
}

async fn sample_rgba(path: &std::path::Path, x: u32, y: u32) -> [u8; 4] {
    let filter = format!("format=rgba,crop=1:1:{x}:{y}");
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-vf", &filter, "-frames:v", "1", "-f", "rawvideo", "pipe:1"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success() && output.stdout.len() >= 4,
        "failed to sample RGBA pixel: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    [
        output.stdout[0],
        output.stdout[1],
        output.stdout[2],
        output.stdout[3],
    ]
}

fn assert_dominant(rgb: [u8; 3], channel: usize) {
    let dominant = rgb[channel];
    for (index, value) in rgb.into_iter().enumerate() {
        if index != channel {
            assert!(
                dominant > value.saturating_add(35),
                "expected channel {channel} to dominate in {rgb:?}"
            );
        }
    }
}

fn compile_export(
    input: &std::path::Path,
    output: &std::path::Path,
    request: EditRequest,
    source: &video_kadr_backend::tools::ProbeInfo,
) -> CompiledExportCommand {
    let plan = Arc::new(
        EditPlan::compile(
            Fingerprint::digest(b"real-render-source"),
            request,
            SourceMediaMetadata::new_with_audio(
                source.width,
                source.height,
                source.duration,
                source.acodec.is_some(),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let execution = RenderExecution::new(
        plan,
        ExportExecutionProfile {
            encode_budget: EncodeBudget::for_profile(
                EncodeProfile::Balanced,
                RuntimeLimits {
                    logical_cpus: 4,
                    memory_mib: Some(2048),
                },
            )
            .unwrap(),
            verify_checksums: true,
        },
    );
    FfmpegExportCompiler
        .compile(ExportCompileRequest {
            input,
            destination: output,
            parallel_jobs: 1,
            execution: &execution,
        })
        .unwrap()
}

fn compile_export_with_lut(
    input: &std::path::Path,
    output: &std::path::Path,
    lut_path: &std::path::Path,
    request: EditRequest,
    source: &video_kadr_backend::tools::ProbeInfo,
) -> CompiledExportCommand {
    let plan = Arc::new(
        EditPlan::compile(
            Fingerprint::digest(b"real-lut-render-source"),
            request,
            SourceMediaMetadata::new(source.width, source.height, source.duration).unwrap(),
        )
        .unwrap(),
    );
    let execution = RenderExecution::new_with_resources(
        plan,
        ExportExecutionProfile {
            encode_budget: EncodeBudget::for_profile(
                EncodeProfile::Balanced,
                RuntimeLimits {
                    logical_cpus: 4,
                    memory_mib: Some(2048),
                },
            )
            .unwrap(),
            verify_checksums: true,
        },
        RenderResources::with_lut_path(lut_path),
    );
    FfmpegExportCompiler
        .compile(ExportCompileRequest {
            input,
            destination: output,
            parallel_jobs: 1,
            execution: &execution,
        })
        .unwrap()
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
    let command = compile_export(&input, &output, req, &probe);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(
        &runtime,
        &command.arguments,
        command.expected_duration_seconds,
        &tx,
        &token,
        Duration::from_secs(60),
    )
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
async fn real_render_extended_speed_keeps_audio_and_video_duration() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real_render_extended_speed_keeps_audio_and_video_duration: ffmpeg/ffprobe not on PATH");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("speed-source.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=30:duration=1",
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
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let source = probe_video(&runtime, &input).await.unwrap();

    for (speed, expected) in [(4.0, 0.25), (0.25, 4.0)] {
        let output = dir.path().join(format!("speed-{speed}.mp4"));
        let request: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "x", "speed": speed
        }))
        .unwrap();
        let command = compile_export(&input, &output, request, &source);
        let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let token = CancellationToken::new();
        let done = run_compiled_ffmpeg(&runtime, &command, &tx, &token, Duration::from_secs(60))
            .await
            .unwrap();
        drop(tx);
        let _ = drain.await;
        assert!(matches!(done, Done::Completed));
        let rendered = probe_video(&runtime, &output).await.unwrap();
        assert!(rendered.acodec.is_some(), "speed {speed} lost audio");
        assert!(
            (rendered.duration - expected).abs() <= 0.12,
            "speed {speed}: expected {expected}s, got {}s",
            rendered.duration,
        );
    }
}

#[tokio::test]
async fn real_trim_selects_the_requested_sixty_fps_frame() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_trim_selects_the_requested_sixty_fps_frame: ffmpeg/ffprobe not on PATH"
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("frame-source.mp4");
    let output = dir.path().join("frame-trim.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:size=160x90:rate=60:duration=0.016667",
            "-f",
            "lavfi",
            "-i",
            "color=green:size=160x90:rate=60:duration=0.016667",
            "-f",
            "lavfi",
            "-i",
            "color=blue:size=160x90:rate=60:duration=0.016667",
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map",
            "[v]",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let source = probe_video(&runtime, &input).await.unwrap();
    let request: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x", "trim": { "start": 0.016667, "end": 0.033334 }, "mute": true
    }))
    .unwrap();
    let command = compile_export(&input, &output, request, &source);
    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_compiled_ffmpeg(&runtime, &command, &tx, &token, Duration::from_secs(60))
        .await
        .unwrap();
    drop(tx);
    let _ = drain.await;
    assert!(matches!(done, Done::Completed));
    assert_dominant(sample_rgb(&output, 0.0).await, 1);
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
    let command = compile_export(&input, &output, req, &probe);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(
        &runtime,
        &command.arguments,
        command.expected_duration_seconds,
        &tx,
        &token,
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    assert!(tokio::fs::metadata(&output).await.unwrap().len() > 0);
}

#[tokio::test]
async fn real_render_chroma_key_preserves_foreground_and_keys_background() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_render_chroma_key_preserves_foreground_and_keys_background: ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("green-screen.mp4");
    let output = dir.path().join("keyed.png");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=0x00ff00:size=64x64:rate=5:duration=0.4",
            "-vf",
            "drawbox=x=20:y=20:w=24:h=24:color=red:t=fill",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "failed to generate green-screen source: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let request: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "format": "png",
        "mute": true,
        "chromaKey": {
            "keyColor": "#00ff00",
            "similarity": 0.25,
            "blend": 0.02,
            "spillSuppression": 0.5
        }
    }))
    .unwrap();
    let source = probe_video(&runtime, &input).await.unwrap();
    let command = compile_export(&input, &output, request, &source);
    assert!(command
        .arguments
        .iter()
        .any(|argument| argument.contains("chromakey=")));
    assert!(command
        .arguments
        .iter()
        .any(|argument| argument.contains("despill=")));

    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    let done = run_compiled_ffmpeg(
        &runtime,
        &command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    assert!(matches!(done, Done::Completed));

    let background = sample_rgba(&output, 2, 2).await;
    let foreground = sample_rgba(&output, 32, 32).await;
    assert!(
        background[3] < 32,
        "background should be transparent: {background:?}"
    );
    assert!(
        foreground[3] > 220,
        "foreground should stay opaque: {foreground:?}"
    );
    assert_dominant([foreground[0], foreground[1], foreground[2]], 0);
}

#[tokio::test]
async fn real_render_partial_lut_with_master_and_rgb_curves() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_render_partial_lut_with_master_and_rgb_curves: ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src.mp4");
    let output = dir.path().join("out.mp4");
    let lut_path = dir.path().join("inverted look.cube");

    let gen = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=96x64:rate=10:duration=0.6",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.6",
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
        "failed to generate LUT test clip: {}",
        String::from_utf8_lossy(&gen.stderr)
    );

    // A valid 2x2x2 LUT that inverts every RGB channel. The space in its
    // temporary filename also exercises filter-option quoting in the real binary.
    tokio::fs::write(
        &lut_path,
        b"LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n1 1 1\n0 1 1\n1 0 1\n0 0 1\n1 1 0\n0 1 0\n1 0 0\n0 0 0\n",
    )
    .await
    .unwrap();

    let request: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "lut": { "id": "look-1", "intensity": 0.4 },
        "curves": {
            "master": [{"x": 0.0, "y": 0.05}, {"x": 0.5, "y": 0.55}, {"x": 1.0, "y": 0.95}],
            "red": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.9}],
            "green": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 1.0}],
            "blue": [{"x": 0.0, "y": 0.0}, {"x": 0.5, "y": 0.6}, {"x": 1.0, "y": 1.0}]
        }
    }))
    .unwrap();
    let probe = probe_video(&runtime, &input).await.unwrap();
    let command = compile_export_with_lut(&input, &output, &lut_path, request, &probe);

    let graph_index = command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .expect("partial LUT should compile to a complex graph");
    let graph = &command.arguments[graph_index + 1];
    assert!(graph.contains("curves=master="), "{graph}");
    assert!(graph.contains(":red="), "{graph}");
    assert!(graph.contains(":green="), "{graph}");
    assert!(graph.contains(":blue="), "{graph}");
    assert!(graph.contains("lut3d=file="), "{graph}");
    assert!(graph.contains("format=gbrap16le"), "{graph}");
    assert!(
        graph.find("format=gbrap16le").unwrap() < graph.find("split=2").unwrap(),
        "{graph}"
    );
    assert!(graph.find("curves=").unwrap() < graph.find("split=2").unwrap());
    assert_eq!(
        command.read_only_files.as_slice(),
        std::slice::from_ref(&lut_path)
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_compiled_ffmpeg(&runtime, &command, &tx, &token, Duration::from_secs(60))
        .await
        .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    assert!(tokio::fs::metadata(&output).await.unwrap().len() > 0);
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!(rendered.duration > 0.0);
    assert_eq!((rendered.width, rendered.height), (96, 64));
    assert_eq!(rendered.acodec.as_deref(), Some("aac"));
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
    let command = compile_export(&input, &output, req, &probe);

    let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let token = CancellationToken::new();
    let done = run_ffmpeg(
        &runtime,
        &command.arguments,
        command.expected_duration_seconds,
        &tx,
        &token,
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(tx);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    let out = probe_video(&runtime, &output).await.unwrap();
    assert!(out.duration > 0.0);
    assert_eq!(out.vcodec.as_deref(), Some("prores"));
}

#[tokio::test]
async fn real_segment_render_accepts_video_without_audio() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_segment_render_accepts_video_without_audio: ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("src.mp4");
    let output = dir.path().join("out.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=15:duration=2",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "failed to generate video-only clip: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let request: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "segments": [
            { "start": 0.0, "end": 0.5 },
            { "start": 1.0, "end": 1.5 }
        ],
        "normalizeAudio": true
    }))
    .unwrap();
    let source = probe_video(&runtime, &input).await.unwrap();
    assert!(source.acodec.is_none());
    let command = compile_export(&input, &output, request, &source);

    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    let done = run_ffmpeg(
        &runtime,
        &command.arguments,
        command.expected_duration_seconds,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;

    assert!(matches!(done, Done::Completed));
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!(rendered.duration > 0.0);
    assert!(rendered.acodec.is_none());
}

#[tokio::test]
async fn real_ordered_segments_preserve_duplicates_overlaps_and_audio() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_ordered_segments_preserve_duplicates_overlaps_and_audio: ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("ordered-source.mp4");
    let output = dir.path().join("ordered-output.mp4");
    let generated = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=64x64:rate=20:duration=0.4",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:size=64x64:rate=20:duration=0.4",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:size=64x64:rate=20:duration=0.4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1.2",
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map",
            "[v]",
            "-map",
            "3:a",
            "-shortest",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "failed to generate ordered source: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let request: EditRequest = serde_json::from_value(serde_json::json!({
        "videoId": "x",
        "segments": [
            { "start": 0.8, "end": 1.2 },
            { "start": 0.0, "end": 0.4 },
            { "start": 0.0, "end": 0.4 },
            { "start": 0.2, "end": 0.6 }
        ]
    }))
    .unwrap();
    let source = probe_video(&runtime, &input).await.unwrap();
    assert!(source.acodec.is_some());
    let command = compile_export(&input, &output, request, &source);
    assert!((command.expected_duration_seconds - 1.6).abs() < 0.01);

    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    let done = run_ffmpeg(
        &runtime,
        &command.arguments,
        command.expected_duration_seconds,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    assert!(matches!(done, Done::Completed));

    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!(rendered.acodec.is_some());
    assert!((rendered.duration - 1.6).abs() < 0.12, "{rendered:?}");
    assert_dominant(sample_rgb(&output, 0.1).await, 2);
    assert_dominant(sample_rgb(&output, 0.5).await, 0);
    assert_dominant(sample_rgb(&output, 0.9).await, 0);
    assert_dominant(sample_rgb(&output, 1.5).await, 1);
}
