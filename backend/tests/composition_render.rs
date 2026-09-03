//! Real FFmpeg smoke test for the first multi-source composition export slice.
//! It is skipped when ffmpeg/ffprobe are unavailable, matching `render.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use video_kadr_backend::domain::composition::{
    AnimatableValue, AudioClip, AudioDucking, AudioVoiceEffect, BlendMode, CanvasBackgroundMode,
    CanvasSpec, ClipPlacement, ClipTransition, Composition, CompositionClipId, CompositionSource,
    CompositionTrack, FrameInterpolation, ImageClip, MaskShape, PlaybackMode, Rgba, SourceId,
    SourceKind, SpeedRampAudioPolicy, SpeedRampInterpolation, SpeedRampPoint, SpeedRampSpec,
    StabilizationSpec, TextClip, TextStyle, TrackId, TransformSpec, TransitionId, TransitionKind,
    VideoClip, VideoEffect, VideoEffectPreset,
};
use video_kadr_backend::domain::keyframes::{Interpolation, Keyframe, KeyframeTrack};
use video_kadr_backend::domain::media_probe::StreamKind;
use video_kadr_backend::ports::{
    CompositionAudioCodec, CompositionAv1Encoder, CompositionExportCommandCompiler,
    CompositionExportCompileRequest, CompositionExportProfile, CompositionExportRange,
    CompositionExportSpec, CompositionMp4Codec, CompositionProResProfile, CompositionTextResource,
    CompositionWebmCodec,
};
use video_kadr_backend::process_control::ProcessRuntime;
use video_kadr_backend::tools::{
    check_tool, inspect_ffmpeg_support, probe_video, run_compiled_ffmpeg, Done,
    FfmpegCompositionExportCompiler,
};

async fn tools_available(runtime: &ProcessRuntime) -> bool {
    check_tool(runtime, "ffmpeg", "-version").await.0
        && check_tool(runtime, "ffprobe", "-version").await.0
}

fn mp4_h264_output(video_quality: u32) -> CompositionExportSpec {
    CompositionExportSpec {
        profile: CompositionExportProfile::default(),
        video_quality,
        video_bitrate_kbps: None,
        av1_encoder: None,
        range: None,
    }
}

async fn generate(args: &[&str], output: &Path) {
    let generated = tokio::process::Command::new("ffmpeg")
        .args(args)
        .arg(output)
        .output()
        .await
        .unwrap();
    assert!(
        generated.status.success(),
        "failed to generate {}: {}",
        output.display(),
        String::from_utf8_lossy(&generated.stderr)
    );
}

async fn sample_rgb(path: &Path, at_seconds: f64) -> [u8; 3] {
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

async fn sample_pixel(path: &Path, at_seconds: f64, x: u32, y: u32) -> [u8; 3] {
    let filter = format!("format=rgb24,crop=1:1:{x}:{y}");
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at_seconds:.3}"), "-i"])
        .arg(path)
        .args(["-vf", &filter, "-frames:v", "1", "-f", "rawvideo", "pipe:1"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success() && output.stdout.len() >= 3,
        "failed to sample pixel ({x}, {y}): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    [output.stdout[0], output.stdout[1], output.stdout[2]]
}

async fn frame_rgb(path: &Path, at_seconds: f64, width: u32, height: u32) -> Vec<u8> {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at_seconds:.3}"), "-i"])
        .arg(path)
        .args([
            "-vf",
            "format=rgb24",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    let expected = width as usize * height as usize * 3;
    assert!(
        output.status.success() && output.stdout.len() >= expected,
        "failed to read RGB frame: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

async fn channel_rms(path: &Path, start_seconds: f64, duration_seconds: f64) -> [f64; 2] {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-ss",
            &format!("{start_seconds:.3}"),
            "-t",
            &format!("{duration_seconds:.3}"),
            "-map",
            "0:a:0",
            "-af",
            "aformat=sample_fmts=flt:sample_rates=48000:channel_layouts=stereo",
            "-c:a",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success() && output.stdout.len() >= 8,
        "failed to sample stereo audio: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut squares = [0.0_f64; 2];
    let mut frames = 0_u64;
    for frame in output.stdout.chunks_exact(8) {
        let left = f32::from_le_bytes(frame[0..4].try_into().unwrap()) as f64;
        let right = f32::from_le_bytes(frame[4..8].try_into().unwrap()) as f64;
        squares[0] += left * left;
        squares[1] += right * right;
        frames += 1;
    }
    assert!(frames > 0, "audio sample window was empty");
    [
        (squares[0] / frames as f64).sqrt(),
        (squares[1] / frames as f64).sqrt(),
    ]
}

async fn band_rms(path: &Path, start_seconds: f64, frequency: u32) -> f64 {
    let filter = format!("bandpass=f={frequency}:width_type=h:w=80,aformat=sample_fmts=flt:sample_rates=48000:channel_layouts=mono");
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{start_seconds:.3}"), "-i"])
        .arg(path)
        .args([
            "-t",
            "0.200",
            "-map",
            "0:a:0",
            "-af",
            &filter,
            "-c:a",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to sample audio band: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let samples: Vec<_> = output
        .stdout
        .chunks_exact(4)
        .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()) as f64)
        .collect();
    assert!(!samples.is_empty());
    (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt()
}

async fn video_frame_count(path: &Path) -> u64 {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to count frames: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

async fn stream_duration(path: &Path, selector: &str) -> f64 {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            selector,
            "-show_entries",
            "stream=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to read {selector} duration: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

async fn stream_packet_end(path: &Path, selector: &str) -> f64 {
    let output = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            selector,
            "-show_entries",
            "packet=pts_time,duration_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to read {selector} packets: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let end = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let mut values = line.split(',');
            let pts = values.next()?.trim().parse::<f64>().ok()?;
            let duration = values
                .next()
                .and_then(|value| value.trim().parse::<f64>().ok())
                .unwrap_or(0.0);
            Some(pts + duration)
        })
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(end.is_finite(), "{selector} had no timestamped packets");
    end
}

async fn unique_rgb_frame_count(path: &Path, width: u32, height: u32) -> usize {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "format=rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to decode frames: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frame_size = width as usize * height as usize * 3;
    assert_eq!(output.stdout.len() % frame_size, 0);
    output
        .stdout
        .chunks_exact(frame_size)
        .map(<[u8]>::to_vec)
        .collect::<BTreeSet<_>>()
        .len()
}

/// Mean absolute luma change between adjacent frames in the central raster.
/// The margin excludes stabilization edge fill, so this measures residual
/// camera jitter in scene content rather than the chosen mirror policy.
async fn temporal_luma_jitter(path: &Path, width: u32, height: u32, margin: u32) -> f64 {
    assert!(margin * 2 < width && margin * 2 < height);
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "format=gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "failed to decode luma frames: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frame_size = width as usize * height as usize;
    assert_eq!(output.stdout.len() % frame_size, 0);
    let frames: Vec<_> = output.stdout.chunks_exact(frame_size).collect();
    assert!(frames.len() >= 2);
    let mut total = 0_u128;
    let mut samples = 0_u128;
    for pair in frames.windows(2) {
        for y in margin..height - margin {
            for x in margin..width - margin {
                let index = (y * width + x) as usize;
                total += u128::from(pair[0][index].abs_diff(pair[1][index]));
                samples += 1;
            }
        }
    }
    total as f64 / samples as f64
}

fn unicode_font() -> Option<PathBuf> {
    [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/opentype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
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

fn source_id(value: &str) -> SourceId {
    SourceId::parse(value).unwrap()
}

fn placement(timeline_start_tick: u64, source_out_tick: u64) -> ClipPlacement {
    ClipPlacement {
        timeline_start_tick,
        source_in_tick: 0,
        source_out_tick,
        speed: 1.0,
        speed_ramp: None,
    }
}

fn video_clip(value: &str, source: &str, placement: ClipPlacement) -> VideoClip {
    VideoClip {
        id: CompositionClipId::parse(value).unwrap(),
        source_id: source_id(source),
        placement,
        playback_mode: PlaybackMode::Forward,
        stabilization: StabilizationSpec::Disabled,
        frame_interpolation: FrameInterpolation::Duplicate,
        transform: TransformSpec::default(),
        opacity: AnimatableValue::constant(1.0),
        blend_mode: BlendMode::Normal,
        effects: Vec::new(),
        source_audio_enabled: true,
        audio_gain: AnimatableValue::constant(1.0),
        audio_pan: AnimatableValue::constant(0.0),
        enabled: true,
    }
}

fn animated(interpolation: Interpolation, from: f64, to: f64) -> AnimatableValue {
    AnimatableValue::Keyframes {
        track: KeyframeTrack::new(
            1_000_000,
            interpolation,
            vec![
                Keyframe {
                    tick: 0,
                    value: from,
                },
                Keyframe {
                    tick: 1_000_000,
                    value: to,
                },
            ],
        )
        .unwrap(),
    }
}

#[tokio::test]
async fn real_overlay_transitions_use_exact_handles_and_preserve_primary_timeline() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real overlay transitions: ffmpeg/ffprobe not on PATH");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let primary = directory.path().join("primary-gray.mp4");
    let red = directory.path().join("overlay-red.mp4");
    let blue = directory.path().join("overlay-blue.mp4");
    for (path, color, duration) in [
        (&primary, "gray", "1.6"),
        (&red, "red", "1.2"),
        (&blue, "blue", "1.2"),
    ] {
        generate(
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("color=c={color}:size=64x48:rate=20:duration={duration}"),
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
            path,
        )
        .await;
    }

    for kind in [
        TransitionKind::Dissolve,
        TransitionKind::FadeBlack,
        TransitionKind::WipeLeft,
        TransitionKind::WipeRight,
        TransitionKind::WipeUp,
        TransitionKind::WipeDown,
        TransitionKind::SmoothLeft,
        TransitionKind::SmoothRight,
        TransitionKind::SmoothUp,
        TransitionKind::SmoothDown,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::CircleOpen,
        TransitionKind::CircleClose,
        TransitionKind::WipeTopLeft,
        TransitionKind::WipeTopRight,
        TransitionKind::WipeBottomLeft,
        TransitionKind::WipeBottomRight,
        TransitionKind::VerticalOpen,
        TransitionKind::VerticalClose,
        TransitionKind::HorizontalOpen,
        TransitionKind::HorizontalClose,
    ] {
        let mut composition = Composition::new(CanvasSpec {
            width: 64,
            height: 48,
            fps_milli: 20_000,
            background: Rgba {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 1.0,
            },
            ..CanvasSpec::default()
        });
        for source in [
            CompositionSource {
                id: source_id("primary-gray"),
                kind: SourceKind::Video,
                duration_ticks: 1_600_000,
                width: 64,
                height: 48,
                has_audio: false,
            },
            CompositionSource {
                id: source_id("overlay-red"),
                kind: SourceKind::Video,
                duration_ticks: 1_200_000,
                width: 64,
                height: 48,
                has_audio: false,
            },
            CompositionSource {
                id: source_id("overlay-blue"),
                kind: SourceKind::Video,
                duration_ticks: 1_200_000,
                width: 64,
                height: 48,
                has_audio: false,
            },
        ] {
            composition.sources.insert(source.id.clone(), source);
        }
        let mut red_clip = video_clip(
            "overlay-red-clip",
            "overlay-red",
            ClipPlacement {
                timeline_start_tick: 0,
                source_in_tick: 100_000,
                source_out_tick: 900_000,
                speed: 1.0,
                speed_ramp: None,
            },
        );
        let mut blue_clip = video_clip(
            "overlay-blue-clip",
            "overlay-blue",
            ClipPlacement {
                timeline_start_tick: 800_000,
                source_in_tick: 100_000,
                source_out_tick: 900_000,
                speed: 1.0,
                speed_ramp: None,
            },
        );
        red_clip.source_audio_enabled = false;
        blue_clip.source_audio_enabled = false;
        composition.tracks = vec![
            CompositionTrack::Video {
                id: TrackId::parse("overlay-transition-track").unwrap(),
                name: "Overlay transition".to_owned(),
                hidden: false,
                muted: true,
                locked: false,
                clips: vec![red_clip, blue_clip],
                transitions: vec![ClipTransition {
                    id: TransitionId::parse("overlay-transition").unwrap(),
                    from_clip_id: CompositionClipId::parse("overlay-red-clip").unwrap(),
                    to_clip_id: CompositionClipId::parse("overlay-blue-clip").unwrap(),
                    duration_ticks: 200_000,
                    kind,
                }],
            },
            CompositionTrack::Video {
                id: TrackId::parse("primary-track").unwrap(),
                name: "Primary".to_owned(),
                hidden: false,
                muted: true,
                locked: false,
                clips: vec![video_clip(
                    "primary-clip",
                    "primary-gray",
                    placement(0, 1_600_000),
                )],
                transitions: Vec::new(),
            },
        ];

        let output = directory.path().join(format!("overlay-{kind:?}.mp4"));
        let inputs = BTreeMap::from([
            (source_id("primary-gray"), primary.clone()),
            (source_id("overlay-red"), red.clone()),
            (source_id("overlay-blue"), blue.clone()),
        ]);
        let command = FfmpegCompositionExportCompiler
            .compile(CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &BTreeMap::new(),
                destination: &output,
                parallel_jobs: 1,
                composition: &composition,
                output: mp4_h264_output(24),
            })
            .unwrap();
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

        assert!(matches!(done, Done::Completed), "{kind:?}");
        assert_dominant(sample_rgb(&output, 0.60).await, 0);
        assert_dominant(sample_rgb(&output, 1.00).await, 2);
        match kind {
            TransitionKind::Dissolve => {
                let middle = sample_rgb(&output, 0.80).await;
                assert!(middle[0] > 40 && middle[2] > 40, "{middle:?}");
            }
            TransitionKind::FadeBlack => {
                let middle = sample_rgb(&output, 0.80).await;
                assert!(middle.into_iter().all(|channel| channel < 50), "{middle:?}");
            }
            TransitionKind::WipeLeft | TransitionKind::SmoothLeft | TransitionKind::SlideLeft => {
                assert_dominant(sample_pixel(&output, 0.80, 8, 24).await, 0);
                assert_dominant(sample_pixel(&output, 0.80, 56, 24).await, 2);
            }
            TransitionKind::WipeRight
            | TransitionKind::SmoothRight
            | TransitionKind::SlideRight => {
                assert_dominant(sample_pixel(&output, 0.80, 8, 24).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 56, 24).await, 0);
            }
            TransitionKind::WipeUp | TransitionKind::SmoothUp | TransitionKind::SlideUp => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 8).await, 0);
                assert_dominant(sample_pixel(&output, 0.80, 32, 40).await, 2);
            }
            TransitionKind::WipeDown | TransitionKind::SmoothDown | TransitionKind::SlideDown => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 8).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 32, 40).await, 0);
            }
            TransitionKind::CircleOpen => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 4, 4).await, 0);
            }
            TransitionKind::CircleClose => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 0);
                assert_dominant(sample_pixel(&output, 0.80, 4, 4).await, 2);
            }
            TransitionKind::WipeTopLeft => {
                assert_dominant(sample_pixel(&output, 0.80, 4, 4).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 60, 44).await, 0);
            }
            TransitionKind::WipeTopRight => {
                assert_dominant(sample_pixel(&output, 0.80, 60, 4).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 4, 44).await, 0);
            }
            TransitionKind::WipeBottomLeft => {
                assert_dominant(sample_pixel(&output, 0.80, 4, 44).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 60, 4).await, 0);
            }
            TransitionKind::WipeBottomRight => {
                assert_dominant(sample_pixel(&output, 0.80, 60, 44).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 4, 4).await, 0);
            }
            TransitionKind::VerticalOpen => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 4, 24).await, 0);
            }
            TransitionKind::VerticalClose => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 0);
                assert_dominant(sample_pixel(&output, 0.80, 4, 24).await, 2);
            }
            TransitionKind::HorizontalOpen => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 2);
                assert_dominant(sample_pixel(&output, 0.80, 32, 4).await, 0);
            }
            TransitionKind::HorizontalClose => {
                assert_dominant(sample_pixel(&output, 0.80, 32, 24).await, 0);
                assert_dominant(sample_pixel(&output, 0.80, 32, 4).await, 2);
            }
        }
        let rendered = probe_video(&runtime, &output).await.unwrap();
        assert!(
            (rendered.duration - 1.6).abs() < 0.15,
            "{kind:?}: {rendered:?}"
        );
    }
}

#[tokio::test]
async fn real_blur_and_checker_canvas_backgrounds_fill_primary_letterbox() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real canvas backgrounds: ffmpeg/ffprobe not on PATH");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("portrait-blue.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:size=32x64:rate=20:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &source_path,
    )
    .await;

    for (mode, filename) in [
        (CanvasBackgroundMode::Blur, "blur.mp4"),
        (CanvasBackgroundMode::Checker, "checker.mp4"),
    ] {
        let destination = directory.path().join(filename);
        let source = CompositionSource {
            id: source_id("portrait"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 32,
            height: 64,
            has_audio: false,
        };
        let mut clip = video_clip("portrait-clip", "portrait", placement(0, 1_000_000));
        clip.source_audio_enabled = false;
        let mut composition = Composition::new(CanvasSpec {
            width: 128,
            height: 64,
            fps_milli: 20_000,
            background: Rgba::BLACK,
            background_mode: mode,
            background_blur: 18.0,
        });
        composition.sources.insert(source.id.clone(), source);
        composition.tracks.push(CompositionTrack::Video {
            id: TrackId::parse("primary").unwrap(),
            name: "Primary".to_owned(),
            hidden: false,
            muted: true,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        });
        let inputs = BTreeMap::from([(source_id("portrait"), source_path.clone())]);
        let command = FfmpegCompositionExportCompiler
            .compile(CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &BTreeMap::new(),
                destination: &destination,
                parallel_jobs: 1,
                composition: &composition,
                output: mp4_h264_output(18),
            })
            .unwrap();
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
        let rendered = probe_video(&runtime, &destination).await.unwrap();
        assert_eq!((rendered.width, rendered.height), (128, 64));
        let left = sample_pixel(&destination, 0.5, 8, 8).await;
        let right = sample_pixel(&destination, 0.5, 88, 8).await;
        match mode {
            CanvasBackgroundMode::Blur => {
                assert!(
                    left[2] > 150 && right[2] > 150,
                    "blur did not fill letterbox: {left:?} {right:?}"
                );
            }
            CanvasBackgroundMode::Checker => {
                assert!(
                    left.iter().zip(right).any(|(a, b)| a.abs_diff(b) > 10),
                    "checker tiles did not differ: {left:?} {right:?}"
                );
            }
            CanvasBackgroundMode::Color => unreachable!(),
        }
    }
}

#[tokio::test]
async fn real_style_effect_presets_change_pixels_and_preserve_contract() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real style effects: ffmpeg/ffprobe not on PATH");
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("effect-source.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=128x64:rate=20:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &source_path,
    )
    .await;

    let presets = [
        None,
        Some(VideoEffectPreset::Blur),
        Some(VideoEffectPreset::Pixelate),
        Some(VideoEffectPreset::Vignette),
        Some(VideoEffectPreset::Sharpen),
        Some(VideoEffectPreset::Edge),
        Some(VideoEffectPreset::RgbSplit),
        Some(VideoEffectPreset::Posterize),
    ];
    let mut baseline = Vec::new();
    for preset in presets {
        let label = preset.map_or("none", |value| match value {
            VideoEffectPreset::Blur => "blur",
            VideoEffectPreset::Pixelate => "pixelate",
            VideoEffectPreset::Vignette => "vignette",
            VideoEffectPreset::Sharpen => "sharpen",
            VideoEffectPreset::Edge => "edge",
            VideoEffectPreset::RgbSplit => "rgb-split",
            VideoEffectPreset::Posterize => "posterize",
        });
        let destination = directory.path().join(format!("{label}.mp4"));
        let source = CompositionSource {
            id: source_id("effect-source"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 128,
            height: 64,
            has_audio: false,
        };
        let mut clip = video_clip("effect-clip", "effect-source", placement(0, 1_000_000));
        clip.source_audio_enabled = false;
        if let Some(preset) = preset {
            clip.effects.push(VideoEffect::Style {
                preset,
                intensity: 0.75,
            });
        }
        let mut composition = Composition::new(CanvasSpec {
            width: 128,
            height: 64,
            fps_milli: 20_000,
            ..CanvasSpec::default()
        });
        composition.sources.insert(source.id.clone(), source);
        composition.tracks.push(CompositionTrack::Video {
            id: TrackId::parse("effect-track").unwrap(),
            name: "Effects".to_owned(),
            hidden: false,
            muted: true,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        });
        let inputs = BTreeMap::from([(source_id("effect-source"), source_path.clone())]);
        let command = FfmpegCompositionExportCompiler
            .compile(CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &BTreeMap::new(),
                destination: &destination,
                parallel_jobs: 1,
                composition: &composition,
                output: mp4_h264_output(12),
            })
            .unwrap();
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
        assert!(matches!(done, Done::Completed), "{label}");
        let rendered = probe_video(&runtime, &destination).await.unwrap();
        assert_eq!((rendered.width, rendered.height), (128, 64), "{label}");
        assert!(
            (rendered.duration - 1.0).abs() < 0.15,
            "{label}: {rendered:?}"
        );
        let frame = frame_rgb(&destination, 0.5, 128, 64).await;
        if preset.is_none() {
            baseline = frame;
        } else {
            let changed = baseline
                .iter()
                .zip(&frame)
                .filter(|(a, b)| a.abs_diff(**b) > 4)
                .count();
            assert!(changed > 300, "{label} changed only {changed} RGB samples");
        }
    }
}

#[tokio::test]
async fn real_multi_source_composition_concats_video_and_mixes_audio() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_multi_source_composition_concats_video_and_mixes_audio: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let red = directory.path().join("red-with-audio.mp4");
    let blue = directory.path().join("blue-silent.mp4");
    let music = directory.path().join("music.wav");
    let output = directory.path().join("composition.mp4");

    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=64x48:rate=20:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-shortest",
            "-vf",
            "drawbox=x=0:y=0:w=32:h=48:color=lime:t=fill",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &red,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:size=96x64:rate=15:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &blue,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=44100:duration=1",
            "-c:a",
            "pcm_s16le",
        ],
        &music,
    )
    .await;

    let mut composition = Composition::new(CanvasSpec {
        width: 96,
        height: 64,
        fps_milli: 20_000,
        background: Rgba {
            red: 0.0,
            green: 1.0,
            blue: 0.0,
            alpha: 1.0,
        },
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("red"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("blue"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 96,
            height: 64,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("music"),
            kind: SourceKind::Audio,
            duration_ticks: 1_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }
    let mut red_clip = video_clip("red-clip", "red", placement(0, 500_000));
    red_clip.opacity = AnimatableValue::constant(0.5);
    red_clip.transform.x = AnimatableValue::constant(16.0);
    red_clip.transform.scale_x = AnimatableValue::constant(0.5);
    red_clip.transform.scale_y = AnimatableValue::constant(0.5);
    red_clip.blend_mode = BlendMode::Screen;
    red_clip.effects.push(VideoEffect::ChromaKey {
        color: Rgba {
            red: 0.0,
            green: 1.0,
            blue: 0.0,
            alpha: 1.0,
        },
        similarity: 0.1,
        softness: 0.05,
        spill: 0.0,
    });
    red_clip.effects.push(VideoEffect::Mask {
        shape: MaskShape::Rectangle,
        x: AnimatableValue::constant(0.5),
        y: AnimatableValue::constant(0.5),
        width: AnimatableValue::constant(0.5),
        height: AnimatableValue::constant(1.0),
        rotation_degrees: AnimatableValue::constant(0.0),
        feather: 0.0,
        inverted: false,
    });
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Video".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![
                red_clip,
                video_clip("blue-clip", "blue", placement(500_000, 500_000)),
            ],
            transitions: Vec::new(),
        },
        CompositionTrack::Audio {
            id: TrackId::parse("music-track").unwrap(),
            name: "Music".to_owned(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("music-clip").unwrap(),
                source_id: source_id("music"),
                placement: placement(0, 1_000_000),
                gain: AnimatableValue::constant(0.25),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: Default::default(),
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                ducking: None,
                enabled: true,
            }],
        },
    ];

    let inputs = BTreeMap::<SourceId, PathBuf>::from([
        (source_id("red"), red),
        (source_id("blue"), blue),
        (source_id("music"), music),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(30),
        })
        .unwrap();
    assert_eq!(command.expected_duration_seconds, 1.0);

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
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert_eq!((rendered.width, rendered.height), (96, 64));
    assert_eq!(rendered.vcodec.as_deref(), Some("h264"));
    assert_eq!(rendered.acodec.as_deref(), Some("aac"));
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");
    let faded_red = sample_pixel(&output, 0.25, 68, 32).await;
    assert!(
        faded_red[0] > 80
            && faded_red[1] > 180
            && faded_red[2] < 40
            && faded_red[1] > faded_red[0],
        "primary screen blend/transform/opacity did not composite over the green canvas: {faded_red:?}"
    );
    assert_dominant(sample_pixel(&output, 0.25, 58, 32).await, 1);
    assert_dominant(sample_pixel(&output, 0.25, 45, 32).await, 1);
    assert_dominant(sample_pixel(&output, 0.25, 10, 32).await, 1);
    assert_dominant(sample_rgb(&output, 0.75).await, 2);
}

#[tokio::test]
async fn real_source_and_independent_audio_gain_pan_automation_is_clip_local() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_source_and_independent_audio_gain_pan_automation_is_clip_local: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    if ![" volume ", " aeval ", " aformat "]
        .iter()
        .all(|filter| filters.contains(filter))
    {
        eprintln!("skipping real audio automation composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let source_tone = directory.path().join("source-tone.mp4");
    let silent_video = directory.path().join("silent-video.mp4");
    let independent_tone = directory.path().join("independent-tone.wav");
    let output = directory.path().join("audio-automation.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:size=64x48:rate=20:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_tone,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:size=64x48:rate=20:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &silent_video,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=1",
            "-c:a",
            "pcm_s16le",
        ],
        &independent_tone,
    )
    .await;

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("source-tone"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("silent-video"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 48,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("independent-tone"),
            kind: SourceKind::Audio,
            duration_ticks: 1_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }
    let mut primary_tone = video_clip("source-tone-clip", "source-tone", placement(0, 1_000_000));
    primary_tone.audio_gain = animated(Interpolation::Linear, 0.2, 1.0);
    primary_tone.audio_pan = animated(Interpolation::Linear, -1.0, 1.0);
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("audio-main").unwrap(),
            name: "Video".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![
                primary_tone,
                video_clip(
                    "silent-video-clip",
                    "silent-video",
                    placement(1_000_000, 1_000_000),
                ),
            ],
            transitions: Vec::new(),
        },
        CompositionTrack::Audio {
            id: TrackId::parse("independent-audio").unwrap(),
            name: "Independent audio".to_owned(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("independent-tone-clip").unwrap(),
                source_id: source_id("independent-tone"),
                placement: placement(1_000_000, 1_000_000),
                gain: animated(Interpolation::Linear, 0.2, 1.0),
                pan: animated(Interpolation::Linear, -1.0, 1.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: Default::default(),
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                ducking: None,
                enabled: true,
            }],
        },
    ];

    let inputs = BTreeMap::from([
        (source_id("source-tone"), source_tone),
        (source_id("silent-video"), silent_video),
        (source_id("independent-tone"), independent_tone),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert_eq!(graph.matches("volume=volume='").count(), 2, "{graph}");
    assert_eq!(graph.matches("aeval=exprs='").count(), 2, "{graph}");

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

    let source_early = channel_rms(&output, 0.1, 0.2).await;
    let source_late = channel_rms(&output, 0.7, 0.2).await;
    let independent_early = channel_rms(&output, 1.1, 0.2).await;
    let independent_late = channel_rms(&output, 1.7, 0.2).await;
    for (label, early, late) in [
        ("source audio", source_early, source_late),
        ("independent audio", independent_early, independent_late),
    ] {
        assert!(
            early[0] > early[1] * 2.0,
            "{label} did not start left-panned: {early:?}"
        );
        assert!(
            late[1] > late[0] * 2.0,
            "{label} did not end right-panned: {late:?}"
        );
        assert!(
            late[1] > early[0] * 1.6,
            "{label} gain did not increase: {early:?} -> {late:?}"
        );
    }
}

#[tokio::test]
async fn real_audio_crossfade_mixes_both_handle_backed_tones_and_preserves_duration() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("silent.mp4");
    let tone_a = directory.path().join("tone-a.wav");
    let tone_b = directory.path().join("tone-b.wav");
    let output = directory.path().join("crossfade.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=64x48:rate=20:duration=3",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &video,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=4",
            "-c:a",
            "pcm_s16le",
        ],
        &tone_a,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=4",
            "-c:a",
            "pcm_s16le",
        ],
        &tone_b,
    )
    .await;

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("silent"),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 48,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("tone-a"),
            kind: SourceKind::Audio,
            duration_ticks: 4_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("tone-b"),
            kind: SourceKind::Audio,
            duration_ticks: 4_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }
    let audio =
        |id: &str, source: &str, start: u64, source_in: u64, source_out: u64, crossfade: u64| {
            AudioClip {
                id: CompositionClipId::parse(id).unwrap(),
                source_id: source_id(source),
                placement: ClipPlacement {
                    timeline_start_tick: start,
                    source_in_tick: source_in,
                    source_out_tick: source_out,
                    speed: 1.0,
                    speed_ramp: None,
                },
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: Default::default(),
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: crossfade,
                ducking: None,
                enabled: true,
            }
        };
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("video").unwrap(),
            name: "Video".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip("video-clip", "silent", placement(0, 3_000_000))],
            transitions: vec![],
        },
        CompositionTrack::Audio {
            id: TrackId::parse("audio").unwrap(),
            name: "Audio".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![
                audio("tone-a-clip", "tone-a", 0, 500_000, 2_000_000, 0),
                audio(
                    "tone-b-clip",
                    "tone-b",
                    1_500_000,
                    2_000_000,
                    3_500_000,
                    1_000_000,
                ),
            ],
        },
    ];
    composition.validate().unwrap();
    let inputs = BTreeMap::from([
        (source_id("silent"), video),
        (source_id("tone-a"), tone_a),
        (source_id("tone-b"), tone_b),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
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
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!((rendered.duration - 3.0).abs() < 0.15, "{rendered:?}");
    let mid_a = band_rms(&output, 1.4, 440).await;
    let mid_b = band_rms(&output, 1.4, 880).await;
    assert!(
        mid_a > 0.005 && mid_b > 0.005,
        "crossfade did not mix both tones: {mid_a}, {mid_b}"
    );
}

#[tokio::test]
async fn real_auto_ducking_reduces_background_band_only_while_primary_voice_is_active() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("voice.mp4");
    let music = directory.path().join("music.wav");
    let output = directory.path().join("ducked.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=64x48:rate=20:duration=3",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.5*sin(2*PI*440*t)*between(t\\,1\\,2):s=48000:d=3",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &video,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=3",
            "-c:a",
            "pcm_s16le",
        ],
        &music,
    )
    .await;
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("voice"),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("music"),
            kind: SourceKind::Audio,
            duration_ticks: 3_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("video").unwrap(),
            name: "Voice".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip("voice-clip", "voice", placement(0, 3_000_000))],
            transitions: vec![],
        },
        CompositionTrack::Audio {
            id: TrackId::parse("music-track").unwrap(),
            name: "Music".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("music-clip").unwrap(),
                source_id: source_id("music"),
                placement: placement(0, 3_000_000),
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: Default::default(),
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                ducking: Some(AudioDucking {
                    threshold_db: -40.0,
                    ratio: 20.0,
                    attack_ms: 5.0,
                    release_ms: 50.0,
                }),
                enabled: true,
            }],
        },
    ];
    composition.validate().unwrap();
    let inputs = BTreeMap::from([(source_id("voice"), video), (source_id("music"), music)]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
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
    let quiet = band_rms(&output, 0.4, 880).await;
    let voiced = band_rms(&output, 1.4, 880).await;
    assert!(
        voiced < quiet * 0.6,
        "background was not ducked under voice: {quiet} -> {voiced}"
    );
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!((rendered.duration - 3.0).abs() < 0.15, "{rendered:?}");
}

#[tokio::test]
async fn real_voice_effects_tone_pitch_and_reverse_preserve_clip_duration() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let video = directory.path().join("black.mp4");
    let voice = directory.path().join("pulse.wav");
    let output = directory.path().join("echo.mp4");
    let tone = directory.path().join("tone.wav");
    let robot_output = directory.path().join("robot.mp4");
    let chipmunk_output = directory.path().join("chipmunk.mp4");
    let custom_pitch_output = directory.path().join("custom-pitch.mp4");
    let tone_mix = directory.path().join("tone-mix.wav");
    let tone_output = directory.path().join("tone.mp4");
    let reverse_output = directory.path().join("reverse-audio.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=64x48:rate=20:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &video,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.3*sin(2*PI*250*t)+0.3*sin(2*PI*4000*t):s=48000:d=1",
            "-c:a",
            "pcm_s16le",
        ],
        &tone_mix,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000:duration=1",
            "-c:a",
            "pcm_s16le",
        ],
        &tone,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=0.8*sin(2*PI*1000*t)*between(t\\,0.20\\,0.22):s=48000:d=1",
            "-c:a",
            "pcm_s16le",
        ],
        &voice,
    )
    .await;
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("black"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 48,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("pulse"),
            kind: SourceKind::Audio,
            duration_ticks: 1_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("tone"),
            kind: SourceKind::Audio,
            duration_ticks: 1_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
        CompositionSource {
            id: source_id("tone-mix"),
            kind: SourceKind::Audio,
            duration_ticks: 1_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("video").unwrap(),
            name: "Video".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip("black-clip", "black", placement(0, 1_000_000))],
            transitions: vec![],
        },
        CompositionTrack::Audio {
            id: TrackId::parse("voice").unwrap(),
            name: "Voice".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("pulse-clip").unwrap(),
                source_id: source_id("pulse"),
                placement: placement(0, 1_000_000),
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: AudioVoiceEffect::Echo,
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                ducking: None,
                enabled: true,
            }],
        },
    ];
    composition.validate().unwrap();
    let inputs = BTreeMap::from([
        (source_id("black"), video),
        (source_id("pulse"), voice),
        (source_id("tone"), tone),
        (source_id("tone-mix"), tone_mix),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
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
    let repeat = channel_rms(&output, 0.255, 0.035).await;
    let silence = channel_rms(&output, 0.75, 0.10).await;
    assert!(
        repeat[0] > silence[0] * 5.0 && repeat[0] > 0.005,
        "echo repeat missing: {repeat:?} vs {silence:?}"
    );
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
        unreachable!();
    };
    clips[0].source_id = source_id("tone");
    clips[0].voice_effect = AudioVoiceEffect::Robot;
    composition.validate().unwrap();
    let robot_command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &robot_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
        &runtime,
        &robot_command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    let lower_sideband = band_rms(&robot_output, 0.4, 965).await;
    let upper_sideband = band_rms(&robot_output, 0.4, 1035).await;
    assert!(
        lower_sideband > 0.002 && upper_sideband > 0.002,
        "robot modulation sidebands missing: {lower_sideband}, {upper_sideband}"
    );
    let rendered = probe_video(&runtime, &robot_output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
        unreachable!();
    };
    clips[0].voice_effect = AudioVoiceEffect::Chipmunk;
    let chipmunk_command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &chipmunk_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
        &runtime,
        &chipmunk_command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    let shifted = band_rms(&chipmunk_output, 0.4, 1500).await;
    let original = band_rms(&chipmunk_output, 0.4, 1000).await;
    assert!(
        shifted > original * 4.0 && shifted > 0.01,
        "chipmunk pitch did not move 1000 Hz to 1500 Hz: {original} -> {shifted}"
    );
    let rendered = probe_video(&runtime, &chipmunk_output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
        unreachable!();
    };
    clips[0].voice_effect = AudioVoiceEffect::None;
    clips[0].pitch_semitones = -12.0;
    let custom_pitch_command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &custom_pitch_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
        &runtime,
        &custom_pitch_command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    let shifted = band_rms(&custom_pitch_output, 0.4, 500).await;
    let original = band_rms(&custom_pitch_output, 0.4, 1000).await;
    assert!(
        shifted > original * 4.0 && shifted > 0.01,
        "custom pitch did not move 1000 Hz to 500 Hz: {original} -> {shifted}"
    );
    let rendered = probe_video(&runtime, &custom_pitch_output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
        unreachable!();
    };
    clips[0].source_id = source_id("tone-mix");
    clips[0].pitch_semitones = 0.0;
    clips[0].tone_db = 12.0;
    let tone_command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &tone_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
        &runtime,
        &tone_command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    let bass = band_rms(&tone_output, 0.4, 250).await;
    let treble = band_rms(&tone_output, 0.4, 4000).await;
    assert!(
        treble > bass * 3.0,
        "positive tone did not tilt toward treble: bass={bass}, treble={treble}"
    );
    let rendered = probe_video(&runtime, &tone_output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
        unreachable!();
    };
    clips[0].source_id = source_id("pulse");
    clips[0].tone_db = 0.0;
    clips[0].reversed = true;
    let reverse_command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &reverse_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(24),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    run_compiled_ffmpeg(
        &runtime,
        &reverse_command,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    let early = channel_rms(&reverse_output, 0.18, 0.08).await;
    let late = channel_rms(&reverse_output, 0.76, 0.08).await;
    assert!(
        late[0] > early[0] * 5.0 && late[0] > 0.01,
        "reversed audio pulse stayed at source time: early={early:?}, late={late:?}"
    );
    let rendered = probe_video(&runtime, &reverse_output).await.unwrap();
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");
}

#[tokio::test]
async fn real_optical_flow_slow_motion_has_exact_frames_motion_and_av_duration() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_optical_flow_slow_motion_has_exact_frames_motion_and_av_duration: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    let has_filter = |name: &str| {
        filters.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|candidate| candidate == name)
        })
    };
    if !has_filter("minterpolate") || !has_filter("tpad") {
        eprintln!("skipping real optical-flow composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("five-fps-motion.mp4");
    let output = directory.path().join("optical-flow.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=s=64x48:r=5:d=1,geq=lum='if(lt(abs(X-(8+30*T)),5)*lt(abs(Y-24),8),235,16)':cb=128:cr=128",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source,
    )
    .await;

    let source_id = source_id("motion-source");
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(
        source_id.clone(),
        CompositionSource {
            id: source_id.clone(),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
    );
    let mut clip = video_clip("motion-clip", "motion-source", placement(0, 1_000_000));
    clip.placement.speed = 0.5;
    clip.frame_interpolation = FrameInterpolation::OpticalFlow;
    composition.tracks = vec![CompositionTrack::Video {
        id: TrackId::parse("motion-track").unwrap(),
        name: "Motion".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![clip],
        transitions: Vec::new(),
    }];
    let inputs = BTreeMap::from([(source_id, source)]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(graph.contains("tpad=stop_mode=clone:stop_duration=2.000000"));
    assert!(graph.contains("minterpolate=fps=20000/1000:mi_mode=mci"));
    assert!(graph.contains("trim=duration=2.000000,setpts=PTS-STARTPTS"));

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

    let rendered = probe_video(&runtime, &output).await.unwrap();
    let frame_count = video_frame_count(&output).await;
    let video_duration = stream_duration(&output, "v:0").await;
    let audio_duration = stream_duration(&output, "a:0").await;
    assert!(
        (rendered.duration - 2.0).abs() < 0.04,
        "{rendered:?}; frames={frame_count}; video={video_duration}; audio={audio_duration}"
    );
    assert_eq!(
        frame_count, 40,
        "video={video_duration}; audio={audio_duration}; rendered={rendered:?}"
    );
    let unique_frames = unique_rgb_frame_count(&output, 64, 48).await;
    assert!(
        unique_frames >= 12,
        "optical flow did not create enough intermediate motion frames: {unique_frames}"
    );
    assert!((video_duration - 2.0).abs() < 0.04, "{video_duration}");
    assert!((audio_duration - 2.0).abs() < 0.04, "{audio_duration}");
    assert!(
        (video_duration - audio_duration).abs() <= 1.0 / 20.0,
        "A/V drift: video={video_duration}, audio={audio_duration}"
    );
}

#[tokio::test]
async fn real_reverse_optical_and_freeze_preserve_order_duration_and_av_sync() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real reverse/freeze composition: ffmpeg/ffprobe not on PATH");
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    let has_filter = |name: &str| {
        filters.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|candidate| candidate == name)
        })
    };
    if ["reverse", "areverse", "tpad", "minterpolate"]
        .into_iter()
        .any(|filter| !has_filter(filter))
    {
        eprintln!("skipping real reverse/freeze composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("ordered-colors.mp4");
    let reverse_output = directory.path().join("reverse-optical.mp4");
    let freeze_output = directory.path().join("freeze.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x48:r=10:d=1[red];color=c=green:s=64x48:r=10:d=1[green];color=c=blue:s=64x48:r=10:d=1[blue];[red][green][blue]concat=n=3:v=1:a=0",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
            "-t",
            "3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_path,
    )
    .await;

    let source = source_id("ordered-source");
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(
        source.clone(),
        CompositionSource {
            id: source.clone(),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
    );
    let mut clip = video_clip("ordered-clip", "ordered-source", placement(0, 3_000_000));
    clip.placement.speed = 0.5;
    clip.playback_mode = PlaybackMode::Reverse;
    clip.frame_interpolation = FrameInterpolation::OpticalFlow;
    composition.tracks = vec![CompositionTrack::Video {
        id: TrackId::parse("ordered-track").unwrap(),
        name: "Ordered".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![clip],
        transitions: Vec::new(),
    }];
    let inputs = BTreeMap::from([(source.clone(), source_path.clone())]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &reverse_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(
        graph.contains("fps=fps=20000/1000,reverse,setpts=(PTS-STARTPTS)/0.500000"),
        "{graph}"
    );
    assert!(graph.contains("asetpts=PTS-STARTPTS,areverse,atempo=0.500000"));
    assert!(graph.contains("minterpolate=fps=20000/1000"));

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
    assert_dominant(sample_rgb(&reverse_output, 0.5).await, 2);
    assert_dominant(sample_rgb(&reverse_output, 2.5).await, 1);
    assert_dominant(sample_rgb(&reverse_output, 4.5).await, 0);
    assert_eq!(video_frame_count(&reverse_output).await, 120);
    let reverse_video_duration = stream_duration(&reverse_output, "v:0").await;
    let reverse_audio_duration = stream_duration(&reverse_output, "a:0").await;
    assert!((reverse_video_duration - 6.0).abs() < 0.04);
    assert!((reverse_audio_duration - 6.0).abs() < 0.04);
    assert!(
        (reverse_video_duration - reverse_audio_duration).abs() <= 1.0 / 20.0,
        "reverse A/V drift: video={reverse_video_duration}, audio={reverse_audio_duration}"
    );

    let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
        unreachable!();
    };
    clips[0].placement.speed = 1.0;
    clips[0].playback_mode = PlaybackMode::Freeze {
        source_tick: 1_200_000,
    };
    clips[0].frame_interpolation = FrameInterpolation::Duplicate;
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &freeze_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(graph.contains("trim=start=1.200000,trim=end_frame=1"));
    assert!(graph.contains("tpad=stop_mode=clone:stop_duration=3.000000"));
    assert!(!graph.contains("areverse"));

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
    for at_seconds in [0.1, 1.5, 2.8] {
        assert_dominant(sample_rgb(&freeze_output, at_seconds).await, 1);
    }
    assert_eq!(video_frame_count(&freeze_output).await, 60);
    let freeze_video_duration = stream_duration(&freeze_output, "v:0").await;
    let freeze_audio_duration = stream_duration(&freeze_output, "a:0").await;
    assert!((freeze_video_duration - 3.0).abs() < 0.04);
    assert!((freeze_audio_duration - 3.0).abs() < 0.04);
    assert!(
        (freeze_video_duration - freeze_audio_duration).abs() <= 1.0 / 20.0,
        "freeze A/V drift: video={freeze_video_duration}, audio={freeze_audio_duration}"
    );
    let frozen_audio = channel_rms(&freeze_output, 0.5, 1.0).await;
    assert!(
        frozen_audio[0] < 0.001 && frozen_audio[1] < 0.001,
        "freeze embedded audio was not silent: {frozen_audio:?}"
    );
}

#[tokio::test]
async fn real_deshake_lowers_central_temporal_jitter_with_reverse_optical_and_keeps_av_exact() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real stabilization composition: ffmpeg/ffprobe not on PATH");
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    let has_filter = |name: &str| {
        filters.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|candidate| candidate == name)
        })
    };
    if ["deshake", "reverse", "areverse", "tpad", "minterpolate"]
        .into_iter()
        .any(|filter| !has_filter(filter))
    {
        eprintln!("skipping real stabilization composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("synthetic-jitter.mp4");
    let baseline_output = directory.path().join("jitter-baseline.mp4");
    let stabilized_output = directory.path().join("jitter-stabilized.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=96x72:r=20:d=3,drawgrid=w=12:h=12:t=2:c=white,drawbox=x=30:y=20:w=20:h=16:c=red:t=fill,crop=64:48:x='16+if(mod(n,2),3,-3)':y='12+if(mod(n,3),2,-2)'",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
            "-t",
            "3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_path,
    )
    .await;

    let source = source_id("jitter-source");
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(
        source.clone(),
        CompositionSource {
            id: source.clone(),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
    );
    let mut clip = video_clip("jitter-clip", "jitter-source", placement(0, 3_000_000));
    clip.placement.speed = 0.5;
    clip.playback_mode = PlaybackMode::Reverse;
    clip.frame_interpolation = FrameInterpolation::OpticalFlow;
    composition.tracks = vec![CompositionTrack::Video {
        id: TrackId::parse("jitter-track").unwrap(),
        name: "Jitter".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![clip],
        transitions: Vec::new(),
    }];
    let inputs = BTreeMap::from([(source, source_path)]);

    let baseline = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &baseline_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    let done = run_compiled_ffmpeg(
        &runtime,
        &baseline,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    assert!(matches!(done, Done::Completed));

    let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
        unreachable!();
    };
    clips[0].stabilization = StabilizationSpec::Deshake {
        radius_x: 16,
        radius_y: 16,
    };
    let stabilized = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &stabilized_output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let graph = &stabilized.arguments[stabilized
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(
        graph.contains(
            "fps=fps=20000/1000,deshake=rx=16:ry=16:edge=mirror:blocksize=8:\
             contrast=20:search=exhaustive,reverse,setpts=(PTS-STARTPTS)/0.500000"
        ),
        "{graph}"
    );
    assert!(graph.contains("minterpolate=fps=20000/1000"));
    let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
    let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
    let done = run_compiled_ffmpeg(
        &runtime,
        &stabilized,
        &progress,
        &CancellationToken::new(),
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    drop(progress);
    let _ = drain.await;
    assert!(matches!(done, Done::Completed));

    let baseline_jitter = temporal_luma_jitter(&baseline_output, 64, 48, 12).await;
    let stabilized_jitter = temporal_luma_jitter(&stabilized_output, 64, 48, 12).await;
    eprintln!(
        "central temporal luma jitter: baseline={baseline_jitter:.3}, \
         stabilized={stabilized_jitter:.3}"
    );
    assert!(
        stabilized_jitter < baseline_jitter * 0.80,
        "deshake did not lower central temporal luma jitter enough: \
         baseline={baseline_jitter:.3}, stabilized={stabilized_jitter:.3}"
    );
    for output in [&baseline_output, &stabilized_output] {
        assert_eq!(video_frame_count(output).await, 120);
        let video_duration = stream_duration(output, "v:0").await;
        let audio_duration = stream_duration(output, "a:0").await;
        assert!((video_duration - 6.0).abs() < 0.04, "{video_duration}");
        assert!((audio_duration - 6.0).abs() < 0.04, "{audio_duration}");
        assert!(
            (video_duration - audio_duration).abs() <= 1.0 / 20.0,
            "stabilization A/V drift: video={video_duration}, audio={audio_duration}"
        );
    }
}

#[tokio::test]
async fn real_linear_speed_ramp_preserves_reverse_order_exact_frames_and_av_sync() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real speed-ramp composition: ffmpeg/ffprobe not on PATH");
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    let has_filter = |name: &str| {
        filters.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|candidate| candidate == name)
        })
    };
    if [
        "setpts",
        "tpad",
        "fps",
        "trim",
        "atrim",
        "asetpts",
        "asplit",
        "atempo",
        "concat",
        "reverse",
        "areverse",
        "deshake",
        "minterpolate",
    ]
    .into_iter()
    .any(|filter| !has_filter(filter))
    {
        eprintln!("skipping real speed-ramp composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("speed-ramp-colors.mp4");
    let music_path = directory.path().join("speed-ramp-music.wav");
    let output = directory.path().join("speed-ramp-reverse.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x48:r=20:d=1[red];color=c=green:s=64x48:r=20:d=1[green];color=c=blue:s=64x48:r=20:d=1[blue];[red][green][blue]concat=n=3:v=1:a=0",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=3",
            "-t",
            "3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_path,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=3",
            "-c:a",
            "pcm_s16le",
        ],
        &music_path,
    )
    .await;

    let source = source_id("speed-ramp-source");
    let music = source_id("speed-ramp-music");
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(
        source.clone(),
        CompositionSource {
            id: source.clone(),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 48,
            has_audio: true,
        },
    );
    composition.sources.insert(
        music.clone(),
        CompositionSource {
            id: music.clone(),
            kind: SourceKind::Audio,
            duration_ticks: 3_000_000,
            width: 0,
            height: 0,
            has_audio: true,
        },
    );
    let mut clip = video_clip(
        "speed-ramp-clip",
        "speed-ramp-source",
        placement(0, 3_000_000),
    );
    clip.placement.speed = 0.5;
    clip.placement.speed_ramp = Some(SpeedRampSpec {
        interpolation: SpeedRampInterpolation::Linear,
        points: vec![
            SpeedRampPoint {
                source_progress_tick: 0,
                speed: 0.5,
            },
            SpeedRampPoint {
                source_progress_tick: 1_500_000,
                speed: 2.0,
            },
            SpeedRampPoint {
                source_progress_tick: 3_000_000,
                speed: 0.5,
            },
        ],
        audio_policy: SpeedRampAudioPolicy::PreservePitch,
    });
    clip.playback_mode = PlaybackMode::Reverse;
    clip.frame_interpolation = FrameInterpolation::OpticalFlow;
    clip.stabilization = StabilizationSpec::Deshake {
        radius_x: 16,
        radius_y: 16,
    };
    composition.tracks = vec![
        CompositionTrack::Video {
            id: TrackId::parse("speed-ramp-track").unwrap(),
            name: "Speed ramp".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        },
        CompositionTrack::Audio {
            id: TrackId::parse("speed-ramp-audio-track").unwrap(),
            name: "Speed ramp music".to_owned(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("speed-ramp-audio-clip").unwrap(),
                source_id: music.clone(),
                placement: ClipPlacement {
                    timeline_start_tick: 0,
                    source_in_tick: 0,
                    source_out_tick: 3_000_000,
                    speed: 0.5,
                    speed_ramp: Some(SpeedRampSpec {
                        interpolation: SpeedRampInterpolation::Linear,
                        points: vec![
                            SpeedRampPoint {
                                source_progress_tick: 0,
                                speed: 0.5,
                            },
                            SpeedRampPoint {
                                source_progress_tick: 1_500_000,
                                speed: 2.0,
                            },
                            SpeedRampPoint {
                                source_progress_tick: 3_000_000,
                                speed: 0.5,
                            },
                        ],
                        audio_policy: SpeedRampAudioPolicy::PreservePitch,
                    }),
                },
                gain: AnimatableValue::constant(0.2),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: Default::default(),
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                ducking: None,
                enabled: true,
            }],
        },
    ];
    composition.validate().unwrap();
    let CompositionTrack::Video { clips, .. } = &composition.tracks[0] else {
        unreachable!();
    };
    let expected_ticks = clips[0].placement.timeline_duration_ticks().unwrap();
    assert_eq!(expected_ticks, 2_772_589);

    let inputs = BTreeMap::from([(source, source_path), (music, music_path)]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    assert!((command.expected_duration_seconds - 2.772589).abs() < 1e-9);
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(graph.contains("deshake=rx=16:ry=16"), "{graph}");
    assert!(graph.contains("reverse,setpts='(if("), "{graph}");
    assert!(
        graph.contains("log((0.500000000000+(1.500000000000)"),
        "{graph}"
    );
    assert!(graph.contains("minterpolate=fps=20000/1000"), "{graph}");
    assert!(graph.contains("areverse,asplit=2"), "{graph}");
    assert!(graph.contains("atempo=1.082021"), "{graph}");
    assert!(graph.contains("[audio_clip_0_ramp_in_0]"), "{graph}");
    assert!(graph.contains("concat=n=2:v=0:a=1"), "{graph}");
    assert!(
        graph.contains("volume=0.200000,adelay=0S:all=1[audio_clip_0]"),
        "{graph}"
    );

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

    assert_dominant(sample_rgb(&output, 0.65).await, 2);
    assert_dominant(sample_rgb(&output, 1.35).await, 1);
    assert_dominant(sample_rgb(&output, 2.40).await, 0);
    // The exporter keeps only complete 20 fps frames inside the exact
    // 2.772589-second interval: floor(2.772589 * 20) = 55.
    assert_eq!(video_frame_count(&output).await, 55);
    let video_end = stream_packet_end(&output, "v:0").await;
    let audio_end = stream_packet_end(&output, "a:0").await;
    assert!((video_end - 2.772589).abs() <= 1.0 / 20.0, "{video_end}");
    assert!((audio_end - 2.772589).abs() <= 1.0 / 20.0, "{audio_end}");
    assert!(
        (video_end - audio_end).abs() <= 1.0 / 20.0,
        "speed-ramp A/V drift: video={video_end}, audio={audio_end}"
    );
    let rms = channel_rms(&output, 1.0, 0.5).await;
    assert!(
        rms[0] > 0.01 && rms[1] > 0.01,
        "ramp audio was silent: {rms:?}"
    );
}

#[tokio::test]
async fn real_visual_layers_preserve_z_order_opacity_chroma_and_image_timing() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_visual_layers_preserve_z_order_opacity_chroma_and_image_timing: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("gray-base.mp4");
    let keyed = directory.path().join("green-with-red-box.mp4");
    let image = directory.path().join("yellow.png");
    let output = directory.path().join("layers.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:size=64x64:rate=20:duration=1",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &base,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=0x00ff00:size=32x32:rate=20:duration=1,drawbox=x=8:y=8:w=16:h=16:color=red:t=fill",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &keyed,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=yellow:size=8x8",
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
        &image,
    )
    .await;

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 64,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("base"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 64,
            height: 64,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("keyed"),
            kind: SourceKind::Video,
            duration_ticks: 1_000_000,
            width: 32,
            height: 32,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("image"),
            kind: SourceKind::Image,
            duration_ticks: 0,
            width: 8,
            height: 8,
            has_audio: false,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }

    let mut overlay = video_clip("keyed-clip", "keyed", placement(0, 1_000_000));
    overlay.blend_mode = BlendMode::Multiply;
    overlay.effects.push(VideoEffect::ChromaKey {
        color: Rgba {
            red: 0.0,
            green: 1.0,
            blue: 0.0,
            alpha: 1.0,
        },
        similarity: 0.1,
        softness: 0.0,
        spill: 0.25,
    });
    let image_transform = TransformSpec {
        x: AnimatableValue::constant(8.0),
        scale_x: AnimatableValue::constant(2.0),
        scale_y: AnimatableValue::constant(2.0),
        rotation_degrees: AnimatableValue::constant(90.0),
        ..TransformSpec::default()
    };
    composition.tracks = vec![
        CompositionTrack::Image {
            id: TrackId::parse("top-image").unwrap(),
            name: "Top image".to_owned(),
            hidden: false,
            locked: false,
            clips: vec![ImageClip {
                id: CompositionClipId::parse("image-clip").unwrap(),
                source_id: source_id("image"),
                timeline_start_tick: 250_000,
                duration_ticks: 500_000,
                transform: image_transform,
                opacity: AnimatableValue::constant(0.5),
                blend_mode: BlendMode::Normal,
                enabled: true,
            }],
        },
        CompositionTrack::Video {
            id: TrackId::parse("keyed-overlay").unwrap(),
            name: "Keyed overlay".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![overlay],
            transitions: Vec::new(),
        },
        CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Base".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip("base-clip", "base", placement(0, 1_000_000))],
            transitions: Vec::new(),
        },
    ];

    let inputs = BTreeMap::from([
        (source_id("base"), base),
        (source_id("keyed"), keyed),
        (source_id("image"), image),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
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

    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert_eq!((rendered.width, rendered.height), (64, 64));
    assert!((rendered.duration - 1.0).abs() < 0.15, "{rendered:?}");

    let outside = sample_pixel(&output, 0.5, 5, 5).await;
    assert!(
        outside.iter().all(|value| (90..=165).contains(value)),
        "{outside:?}"
    );
    let keyed_away = sample_pixel(&output, 0.5, 18, 18).await;
    assert!(
        keyed_away.iter().all(|value| (90..=165).contains(value)),
        "{keyed_away:?}"
    );
    let multiplied_red = sample_pixel(&output, 0.5, 25, 32).await;
    assert!(
        multiplied_red[0] > 90 && multiplied_red[1] < 55 && multiplied_red[2] < 55,
        "{multiplied_red:?}"
    );
    let top_image = sample_pixel(&output, 0.5, 36, 32).await;
    assert!(
        top_image[0] > 150 && top_image[1] > 70 && top_image[2] < 70,
        "{top_image:?}"
    );
    for outside_image_time in [0.1, 0.9] {
        let without_image = sample_pixel(&output, outside_image_time, 36, 32).await;
        assert!(
            without_image[0] > 90 && without_image[1] < 55 && without_image[2] < 55,
            "image leaked outside its interval at {outside_image_time}: {without_image:?}"
        );
    }
    let scaled_image = sample_pixel(&output, 0.5, 45, 32).await;
    assert!(
        scaled_image[0] > 150 && scaled_image[1] > 150 && scaled_image[2] < 110,
        "{scaled_image:?}"
    );
}

#[tokio::test]
async fn real_keyframes_and_shape_masks_move_pixels_with_feather_and_inversion() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real keyframe/mask composition: ffmpeg/ffprobe not on PATH");
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    if ![" geq ", " rotate ", " overlay "]
        .iter()
        .all(|filter| filters.contains(filter))
    {
        eprintln!("skipping real keyframe/mask composition: required filters unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("mask-base.mp4");
    let red = directory.path().join("mask-red.mp4");
    let image = directory.path().join("animated-yellow.png");
    let output = directory.path().join("keyframes-masks.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:size=64x64:rate=20:duration=3",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &base,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=64x64:rate=20:duration=3",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &red,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=yellow:size=8x8",
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
        &image,
    )
    .await;

    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 64,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    for source in [
        CompositionSource {
            id: source_id("mask-base"),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 64,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("mask-red"),
            kind: SourceKind::Video,
            duration_ticks: 3_000_000,
            width: 64,
            height: 64,
            has_audio: false,
        },
        CompositionSource {
            id: source_id("animated-yellow"),
            kind: SourceKind::Image,
            duration_ticks: 0,
            width: 8,
            height: 8,
            has_audio: false,
        },
    ] {
        composition.sources.insert(source.id.clone(), source);
    }

    let mut moving_mask = video_clip("moving-mask", "mask-red", placement(250_000, 1_000_000));
    moving_mask.effects.push(VideoEffect::Mask {
        shape: MaskShape::Rectangle,
        x: animated(Interpolation::EaseInOut, 0.25, 0.75),
        y: AnimatableValue::constant(0.5),
        width: AnimatableValue::constant(0.3),
        height: AnimatableValue::constant(0.5),
        rotation_degrees: AnimatableValue::constant(0.0),
        feather: 0.2,
        inverted: false,
    });
    let mut inverted_mask = video_clip(
        "inverted-mask",
        "mask-red",
        ClipPlacement {
            timeline_start_tick: 1_250_000,
            source_in_tick: 1_000_000,
            source_out_tick: 1_750_000,
            speed: 1.0,
            speed_ramp: None,
        },
    );
    inverted_mask.effects.push(VideoEffect::Mask {
        shape: MaskShape::Ellipse,
        x: AnimatableValue::constant(0.5),
        y: AnimatableValue::constant(0.5),
        width: AnimatableValue::constant(0.8),
        height: AnimatableValue::constant(0.3),
        rotation_degrees: AnimatableValue::constant(45.0),
        feather: 0.0,
        inverted: true,
    });
    let mut linear_mask = video_clip(
        "linear-mask",
        "mask-red",
        ClipPlacement {
            timeline_start_tick: 2_000_000,
            source_in_tick: 2_000_000,
            source_out_tick: 3_000_000,
            speed: 1.0,
            speed_ramp: None,
        },
    );
    linear_mask.effects.push(VideoEffect::LinearMask {
        x: AnimatableValue::constant(0.5),
        y: AnimatableValue::constant(0.5),
        rotation_degrees: animated(Interpolation::Linear, 0.0, 90.0),
        feather: 0.1,
        inverted: false,
    });

    let animated_image = ImageClip {
        id: CompositionClipId::parse("animated-image").unwrap(),
        source_id: source_id("animated-yellow"),
        timeline_start_tick: 0,
        duration_ticks: 1_000_000,
        transform: TransformSpec {
            x: animated(Interpolation::Linear, -20.0, 20.0),
            y: AnimatableValue::constant(-24.0),
            scale_x: animated(Interpolation::EaseIn, 1.0, 2.0),
            scale_y: animated(Interpolation::EaseOut, 1.0, 2.0),
            rotation_degrees: animated(Interpolation::EaseInOut, 0.0, 90.0),
            anchor_x: 0.5,
            anchor_y: 0.5,
        },
        opacity: animated(Interpolation::Linear, 0.25, 1.0),
        blend_mode: BlendMode::Normal,
        enabled: true,
    };
    composition.tracks = vec![
        CompositionTrack::Image {
            id: TrackId::parse("animated-image-track").unwrap(),
            name: "Animated image".to_owned(),
            hidden: false,
            locked: false,
            clips: vec![animated_image],
        },
        CompositionTrack::Video {
            id: TrackId::parse("mask-track").unwrap(),
            name: "Masks".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![moving_mask, inverted_mask, linear_mask],
            transitions: Vec::new(),
        },
        CompositionTrack::Video {
            id: TrackId::parse("mask-main").unwrap(),
            name: "Base".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip(
                "mask-base-clip",
                "mask-base",
                placement(0, 3_000_000),
            )],
            transitions: Vec::new(),
        },
    ];

    let inputs = BTreeMap::from([
        (source_id("mask-base"), base),
        (source_id("mask-red"), red),
        (source_id("animated-yellow"), image),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|argument| argument == "-filter_complex")
        .unwrap()
        + 1];
    assert!(graph.contains("eval=frame"), "{graph}");
    assert!(graph.contains("geq=r='r(X,Y)'"), "{graph}");

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

    let early_mask = sample_pixel(&output, 0.45, 20, 32).await;
    let early_outside = sample_pixel(&output, 0.45, 44, 32).await;
    let late_mask = sample_pixel(&output, 1.05, 44, 32).await;
    let late_outside = sample_pixel(&output, 1.05, 20, 32).await;
    assert_dominant(early_mask, 0);
    assert_dominant(late_mask, 0);
    assert!(early_outside.iter().all(|value| (90..=165).contains(value)));
    assert!(late_outside.iter().all(|value| (90..=165).contains(value)));
    let feather_edge = sample_pixel(&output, 0.75, 23, 32).await;
    assert!(
        (155..=240).contains(&feather_edge[0]) && feather_edge[1] < 115 && feather_edge[2] < 115,
        "rectangle feather did not produce partial alpha: {feather_edge:?}"
    );

    let inverted_center = sample_pixel(&output, 1.6, 32, 32).await;
    let inverted_corner = sample_pixel(&output, 1.6, 4, 4).await;
    let rotated_major_axis = sample_pixel(&output, 1.6, 46, 46).await;
    let rotated_minor_outside = sample_pixel(&output, 1.6, 50, 32).await;
    assert!(inverted_center
        .iter()
        .all(|value| (90..=165).contains(value)));
    assert_dominant(inverted_corner, 0);
    assert!(rotated_major_axis
        .iter()
        .all(|value| (90..=165).contains(value)));
    assert_dominant(rotated_minor_outside, 0);
    let linear_selected = sample_pixel(&output, 2.5, 32, 8).await;
    let linear_rejected = sample_pixel(&output, 2.5, 32, 56).await;
    assert_dominant(linear_selected, 0);
    assert!(linear_rejected
        .iter()
        .all(|value| (90..=165).contains(value)));

    let early_image = sample_pixel(&output, 0.2, 20, 8).await;
    let early_image_destination = sample_pixel(&output, 0.2, 44, 8).await;
    let late_image = sample_pixel(&output, 0.8, 44, 8).await;
    let late_image_origin = sample_pixel(&output, 0.8, 20, 8).await;
    assert!(early_image[0] > early_image[2] + 20, "{early_image:?}");
    assert!(late_image[0] > 200 && late_image[1] > 180, "{late_image:?}");
    assert!(
        late_image[0] > early_image[0] + 25,
        "{early_image:?} -> {late_image:?}"
    );
    assert!(early_image_destination
        .iter()
        .all(|value| (90..=165).contains(value)));
    assert!(late_image_origin
        .iter()
        .all(|value| (90..=165).contains(value)));
}

#[tokio::test]
async fn real_dissolve_preserves_timeline_with_exact_handles_and_no_black_frames() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real dissolve composition: ffmpeg/ffprobe not on PATH");
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    if !String::from_utf8_lossy(&filters.stdout).contains(" xfade ") {
        eprintln!("skipping real dissolve composition: xfade unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let red = directory.path().join("transition-red.mp4");
    let blue = directory.path().join("transition-blue.mp4");
    let output = directory.path().join("transition.mp4");
    for (color, path) in [("red", &red), ("blue", &blue)] {
        let source = format!("color=c={color}:size=128x72:rate=25:duration=2");
        generate(
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &source,
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ],
            path,
        )
        .await;
    }

    let mut composition = Composition::new(CanvasSpec {
        width: 128,
        height: 72,
        fps_milli: 25_000,
        ..CanvasSpec::default()
    });
    for id in ["transition-red", "transition-blue"] {
        let source = CompositionSource {
            id: source_id(id),
            kind: SourceKind::Video,
            duration_ticks: 2_000_000,
            width: 128,
            height: 72,
            has_audio: false,
        };
        composition.sources.insert(source.id.clone(), source);
    }
    let clip = |id: &str, source: &str, timeline_start_tick| VideoClip {
        id: CompositionClipId::parse(id).unwrap(),
        source_id: source_id(source),
        placement: ClipPlacement {
            timeline_start_tick,
            source_in_tick: 250_000,
            source_out_tick: 1_000_000,
            speed: 1.0,
            speed_ramp: None,
        },
        playback_mode: PlaybackMode::Forward,
        stabilization: StabilizationSpec::Disabled,
        frame_interpolation: FrameInterpolation::Duplicate,
        transform: TransformSpec::default(),
        opacity: AnimatableValue::constant(1.0),
        blend_mode: BlendMode::Normal,
        effects: Vec::new(),
        source_audio_enabled: true,
        audio_gain: AnimatableValue::constant(1.0),
        audio_pan: AnimatableValue::constant(0.0),
        enabled: true,
    };
    let first = clip("transition-red-clip", "transition-red", 0);
    let second = clip("transition-blue-clip", "transition-blue", 750_000);
    composition.tracks = vec![CompositionTrack::Video {
        id: TrackId::parse("transition-main").unwrap(),
        name: "Video".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![first.clone(), second.clone()],
        transitions: vec![ClipTransition {
            id: TransitionId::parse("transition-red-blue").unwrap(),
            from_clip_id: first.id,
            to_clip_id: second.id,
            duration_ticks: 400_000,
            kind: TransitionKind::Dissolve,
        }],
    }];
    let inputs = BTreeMap::from([
        (source_id("transition-red"), red),
        (source_id("transition-blue"), blue),
    ]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &BTreeMap::new(),
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
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
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!((rendered.duration - 1.5).abs() < 0.12, "{rendered:?}");
    assert_dominant(sample_rgb(&output, 0.15).await, 0);
    assert_dominant(sample_rgb(&output, 1.25).await, 2);
    let middle = sample_rgb(&output, 0.75).await;
    assert!(
        middle[0] > 55 && middle[2] > 55 && middle[1] < 45,
        "transition produced an unexpected/black midpoint: {middle:?}"
    );
}

#[tokio::test]
async fn real_unicode_textfile_and_dissolve_use_exact_handles_without_black_frames() {
    let runtime = ProcessRuntime::local_default();
    let Some(font_file) = unicode_font() else {
        eprintln!("skipping real Unicode text composition: no allowlisted Unicode font installed");
        return;
    };
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_unicode_textfile_and_dissolve_use_exact_handles_without_black_frames: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }
    let filters = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .await
        .unwrap();
    let filters = String::from_utf8_lossy(&filters.stdout);
    if !filters.contains(" drawtext ") || !filters.contains(" xfade ") {
        eprintln!("skipping real Unicode text composition: drawtext/xfade unavailable");
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let red = directory.path().join("red.mp4");
    let blue = directory.path().join("blue.mp4");
    let text_file = directory.path().join("unicode-title.txt");
    let output = directory.path().join("text-transition.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=128x72:rate=25:duration=2",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &red,
    )
    .await;
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:size=128x72:rate=25:duration=2",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ],
        &blue,
    )
    .await;
    tokio::fs::write(&text_file, "Привет, мир!\n字幕テスト")
        .await
        .unwrap();

    let mut composition = Composition::new(CanvasSpec {
        width: 128,
        height: 72,
        fps_milli: 25_000,
        ..CanvasSpec::default()
    });
    for id in ["red", "blue"] {
        let source = CompositionSource {
            id: source_id(id),
            kind: SourceKind::Video,
            duration_ticks: 2_000_000,
            width: 128,
            height: 72,
            has_audio: false,
        };
        composition.sources.insert(source.id.clone(), source);
    }
    let clip = |id: &str, source: &str, timeline_start_tick| VideoClip {
        id: CompositionClipId::parse(id).unwrap(),
        source_id: source_id(source),
        placement: ClipPlacement {
            timeline_start_tick,
            source_in_tick: 250_000,
            source_out_tick: 1_000_000,
            speed: 1.0,
            speed_ramp: None,
        },
        playback_mode: PlaybackMode::Forward,
        stabilization: StabilizationSpec::Disabled,
        frame_interpolation: FrameInterpolation::Duplicate,
        transform: TransformSpec::default(),
        opacity: AnimatableValue::constant(1.0),
        blend_mode: BlendMode::Normal,
        effects: Vec::new(),
        source_audio_enabled: true,
        audio_gain: AnimatableValue::constant(1.0),
        audio_pan: AnimatableValue::constant(0.0),
        enabled: true,
    };
    let first = clip("red-clip", "red", 0);
    let second = clip("blue-clip", "blue", 750_000);
    let text_id = CompositionClipId::parse("unicode-title").unwrap();
    composition.tracks = vec![
        CompositionTrack::Text {
            id: TrackId::parse("title-track").unwrap(),
            name: "Title".to_owned(),
            hidden: false,
            locked: false,
            clips: vec![TextClip {
                id: text_id.clone(),
                timeline_start_tick: 100_000,
                timeline_end_tick: 600_000,
                text: "Привет, мир!\n字幕テスト".to_owned(),
                style: TextStyle {
                    font_family: "Noto Sans".to_owned(),
                    font_size: 22.0,
                    color: Rgba {
                        red: 1.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: 1.0,
                    },
                    background: Rgba {
                        red: 0.0,
                        green: 0.0,
                        blue: 0.0,
                        alpha: 0.0,
                    },
                    stroke: Rgba::BLACK,
                    stroke_width: 1.0,
                    shadow: Rgba::BLACK,
                    shadow_x: 1.0,
                    shadow_y: 1.0,
                },
                transform: TransformSpec::default(),
                opacity: AnimatableValue::constant(1.0),
                enabled: true,
            }],
        },
        CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Video".to_owned(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![first.clone(), second.clone()],
            transitions: vec![ClipTransition {
                id: TransitionId::parse("red-blue").unwrap(),
                from_clip_id: first.id,
                to_clip_id: second.id,
                duration_ticks: 400_000,
                kind: TransitionKind::Dissolve,
            }],
        },
    ];
    let inputs = BTreeMap::from([(source_id("red"), red), (source_id("blue"), blue)]);
    let text_resources = BTreeMap::from([(
        text_id,
        CompositionTextResource {
            text_file: text_file.clone(),
            font_file: font_file.clone(),
        },
    )]);
    let command = FfmpegCompositionExportCompiler
        .compile(CompositionExportCompileRequest {
            inputs: &inputs,
            text_resources: &text_resources,
            destination: &output,
            parallel_jobs: 1,
            composition: &composition,
            output: mp4_h264_output(18),
        })
        .unwrap();
    assert!(command.read_only_files.contains(&text_file));
    assert!(command.read_only_files.contains(&font_file));
    assert!(!command
        .arguments
        .iter()
        .any(|argument| argument.contains("Привет")));

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
    let rendered = probe_video(&runtime, &output).await.unwrap();
    assert!((rendered.duration - 1.5).abs() < 0.12, "{rendered:?}");
    assert_dominant(sample_rgb(&output, 0.15).await, 0);
    assert_dominant(sample_rgb(&output, 1.25).await, 2);
    let middle = sample_rgb(&output, 0.75).await;
    assert!(
        middle[0] > 55 && middle[2] > 55 && middle[1] < 45,
        "transition produced an unexpected/black midpoint: {middle:?}"
    );
    let text_frame = frame_rgb(&output, 0.35, 128, 72).await;
    let light_pixels = text_frame
        .chunks_exact(3)
        .filter(|pixel| pixel[0] > 150 && pixel[1] > 100 && pixel[2] > 100)
        .count();
    assert!(
        light_pixels > 20,
        "Unicode text was not visibly rendered: {light_pixels} light pixels"
    );
}

#[tokio::test]
async fn real_delivery_profiles_probe_truthful_container_codecs_timing_and_dimensions() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!(
            "skipping real_delivery_profiles_probe_truthful_container_codecs_timing_and_dimensions: \
             ffmpeg/ffprobe not on PATH"
        );
        return;
    }
    let (encoders, muxers, _) = inspect_ffmpeg_support(&runtime).await;
    let has_encoder = |name: &str| encoders.iter().any(|encoder| encoder == name);
    let has_muxer = |name: &str| muxers.iter().any(|muxer| muxer == name);
    if !has_encoder("libx264") || !has_encoder("aac") || !has_muxer("mp4") {
        eprintln!("skipping delivery profile matrix: base fixture encoder unavailable");
        return;
    }

    #[derive(Clone, Copy)]
    struct DeliveryCase {
        name: &'static str,
        profile: CompositionExportProfile,
        quality: u32,
        av1_encoder: Option<CompositionAv1Encoder>,
        video_codec: &'static str,
        audio_codec: &'static str,
        pixel_format: &'static str,
        prores_profile: Option<&'static str>,
    }

    let mut cases = vec![DeliveryCase {
        name: "h264",
        profile: CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H264,
        },
        quality: 23,
        av1_encoder: None,
        video_codec: "h264",
        audio_codec: "aac",
        pixel_format: "yuv420p",
        prores_profile: None,
    }];
    if has_muxer("mp4") && has_encoder("libx265") && has_encoder("aac") {
        cases.push(DeliveryCase {
            name: "h265",
            profile: CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H265,
            },
            quality: 25,
            av1_encoder: None,
            video_codec: "hevc",
            audio_codec: "aac",
            pixel_format: "yuv420p",
            prores_profile: None,
        });
    }
    if has_muxer("webm") && has_encoder("libvpx-vp9") && has_encoder("libopus") {
        cases.push(DeliveryCase {
            name: "vp9",
            profile: CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Vp9,
            },
            quality: 32,
            av1_encoder: None,
            video_codec: "vp9",
            audio_codec: "opus",
            pixel_format: "yuv420p",
            prores_profile: None,
        });
    }
    let av1_encoder = if has_encoder("libsvtav1") {
        Some(CompositionAv1Encoder::LibSvtAv1)
    } else if has_encoder("libaom-av1") {
        Some(CompositionAv1Encoder::LibAomAv1)
    } else {
        None
    };
    if has_muxer("webm") && has_encoder("libopus") && av1_encoder.is_some() {
        cases.push(DeliveryCase {
            name: "av1",
            profile: CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Av1,
            },
            quality: 32,
            av1_encoder,
            video_codec: "av1",
            audio_codec: "opus",
            pixel_format: "yuv420p",
            prores_profile: None,
        });
    }
    if has_muxer("mov") && has_encoder("prores_ks") && has_encoder("pcm_s16le") {
        for (name, profile, expected_profile) in [
            ("prores-proxy", CompositionProResProfile::Proxy, "Proxy"),
            ("prores-lt", CompositionProResProfile::Lt, "LT"),
            (
                "prores-standard",
                CompositionProResProfile::Standard,
                "Standard",
            ),
            ("prores-hq", CompositionProResProfile::Hq, "HQ"),
        ] {
            cases.push(DeliveryCase {
                name,
                profile: CompositionExportProfile::Mov { profile },
                quality: 9,
                av1_encoder: None,
                video_codec: "prores",
                audio_codec: "pcm_s16le",
                pixel_format: "yuv422p10le",
                prores_profile: Some(expected_profile),
            });
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("delivery-source.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=128x72:rate=10:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_path,
    )
    .await;

    let source = CompositionSource {
        id: source_id("delivery-source"),
        kind: SourceKind::Video,
        duration_ticks: 1_000_000,
        width: 128,
        height: 72,
        has_audio: true,
    };
    let mut composition = Composition::new(CanvasSpec {
        width: 128,
        height: 72,
        fps_milli: 10_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(source.id.clone(), source);
    composition.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("delivery-track").unwrap(),
        name: "Delivery".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![video_clip(
            "delivery-clip",
            "delivery-source",
            placement(0, 1_000_000),
        )],
        transitions: Vec::new(),
    });
    let inputs = BTreeMap::from([(source_id("delivery-source"), source_path.clone())]);

    for case in cases {
        let output = directory.path().join(format!(
            "delivery-{}.{}",
            case.name,
            case.profile.extension()
        ));
        let command = FfmpegCompositionExportCompiler
            .compile(CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &BTreeMap::new(),
                destination: &output,
                parallel_jobs: 1,
                composition: &composition,
                output: CompositionExportSpec {
                    profile: case.profile,
                    video_quality: case.quality,
                    video_bitrate_kbps: (case.name == "h264").then_some(500),
                    av1_encoder: case.av1_encoder,
                    range: Some(CompositionExportRange {
                        start_ticks: 200_000,
                        end_ticks: 700_000,
                    }),
                },
            })
            .unwrap();
        let (progress, mut updates) = mpsc::unbounded_channel::<f64>();
        let drain = tokio::spawn(async move { while updates.recv().await.is_some() {} });
        let done = run_compiled_ffmpeg(
            &runtime,
            &command,
            &progress,
            &CancellationToken::new(),
            Duration::from_secs(90),
        )
        .await
        .unwrap_or_else(|error| panic!("{} delivery failed: {error:#}", case.name));
        drop(progress);
        let _ = drain.await;
        assert!(matches!(done, Done::Completed), "{}", case.name);

        let probe = probe_video(&runtime, &output).await.unwrap();
        assert_eq!((probe.width, probe.height), (128, 72), "{}", case.name);
        assert_eq!(
            probe.vcodec.as_deref(),
            Some(case.video_codec),
            "{}",
            case.name
        );
        assert_eq!(
            probe.acodec.as_deref(),
            Some(case.audio_codec),
            "{}",
            case.name
        );
        assert!(
            probe
                .container
                .format_names
                .iter()
                .any(|name| name == case.profile.muxer()),
            "{}: {:?}",
            case.name,
            probe.container.format_names
        );
        assert!(
            (probe.fps.unwrap() - 10.0).abs() < 0.01,
            "{}: {probe:?}",
            case.name
        );
        assert!(
            (probe.duration - 0.5).abs() <= 0.11,
            "{}: {probe:?}",
            case.name
        );
        assert_eq!(video_frame_count(&output).await, 5, "{}", case.name);
        if case.name == "h264" {
            let actual = frame_rgb(&output, 0.0, 128, 72).await;
            let expected = frame_rgb(&source_path, 0.2, 128, 72).await;
            let wrong_start = frame_rgb(&source_path, 0.0, 128, 72).await;
            let mean_error = |left: &[u8], right: &[u8]| {
                left.iter()
                    .zip(right)
                    .map(|(a, b)| (*a as f64 - *b as f64).abs())
                    .sum::<f64>()
                    / left.len() as f64
            };
            assert!(
                mean_error(&actual, &expected) + 1.0 < mean_error(&actual, &wrong_start),
                "range export did not begin at the authored In point"
            );
        }

        let video_stream = probe
            .streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Video)
            .unwrap();
        assert_eq!(
            video_stream.color.pixel_format.as_deref(),
            Some(case.pixel_format),
            "{}: {video_stream:?}",
            case.name
        );
        if let Some(expected_profile) = case.prores_profile {
            assert_eq!(
                video_stream.profile.as_deref(),
                Some(expected_profile),
                "{}: {video_stream:?}",
                case.name
            );
        }
        let video_end = stream_packet_end(&output, "v:0").await;
        let audio_end = stream_packet_end(&output, "a:0").await;
        assert!(
            (video_end - audio_end).abs() <= 0.1 + 1e-6,
            "{} A/V drift: video={video_end}, audio={audio_end}",
            case.name
        );
    }
}

#[tokio::test]
async fn real_composition_audio_only_profiles_have_no_video_stream_and_exact_clock() {
    let runtime = ProcessRuntime::local_default();
    if !tools_available(&runtime).await {
        eprintln!("skipping real composition audio-only profiles: ffmpeg/ffprobe unavailable");
        return;
    }
    let (encoders, muxers, _) = inspect_ffmpeg_support(&runtime).await;
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("audio-delivery-source.mp4");
    generate(
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=64x48:rate=20:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-shortest",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ],
        &source_path,
    )
    .await;
    let source = CompositionSource {
        id: source_id("audio-delivery-source"),
        kind: SourceKind::Video,
        duration_ticks: 1_000_000,
        width: 64,
        height: 48,
        has_audio: true,
    };
    let mut composition = Composition::new(CanvasSpec {
        width: 64,
        height: 48,
        fps_milli: 20_000,
        ..CanvasSpec::default()
    });
    composition.sources.insert(source.id.clone(), source);
    composition.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("audio-delivery-track").unwrap(),
        name: "Audio delivery".to_owned(),
        hidden: false,
        muted: false,
        locked: false,
        clips: vec![video_clip(
            "audio-delivery-clip",
            "audio-delivery-source",
            placement(0, 1_000_000),
        )],
        transitions: Vec::new(),
    });
    let inputs = BTreeMap::from([(source_id("audio-delivery-source"), source_path)]);

    for (codec, extension, muxer, encoder, expected_codec) in [
        (
            CompositionAudioCodec::Mp3,
            "mp3",
            "mp3",
            "libmp3lame",
            "mp3",
        ),
        (
            CompositionAudioCodec::Wav,
            "wav",
            "wav",
            "pcm_s16le",
            "pcm_s16le",
        ),
        (CompositionAudioCodec::Aac, "aac", "adts", "aac", "aac"),
        (CompositionAudioCodec::Flac, "flac", "flac", "flac", "flac"),
    ] {
        if !encoders.iter().any(|value| value == encoder)
            || !muxers.iter().any(|value| value == muxer)
        {
            continue;
        }
        let output = directory
            .path()
            .join(format!("composition-audio.{extension}"));
        let command = FfmpegCompositionExportCompiler
            .compile(CompositionExportCompileRequest {
                inputs: &inputs,
                text_resources: &BTreeMap::new(),
                destination: &output,
                parallel_jobs: 1,
                composition: &composition,
                output: CompositionExportSpec {
                    profile: CompositionExportProfile::Audio { codec },
                    video_quality: 0,
                    video_bitrate_kbps: None,
                    av1_encoder: None,
                    range: Some(CompositionExportRange {
                        start_ticks: 200_000,
                        end_ticks: 700_000,
                    }),
                },
            })
            .unwrap();
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
        assert!(matches!(done, Done::Completed), "{extension}");
        let probe = probe_video(&runtime, &output).await.unwrap();
        assert!(
            probe
                .streams
                .iter()
                .all(|stream| stream.kind != StreamKind::Video),
            "{probe:?}"
        );
        assert_eq!(probe.acodec.as_deref(), Some(expected_codec), "{probe:?}");
        assert!((probe.duration - 0.5).abs() < 0.12, "{probe:?}");
    }
}
