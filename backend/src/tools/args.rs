//! Pure FFmpeg adapter: compiles an immutable `EditPlan` into command arguments.
//! No I/O and no process spawning, so it is fully unit-testable.

use std::path::Path;

use crate::config::encode_budget::EncodeBudget;
use crate::domain::edit::{
    ColorWheels, EditSpec, HslAdjustments, HslBand, Rotation, TimeRange, ToneCurve, ToneCurves,
};
use crate::domain::filter_graph::{FilterGraph, MediaKind};
use crate::domain::output::{OutputFormat, OutputSpec, VideoCodec};
use crate::ports::{CompiledExportCommand, ExportCommandCompiler, ExportCompileRequest};
use crate::services::render::EditPlan;

use super::looks::look_preset_definition;

#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegExportCompiler;

impl ExportCommandCompiler for FfmpegExportCompiler {
    fn compile(&self, request: ExportCompileRequest<'_>) -> anyhow::Result<CompiledExportCommand> {
        if request.parallel_jobs == 0 {
            anyhow::bail!("invalid FFmpeg export compile request");
        }
        compile_ffmpeg_command_with_budget(
            request.input,
            request.destination,
            request.execution.plan(),
            &request.execution.profile.encode_budget,
            request.parallel_jobs,
            request.execution.resources().lut_path(),
        )
    }
}

fn serialize_filter_chain(media: MediaKind, filters: &[String]) -> String {
    FilterGraph::linear(media, filters)
        .and_then(|graph| graph.ffmpeg_linear_chain())
        .expect("compiler emitted an invalid linear filter graph")
}

/// Output file extension for a requested export format.
pub fn output_ext(format: OutputFormat) -> &'static str {
    format.extension()
}

#[derive(Debug)]
struct VideoFilterParts {
    before_lut: Vec<String>,
    after_lut: Vec<String>,
}

#[derive(Debug)]
enum VideoFilterProgram {
    Linear(Vec<String>),
    Complex(String),
}

fn tone_curve_points(curve: &ToneCurve) -> String {
    curve
        .points()
        .iter()
        // f64 Display uses the shortest round-tripping representation. Fixed
        // six-decimal output could collapse distinct, validated x coordinates.
        .map(|point| format!("{}/{}", point.x(), point.y()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn curves_filter(curves: &ToneCurves) -> String {
    let mut options = Vec::new();
    for (name, curve) in [
        ("master", curves.master()),
        ("red", curves.red()),
        ("green", curves.green()),
        ("blue", curves.blue()),
    ] {
        if let Some(curve) = curve {
            options.push(format!("{name}='{}'", tone_curve_points(curve)));
        }
    }
    options.push("interp=pchip".into());
    format!("curves={}", options.join(":"))
}

fn hsl_filters(adjustments: &HslAdjustments) -> Vec<String> {
    [
        ("r", adjustments.red),
        ("y", adjustments.yellow),
        ("g", adjustments.green),
        ("c", adjustments.cyan),
        ("b", adjustments.blue),
        ("m", adjustments.magenta),
    ]
    .into_iter()
    .filter_map(|(color, band)| hsl_band_filter(color, band))
    .collect()
}

fn hsl_band_filter(color: &str, band: HslBand) -> Option<String> {
    (!band.is_identity()).then(|| {
        format!(
            "huesaturation=hue={:.6}:saturation={:.6}:intensity={:.6}:colors={color}:strength=1",
            band.hue, band.saturation, band.lightness
        )
    })
}

fn color_wheels_filter(wheels: &ColorWheels) -> Option<String> {
    (!wheels.is_identity()).then(|| {
        format!(
            "colorbalance=rs={:.6}:gs={:.6}:bs={:.6}:rm={:.6}:gm={:.6}:bm={:.6}:rh={:.6}:gh={:.6}:bh={:.6}:pl={}",
            wheels.shadows.red,
            wheels.shadows.green,
            wheels.shadows.blue,
            wheels.midtones.red,
            wheels.midtones.green,
            wheels.midtones.blue,
            wheels.highlights.red,
            wheels.highlights.green,
            wheels.highlights.blue,
            u8::from(wheels.preserve_luminosity),
        )
    })
}

/// Escape a path for one quoted FFmpeg filter option. This is filtergraph
/// escaping, not shell escaping: the command is still passed as an argv vector.
fn escape_filter_value(path: &Path) -> anyhow::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("LUT path is not valid UTF-8"))?;
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if matches!(character, '\\' | '\'' | ':' | ',' | ';' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(format!("'{escaped}'"))
}

fn lut3d_filter(path: &Path) -> anyhow::Result<String> {
    Ok(format!(
        "lut3d=file={}:interp=tetrahedral",
        escape_filter_value(path)?
    ))
}

/// Build filters before and after the LUT mix point. The original branch of a
/// partial-intensity LUT already contains geometry, eq, presets and curves; only
/// the LUT itself is blended, then temporal/post effects run once.
fn video_filter_parts(edit: &EditSpec, out_dur: f64, temporal: bool) -> VideoFilterParts {
    let timing = edit.timing();
    let geometry = edit.geometry();
    let video = edit.video();
    let mut before_lut: Vec<String> = Vec::new();
    // Censor box first, in source coordinates (matches the on-video selection).
    if let Some(censor) = &geometry.censor {
        let rect = censor.rect;
        let color = censor.color.ffmpeg_name();
        before_lut.push(format!(
            "drawbox=x={}:y={}:w={}:h={}:color={color}:t=fill",
            rect.x, rect.y, rect.width, rect.height
        ));
    }
    if let Some(crop) = geometry.crop {
        // Force even dimensions when possible; never turn a one-pixel source
        // edge into FFmpeg's invalid zero-sized crop.
        let width = (crop.width & !1).max(1);
        let height = (crop.height & !1).max(1);
        before_lut.push(format!("crop={width}:{height}:{}:{}", crop.x, crop.y));
    }
    match geometry.rotation {
        Rotation::Clockwise90 => before_lut.push("transpose=1".into()),
        Rotation::Clockwise180 => {
            before_lut.push("transpose=1".into());
            before_lut.push("transpose=1".into());
        }
        Rotation::Clockwise270 => before_lut.push("transpose=2".into()),
        Rotation::None => {}
    }
    if geometry.flip_horizontal {
        before_lut.push("hflip".into());
    }
    if geometry.flip_vertical {
        before_lut.push("vflip".into());
    }
    if let Some(chroma_key) = &video.chroma_key {
        before_lut.push(format!(
            "chromakey=color={}:similarity={:.6}:blend={:.6}",
            chroma_key.ffmpeg_key_color(),
            chroma_key.similarity(),
            chroma_key.blend()
        ));
        if chroma_key.spill_suppression() > 1e-9 {
            before_lut.push(format!(
                "despill=type={}:mix={:.6}",
                chroma_key.ffmpeg_spill_screen(),
                chroma_key.spill_suppression()
            ));
        }
    }
    if video.denoise {
        before_lut.push("hqdn3d".into());
    }
    let eq_changed = video.brightness.abs() > 1e-6
        || (video.contrast - 1.0).abs() > 1e-6
        || (video.saturation - 1.0).abs() > 1e-6;
    if eq_changed {
        before_lut.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            video.brightness, video.contrast, video.saturation
        ));
    }
    if let Some(hsl) = &video.hsl {
        before_lut.extend(hsl_filters(hsl));
    }
    if let Some(color_wheels) = &video.color_wheels {
        if let Some(filter) = color_wheels_filter(color_wheels) {
            before_lut.push(filter);
        }
    }
    // Curves and 3D LUT mixing share an explicit high-bit RGB(A) working
    // format. Besides avoiding 8-bit curve-point quantisation, retaining alpha
    // here prevents still-image grades from silently becoming opaque.
    if video.curves.is_some() || video.lut.is_some() {
        before_lut.push("format=gbrap16le".into());
    }
    if let Some(look) = video.look {
        before_lut.push(look_preset_definition(look).ffmpeg_filter_chain.into());
    }
    if let Some(curves) = &video.curves {
        before_lut.push(curves_filter(curves));
    }
    let mut after_lut = Vec::new();
    // Resize after nonlinear colour work so a grade is independent of export
    // resolution. Spatial finishing effects intentionally run at output size.
    if let Some(scale) = geometry.scale {
        after_lut.push(format!("scale={}:{}", scale.width, scale.height));
    }
    if video.sharpen > 1e-6 {
        after_lut.push(format!("unsharp=5:5:{:.3}:5:5:0.0", video.sharpen));
    }
    if video.vignette {
        after_lut.push("vignette".into());
    }
    if video.grain > 1e-6 {
        after_lut.push(format!("noise=alls={:.0}:allf=t", video.grain));
    }
    // Add letterbox/pillarbox after every visual effect so generated bars stay
    // truly black instead of being lifted, tinted, sharpened, or given grain.
    if let Some(aspect) = geometry.pad_aspect {
        let (tw, th) = (aspect.width, aspect.height);
        after_lut.push(format!(
            "pad=w='ceil(max(iw,ih*{tw}/{th})/2)*2':h='ceil(max(ih,iw*{th}/{tw})/2)*2':x='(ow-iw)/2':y='(oh-ih)/2':color=black"
        ));
    }
    if timing.reverse {
        after_lut.push("reverse".into());
    }
    if temporal {
        let speed = timing.speed;
        if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
            after_lut.push(format!("setpts={:.6}*PTS", 1.0 / speed));
        }
        if timing.fade_in_seconds > 0.0 {
            after_lut.push(format!("fade=t=in:st=0:d={:.3}", timing.fade_in_seconds));
        }
        if timing.fade_out_seconds > 0.0 && out_dur > timing.fade_out_seconds {
            after_lut.push(format!(
                "fade=t=out:st={:.3}:d={:.3}",
                out_dur - timing.fade_out_seconds,
                timing.fade_out_seconds
            ));
        }
    }
    VideoFilterParts {
        before_lut,
        after_lut,
    }
}

fn video_filter_program(
    edit: &EditSpec,
    out_dur: f64,
    temporal: bool,
    lut_path: Option<&Path>,
    input_label: &str,
    output_label: &str,
) -> anyhow::Result<VideoFilterProgram> {
    let mut parts = video_filter_parts(edit, out_dur, temporal);
    let Some(lut) = &edit.video().lut else {
        parts.before_lut.extend(parts.after_lut);
        return Ok(VideoFilterProgram::Linear(parts.before_lut));
    };
    let path = lut_path.ok_or_else(|| {
        anyhow::anyhow!(
            "render plan selects LUT '{}' without a resolved LUT path",
            lut.id()
        )
    })?;
    let lut_filter = lut3d_filter(path)?;
    if lut.intensity() >= 1.0 - 1e-9 {
        parts.before_lut.push(lut_filter);
        parts.before_lut.extend(parts.after_lut);
        return Ok(VideoFilterProgram::Linear(parts.before_lut));
    }

    let mut graph = format!("[{input_label}]");
    if !parts.before_lut.is_empty() {
        graph.push_str(&serialize_filter_chain(MediaKind::Video, &parts.before_lut));
        graph.push(',');
    }
    graph.push_str("split=2[lut_base][lut_input];");
    graph.push_str(&format!("[lut_input]{lut_filter}[lut_applied];"));
    let intensity = lut.intensity();
    graph.push_str(&format!(
        "[lut_base][lut_applied]blend=all_expr='A*(1-{intensity:.6})+B*{intensity:.6}'"
    ));
    if !parts.after_lut.is_empty() {
        graph.push(',');
        graph.push_str(&serialize_filter_chain(MediaKind::Video, &parts.after_lut));
    }
    graph.push_str(&format!("[{output_label}]"));
    Ok(VideoFilterProgram::Complex(graph))
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
    if let Some(eq) = audio.eq.filter(|eq| !eq.is_identity()) {
        for (frequency, gain) in [
            (100, eq.low_gain_db),
            (1_000, eq.mid_gain_db),
            (10_000, eq.high_gain_db),
        ] {
            if gain.abs() > 1e-9 {
                af.push(format!(
                    "equalizer=f={frequency}:t=q:w=0.707:g={gain:.6}:n=1"
                ));
            }
        }
    }
    if (audio.volume - 1.0).abs() > 1e-6 {
        af.push(format!("volume={:.3}", audio.volume));
    }
    if audio.pan.abs() > 1e-9 {
        af.push("aformat=channel_layouts=stereo".into());
        af.push(format!("stereotools=balance_out={:.6}", audio.pan));
    }
    if let Some(compressor) = audio.compressor {
        af.push(format!(
            "acompressor=threshold={:.9}:ratio={:.6}:attack={:.6}:release={:.6}:makeup={:.9}:knee=2.828427:link=average:detection=rms:mix=1",
            db_to_linear(compressor.threshold_db),
            compressor.ratio,
            compressor.attack_ms,
            compressor.release_ms,
            db_to_linear(compressor.makeup_gain_db),
        ));
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
    if let Some(limiter) = audio.limiter {
        af.push(format!(
            "alimiter=limit={:.9}:attack=5:release={:.6}:level=0:latency=1",
            db_to_linear(limiter.ceiling_db),
            limiter.release_ms,
        ));
    }
    af
}

fn db_to_linear(decibels: f64) -> f64 {
    10_f64.powf(decibels / 20.0)
}

/// Append audio options when the compiled output includes an audio stream.
fn push_audio(
    args: &mut Vec<String>,
    edit: &EditSpec,
    out_dur: f64,
    codec: &str,
    include_audio: bool,
) {
    if !include_audio {
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

/// Add a single-output video program. Returns true when an explicit complex
/// video mapping was added, in which case callers must also map optional audio.
fn push_video_program(args: &mut Vec<String>, program: VideoFilterProgram) -> bool {
    match program {
        VideoFilterProgram::Linear(filters) => {
            if !filters.is_empty() {
                args.push("-vf".into());
                args.push(serialize_filter_chain(MediaKind::Video, &filters));
            }
            false
        }
        VideoFilterProgram::Complex(graph) => {
            args.push("-filter_complex".into());
            args.push(graph);
            args.push("-map".into());
            args.push("[vout]".into());
            true
        }
    }
}

fn map_optional_audio_for_complex_video(
    args: &mut Vec<String>,
    include_audio: bool,
    complex: bool,
) {
    if complex && include_audio {
        args.push("-map".into());
        args.push("0:a?".into());
    }
}

/// Build the FFmpeg argument list for an immutable edit plan.
///
/// Trim is applied as an *input* option (`-ss` + `-t`) so it happens before the
/// filter graph; geometry/colour/speed/fade then operate on the trimmed stream.
/// The output container/codecs depend on `edit.format` (mp4/webm/gif/png/mp3).
pub fn build_ffmpeg_args(input: &Path, destination: &Path, plan: &EditPlan) -> Vec<String> {
    compile_ffmpeg_command(input, destination, plan, None)
        .expect("build_ffmpeg_args requires all selected render resources")
        .arguments
}

fn compile_ffmpeg_command(
    input: &Path,
    destination: &Path,
    plan: &EditPlan,
    lut_path: Option<&Path>,
) -> anyhow::Result<CompiledExportCommand> {
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
        return Ok(CompiledExportCommand {
            arguments: build_concat_args(input, destination, plan, segs, out_dur, lut_path)?,
            expected_duration_seconds: out_dur,
            read_only_files: lut_path.into_iter().map(Path::to_path_buf).collect(),
        });
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
            let program = video_filter_program(edit, out_dur, false, lut_path, "0:v", "vout")?;
            push_video_program(&mut args, program);
            args.push("-frames:v".into());
            args.push("1".into());
            args.push("-an".into());
        }
        OutputFormat::Gif => {
            // Generate a per-clip palette for a good-looking gif (single pass
            // via split + palettegen/paletteuse).
            let fps = output.fps().unwrap_or(12.0);
            match video_filter_program(edit, out_dur, true, lut_path, "0:v", "graded")? {
                VideoFilterProgram::Linear(mut parts) => {
                    parts.push(format!("fps={fps:.3}"));
                    let graph = format!(
                        "{},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
                        serialize_filter_chain(MediaKind::Video, &parts)
                    );
                    args.push("-vf".into());
                    args.push(graph);
                }
                VideoFilterProgram::Complex(mut graph) => {
                    graph.push_str(&format!(
                        ";[graded]fps={fps:.3},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle[vout]"
                    ));
                    args.push("-filter_complex".into());
                    args.push(graph);
                    args.push("-map".into());
                    args.push("[vout]".into());
                }
            }
            args.push("-an".into());
        }
        OutputFormat::Webm => {
            let program = video_filter_program(edit, out_dur, true, lut_path, "0:v", "vout")?;
            let complex = push_video_program(&mut args, program);
            let include_audio = output.audio_codec.is_some();
            map_optional_audio_for_complex_video(&mut args, include_audio, complex);
            push_audio(&mut args, edit, out_dur, "libopus", include_audio);
            push_video_codec(&mut args, output);
        }
        OutputFormat::Av1 => {
            // Modern, compact codec in an mp4 container (needs libsvtav1).
            let program = video_filter_program(edit, out_dur, true, lut_path, "0:v", "vout")?;
            let complex = push_video_program(&mut args, program);
            let include_audio = output.audio_codec.is_some();
            map_optional_audio_for_complex_video(&mut args, include_audio, complex);
            push_audio(&mut args, edit, out_dur, "aac", include_audio);
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
            let program = video_filter_program(edit, out_dur, true, lut_path, "0:v", "vout")?;
            let complex = push_video_program(&mut args, program);
            let include_audio = output.audio_codec.is_some();
            map_optional_audio_for_complex_video(&mut args, include_audio, complex);
            if !include_audio {
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
            let program = video_filter_program(edit, out_dur, true, lut_path, "0:v", "vout")?;
            let complex = push_video_program(&mut args, program);
            let include_audio = output.audio_codec.is_some();
            map_optional_audio_for_complex_video(&mut args, include_audio, complex);
            push_audio(&mut args, edit, out_dur, "aac", include_audio);
            push_video_codec(&mut args, output);
        }
    }

    args.push(destination.to_string_lossy().into_owned());
    Ok(CompiledExportCommand {
        arguments: args,
        expected_duration_seconds: out_dur,
        read_only_files: lut_path.into_iter().map(Path::to_path_buf).collect(),
    })
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
    compile_ffmpeg_command_with_budget(input, output, plan, budget, parallel_jobs, None)
        .expect("build_ffmpeg_args_with_budget requires all selected render resources")
        .arguments
}

fn compile_ffmpeg_command_with_budget(
    input: &Path,
    output: &Path,
    plan: &EditPlan,
    budget: &EncodeBudget,
    parallel_jobs: usize,
    lut_path: Option<&Path>,
) -> anyhow::Result<CompiledExportCommand> {
    let mut command = compile_ffmpeg_command(input, output, plan, lut_path)?;
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
    Ok(command)
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
    lut_path: Option<&Path>,
) -> anyhow::Result<Vec<String>> {
    let edit = &plan.edit;
    let output = &plan.output;
    let format = output.format;
    let include_audio = output.audio_codec.is_some();
    let n = segments.len();
    let mut graph = String::new();
    for (i, s) in segments.iter().enumerate() {
        graph.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{i}];",
            s.start_seconds(),
            s.end_seconds()
        ));
        if include_audio {
            graph.push_str(&format!(
                "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}];",
                s.start_seconds(),
                s.end_seconds()
            ));
        }
    }
    for i in 0..n {
        graph.push_str(&format!("[v{i}]"));
        if include_audio {
            graph.push_str(&format!("[a{i}]"));
        }
    }
    if include_audio {
        graph.push_str(&format!("concat=n={n}:v=1:a=1[cv][ca]"));
    } else {
        graph.push_str(&format!("concat=n={n}:v=1:a=0[cv]"));
    }

    // Effects apply to the concatenated stream.
    let vmap = match video_filter_program(edit, out_dur, true, lut_path, "cv", "vout")? {
        VideoFilterProgram::Linear(vf) if vf.is_empty() => "[cv]".to_string(),
        VideoFilterProgram::Linear(vf) => {
            graph.push_str(&format!(
                ";[cv]{}[vout]",
                serialize_filter_chain(MediaKind::Video, &vf)
            ));
            "[vout]".to_string()
        }
        VideoFilterProgram::Complex(lut_graph) => {
            graph.push(';');
            graph.push_str(&lut_graph);
            "[vout]".to_string()
        }
    };
    let amap = if include_audio {
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
    } else {
        None
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
    Ok(args)
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
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::config::encode_budget::TileLayout;
    use crate::domain::artifact_graph::Fingerprint;
    use crate::model::EditRequest;
    use crate::services::render::{
        ExportExecutionProfile, RenderExecution, RenderResources, SourceMediaMetadata,
    };
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

    fn plan_without_audio(value: serde_json::Value, duration_seconds: f64) -> EditPlan {
        let request: EditRequest = serde_json::from_value(value).expect("valid EditRequest");
        EditPlan::compile(
            Fingerprint::digest(b"video-only-source"),
            request,
            SourceMediaMetadata::new_with_audio(1920, 1080, duration_seconds, false).unwrap(),
        )
        .expect("valid video-only EditPlan")
    }

    fn args_for(v: serde_json::Value, dur: f64) -> Vec<String> {
        let input = Path::new("/in.mp4");
        let output = Path::new("/out.mp4");
        build_ffmpeg_args(input, output, &plan_for_duration(v, dur))
    }

    fn command_with_lut(
        value: serde_json::Value,
        duration: f64,
        lut_path: &Path,
    ) -> CompiledExportCommand {
        compile_ffmpeg_command(
            Path::new("/in.mp4"),
            Path::new("/out.mp4"),
            &plan_for_duration(value, duration),
            Some(lut_path),
        )
        .unwrap()
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
        assert!(order("hflip") < order("hue=s=0"));
        assert!(order("hue=s=0") < order("scale"));
        assert!(chain.contains("fade=t=out:st=9.000:d=1.000"), "{chain}");
    }

    #[test]
    fn chroma_key_compiles_before_colour_grade_with_optional_despill() {
        let args = args_for(
            json!({
                "videoId": "x",
                "chromaKey": {
                    "keyColor": "#00ff00",
                    "similarity": 0.18,
                    "blend": 0.07,
                    "spillSuppression": 0.65
                },
                "brightness": 0.1
            }),
            10.0,
        );
        let chain = vf(&args);
        let key = "chromakey=color=0x00ff00:similarity=0.180000:blend=0.070000";
        let despill = "despill=type=green:mix=0.650000";

        assert!(chain.contains(key), "{chain}");
        assert!(chain.contains(despill), "{chain}");
        assert!(
            chain.find(key).unwrap() < chain.find(despill).unwrap(),
            "{chain}"
        );
        assert!(
            chain.find(despill).unwrap() < chain.find("eq=").unwrap(),
            "{chain}"
        );
    }

    #[test]
    fn zero_spill_suppression_omits_despill_filter() {
        let args = args_for(
            json!({
                "videoId": "x",
                "chromaKey": {
                    "keyColor": "#0033ff",
                    "similarity": 0.2,
                    "blend": 0.0,
                    "spillSuppression": 0.0
                }
            }),
            10.0,
        );
        let chain = vf(&args);

        assert!(chain.contains("chromakey=color=0x0033ff:similarity=0.200000:blend=0.000000"));
        assert!(!chain.contains("despill="), "{chain}");
    }

    #[test]
    fn selective_hsl_and_color_wheels_compile_in_declared_tonal_order() {
        let args = args_for(
            json!({
                "videoId": "x",
                "brightness": 0.1,
                "hsl": {
                    "red": {"hue": 15.0, "saturation": -0.2, "lightness": 0.1},
                    "blue": {"hue": -20.0, "saturation": 0.3, "lightness": -0.15}
                },
                "colorWheels": {
                    "shadows": {"red": 0.2, "green": -0.1, "blue": 0.05},
                    "midtones": {"blue": 0.12},
                    "highlights": {"red": -0.08, "green": 0.04},
                    "preserveLuminosity": false
                },
                "curves": {
                    "master": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}]
                }
            }),
            10.0,
        );
        let chain = vf(&args);
        let red = "huesaturation=hue=15.000000:saturation=-0.200000:intensity=0.100000:colors=r:strength=1";
        let blue = "huesaturation=hue=-20.000000:saturation=0.300000:intensity=-0.150000:colors=b:strength=1";
        let wheels = "colorbalance=rs=0.200000:gs=-0.100000:bs=0.050000:rm=0.000000:gm=0.000000:bm=0.120000:rh=-0.080000:gh=0.040000:bh=0.000000:pl=0";

        assert!(chain.contains(red), "{chain}");
        assert!(chain.contains(blue), "{chain}");
        assert!(chain.contains(wheels), "{chain}");
        assert!(
            chain.find("eq=").unwrap() < chain.find(red).unwrap(),
            "{chain}"
        );
        assert!(
            chain.find(red).unwrap() < chain.find(blue).unwrap(),
            "{chain}"
        );
        assert!(
            chain.find(blue).unwrap() < chain.find(wheels).unwrap(),
            "{chain}"
        );
        assert!(
            chain.find(wheels).unwrap() < chain.find("curves=").unwrap(),
            "{chain}"
        );
        assert_eq!(chain.matches("huesaturation=").count(), 2, "{chain}");
    }

    #[test]
    fn identity_manual_color_objects_emit_no_filters() {
        let args = args_for(
            json!({
                "videoId": "x",
                "hsl": {},
                "colorWheels": {}
            }),
            10.0,
        );
        assert!(!args.iter().any(|argument| argument == "-vf"));
        assert!(!args.iter().any(|argument| argument == "-filter_complex"));
    }

    #[test]
    fn custom_curves_emit_normalized_channels_before_post_effects() {
        let args = args_for(
            json!({
                "videoId": "x",
                "curves": {
                    "master": [{"x": 0.0, "y": 0.05}, {"x": 1.0, "y": 0.95}],
                    "red": [{"x": 0.0, "y": 0.0}, {"x": 0.500000123456789, "y": 0.6}, {"x": 1.0, "y": 1.0}],
                    "green": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                    "blue": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 0.9}]
                },
                "sharpen": 1.0
            }),
            10.0,
        );
        let chain = vf(&args);
        assert!(chain.contains("master='0/0.05 1/0.95'"), "{chain}");
        assert!(
            chain.contains("red='0/0 0.500000123456789/0.6 1/1'"),
            "{chain}"
        );
        assert!(chain.contains("green='0/0 1/1'"), "{chain}");
        assert!(chain.contains("blue='0/0.1 1/0.9'"), "{chain}");
        assert!(chain.contains("interp=pchip"), "{chain}");
        assert!(chain.find("format=gbrap16le").unwrap() < chain.find("curves=").unwrap());
        assert!(chain.find("curves=").unwrap() < chain.find("unsharp=").unwrap());
    }

    #[test]
    fn full_intensity_lut_is_linear_and_declared_read_only() {
        let path = Path::new("/private/luts/look.cube");
        let command = command_with_lut(
            json!({"videoId": "x", "lut": {"id": "look-1", "intensity": 1.0}}),
            10.0,
            path,
        );
        let chain = vf(&command.arguments);
        assert!(chain.starts_with("format=gbrap16le,lut3d="), "{chain}");
        assert!(chain.contains("lut3d=file='/private/luts/look.cube':interp=tetrahedral"));
        assert!(!chain.contains("split=2"));
        assert_eq!(command.read_only_files, [path.to_path_buf()]);
    }

    #[test]
    fn partial_lut_uses_split_lut3d_blend_and_preserves_audio_mapping() {
        let command = command_with_lut(
            json!({
                "videoId": "x",
                "brightness": 0.1,
                "lut": {"id": "look-1", "intensity": 0.35},
                "scale": {"w": 640, "h": 360},
                "sharpen": 1.0,
                "vignette": true,
                "grain": 5.0,
                "pad": "4:3",
                "fadeOut": 1.0
            }),
            10.0,
            Path::new("/private/luts/look.cube"),
        );
        let graph = filter_complex(&command.arguments);
        assert!(graph.starts_with("[0:v]eq="), "{graph}");
        let working_format = graph.find("format=gbrap16le").unwrap();
        let split = graph.find("split=2[lut_base][lut_input]").unwrap();
        assert!(working_format < split, "{graph}");
        assert!(graph.contains("split=2[lut_base][lut_input]"), "{graph}");
        assert!(graph.contains("[lut_input]lut3d=file="), "{graph}");
        assert!(
            graph.contains("blend=all_expr='A*(1-0.350000)+B*0.350000'"),
            "{graph}"
        );
        let blend = graph.find("blend=").unwrap();
        let scale = graph.find("scale=640:360").unwrap();
        let sharpen = graph.find("unsharp=").unwrap();
        let vignette = graph.find("vignette").unwrap();
        let grain = graph.find("noise=").unwrap();
        let pad = graph.find("pad=").unwrap();
        let fade = graph.find("fade=t=out").unwrap();
        assert!(blend < scale, "{graph}");
        assert!(
            scale < sharpen && sharpen < vignette && vignette < grain,
            "{graph}"
        );
        assert!(grain < pad && pad < fade, "{graph}");
        assert!(command
            .arguments
            .windows(2)
            .any(|pair| pair == ["-map", "[vout]"]));
        assert!(command
            .arguments
            .windows(2)
            .any(|pair| pair == ["-map", "0:a?"]));
    }

    #[test]
    fn partial_lut_is_embedded_after_concat() {
        let command = command_with_lut(
            json!({
                "videoId": "x",
                "segments": [{"start": 0.0, "end": 1.0}, {"start": 2.0, "end": 3.0}],
                "lut": {"id": "look-1", "intensity": 0.5}
            }),
            5.0,
            Path::new("/private/luts/look.cube"),
        );
        let graph = filter_complex(&command.arguments);
        let concat = graph.find("concat=n=2:v=1:a=1[cv][ca]").unwrap();
        let split = graph
            .find("[cv]format=gbrap16le,split=2[lut_base][lut_input]")
            .unwrap();
        assert!(concat < split, "{graph}");
        assert!(
            graph.contains("blend=all_expr='A*(1-0.500000)+B*0.500000'[vout]"),
            "{graph}"
        );
    }

    #[test]
    fn full_and_partial_lut_curves_compile_for_every_video_output_format() {
        let path = Path::new("/private/luts/look.cube");
        for format in ["mp4", "webm", "av1", "prores", "gif", "png", "jpg"] {
            for intensity in [1.0, 0.5] {
                let command = command_with_lut(
                    json!({
                        "videoId": "x",
                        "format": format,
                        "lut": {"id": "look-1", "intensity": intensity},
                        "curves": {
                            "master": [{"x": 0.0, "y": 0.05}, {"x": 1.0, "y": 0.95}]
                        }
                    }),
                    10.0,
                    path,
                );
                let graph = command
                    .arguments
                    .windows(2)
                    .find(|pair| pair[0] == "-vf" || pair[0] == "-filter_complex")
                    .map(|pair| pair[1].as_str())
                    .unwrap_or_default();
                assert!(graph.contains("curves="), "{format} {intensity}: {graph}");
                assert!(graph.contains("lut3d="), "{format} {intensity}: {graph}");
                assert_eq!(command.read_only_files, [path.to_path_buf()]);
                if intensity < 1.0 {
                    assert!(graph.contains("split=2"), "{format}: {graph}");
                    assert!(command
                        .arguments
                        .iter()
                        .any(|value| value == "-filter_complex"));
                }
                if intensity < 1.0 && matches!(format, "mp4" | "webm" | "av1" | "prores") {
                    assert!(command
                        .arguments
                        .windows(2)
                        .any(|pair| pair == ["-map", "0:a?"]));
                }
                if matches!(format, "gif" | "png" | "jpg") {
                    assert!(command.arguments.iter().any(|value| value == "-an"));
                }
            }
        }
    }

    #[test]
    fn export_compiler_requires_and_accepts_explicit_lut_resource() {
        let plan = Arc::new(plan_for_duration(
            json!({"videoId": "x", "lut": {"id": "look-1", "intensity": 1.0}}),
            10.0,
        ));
        let profile = ExportExecutionProfile {
            encode_budget: EncodeBudget {
                threads: 2,
                tiles: TileLayout {
                    columns: 1,
                    rows: 1,
                },
                speed: 6,
                memory_mib: 512,
            },
            verify_checksums: true,
        };
        let missing = RenderExecution::new(plan.clone(), profile.clone());
        assert!(FfmpegExportCompiler
            .compile(ExportCompileRequest {
                input: Path::new("/in.mp4"),
                destination: Path::new("/out.mp4"),
                parallel_jobs: 1,
                execution: &missing,
            })
            .is_err());

        let resolved = RenderExecution::new_with_resources(
            plan,
            profile,
            RenderResources::with_lut_path("/private/luts/look.cube"),
        );
        let command = FfmpegExportCompiler
            .compile(ExportCompileRequest {
                input: Path::new("/in.mp4"),
                destination: Path::new("/out.mp4"),
                parallel_jobs: 1,
                execution: &resolved,
            })
            .unwrap();
        assert_eq!(
            command.read_only_files,
            [PathBuf::from("/private/luts/look.cube")]
        );
    }

    #[test]
    fn mp3_bypasses_video_grading_without_a_lut_resource() {
        let plan = Arc::new(plan_for_duration(
            json!({
                "videoId": "x",
                "format": "mp3",
                "lut": {"id": "look-1", "intensity": 1.0},
                "curves": {
                    "master": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 0.9}]
                }
            }),
            10.0,
        ));
        let execution = RenderExecution::new(
            plan,
            ExportExecutionProfile {
                encode_budget: EncodeBudget {
                    threads: 2,
                    tiles: TileLayout {
                        columns: 1,
                        rows: 1,
                    },
                    speed: 6,
                    memory_mib: 512,
                },
                verify_checksums: true,
            },
        );
        let command = FfmpegExportCompiler
            .compile(ExportCompileRequest {
                input: Path::new("/in.mp4"),
                destination: Path::new("/out.mp3"),
                parallel_jobs: 1,
                execution: &execution,
            })
            .unwrap();

        assert!(command.read_only_files.is_empty());
        assert!(command.arguments.contains(&"-vn".to_string()));
        assert!(!command
            .arguments
            .iter()
            .any(|argument| argument.contains("curves=") || argument.contains("lut3d=")));
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
    fn deterministic_audio_dsp_compiles_in_stable_order() {
        let args = args_for(
            json!({
                "videoId": "x",
                "highpass": true,
                "audioEq": {"lowGainDb": 2.0, "midGainDb": -1.5, "highGainDb": 0.0},
                "volume": 1.25,
                "pan": -0.4,
                "compressor": {
                    "thresholdDb": -18.0,
                    "ratio": 4.0,
                    "attackMs": 10.0,
                    "releaseMs": 180.0,
                    "makeupGainDb": 3.0
                },
                "normalizeAudio": true,
                "limiter": {"ceilingDb": -1.0, "releaseMs": 80.0}
            }),
            10.0,
        );
        let chain = af(&args).expect("has -af");
        let low = "equalizer=f=100:t=q:w=0.707:g=2.000000:n=1";
        let mid = "equalizer=f=1000:t=q:w=0.707:g=-1.500000:n=1";
        let compressor = "acompressor=threshold=0.125892541:ratio=4.000000:attack=10.000000:release=180.000000:makeup=1.412537545";
        let limiter = "alimiter=limit=0.891250938:attack=5:release=80.000000:level=0:latency=1";

        for expected in [
            "highpass=f=100",
            low,
            mid,
            "volume=1.250",
            "aformat=channel_layouts=stereo",
            "stereotools=balance_out=-0.400000",
            compressor,
            "loudnorm=I=-14:TP=-1.5:LRA=11",
            limiter,
        ] {
            assert!(chain.contains(expected), "missing {expected} in {chain}");
        }
        let position = |value: &str| chain.find(value).unwrap();
        assert!(position("highpass") < position(low));
        assert!(position(mid) < position("volume="));
        assert!(position("stereotools") < position("acompressor"));
        assert!(position("acompressor") < position("loudnorm"));
        assert!(position("loudnorm") < position("alimiter"));
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
        let presets = [
            ("grayscale", "hue=s=0"),
            (
                "sepia",
                "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131",
            ),
            ("warm", "colorbalance=rs=0.2:gs=0.05:bs=-0.2"),
            ("cold", "colorbalance=rs=-0.2:gs=0:bs=0.2"),
            (
                "teal-orange",
                "colorbalance=rs=-0.15:bs=0.15:rm=0.1:bm=-0.05:rh=0.15:bh=-0.15",
            ),
            ("faded", "curves=all='0/0.08 1/0.92'"),
            ("noir", "hue=s=0,eq=contrast=1.4"),
            (
                "vintage",
                "curves=all='0/0.06 1/0.95',colorbalance=rs=0.15:gs=0.05:bs=-0.1",
            ),
        ];

        for (id, expected_chain) in presets {
            let args = args_for(json!({ "videoId": "x", "filter": id }), 10.0);
            assert_eq!(vf(&args), expected_chain, "preset {id}");
        }
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
    fn segments_concat_preserves_out_of_order_duplicates_and_overlaps() {
        let plan = plan(json!({
            "videoId": "x",
            "segments": [
                { "start": 6.0, "end": 8.0 },
                { "start": 1.0, "end": 4.0 },
                { "start": 1.0, "end": 4.0 },
                { "start": 3.0, "end": 7.0 }
            ]
        }));
        let args = build_ffmpeg_args(Path::new("/in.mp4"), Path::new("/out.mp4"), &plan);
        let graph = filter_complex(&args);

        let ordered_video_segments = [
            "[0:v]trim=start=6.000:end=8.000,setpts=PTS-STARTPTS[v0]",
            "[0:v]trim=start=1.000:end=4.000,setpts=PTS-STARTPTS[v1]",
            "[0:v]trim=start=1.000:end=4.000,setpts=PTS-STARTPTS[v2]",
            "[0:v]trim=start=3.000:end=7.000,setpts=PTS-STARTPTS[v3]",
        ];
        let positions: Vec<_> = ordered_video_segments
            .iter()
            .map(|segment| {
                graph
                    .find(segment)
                    .unwrap_or_else(|| panic!("missing {segment}: {graph}"))
            })
            .collect();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{graph}"
        );
        assert!(
            graph.contains("[v0][a0][v1][a1][v2][a2][v3][a3]concat=n=4:v=1:a=1[cv][ca]"),
            "{graph}"
        );
        assert_eq!(expected_output_secs(&plan.edit, 10.0), 12.0);
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
    fn segments_from_video_only_sources_never_reference_audio() {
        for format in ["mp4", "av1", "prores"] {
            let plan = plan_without_audio(
                json!({
                    "videoId": "x",
                    "format": format,
                    "normalizeAudio": true,
                    "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
                }),
                5.0,
            );
            assert!(plan.output.audio_codec.is_none());
            let args = build_ffmpeg_args(Path::new("/in.mp4"), Path::new("/out.mp4"), &plan);
            let graph = filter_complex(&args);
            assert!(
                graph.contains("concat=n=2:v=1:a=0[cv]"),
                "{format}: {graph}"
            );
            assert!(!graph.contains("[0:a]"), "{format}: {graph}");
            assert!(!graph.contains("atrim"), "{format}: {graph}");
            assert_eq!(args.iter().filter(|arg| *arg == "-map").count(), 1);
            assert!(!args.contains(&"-c:a".to_string()));
        }
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
    fn segments_are_rejected_for_unsupported_outputs() {
        for format in ["gif", "png", "jpg", "mp3"] {
            let request: EditRequest = serde_json::from_value(json!({
                "videoId": "x",
                "format": format,
                "segments": [{ "start": 0.0, "end": 1.0 }]
            }))
            .unwrap();
            assert!(
                EditPlan::compile(
                    Fingerprint::digest(b"source"),
                    request,
                    SourceMediaMetadata::new(1920, 1080, 5.0).unwrap(),
                )
                .is_err(),
                "{format} unexpectedly accepted timeline segments"
            );
        }
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
