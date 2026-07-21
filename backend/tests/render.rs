//! End-to-end render test against the real ffmpeg binary. It generates a tiny
//! test clip with lavfi, runs it through the actual export compiler +
//! process-policy render pipeline, and probes the output. Skipped (not failed) when
//! ffmpeg/ffprobe are not on PATH, so `cargo test` stays green without them;
//! CI installs ffmpeg so this runs for real there.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use video_editor_backend::config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use video_editor_backend::domain::artifact_graph::Fingerprint;
use video_editor_backend::model::EditRequest;
use video_editor_backend::ports::{
    CompiledExportCommand, ExportCommandCompiler, ExportCompileRequest,
};
use video_editor_backend::process_control::ProcessRuntime;
use video_editor_backend::services::render::{
    EditPlan, ExportExecutionProfile, RenderExecution, RenderResources, SourceMediaMetadata,
};
use video_editor_backend::tools::{
    check_tool, probe_video, run_compiled_ffmpeg, run_ffmpeg, Done, FfmpegExportCompiler,
};

async fn tools_available(runtime: &ProcessRuntime) -> bool {
    check_tool(runtime, "ffmpeg", "-version").await.0
        && check_tool(runtime, "ffprobe", "-version").await.0
}

fn compile_export(
    input: &std::path::Path,
    output: &std::path::Path,
    request: EditRequest,
    source: &video_editor_backend::tools::ProbeInfo,
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
    source: &video_editor_backend::tools::ProbeInfo,
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
