//! End-to-end renders of the parity-wave features against the real ffmpeg
//! binary. Each test builds an `EditRequest`, compiles it through the actual
//! export compiler, runs the emitted command, and probes the result. Skipped
//! (not failed) when ffmpeg/ffprobe are missing, and each test additionally
//! skips when the filter it needs is not compiled into the local binary.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use video_editor_backend::config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits};
use video_editor_backend::domain::artifact_graph::Fingerprint;
use video_editor_backend::model::EditRequest;
use video_editor_backend::ports::{ExportCommandCompiler, ExportCompileRequest};
use video_editor_backend::process_control::ProcessRuntime;
use video_editor_backend::services::render::{
    EditPlan, ExportExecutionProfile, RenderExecution, RenderResources, SourceMediaMetadata,
};
use video_editor_backend::tools::{
    check_tool, inspect_ffmpeg_support, probe_video, run_compiled_ffmpeg, run_ffmpeg_prepass, Done,
    FfmpegExportCompiler, ProbeInfo,
};

const SOURCE_SECONDS: f64 = 5.0;

/// A ready-to-render fixture: the runtime, a temp dir, the synthetic source and
/// its probe. `None` when ffmpeg or ffprobe are not on PATH.
struct Fixture {
    runtime: ProcessRuntime,
    dir: tempfile::TempDir,
    input: PathBuf,
    probe: ProbeInfo,
    filters: Vec<String>,
}

impl Fixture {
    async fn setup(name: &str) -> Option<Self> {
        let runtime = ProcessRuntime::local_default();
        if !check_tool(&runtime, "ffmpeg", "-version").await.0
            || !check_tool(&runtime, "ffprobe", "-version").await.0
        {
            eprintln!("skipping {name}: ffmpeg/ffprobe not on PATH");
            return None;
        }
        let (_, _, filters) = inspect_ffmpeg_support(&runtime).await;
        let dir = tempfile::tempdir().ok()?;
        let input = dir.path().join("src.mp4");
        // testsrc2 has moving content, which makes stabilization and motion
        // filters do real work instead of operating on a still frame.
        run_lavfi(
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=15:duration=5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=5",
                "-shortest",
                "-pix_fmt",
                "yuv420p",
            ],
            &input,
        )
        .await;
        let probe = probe_video(&runtime, &input).await.ok()?;
        Some(Self {
            runtime,
            dir,
            input,
            probe,
            filters,
        })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn has_filter(&self, name: &str) -> bool {
        self.filters.iter().any(|filter| filter == name)
    }

    /// Skip the test unless every named filter is present in this build.
    fn requires(&self, test: &str, needed: &[&str]) -> bool {
        for filter in needed {
            if !self.has_filter(filter) {
                eprintln!("skipping {test}: ffmpeg has no filter {filter}");
                return false;
            }
        }
        true
    }

    /// Compile and run one request, returning the probed output.
    async fn render(&self, request: serde_json::Value, resources: RenderResources) -> ProbeInfo {
        let output = self.path("out.mp4");
        let request: EditRequest = serde_json::from_value(request).expect("valid edit request");
        let plan = Arc::new(
            EditPlan::compile(
                Fingerprint::digest(b"parity-render-source"),
                request,
                SourceMediaMetadata::new_with_audio(
                    self.probe.width,
                    self.probe.height,
                    self.probe.duration,
                    self.probe.acodec.is_some(),
                )
                .expect("valid source metadata")
                .with_fps(self.probe.fps),
            )
            .expect("edit plan compiles"),
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
                .expect("encode budget"),
                verify_checksums: true,
            },
            resources,
        );
        let command = FfmpegExportCompiler
            .compile(ExportCompileRequest {
                input: &self.input,
                destination: &output,
                parallel_jobs: 1,
                execution: &execution,
            })
            .expect("export command compiles");

        let (tx, mut rx) = mpsc::unbounded_channel::<f64>();
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let token = CancellationToken::new();
        if let Some(prepass) = &command.prepass {
            let done = run_ffmpeg_prepass(
                &self.runtime,
                prepass,
                command.expected_duration_seconds,
                &tx,
                &token,
                Duration::from_secs(180),
            )
            .await
            .expect("stabilization analysis pass runs");
            assert!(matches!(done, Done::Completed));
        }
        let done = run_compiled_ffmpeg(
            &self.runtime,
            &command,
            &tx,
            &token,
            Duration::from_secs(180),
        )
        .await
        .unwrap_or_else(|error| panic!("ffmpeg rejected the emitted graph: {error}"));
        drop(tx);
        let _ = drain.await;
        assert!(matches!(done, Done::Completed));

        let size = tokio::fs::metadata(&output)
            .await
            .expect("output exists")
            .len();
        assert!(size > 0, "output file is empty");
        probe_video(&self.runtime, &output)
            .await
            .expect("output probes")
    }
}

/// Generate a fixture file with lavfi. Panics on failure: a broken fixture is a
/// broken test, not a skipped one.
async fn run_lavfi(args: &[&str], destination: &Path) {
    let generated = tokio::process::Command::new("ffmpeg")
        .arg("-y")
        .args(args)
        .arg(destination)
        .output()
        .await
        .expect("ffmpeg runs");
    assert!(
        generated.status.success(),
        "failed to generate {}: {}",
        destination.display(),
        String::from_utf8_lossy(&generated.stderr)
    );
}

fn assert_duration(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected about {expected:.2}s, got {actual:.2}s"
    );
}

#[tokio::test]
async fn real_render_segment_transition_crossfades_and_shortens_the_timeline() {
    let test = "real_render_segment_transition_crossfades_and_shortens_the_timeline";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["xfade", "acrossfade"]) {
        return;
    }

    // Two 2s segments with a 0.5s crossfade: 2 + 2 - 0.5 = 3.5s.
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "segments": [
                    { "start": 0.0, "end": 2.0 },
                    { "start": 3.0, "end": 5.0 }
                ],
                "segmentTransition": { "kind": "fade", "duration": 0.5 }
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, 3.5, 0.35, "crossfaded segments");
    assert!(out.acodec.is_some(), "audio survives the crossfade");
}

#[tokio::test]
async fn real_render_multi_source_clips_stitch_two_files() {
    let test = "real_render_multi_source_clips_stitch_two_files";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["xfade", "concat"]) {
        return;
    }

    // A second source with different content and a different frame rate, so the
    // normalization the composer emits is actually exercised.
    let second = fixture.path("second.mp4");
    run_lavfi(
        &[
            "-f",
            "lavfi",
            "-i",
            "smptebars=size=640x360:rate=25:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=220:duration=4",
            "-shortest",
            "-pix_fmt",
            "yuv420p",
        ],
        &second,
    )
    .await;

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "clips": [
                    { "sourceId": "x", "start": 0.0, "end": 2.0 },
                    {
                        "sourceId": "second",
                        "start": 0.0,
                        "end": 2.0,
                        "transitionIn": { "kind": "wipeleft", "duration": 0.5 }
                    }
                ]
            }),
            RenderResources::default().with_asset("second", &second),
        )
        .await;
    assert_duration(out.duration, 3.5, 0.35, "two stitched clips");
}

#[tokio::test]
async fn real_render_image_overlay_and_chroma_key() {
    let test = "real_render_image_overlay_and_chroma_key";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["overlay", "colorkey", "rotate"]) {
        return;
    }

    let logo = fixture.path("logo.png");
    run_lavfi(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=green:size=80x60",
            "-frames:v",
            "1",
        ],
        &logo,
    )
    .await;

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "overlays": [{
                    "assetId": "ast_0123456789abcdef01",
                    "kind": "image",
                    "x": 0.05,
                    "y": 0.05,
                    "width": 0.25,
                    "opacity": 0.8,
                    "rotation": 10.0,
                    "start": 1.0,
                    "end": 4.0,
                    "fadeIn": 0.3,
                    "fadeOut": 0.3,
                    "chromaKey": { "color": "#00FF00", "similarity": 0.2, "blend": 0.1 }
                }]
            }),
            RenderResources::default().with_asset("ast_0123456789abcdef01", &logo),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.4, "overlay render");
}

#[tokio::test]
async fn real_render_video_overlay_audio_reaches_the_mix() {
    let test = "real_render_video_overlay_audio_reaches_the_mix";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["overlay", "amix", "adelay"]) {
        return;
    }

    let pip = fixture.path("pip.mp4");
    run_lavfi(
        &[
            "-f",
            "lavfi",
            "-i",
            "smptebars=size=160x120:rate=15:duration=4",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=4",
            "-shortest",
            "-pix_fmt",
            "yuv420p",
        ],
        &pip,
    )
    .await;

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "overlays": [{
                    "assetId": "ast_0123456789abcdef02",
                    "kind": "video",
                    "x": 0.6,
                    "y": 0.6,
                    "width": 0.35,
                    "start": 0.5,
                    "end": 4.0,
                    "audio": { "enabled": true, "volume": 0.5 }
                }]
            }),
            RenderResources::default().with_asset("ast_0123456789abcdef02", &pip),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.5, "picture-in-picture");
    assert!(out.acodec.is_some(), "mixed audio survives");
}

#[tokio::test]
async fn real_render_audio_bed_with_ducking_and_dynamics() {
    let test = "real_render_audio_bed_with_ducking_and_dynamics";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(
        test,
        &[
            "amix",
            "sidechaincompress",
            "acompressor",
            "alimiter",
            "agate",
            "afftdn",
        ],
    ) {
        return;
    }

    let music = fixture.path("music.wav");
    run_lavfi(
        &["-f", "lavfi", "-i", "sine=frequency=180:duration=3"],
        &music,
    )
    .await;

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{
                    "assetId": "ast_0123456789abcdef03",
                    "role": "music",
                    "gain": 0.6,
                    "start": 0.5,
                    "sourceStart": 0.2,
                    "loop": true,
                    "fadeIn": 0.4,
                    "fadeOut": 0.4,
                    "ducking": { "enabled": true, "threshold": 0.05, "ratio": 8,
                                 "attack": 20, "release": 300 }
                }],
                "audioDynamics": {
                    "denoise": 0.3,
                    "compressor": { "threshold": -18, "ratio": 3, "attack": 20,
                                    "release": 250, "makeup": 1.0 },
                    "limiter": { "ceiling": -1.0 },
                    "gate": { "threshold": -45, "ratio": 2 },
                    "highpassHz": 80,
                    "lowpassHz": 15000,
                    "bitrateKbps": 192,
                    "volumeEnvelope": [
                        { "t": 0.0, "v": 1.0, "interp": "linear" },
                        { "t": 4.0, "v": 0.4, "interp": "linear" }
                    ]
                }
            }),
            RenderResources::default().with_asset("ast_0123456789abcdef03", &music),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.6, "audio bed render");
    assert!(out.acodec.is_some(), "output keeps an audio stream");
}

#[tokio::test]
async fn real_render_advanced_colour_grade() {
    let test = "real_render_advanced_colour_grade";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(
        test,
        &[
            "colortemperature",
            "colorbalance",
            "exposure",
            "curves",
            "huesaturation",
        ],
    ) {
        return;
    }

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "colorAdvanced": {
                    "temperature": 0.4,
                    "tint": -0.2,
                    "exposure": 0.5,
                    "highlights": -0.3,
                    "shadows": 0.25,
                    "lift": { "r": 0.05, "g": 0.0, "b": -0.05 },
                    "gamma": { "r": 1.1, "g": 1.0, "b": 0.9 },
                    "gain": { "r": 1.05, "g": 1.0, "b": 1.1 },
                    "hsl": [
                        { "band": "red", "hue": 10.0, "saturation": 1.4, "luminance": 1.1 },
                        { "band": "blue", "hue": -8.0, "saturation": 0.7, "luminance": 0.95 }
                    ]
                }
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.4, "colour grade render");
}

#[tokio::test]
async fn real_render_ken_burns_and_speed_ramp() {
    let test = "real_render_ken_burns_and_speed_ramp";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["zoompan", "rotate", "setpts", "atempo"]) {
        return;
    }

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "motion": {
                    "zoom": [
                        { "t": 0.0, "v": 1.0, "interp": "linear" },
                        { "t": 5.0, "v": 1.6, "interp": "linear" }
                    ],
                    "panX": [
                        { "t": 0.0, "v": -0.5, "interp": "linear" },
                        { "t": 5.0, "v": 0.5, "interp": "linear" }
                    ],
                    "rotation": [
                        { "t": 0.0, "v": 0.0, "interp": "linear" },
                        { "t": 5.0, "v": 6.0, "interp": "linear" }
                    ]
                }
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.6, "ken burns render");
}

#[tokio::test]
async fn real_render_speed_ramp_changes_the_output_length() {
    let test = "real_render_speed_ramp_changes_the_output_length";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["setpts", "atempo", "concat"]) {
        return;
    }

    // A constant 2x ramp over the whole source halves the output.
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "speedRamps": [
                    { "t": 0.0, "v": 2.0, "interp": "hold" },
                    { "t": 5.0, "v": 2.0, "interp": "hold" }
                ]
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS / 2.0, 0.6, "2x speed ramp");
}

#[tokio::test]
async fn real_render_360_reframe_and_lens_correction() {
    let test = "real_render_360_reframe_and_lens_correction";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["v360", "lenscorrection", "sendcmd"]) {
        return;
    }

    // A 2:1 source so the equirectangular aspect guard is satisfied.
    let equirect = fixture.path("equirect.mp4");
    run_lavfi(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x320:rate=15:duration=3",
            "-pix_fmt",
            "yuv420p",
        ],
        &equirect,
    )
    .await;
    let probe = probe_video(&fixture.runtime, &equirect).await.unwrap();
    let panoramic = Fixture {
        runtime: ProcessRuntime::local_default(),
        dir: tempfile::tempdir().unwrap(),
        input: equirect,
        probe,
        filters: fixture.filters.clone(),
    };

    let out = panoramic
        .render(
            serde_json::json!({
                "videoId": "x",
                "mute": true,
                "reframe360": {
                    "inputProjection": "equirect",
                    "outputProjection": "flat",
                    "fov": [{ "t": 0.0, "v": 90.0, "interp": "linear" }],
                    "yaw": [
                        { "t": 0.0, "v": 0.0, "interp": "linear" },
                        { "t": 3.0, "v": 120.0, "interp": "linear" }
                    ],
                    "pitch": [{ "t": 0.0, "v": 10.0, "interp": "linear" }],
                    "roll": [],
                    "outputWidth": 640,
                    "outputHeight": 360,
                    "horizonLock": false
                },
                "lensCorrection": { "k1": -0.05, "k2": 0.01 }
            }),
            RenderResources::default(),
        )
        .await;
    assert_eq!(out.width, 640, "reframed output width");
    assert_eq!(out.height, 360, "reframed output height");
}

#[tokio::test]
async fn real_render_fast_stabilization() {
    let test = "real_render_fast_stabilization";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["deshake"]) {
        return;
    }

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "stabilize": { "mode": "fast", "smoothing": 12, "zoom": 4.0,
                               "horizonLock": false }
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.5, "deshake render");
}

#[tokio::test]
async fn real_render_precise_stabilization_runs_two_passes() {
    let test = "real_render_precise_stabilization_runs_two_passes";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["vidstabdetect", "vidstabtransform"]) {
        return;
    }

    let transforms = fixture.path("vidstab.trf");
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "mute": true,
                "stabilize": { "mode": "precise", "smoothing": 10, "zoom": 2.0,
                               "horizonLock": true }
            }),
            RenderResources::default().with_asset(
                video_editor_backend::render::graph::spatial::TRANSFORMS_CONTEXT_KEY,
                &transforms,
            ),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.5, "vidstab render");
}

#[tokio::test]
async fn real_render_titles_and_burned_in_subtitles() {
    let test = "real_render_titles_and_burned_in_subtitles";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["drawtext", "subtitles"]) {
        return;
    }

    let subs = fixture.path("subs.srt");
    tokio::fs::write(
        &subs,
        "1\n00:00:00,500 --> 00:00:02,000\nПервая строка\n\n\
         2\n00:00:02,500 --> 00:00:04,000\nВторая строка\n",
    )
    .await
    .expect("subtitle fixture written");

    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                // A title whose text is full of filter syntax: if escaping is
                // wrong, ffmpeg rejects the graph and this test fails.
                "titles": [{
                    "text": "Тест: a'b\\c,d[e]f=g%{pts}",
                    "fontSize": 48,
                    "color": "#FFFFFF",
                    "x": 0.5,
                    "y": 0.85,
                    "align": "center",
                    "box": { "color": "#000000", "opacity": 0.5, "padding": 12 },
                    "borderWidth": 2.0,
                    "start": 0.5,
                    "end": 4.0,
                    "fadeIn": 0.3,
                    "fadeOut": 0.3,
                    "animation": "fade"
                }],
                "subtitles": {
                    "assetId": "ast_0123456789abcdef04",
                    "burnIn": true,
                    "fontSize": 24,
                    "color": "#FFFFFF",
                    "outlineWidth": 2,
                    "position": "bottom",
                    "marginV": 40
                }
            }),
            RenderResources::default().with_asset("ast_0123456789abcdef04", &subs),
        )
        .await;
    assert_duration(out.duration, SOURCE_SECONDS, 0.4, "titles render");
}

#[tokio::test]
async fn real_render_segments_without_a_transition_still_cut_when_a_stage_is_active() {
    let test = "real_render_segments_without_a_transition_still_cut_when_a_stage_is_active";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["concat", "huesaturation"]) {
        return;
    }

    // No `segmentTransition`: the timeline is plain segments, but a colour
    // grade forces the composed path. The cuts must survive, so the output is
    // 4s, not the untrimmed 5s.
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "segments": [
                    { "start": 0.0, "end": 2.0 },
                    { "start": 3.0, "end": 5.0 }
                ],
                "colorAdvanced": {
                    "exposure": 0.3,
                    "hsl": [{ "band": "red", "hue": 4.0, "saturation": 1.1,
                              "luminance": 1.0 }]
                }
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, 4.0, 0.4, "segments plus a grade");
}

#[tokio::test]
async fn real_render_clip_timeline_ignores_the_legacy_trim() {
    let test = "real_render_clip_timeline_ignores_the_legacy_trim";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(test, &["concat"]) {
        return;
    }

    // `clips` resolve their in/out points against the untrimmed source, so the
    // input-side `-ss`/`-t` of the legacy trim must not be emitted as well.
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "trim": { "start": 1.0, "end": 4.0 },
                "clips": [
                    { "sourceId": "x", "start": 0.0, "end": 1.5 },
                    { "sourceId": "x", "start": 3.0, "end": 5.0 }
                ]
            }),
            RenderResources::default(),
        )
        .await;
    assert_duration(out.duration, 3.5, 0.4, "clip timeline overrides trim");
}

#[tokio::test]
async fn real_render_stacks_every_available_feature_at_once() {
    let test = "real_render_stacks_every_available_feature_at_once";
    let Some(fixture) = Fixture::setup(test).await else {
        return;
    };
    if !fixture.requires(
        test,
        &[
            "xfade",
            "overlay",
            "amix",
            "zoompan",
            "huesaturation",
            "deshake",
        ],
    ) {
        return;
    }

    let logo = fixture.path("logo.png");
    run_lavfi(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=64x48",
            "-frames:v",
            "1",
        ],
        &logo,
    )
    .await;
    let music = fixture.path("music.wav");
    run_lavfi(
        &["-f", "lavfi", "-i", "sine=frequency=150:duration=4"],
        &music,
    )
    .await;

    // Segments plus a colour grade: the guard that used to let an unconsumed
    // segment timeline through silently is exercised here.
    let out = fixture
        .render(
            serde_json::json!({
                "videoId": "x",
                "segments": [
                    { "start": 0.0, "end": 2.0 },
                    { "start": 3.0, "end": 5.0 }
                ],
                "segmentTransition": { "kind": "dissolve", "duration": 0.4 },
                "scale": { "w": 320, "h": -2 },
                "colorAdvanced": {
                    "temperature": 0.2,
                    "exposure": 0.2,
                    "hsl": [{ "band": "green", "hue": 5.0, "saturation": 1.2,
                              "luminance": 1.0 }]
                },
                "motion": {
                    "zoom": [
                        { "t": 0.0, "v": 1.0, "interp": "linear" },
                        { "t": 3.5, "v": 1.3, "interp": "linear" }
                    ]
                },
                "stabilize": { "mode": "fast", "smoothing": 8, "zoom": 0.0,
                               "horizonLock": false },
                "overlays": [{
                    "assetId": "ast_0123456789abcdef05",
                    "kind": "image",
                    "x": 0.02, "y": 0.02, "width": 0.2,
                    "opacity": 0.9
                }],
                "audioTracks": [{
                    "assetId": "ast_0123456789abcdef06",
                    "role": "music",
                    "gain": 0.5,
                    "ducking": { "enabled": true, "threshold": 0.05, "ratio": 6,
                                 "attack": 20, "release": 250 }
                }],
                "normalizeAudio": true,
                "volume": 1.5
            }),
            RenderResources::default()
                .with_asset("ast_0123456789abcdef05", &logo)
                .with_asset("ast_0123456789abcdef06", &music),
        )
        .await;
    assert_duration(out.duration, 3.6, 0.6, "stacked features");
    assert!(out.acodec.is_some(), "stacked render keeps audio");
}
