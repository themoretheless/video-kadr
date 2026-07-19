//! Pure FFmpeg adapter: compiles an immutable `EditPlan` into command arguments.
//! No I/O and no process spawning, so it is fully unit-testable.

use std::path::Path;

use crate::config::encode_budget::EncodeBudget;
use crate::domain::edit::{EditSpec, LookPreset, Rotation, TimeRange};
use crate::domain::filter_graph::{FilterGraph, MediaKind};
use crate::domain::output::{OutputFormat, OutputSpec, VideoCodec};
use crate::ports::{CompiledExportCommand, ExportCommandCompiler, ExportCompileRequest};
use crate::services::render::EditPlan;

#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegExportCompiler;

impl ExportCommandCompiler for FfmpegExportCompiler {
    fn compile(&self, request: ExportCompileRequest<'_>) -> anyhow::Result<CompiledExportCommand> {
        if request.parallel_jobs == 0 {
            anyhow::bail!("invalid FFmpeg export compile request");
        }
        Ok(compile_ffmpeg_command_with_budget(
            request.input,
            request.destination,
            request.execution.plan(),
            &request.execution.profile.encode_budget,
            request.parallel_jobs,
        ))
    }
}

fn serialize_filter_chain(media: MediaKind, filters: &[String]) -> String {
    FilterGraph::linear(media, filters)
        .and_then(|graph| graph.ffmpeg_linear_chain())
        .expect("compiler emitted an invalid linear filter graph")
}

/// Map a typed look preset to its FFmpeg filter string.
fn filter_preset(preset: LookPreset) -> &'static str {
    match preset {
        LookPreset::Grayscale => "hue=s=0",
        LookPreset::Sepia => "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131",
        LookPreset::Warm => "colorbalance=rs=0.2:gs=0.05:bs=-0.2",
        LookPreset::Cold => "colorbalance=rs=-0.2:gs=0:bs=0.2",
        // Shadows toward teal, highlights toward orange (the blockbuster look).
        LookPreset::TealOrange => "colorbalance=rs=-0.15:bs=0.15:rm=0.1:bm=-0.05:rh=0.15:bh=-0.15",
        // Lifted blacks + lowered whites for a flat, matte film look.
        LookPreset::Faded => "curves=all='0/0.08 1/0.92'",
        // High-contrast black and white.
        LookPreset::Noir => "hue=s=0,eq=contrast=1.4",
        // Warm, slightly faded vintage.
        LookPreset::Vintage => "curves=all='0/0.06 1/0.95',colorbalance=rs=0.15:gs=0.05:bs=-0.1",
    }
}

/// Output file extension for a requested export format.
pub fn output_ext(format: OutputFormat) -> &'static str {
    format.extension()
}

/// Build the geometry/colour (and optionally temporal) video filter chain.
/// Order matters: crop -> rotate -> flip -> scale -> eq -> preset -> reverse
/// -> setpts -> fade. `temporal=false` skips speed/fade (used for still frames).
fn video_filters(edit: &EditSpec, out_dur: f64, temporal: bool) -> Vec<String> {
    let timing = edit.timing();
    let geometry = edit.geometry();
    let video = edit.video();
    let mut vf: Vec<String> = Vec::new();
    // Censor box first, in source coordinates (matches the on-video selection).
    if let Some(censor) = &geometry.censor {
        let rect = censor.rect;
        let color = censor.color.ffmpeg_name();
        vf.push(format!(
            "drawbox=x={}:y={}:w={}:h={}:color={color}:t=fill",
            rect.x, rect.y, rect.width, rect.height
        ));
    }
    if let Some(crop) = geometry.crop {
        // Force even dimensions when possible; never turn a one-pixel source
        // edge into FFmpeg's invalid zero-sized crop.
        let width = (crop.width & !1).max(1);
        let height = (crop.height & !1).max(1);
        vf.push(format!("crop={width}:{height}:{}:{}", crop.x, crop.y));
    }
    match geometry.rotation {
        Rotation::Clockwise90 => vf.push("transpose=1".into()),
        Rotation::Clockwise180 => {
            vf.push("transpose=1".into());
            vf.push("transpose=1".into());
        }
        Rotation::Clockwise270 => vf.push("transpose=2".into()),
        Rotation::None => {}
    }
    if geometry.flip_horizontal {
        vf.push("hflip".into());
    }
    if geometry.flip_vertical {
        vf.push("vflip".into());
    }
    if let Some(scale) = geometry.scale {
        vf.push(format!("scale={}:{}", scale.width, scale.height));
    }
    // Letterbox/pillarbox to a target aspect (adds bars, keeps whole frame).
    if let Some(aspect) = geometry.pad_aspect {
        let (tw, th) = (aspect.width, aspect.height);
        vf.push(format!(
            "pad=w='ceil(max(iw,ih*{tw}/{th})/2)*2':h='ceil(max(ih,iw*{th}/{tw})/2)*2':x='(ow-iw)/2':y='(oh-ih)/2':color=black"
        ));
    }
    if video.denoise {
        vf.push("hqdn3d".into());
    }
    let eq_changed = video.brightness.abs() > 1e-6
        || (video.contrast - 1.0).abs() > 1e-6
        || (video.saturation - 1.0).abs() > 1e-6;
    if eq_changed {
        vf.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            video.brightness, video.contrast, video.saturation
        ));
    }
    if let Some(look) = video.look {
        vf.push(filter_preset(look).into());
    }
    if video.sharpen > 1e-6 {
        vf.push(format!("unsharp=5:5:{:.3}:5:5:0.0", video.sharpen));
    }
    if video.vignette {
        vf.push("vignette".into());
    }
    if video.grain > 1e-6 {
        vf.push(format!("noise=alls={:.0}:allf=t", video.grain));
    }
    if timing.reverse {
        vf.push("reverse".into());
    }
    if temporal {
        let speed = timing.speed;
        if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
            vf.push(format!("setpts={:.6}*PTS", 1.0 / speed));
        }
        if timing.fade_in_seconds > 0.0 {
            vf.push(format!("fade=t=in:st=0:d={:.3}", timing.fade_in_seconds));
        }
        if timing.fade_out_seconds > 0.0 && out_dur > timing.fade_out_seconds {
            vf.push(format!(
                "fade=t=out:st={:.3}:d={:.3}",
                out_dur - timing.fade_out_seconds,
                timing.fade_out_seconds
            ));
        }
    }
    vf
}

/// Build the audio filter chain: areverse -> volume -> atempo -> afade.
fn audio_filters(edit: &EditSpec, out_dur: f64) -> Vec<String> {
    let timing = edit.timing();
    let audio = edit.audio();
    let mut af: Vec<String> = Vec::new();
    if timing.reverse {
        af.push("areverse".into());
    }
    if audio.highpass {
        af.push("highpass=f=100".into());
    }
    if (audio.volume - 1.0).abs() > 1e-6 {
        af.push(format!("volume={:.3}", audio.volume));
    }
    let speed = timing.speed;
    if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
        // atempo only accepts 0.5..=2.0; EditPlan validates that range.
        af.push(format!("atempo={:.6}", speed.clamp(0.5, 2.0)));
    }
    if timing.fade_in_seconds > 0.0 {
        af.push(format!("afade=t=in:st=0:d={:.3}", timing.fade_in_seconds));
    }
    if timing.fade_out_seconds > 0.0 && out_dur > timing.fade_out_seconds {
        af.push(format!(
            "afade=t=out:st={:.3}:d={:.3}",
            out_dur - timing.fade_out_seconds,
            timing.fade_out_seconds
        ));
    }
    // Loudness normalization is applied last, on the finished chain.
    if audio.normalize {
        af.push("loudnorm=I=-14:TP=-1.5:LRA=11".into());
    }
    af
}

/// Append audio options: `-an` when muted, otherwise the filter chain + codec.
fn push_audio(args: &mut Vec<String>, edit: &EditSpec, out_dur: f64, codec: &str) {
    if edit.audio().muted {
        args.push("-an".into());
        return;
    }
    let af = audio_filters(edit, out_dur);
    if !af.is_empty() {
        args.push("-af".into());
        args.push(serialize_filter_chain(MediaKind::Audio, &af));
    }
    args.push("-c:a".into());
    args.push(codec.into());
    args.push("-b:a".into());
    args.push("128k".into());
}

fn push_fps(args: &mut Vec<String>, output: &OutputSpec) {
    if let Some(fps) = output.fps() {
        args.push("-r".into());
        args.push(format!("{fps:.3}"));
    }
}

/// Build the FFmpeg argument list for an immutable edit plan.
///
/// Trim is applied as an *input* option (`-ss` + `-t`) so it happens before the
/// filter graph; geometry/colour/speed/fade then operate on the trimmed stream.
/// The output container/codecs depend on `edit.format` (mp4/webm/gif/png/mp3).
pub fn build_ffmpeg_args(input: &Path, destination: &Path, plan: &EditPlan) -> Vec<String> {
    compile_ffmpeg_command(input, destination, plan).arguments
}

fn compile_ffmpeg_command(
    input: &Path,
    destination: &Path,
    plan: &EditPlan,
) -> CompiledExportCommand {
    let edit = &plan.edit;
    let output = &plan.output;
    let format = output.format;
    let source_duration = plan.source.duration_seconds();
    let out_dur = expected_output_secs(edit, source_duration);

    // Multi-segment edits (cut from the middle / stitch ranges) need a concat
    // filter graph; only meaningful for the video containers.
    let segs = valid_segments(edit);
    if !segs.is_empty()
        && matches!(
            format,
            OutputFormat::Mp4 | OutputFormat::Webm | OutputFormat::Av1 | OutputFormat::Prores
        )
    {
        return CompiledExportCommand {
            arguments: build_concat_args(input, destination, plan, segs, out_dur),
            expected_duration_seconds: out_dur,
        };
    }

    let mut args: Vec<String> = vec!["-y".into()];

    // --- input-side trim ---
    if let Some(trim) = edit.timing().trim {
        args.push("-ss".into());
        args.push(format_secs(trim.start_seconds()));
        args.push("-t".into());
        args.push(format_secs(trim.duration_seconds()));
    }

    args.push("-i".into());
    args.push(input.to_string_lossy().into_owned());

    match format {
        OutputFormat::Mp3 => {
            // Audio-only extraction.
            let af = audio_filters(edit, out_dur);
            if !af.is_empty() {
                args.push("-af".into());
                args.push(serialize_filter_chain(MediaKind::Audio, &af));
            }
            args.push("-vn".into());
            args.push("-c:a".into());
            args.push("libmp3lame".into());
            args.push("-q:a".into());
            args.push("2".into());
        }
        OutputFormat::Png | OutputFormat::Jpg => {
            // Single still frame at the trim start (positioned by -ss above).
            // The encoder is chosen by the output extension (png / mjpeg).
            let vf = video_filters(edit, out_dur, false);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &vf));
            }
            args.push("-frames:v".into());
            args.push("1".into());
            args.push("-an".into());
        }
        OutputFormat::Gif => {
            // Generate a per-clip palette for a good-looking gif (single pass
            // via split + palettegen/paletteuse).
            let mut parts = video_filters(edit, out_dur, true);
            let fps = output.fps().unwrap_or(12.0);
            parts.push(format!("fps={fps:.3}"));
            let graph = format!(
                "{},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
                serialize_filter_chain(MediaKind::Video, &parts)
            );
            args.push("-vf".into());
            args.push(graph);
            args.push("-an".into());
        }
        OutputFormat::Webm => {
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &vf));
            }
            push_audio(&mut args, edit, out_dur, "libopus");
            push_video_codec(&mut args, output);
        }
        OutputFormat::Av1 => {
            // Modern, compact codec in an mp4 container (needs libsvtav1).
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &vf));
            }
            push_audio(&mut args, edit, out_dur, "aac");
            args.push("-c:v".into());
            args.push("libsvtav1".into());
            args.push("-crf".into());
            args.push(output.crf.expect("AV1 output has CRF").to_string());
            args.push("-preset".into());
            args.push("6".into());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
            push_fps(&mut args, output);
            args.push("-movflags".into());
            args.push("+faststart".into());
        }
        OutputFormat::Prores => {
            // Intra-only edit codec in a .mov; audio as PCM. prores_ks is built in.
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &vf));
            }
            if edit.audio().muted {
                args.push("-an".into());
            } else {
                let af = audio_filters(edit, out_dur);
                if !af.is_empty() {
                    args.push("-af".into());
                    args.push(serialize_filter_chain(MediaKind::Audio, &af));
                }
                args.push("-c:a".into());
                args.push("pcm_s16le".into());
            }
            args.push("-c:v".into());
            args.push("prores_ks".into());
            args.push("-profile:v".into());
            args.push("3".into());
            args.push("-pix_fmt".into());
            args.push("yuv422p10le".into());
            push_fps(&mut args, output);
        }
        OutputFormat::Mp4 => {
            // mp4 (default): H.264 or H.265.
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &vf));
            }
            push_audio(&mut args, edit, out_dur, "aac");
            push_video_codec(&mut args, output);
        }
    }

    args.push(destination.to_string_lossy().into_owned());
    CompiledExportCommand {
        arguments: args,
        expected_duration_seconds: out_dur,
    }
}

/// Apply the process-wide encode budget to one render. The global thread budget
/// is divided across concurrently admitted render jobs to avoid oversubscription.
pub fn build_ffmpeg_args_with_budget(
    input: &Path,
    output: &Path,
    plan: &EditPlan,
    budget: &EncodeBudget,
    parallel_jobs: usize,
) -> Vec<String> {
    compile_ffmpeg_command_with_budget(input, output, plan, budget, parallel_jobs).arguments
}

fn compile_ffmpeg_command_with_budget(
    input: &Path,
    output: &Path,
    plan: &EditPlan,
    budget: &EncodeBudget,
    parallel_jobs: usize,
) -> CompiledExportCommand {
    let mut command = compile_ffmpeg_command(input, output, plan);
    let destination = command
        .arguments
        .pop()
        .expect("ffmpeg arguments always end in output");
    command
        .arguments
        .extend(budget.ffmpeg_thread_args_for_job(parallel_jobs));
    if plan.output.format == OutputFormat::Av1 {
        command.arguments.extend(budget.ffmpeg_av1_tile_args());
        if let Some(preset) = command
            .arguments
            .iter()
            .position(|argument| argument == "-preset")
        {
            if let Some(value) = command.arguments.get_mut(preset + 1) {
                *value = budget.speed.to_string();
            }
        }
    }
    command.arguments.push(destination);
    command
}

/// Push the video encoder and its quality/pixel-format options for `format`
/// (VP9 for webm, otherwise H.264/H.265), then fps and (for mp4) faststart.
/// Shared by the single-pass and concat paths so codec settings live in one place.
fn push_video_codec(args: &mut Vec<String>, output: &OutputSpec) {
    match output.format {
        OutputFormat::Webm => {
            args.push("-c:v".into());
            args.push("libvpx-vp9".into());
            args.push("-crf".into());
            args.push(output.crf.expect("WebM output has CRF").to_string());
            args.push("-b:v".into());
            args.push("0".into());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
            push_fps(args, output);
        }
        OutputFormat::Av1 => {
            args.push("-c:v".into());
            args.push("libsvtav1".into());
            args.push("-crf".into());
            args.push(output.crf.expect("AV1 output has CRF").to_string());
            args.push("-preset".into());
            args.push("6".into());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
            push_fps(args, output);
            args.push("-movflags".into());
            args.push("+faststart".into());
        }
        OutputFormat::Prores => {
            args.push("-c:v".into());
            args.push("prores_ks".into());
            args.push("-profile:v".into());
            args.push("3".into());
            args.push("-pix_fmt".into());
            args.push("yuv422p10le".into());
            push_fps(args, output);
        }
        OutputFormat::Mp4 => {
            let h265 = output.video_codec == Some(VideoCodec::H265);
            args.push("-c:v".into());
            args.push(if h265 { "libx265" } else { "libx264" }.into());
            args.push("-preset".into());
            args.push("veryfast".into());
            args.push("-crf".into());
            args.push(output.crf.expect("MP4 output has CRF").to_string());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
            if h265 {
                // hvc1 tag keeps the result playable in QuickTime/Safari.
                args.push("-tag:v".into());
                args.push("hvc1".into());
            }
            push_fps(args, output);
            args.push("-movflags".into());
            args.push("+faststart".into());
        }
        _ => unreachable!("still, GIF, and audio-only outputs do not use video codec options"),
    }
}

/// Build args for a multi-segment edit: trim each keep-segment, concat them,
/// then apply the usual geometry/colour/speed/fade filters to the result.
fn build_concat_args(
    input: &Path,
    destination: &Path,
    plan: &EditPlan,
    segments: &[TimeRange],
    out_dur: f64,
) -> Vec<String> {
    let edit = &plan.edit;
    let output = &plan.output;
    let format = output.format;
    let muted = edit.audio().muted;
    let n = segments.len();
    let mut graph = String::new();
    for (i, s) in segments.iter().enumerate() {
        graph.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{i}];",
            s.start_seconds(),
            s.end_seconds()
        ));
        if !muted {
            graph.push_str(&format!(
                "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}];",
                s.start_seconds(),
                s.end_seconds()
            ));
        }
    }
    for i in 0..n {
        graph.push_str(&format!("[v{i}]"));
        if !muted {
            graph.push_str(&format!("[a{i}]"));
        }
    }
    if muted {
        graph.push_str(&format!("concat=n={n}:v=1:a=0[cv]"));
    } else {
        graph.push_str(&format!("concat=n={n}:v=1:a=1[cv][ca]"));
    }

    // Effects apply to the concatenated stream.
    let vf = video_filters(edit, out_dur, true);
    let vmap = if vf.is_empty() {
        "[cv]".to_string()
    } else {
        graph.push_str(&format!(
            ";[cv]{}[vout]",
            serialize_filter_chain(MediaKind::Video, &vf)
        ));
        "[vout]".to_string()
    };
    let amap = if muted {
        None
    } else {
        let af = audio_filters(edit, out_dur);
        if af.is_empty() {
            Some("[ca]".to_string())
        } else {
            graph.push_str(&format!(
                ";[ca]{}[aout]",
                serialize_filter_chain(MediaKind::Audio, &af)
            ));
            Some("[aout]".to_string())
        }
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        graph,
        "-map".into(),
        vmap,
    ];
    if let Some(am) = &amap {
        args.push("-map".into());
        args.push(am.clone());
    }

    push_video_codec(&mut args, output);
    if amap.is_some() {
        args.push("-c:a".into());
        args.push(
            match format {
                OutputFormat::Webm => "libopus",
                OutputFormat::Prores => "pcm_s16le",
                _ => "aac",
            }
            .into(),
        );
        if format != OutputFormat::Prores {
            args.push("-b:a".into());
            args.push("128k".into());
        }
    }

    args.push(destination.to_string_lossy().into_owned());
    args
}

/// Valid keep-segments (positive length), if any were requested.
fn valid_segments(edit: &EditSpec) -> &[TimeRange] {
    &edit.timing().segments
}

/// Expected output duration (seconds) for an edit, used to scale ffmpeg progress.
pub fn expected_output_secs(edit: &EditSpec, source_duration: f64) -> f64 {
    let segs = valid_segments(edit);
    let base = if !segs.is_empty() {
        segs.iter().map(|range| range.duration_seconds()).sum()
    } else {
        match edit.timing().trim {
            Some(range) => range.duration_seconds(),
            None => source_duration,
        }
    };
    base / edit.timing().speed
}

/// Format seconds without scientific notation, trimming trailing noise.
fn format_secs(s: f64) -> String {
    format!("{s:.3}")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::encode_budget::TileLayout;
    use crate::domain::artifact_graph::Fingerprint;
    use crate::model::EditRequest;
    use crate::services::render::{ExportExecutionProfile, RenderExecution, SourceMediaMetadata};
    use serde_json::json;

    fn plan_for_duration(value: serde_json::Value, duration_seconds: f64) -> EditPlan {
        let request: EditRequest = serde_json::from_value(value).expect("valid EditRequest");
        EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1920, 1080, duration_seconds).unwrap(),
        )
        .expect("valid EditPlan")
    }

    fn plan(value: serde_json::Value) -> EditPlan {
        plan_for_duration(value, 60.0)
    }

    fn args_for(v: serde_json::Value, dur: f64) -> Vec<String> {
        let input = Path::new("/in.mp4");
        let output = Path::new("/out.mp4");
        build_ffmpeg_args(input, output, &plan_for_duration(v, dur))
    }

    #[test]
    fn process_budget_is_divided_across_parallel_renders() {
        let plan = Arc::new(plan_for_duration(
            json!({ "videoId": "x", "format": "av1" }),
            10.0,
        ));
        let budget = EncodeBudget {
            threads: 8,
            tiles: TileLayout {
                columns: 2,
                rows: 1,
            },
            speed: 3,
            memory_mib: 2048,
        };
        let execution = RenderExecution::new(
            plan,
            ExportExecutionProfile {
                encode_budget: budget,
                verify_checksums: true,
            },
        );
        let command = FfmpegExportCompiler
            .compile(ExportCompileRequest {
                input: Path::new("/in.mp4"),
                destination: Path::new("/out.mp4"),
                parallel_jobs: 2,
                execution: &execution,
            })
            .unwrap();
        let args = command.arguments;
        let filter_threads = args
            .iter()
            .position(|argument| argument == "-filter_threads")
            .unwrap();
        let encoder_threads = args
            .iter()
            .position(|argument| argument == "-threads:v")
            .unwrap();
        let preset = args
            .iter()
            .position(|argument| argument == "-preset")
            .unwrap();
        let tile_columns = args
            .iter()
            .position(|argument| argument == "-tile_columns")
            .unwrap();
        let tile_rows = args
            .iter()
            .position(|argument| argument == "-tile_rows")
            .unwrap();
        assert_eq!(args[filter_threads + 1], "4");
        assert_eq!(args[encoder_threads + 1], "4");
        assert_eq!(args[preset + 1], "3");
        assert_eq!(args[tile_columns + 1], "1");
        assert_eq!(args[tile_rows + 1], "0");
        assert_eq!(command.expected_duration_seconds, 10.0);
    }

    #[test]
    fn process_budget_never_rounds_parallel_jobs_up_past_total() {
        let budget = EncodeBudget {
            threads: 8,
            tiles: TileLayout {
                columns: 1,
                rows: 1,
            },
            speed: 6,
            memory_mib: 1024,
        };
        let args = budget.ffmpeg_thread_args_for_job(3);
        assert_eq!(args[1], "2");
        assert_eq!(args[3], "2");
    }

    fn vf(args: &[String]) -> String {
        let i = args.iter().position(|a| a == "-vf").expect("has -vf");
        args[i + 1].clone()
    }

    fn af(args: &[String]) -> Option<String> {
        args.iter()
            .position(|a| a == "-af")
            .map(|i| args[i + 1].clone())
    }

    #[test]
    fn default_edit_reencodes_with_audio() {
        let args = args_for(json!({ "videoId": "x" }), 10.0);
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"aac".to_string()));
        assert!(!args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
    }

    #[test]
    fn trim_is_input_side() {
        let args = args_for(
            json!({ "videoId": "x", "trim": { "start": 2.0, "end": 5.0 } }),
            10.0,
        );
        let ss = args.iter().position(|a| a == "-ss").unwrap();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert!(ss < i, "-ss must precede -i");
        assert_eq!(
            args[args.iter().position(|a| a == "-t").unwrap() + 1],
            "3.000"
        );
    }

    #[test]
    fn mute_drops_audio() {
        let args = args_for(json!({ "videoId": "x", "mute": true }), 10.0);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"aac".to_string()));
        assert!(af(&args).is_none());
    }

    #[test]
    fn geometry_and_colour_order() {
        let args = args_for(
            json!({
                "videoId": "x",
                "crop": { "x": 10, "y": 20, "w": 101, "h": 51 },
                "rotate": 90,
                "flipH": true,
                "scale": { "w": 640, "h": -2 },
                "filter": "grayscale",
                "fadeOut": 1.0
            }),
            10.0,
        );
        let chain = vf(&args);
        // Crop forces even dimensions.
        assert!(chain.contains("crop=100:50:10:20"), "{chain}");
        let order = |s: &str| chain.find(s).unwrap();
        assert!(order("crop") < order("transpose=1"));
        assert!(order("transpose=1") < order("hflip"));
        assert!(order("hflip") < order("scale"));
        assert!(order("scale") < order("hue=s=0"));
        assert!(chain.contains("fade=t=out:st=9.000:d=1.000"), "{chain}");
    }

    #[test]
    fn one_pixel_source_crop_never_compiles_zero_dimension() {
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "crop": { "x": 0, "y": 0, "w": 1, "h": 1 }
        }))
        .unwrap();
        let plan = EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1, 1, 10.0).unwrap(),
        )
        .unwrap();
        let args = build_ffmpeg_args(Path::new("/in.mp4"), Path::new("/out.mp4"), &plan);

        assert!(vf(&args).contains("crop=1:1:0:0"));
    }

    #[test]
    fn audio_chain_volume_and_speed() {
        let args = args_for(json!({ "videoId": "x", "volume": 1.5, "speed": 2.0 }), 10.0);
        let chain = af(&args).expect("has -af");
        assert!(chain.contains("volume=1.500"), "{chain}");
        assert!(chain.contains("atempo=2.000000"), "{chain}");
        // Speed halves duration: setpts on the video side.
        assert!(vf(&args).contains("setpts="));
    }

    #[test]
    fn fps_override_present() {
        let args = args_for(json!({ "videoId": "x", "fps": 30.0 }), 10.0);
        let i = args.iter().position(|a| a == "-r").expect("has -r");
        assert_eq!(args[i + 1], "30.000");
    }

    #[test]
    fn output_ext_maps_formats() {
        assert_eq!(output_ext(OutputFormat::Mp4), "mp4");
        assert_eq!(output_ext(OutputFormat::Webm), "webm");
        assert_eq!(output_ext(OutputFormat::Gif), "gif");
        assert_eq!(output_ext(OutputFormat::Png), "png");
        assert_eq!(output_ext(OutputFormat::Jpg), "jpg");
        assert_eq!(output_ext(OutputFormat::Mp3), "mp3");
        assert_eq!(output_ext(OutputFormat::Prores), "mov");
        assert_eq!(output_ext(OutputFormat::Av1), "mp4");
        assert!(OutputFormat::parse(Some("weird")).is_err());
    }

    #[test]
    fn audio_normalize_and_highpass() {
        let args = args_for(
            json!({ "videoId": "x", "normalizeAudio": true, "highpass": true }),
            10.0,
        );
        let chain = af(&args).expect("has -af");
        assert!(chain.contains("loudnorm"), "{chain}");
        assert!(chain.contains("highpass=f=100"), "{chain}");
    }

    #[test]
    fn av1_format_uses_svtav1() {
        let args = args_for(
            json!({ "videoId": "x", "format": "av1", "quality": 30 }),
            10.0,
        );
        assert!(args.contains(&"libsvtav1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "30");
    }

    #[test]
    fn prores_format_uses_prores_ks_and_pcm() {
        let args = args_for(json!({ "videoId": "x", "format": "prores" }), 10.0);
        assert!(args.contains(&"prores_ks".to_string()));
        assert!(args.contains(&"pcm_s16le".to_string())); // not muted -> PCM audio
        assert!(!args.contains(&"+faststart".to_string())); // mov, not mp4
    }

    #[test]
    fn jpg_grabs_single_frame() {
        let args = args_for(json!({ "videoId": "x", "format": "jpg" }), 10.0);
        assert_eq!(
            args[args.iter().position(|a| a == "-frames:v").unwrap() + 1],
            "1"
        );
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn h265_codec_and_quality() {
        let args = args_for(
            json!({ "videoId": "x", "codec": "h265", "quality": 20 }),
            10.0,
        );
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "20");
    }

    #[test]
    fn webm_uses_vp9_and_opus() {
        let args = args_for(json!({ "videoId": "x", "format": "webm" }), 10.0);
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.contains(&"libopus".to_string()));
        assert!(!args.contains(&"+faststart".to_string()));
    }

    #[test]
    fn gif_has_palette_graph_and_no_audio() {
        let args = args_for(json!({ "videoId": "x", "format": "gif" }), 10.0);
        let chain = vf(&args);
        assert!(chain.contains("palettegen"), "{chain}");
        assert!(chain.contains("paletteuse"), "{chain}");
        assert!(chain.contains("fps="), "{chain}");
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn png_grabs_single_frame() {
        let args = args_for(
            json!({ "videoId": "x", "format": "png", "trim": { "start": 3.0, "end": 9.0 } }),
            10.0,
        );
        let frames = args.iter().position(|a| a == "-frames:v").unwrap();
        assert_eq!(args[frames + 1], "1");
        assert!(args.contains(&"-an".to_string()));
        // Trim still positions the grab.
        assert_eq!(
            args[args.iter().position(|a| a == "-ss").unwrap() + 1],
            "3.000"
        );
    }

    fn filter_complex(args: &[String]) -> String {
        let i = args
            .iter()
            .position(|a| a == "-filter_complex")
            .expect("has -filter_complex");
        args[i + 1].clone()
    }

    #[test]
    fn censor_vignette_pad_in_chain() {
        let args = args_for(
            json!({
                "videoId": "x",
                "censor": { "x": 10, "y": 20, "w": 100, "h": 50 },
                "censorColor": "white",
                "vignette": true,
                "pad": "9:16",
                "crop": { "x": 0, "y": 0, "w": 320, "h": 240 }
            }),
            10.0,
        );
        let chain = vf(&args);
        assert!(
            chain.contains("drawbox=x=10:y=20:w=100:h=50:color=white:t=fill"),
            "{chain}"
        );
        assert!(chain.contains("vignette"), "{chain}");
        assert!(chain.contains("pad=w="), "{chain}");
        // Censor is applied before crop (source coordinates).
        assert!(chain.find("drawbox").unwrap() < chain.find("crop=").unwrap());
    }

    #[test]
    fn denoise_sharpen_grain_in_chain() {
        let args = args_for(
            json!({ "videoId": "x", "denoise": true, "sharpen": 1.5, "grain": 20.0, "filter": "teal-orange" }),
            10.0,
        );
        let chain = vf(&args);
        assert!(chain.contains("hqdn3d"), "{chain}");
        assert!(chain.contains("unsharp=5:5:1.500"), "{chain}");
        assert!(chain.contains("noise=alls=20"), "{chain}");
        assert!(chain.contains("colorbalance="), "{chain}"); // teal-orange preset
                                                             // Order: denoise -> preset -> sharpen -> grain.
        assert!(chain.find("hqdn3d").unwrap() < chain.find("unsharp").unwrap());
        assert!(chain.find("unsharp").unwrap() < chain.find("noise=alls").unwrap());
    }

    #[test]
    fn look_presets_map_to_filters() {
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "faded" }),
            10.0
        ))
        .contains("curves="));
        assert!(
            vf(&args_for(json!({ "videoId": "x", "filter": "noir" }), 10.0)).contains("hue=s=0")
        );
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "vintage" }),
            10.0
        ))
        .contains("curves="));
    }

    #[test]
    fn unknown_censor_color_is_rejected() {
        let request: EditRequest = serde_json::from_value(
            json!({ "videoId": "x", "censor": { "x": 0, "y": 0, "w": 10, "h": 10 }, "censorColor": "; rm -rf" }),
        )
        .unwrap();
        let result = EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn segments_concat_graph() {
        let args = args_for(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 4.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("trim=start=0.000:end=1.000"), "{graph}");
        assert!(graph.contains("trim=start=3.000:end=4.000"), "{graph}");
        assert!(graph.contains("concat=n=2:v=1:a=1[cv][ca]"), "{graph}");
        assert!(args.contains(&"libx264".to_string()));
        // The concat path replaces input-side trim.
        assert!(!args.contains(&"-ss".to_string()));
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 2);
    }

    #[test]
    fn segments_muted_drops_audio_streams() {
        let args = args_for(
            json!({
                "videoId": "x",
                "mute": true,
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("concat=n=2:v=1:a=0[cv]"), "{graph}");
        assert!(!graph.contains("atrim"), "{graph}");
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 1);
    }

    #[test]
    fn segments_carry_codec_and_faststart() {
        // The concat path must produce the same encoder settings as the simple
        // path (shared via push_video_codec): h265 tag + faststart + mapped audio.
        let args = args_for(
            json!({
                "videoId": "x",
                "codec": "h265",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        assert!(args.contains(&"aac".to_string())); // not muted -> audio mapped
    }

    #[test]
    fn segments_concat_supports_av1() {
        let args = args_for(
            json!({
                "videoId": "x",
                "format": "av1",
                "quality": 30,
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(filter_complex(&args).contains("concat=n=2:v=1:a=1[cv][ca]"));
        assert!(args.contains(&"libsvtav1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "30");
    }

    #[test]
    fn segments_concat_supports_prores() {
        let args = args_for(
            json!({
                "videoId": "x",
                "format": "prores",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(filter_complex(&args).contains("concat=n=2:v=1:a=1[cv][ca]"));
        assert!(args.contains(&"prores_ks".to_string()));
        assert!(args.contains(&"pcm_s16le".to_string()));
        assert!(!args.contains(&"+faststart".to_string()));
    }

    #[test]
    fn segments_ignored_for_gif() {
        // gif is not a concat target, so it falls back to the normal path.
        let args = args_for(
            json!({ "videoId": "x", "format": "gif", "segments": [{ "start": 0.0, "end": 1.0 }] }),
            5.0,
        );
        assert!(!args.contains(&"-filter_complex".to_string()));
        assert!(vf(&args).contains("palettegen"));
    }

    #[test]
    fn expected_output_secs_trim_speed_segments() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Whole clip.
        assert!(close(
            expected_output_secs(&plan(json!({ "videoId": "x" })).edit, 10.0),
            10.0
        ));
        // Trim narrows to its length.
        assert!(close(
            expected_output_secs(
                &plan(json!({ "videoId": "x", "trim": { "start": 2.0, "end": 7.0 } })).edit,
                10.0
            ),
            5.0
        ));
        // Speed shortens proportionally.
        assert!(close(
            expected_output_secs(&plan(json!({ "videoId": "x", "speed": 2.0 })).edit, 10.0),
            5.0
        ));
        // Segments sum their lengths (and override trim).
        assert!(close(
            expected_output_secs(
                &plan(json!({
                    "videoId": "x",
                    "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 5.0 }]
                }))
                .edit,
                10.0
            ),
            3.0
        ));
    }

    #[test]
    fn mp3_is_audio_only() {
        let args = args_for(
            json!({ "videoId": "x", "format": "mp3", "volume": 0.5 }),
            10.0,
        );
        assert!(args.contains(&"-vn".to_string()));
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
        assert!(af(&args).unwrap().contains("volume=0.500"));
    }
}
