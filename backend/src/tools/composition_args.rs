//! Pure FFmpeg compiler for the first multi-source composition slice.
//!
//! The supported shape has one bottom, gapless video track plus animated video,
//! image, and text layers above it, exact handle-backed video transitions,
//! Rectangle/Ellipse alpha masks, zero or more audio tracks, and MP4/H.264/AAC
//! output. Active slow-motion video clips may select bounded deterministic
//! optical flow; bounded classical source stabilization uses FFmpeg deshake.
//! Richer domain features stay representable without being silently compiled
//! incorrectly.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use crate::domain::composition::{
    AnimatableValue, AudioClip, BlendMode, CanvasSpec, ClipPlacement, ClipTransition, Composition,
    CompositionClipId, CompositionTrack, FrameInterpolation, ImageClip, MaskShape, PlaybackMode,
    SourceId, SourceKind, SpeedRampAudioPolicy, SpeedRampInterpolation, StabilizationSpec,
    TextClip, TransformSpec, TransitionKind, VideoClip, VideoEffect, DEFAULT_TIME_BASE,
    MAX_ACTIVE_SPEED_RAMP_SEGMENTS, MAX_OPTICAL_FLOW_CLIP_TICKS, MAX_OPTICAL_FLOW_EDGE,
    MAX_OPTICAL_FLOW_PIXELS, MAX_OPTICAL_FLOW_PIXEL_FRAMES, MAX_REVERSE_CLIP_TICKS,
    MAX_REVERSE_EDGE, MAX_REVERSE_PIXELS, MAX_REVERSE_PIXEL_FRAMES, MAX_STABILIZATION_CLIP_TICKS,
    MAX_STABILIZATION_EDGE, MAX_STABILIZATION_PIXELS, MAX_STABILIZATION_PIXEL_FRAMES,
};
use crate::domain::keyframes::FfmpegKeyframeAdapter;
use crate::ports::{
    CompiledExportCommand, CompositionAv1Encoder, CompositionExportCommandCompiler,
    CompositionExportCompileRequest, CompositionExportProfile, CompositionExportSpec,
    CompositionMp4Codec, CompositionTextResource, CompositionWebmCodec,
};

const AUDIO_SAMPLE_RATE: u64 = 48_000;
const DEFAULT_AUDIO_BITRATE: &str = "192k";
const MAX_VISUAL_FILTER_DIMENSION: f64 = 8_192.0;
const MAX_VISUAL_POSITION: f64 = 32_768.0;
const MAX_ACTIVE_KEYFRAME_POINTS: usize = 2_048;
const MAX_ACTIVE_AUDIO_KEYFRAME_POINTS: usize = 2_048;
const MAX_AUDIO_GAIN: f64 = 16.0;
const MAX_FILTER_EXPRESSION_BYTES: usize = 65_536;
const MAX_FILTER_GRAPH_BYTES: usize = 2 * 1_024 * 1_024;

#[derive(Debug, Clone, Copy, Default)]
pub struct FfmpegCompositionExportCompiler;

impl CompositionExportCommandCompiler for FfmpegCompositionExportCompiler {
    fn compile(
        &self,
        request: CompositionExportCompileRequest<'_>,
    ) -> Result<CompiledExportCommand> {
        build_composition_ffmpeg_command_with_text_resources_for_output(
            request.inputs,
            request.text_resources,
            request.destination,
            request.composition,
            request.output,
            request.parallel_jobs,
        )
    }
}

/// Compile a composition into an argv vector without touching the filesystem.
pub fn build_composition_ffmpeg_args(
    inputs: &BTreeMap<SourceId, PathBuf>,
    destination: &Path,
    composition: &Composition,
    quality: u32,
    parallel_jobs: usize,
) -> Result<Vec<String>> {
    Ok(
        build_composition_ffmpeg_command(inputs, destination, composition, quality, parallel_jobs)?
            .arguments,
    )
}

/// Compile a composition into the process-neutral command envelope used by
/// the existing FFmpeg runner.
pub fn build_composition_ffmpeg_command(
    inputs: &BTreeMap<SourceId, PathBuf>,
    destination: &Path,
    composition: &Composition,
    quality: u32,
    parallel_jobs: usize,
) -> Result<CompiledExportCommand> {
    build_composition_ffmpeg_command_with_text_resources(
        inputs,
        &BTreeMap::new(),
        destination,
        composition,
        quality,
        parallel_jobs,
    )
}

pub fn build_composition_ffmpeg_command_with_text_resources(
    inputs: &BTreeMap<SourceId, PathBuf>,
    text_resources: &BTreeMap<CompositionClipId, CompositionTextResource>,
    destination: &Path,
    composition: &Composition,
    quality: u32,
    parallel_jobs: usize,
) -> Result<CompiledExportCommand> {
    build_composition_ffmpeg_command_with_text_resources_for_output(
        inputs,
        text_resources,
        destination,
        composition,
        CompositionExportSpec {
            profile: CompositionExportProfile::default(),
            video_quality: quality,
            av1_encoder: None,
        },
        parallel_jobs,
    )
}

/// Compile a composition using an already capability-resolved delivery profile.
pub fn build_composition_ffmpeg_command_for_output(
    inputs: &BTreeMap<SourceId, PathBuf>,
    destination: &Path,
    composition: &Composition,
    output: CompositionExportSpec,
    parallel_jobs: usize,
) -> Result<CompiledExportCommand> {
    build_composition_ffmpeg_command_with_text_resources_for_output(
        inputs,
        &BTreeMap::new(),
        destination,
        composition,
        output,
        parallel_jobs,
    )
}

pub fn build_composition_ffmpeg_command_with_text_resources_for_output(
    inputs: &BTreeMap<SourceId, PathBuf>,
    text_resources: &BTreeMap<CompositionClipId, CompositionTextResource>,
    destination: &Path,
    composition: &Composition,
    output: CompositionExportSpec,
    parallel_jobs: usize,
) -> Result<CompiledExportCommand> {
    if parallel_jobs == 0 {
        bail!("composition export requires at least one parallel job");
    }
    validate_export_spec(output)?;
    let actual_extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !actual_extension.eq_ignore_ascii_case(output.profile.extension()) {
        bail!(
            "composition {:?} output requires .{} destination",
            output.profile,
            output.profile.extension()
        );
    }

    composition.validate().context("invalid composition")?;
    let visual = visual_slice(composition)?;
    let video_duration_ticks = validate_gapless_video(&visual.primary)?;
    let transition_plan = transition_plan(composition, &visual.primary, visual.transitions)?;
    let compiled_overlays =
        compile_visual_overlays(composition, &visual.overlays, video_duration_ticks)?;
    validate_optical_flow_workload(composition, &visual, &transition_plan)?;
    validate_reverse_workload(composition, &visual)?;
    validate_stabilization_workload(composition, &visual, &transition_plan)?;
    validate_text_resources(&visual.overlays, text_resources)?;
    let audio_clips = active_audio_clips(composition);
    validate_speed_ramp_budget(&visual, &audio_clips)?;
    let mut audio_keyframe_points = 0_usize;
    let primary_audio = compile_primary_source_audio(
        composition,
        &visual.primary,
        visual.primary_muted,
        &mut audio_keyframe_points,
    )?;
    let compiled_audio_clips = compile_audio_slice(
        &audio_clips,
        video_duration_ticks,
        composition.time_base,
        &mut audio_keyframe_points,
    )?;
    if audio_keyframe_points > MAX_ACTIVE_AUDIO_KEYFRAME_POINTS {
        bail!("composition has too many active audio keyframes");
    }

    let mut video_uses = BTreeMap::<SourceId, usize>::new();
    let mut audio_uses = BTreeMap::<SourceId, usize>::new();
    for (clip, source_audio) in visual.primary.iter().zip(&primary_audio) {
        increment(&mut video_uses, &clip.source_id);
        if source_audio.is_some() {
            increment(&mut audio_uses, &clip.source_id);
        }
    }
    for clip in &visual.overlays {
        if let Some(source_id) = clip.source_id() {
            increment(&mut video_uses, source_id);
        }
    }
    for compiled in &compiled_audio_clips {
        increment(&mut audio_uses, &compiled.clip.source_id);
    }

    let used_source_ids: BTreeSet<_> = video_uses
        .keys()
        .chain(audio_uses.keys())
        .cloned()
        .collect();
    let mut input_indexes = BTreeMap::new();
    let mut arguments = vec!["-y".to_owned()];
    for (index, source_id) in used_source_ids.iter().enumerate() {
        let path = inputs.get(source_id).ok_or_else(|| {
            anyhow!(
                "missing resolved input for composition source {}",
                source_id.as_str()
            )
        })?;
        if path.as_os_str().is_empty() {
            bail!(
                "resolved input for composition source {} is empty",
                source_id.as_str()
            );
        }
        let source = composition
            .sources
            .get(source_id)
            .expect("composition validation checked the source");
        if source.kind == SourceKind::Image {
            arguments.extend([
                "-loop".to_owned(),
                "1".to_owned(),
                "-framerate".to_owned(),
                format!("{}/1000", composition.canvas.fps_milli),
            ]);
        }
        arguments.push("-i".to_owned());
        arguments.push(path.to_string_lossy().into_owned());
        input_indexes.insert(source_id.clone(), index);
    }

    let mut graph = Vec::new();
    let mut pads = build_source_pads(&input_indexes, &video_uses, &audio_uses, &mut graph);
    let time_base = composition.time_base;
    let canvas = composition.canvas;
    let background = ffmpeg_rgb(
        canvas.background.red,
        canvas.background.green,
        canvas.background.blue,
    );

    let mut concat_inputs = String::new();
    let mut concat_audio_inputs = String::new();
    for (clip_index, clip) in visual.primary.iter().enumerate() {
        let duration_ticks = clip.placement.timeline_duration_ticks()?;
        let handles = transition_plan.handles[clip_index];
        let source_start_tick = clip.placement.source_in_tick as f64
            - handles.source_head_ticks as f64 * clip.placement.speed;
        let source_end_tick = clip.placement.source_out_tick as f64
            + handles.source_tail_ticks as f64 * clip.placement.speed;
        let render_duration_ticks = duration_ticks
            .checked_add(handles.source_head_ticks)
            .and_then(|value| value.checked_add(handles.source_tail_ticks))
            .ok_or_else(|| anyhow!("primary video clip duration overflow"))?;
        let video_pad = pads.take_video(&clip.source_id)?;
        let video_label = format!("video_clip_{clip_index}");
        let mut video_filters = source_video_filters(
            clip,
            source_start_tick,
            source_end_tick,
            canvas.fps_milli,
            time_base,
        )?;
        video_filters.extend([
            format!(
                "scale={}:{}:force_original_aspect_ratio=decrease",
                canvas.width, canvas.height
            ),
            format!(
                "pad={}:{}:(ow-iw)/2:(oh-ih)/2:color={background}",
                canvas.width, canvas.height
            ),
            "setsar=1".to_owned(),
        ]);
        video_filters.extend(playback_frame_filters(
            clip,
            canvas.fps_milli,
            render_duration_ticks,
            time_base,
        ));
        video_filters.extend([
            "format=pix_fmts=yuv420p".to_owned(),
            "settb=AVTB".to_owned(),
        ]);
        graph.push(format!(
            "[{video_pad}]{}[{video_label}]",
            video_filters.join(",")
        ));

        let audio_label = format!("source_audio_{clip_index}");
        if let Some(automation) = &primary_audio[clip_index] {
            let audio_pad = pads.take_audio(&clip.source_id)?;
            graph.push(media_audio_filter(
                &audio_pad,
                &clip.placement,
                duration_ticks,
                time_base,
                &audio_label,
                clip.playback_mode == PlaybackMode::Reverse,
                Some(AudioFilterOptions {
                    automation,
                    timeline_start_tick: None,
                    fade_in_ticks: 0,
                    fade_out_ticks: 0,
                }),
            )?);
        } else {
            graph.push(format!(
                "anullsrc=channel_layout=stereo:sample_rate={AUDIO_SAMPLE_RATE},\
                 atrim=duration={},asetpts=PTS-STARTPTS[{audio_label}]",
                seconds(duration_ticks, time_base),
            ));
        }
        concat_inputs.push_str(&format!("[{video_label}][{audio_label}]"));
        concat_audio_inputs.push_str(&format!("[{audio_label}]"));
    }

    let concat_video_label = if visual.overlays.is_empty() {
        "vout"
    } else {
        "base_video"
    };
    if transition_plan.has_transitions() {
        graph.push(format!(
            "{concat_audio_inputs}concat=n={}:v=0:a=1[source_audio]",
            visual.primary.len()
        ));
        let mut current = "video_clip_0".to_owned();
        for clip_index in 1..visual.primary.len() {
            let output = if clip_index + 1 == visual.primary.len() {
                concat_video_label.to_owned()
            } else {
                format!("primary_video_{clip_index}")
            };
            if let Some(transition) = transition_plan.boundaries[clip_index] {
                let before_edit = transition.duration_ticks / 2;
                let offset_tick = visual.primary[clip_index]
                    .placement
                    .timeline_start_tick
                    .checked_sub(before_edit)
                    .expect("transition handle validation checked the offset");
                graph.push(format!(
                    "[{current}][video_clip_{clip_index}]xfade=transition={}:duration={}:\
                     offset={}[{output}]",
                    transition_filter_name(transition.kind),
                    seconds(transition.duration_ticks, time_base),
                    seconds(offset_tick, time_base),
                ));
            } else {
                graph.push(format!(
                    "[{current}][video_clip_{clip_index}]concat=n=2:v=1:a=0[{output}]"
                ));
            }
            current = output;
        }
    } else {
        graph.push(format!(
            "{concat_inputs}concat=n={}:v=1:a=1[{concat_video_label}][source_audio]",
            visual.primary.len()
        ));
    }

    if !visual.overlays.is_empty() {
        graph.push("[base_video]format=pix_fmts=gbrp[visual_base]".to_owned());
        let mut current = "visual_base".to_owned();
        let canvas_duration = seconds(video_duration_ticks, time_base);
        for (layer_index, compiled) in compiled_overlays.iter().enumerate() {
            let clip = compiled.clip;
            let clip_label = format!("visual_clip_{layer_index}");
            match clip {
                VisualClip::Text(text) => graph.push(text_clip_filter(
                    text,
                    &text_resources[&text.id],
                    &clip_label,
                    time_base,
                    canvas,
                    compiled,
                )?),
                VisualClip::Video(_) | VisualClip::Image(_) => {
                    let input_pad = pads.take_video(
                        clip.source_id()
                            .expect("media visual clips have a source id"),
                    )?;
                    graph.push(visual_clip_filter(
                        clip,
                        &input_pad,
                        &clip_label,
                        time_base,
                        canvas.fps_milli,
                        compiled,
                    )?);
                }
            }

            let transparent = format!("transparent_{layer_index}");
            graph.push(format!(
                "color=c=black@0.0:s={}x{}:r={}/1000:d={canvas_duration},\
                 format=pix_fmts=rgba,settb=AVTB,setpts=PTS-STARTPTS[{transparent}]",
                canvas.width, canvas.height, canvas.fps_milli,
            ));
            let full_layer = format!("full_layer_{layer_index}");
            let overlay_evaluation = if compiled.transform.x.constant.is_some()
                && compiled.transform.y.constant.is_some()
            {
                "init"
            } else {
                "frame"
            };
            graph.push(format!(
                "[{transparent}][{clip_label}]overlay=x='{}':y='{}':eval={overlay_evaluation}:\
                 eof_action=pass:repeatlast=0:shortest=0:format=auto:\
                 enable='between(t,{},{})'[{full_layer}]",
                overlay_coordinate(
                    &compiled.transform.x.expression,
                    compiled.transform.overlay_anchor_x,
                    "main_w",
                    "overlay_w"
                ),
                overlay_coordinate(
                    &compiled.transform.y.expression,
                    compiled.transform.overlay_anchor_y,
                    "main_h",
                    "overlay_h"
                ),
                seconds(clip.timeline_start_tick(), time_base),
                seconds(clip.timeline_end_tick()?, time_base),
            ));

            let layer_color = format!("layer_color_{layer_index}");
            let layer_alpha = format!("layer_alpha_{layer_index}");
            let layer_mask = format!("layer_mask_{layer_index}");
            let layer_rgb = format!("layer_rgb_{layer_index}");
            graph.push(format!(
                "[{full_layer}]split=2[{layer_color}][{layer_alpha}]"
            ));
            graph.push(format!("[{layer_alpha}]alphaextract[{layer_mask}]"));
            graph.push(format!("[{layer_color}]format=pix_fmts=gbrp[{layer_rgb}]"));

            let base_keep = format!("base_keep_{layer_index}");
            let base_blend = format!("base_blend_{layer_index}");
            let blended = format!("blended_{layer_index}");
            let composite = format!("composite_{layer_index}");
            graph.push(format!("[{current}]split=2[{base_keep}][{base_blend}]"));
            graph.push(format!(
                "[{layer_rgb}][{base_blend}]blend=all_mode={}:\
                 eof_action=pass:repeatlast=0:shortest=0[{blended}]",
                blend_mode_name(clip.blend_mode())
            ));
            graph.push(format!(
                "[{base_keep}][{blended}][{layer_mask}]maskedmerge[{composite}]"
            ));
            current = composite;
        }
        graph.push(format!("[{current}]format=pix_fmts=yuv420p[vout]"));
    }

    let mut mix_labels = vec!["[source_audio]".to_owned()];
    for (clip_index, compiled) in compiled_audio_clips.iter().enumerate() {
        let clip = compiled.clip;
        let duration_ticks = clip.placement.timeline_duration_ticks()?;
        let audio_pad = pads.take_audio(&clip.source_id)?;
        let output_label = format!("audio_clip_{clip_index}");
        graph.push(media_audio_filter(
            &audio_pad,
            &clip.placement,
            duration_ticks,
            time_base,
            &output_label,
            false,
            Some(AudioFilterOptions {
                automation: &compiled.automation,
                timeline_start_tick: Some(clip.placement.timeline_start_tick),
                fade_in_ticks: clip.fade_in_ticks,
                fade_out_ticks: clip.fade_out_ticks,
            }),
        )?);
        mix_labels.push(format!("[{output_label}]"));
    }

    let output_duration = seconds(video_duration_ticks, time_base);
    if mix_labels.len() == 1 {
        graph.push(format!(
            "[source_audio]atrim=duration={output_duration},asetpts=PTS-STARTPTS[aout]"
        ));
    } else {
        graph.push(format!(
            "{}amix=inputs={}:duration=first:dropout_transition=0:normalize=0,\
             alimiter=limit=0.950000,atrim=duration={output_duration},\
             asetpts=PTS-STARTPTS[aout]",
            mix_labels.join(""),
            mix_labels.len(),
        ));
    }

    let graph = graph.join(";");
    if graph.len() > MAX_FILTER_GRAPH_BYTES {
        bail!("composition filter graph exceeds the bounded compiler budget");
    }
    arguments.push("-filter_complex".to_owned());
    arguments.push(graph);
    arguments.extend([
        "-map".to_owned(),
        "[vout]".to_owned(),
        "-map".to_owned(),
        "[aout]".to_owned(),
    ]);
    append_delivery_arguments(
        &mut arguments,
        output,
        parallel_jobs,
        output_duration,
        destination,
    );

    let mut read_only_files: Vec<_> = text_resources
        .values()
        .flat_map(|resource| [resource.text_file.clone(), resource.font_file.clone()])
        .collect();
    read_only_files.sort();
    read_only_files.dedup();
    Ok(CompiledExportCommand {
        arguments,
        expected_duration_seconds: video_duration_ticks as f64 / time_base as f64,
        read_only_files,
    })
}

fn validate_export_spec(output: CompositionExportSpec) -> Result<()> {
    match output.profile {
        CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H264,
        }
        | CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H265,
        } => {
            if output.video_quality > 51 {
                bail!("composition H.264/H.265 quality must be a CRF in 0..=51");
            }
        }
        CompositionExportProfile::Webm { .. } => {
            if output.video_quality > 63 {
                bail!("composition VP9/AV1 quality must be a CRF in 0..=63");
            }
        }
        CompositionExportProfile::Mov { .. } => {
            if !(1..=31).contains(&output.video_quality) {
                bail!("composition ProRes quality must be a qscale in 1..=31");
            }
        }
    }
    let is_av1 = matches!(
        output.profile,
        CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Av1
        }
    );
    match (is_av1, output.av1_encoder) {
        (true, None) => bail!("composition AV1 output requires a resolved encoder"),
        (false, Some(_)) => bail!("composition AV1 encoder was supplied for a non-AV1 profile"),
        _ => Ok(()),
    }
}

fn append_delivery_arguments(
    arguments: &mut Vec<String>,
    output: CompositionExportSpec,
    parallel_jobs: usize,
    output_duration: String,
    destination: &Path,
) {
    match output.profile {
        CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H264,
        } => arguments.extend([
            "-c:v".to_owned(),
            "libx264".to_owned(),
            "-preset".to_owned(),
            "veryfast".to_owned(),
            "-crf".to_owned(),
            output.video_quality.to_string(),
            "-pix_fmt".to_owned(),
            "yuv420p".to_owned(),
        ]),
        CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H265,
        } => arguments.extend([
            "-c:v".to_owned(),
            "libx265".to_owned(),
            "-preset".to_owned(),
            "veryfast".to_owned(),
            "-crf".to_owned(),
            output.video_quality.to_string(),
            "-tag:v".to_owned(),
            "hvc1".to_owned(),
            "-pix_fmt".to_owned(),
            "yuv420p".to_owned(),
        ]),
        CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Vp9,
        } => arguments.extend([
            "-c:v".to_owned(),
            "libvpx-vp9".to_owned(),
            "-deadline".to_owned(),
            "good".to_owned(),
            "-cpu-used".to_owned(),
            "2".to_owned(),
            "-crf".to_owned(),
            output.video_quality.to_string(),
            "-b:v".to_owned(),
            "0".to_owned(),
            "-row-mt".to_owned(),
            "1".to_owned(),
            "-pix_fmt".to_owned(),
            "yuv420p".to_owned(),
        ]),
        CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Av1,
        } => {
            let encoder = output
                .av1_encoder
                .expect("validated AV1 output has a resolved encoder");
            arguments.extend(["-c:v".to_owned(), encoder.ffmpeg_name().to_owned()]);
            match encoder {
                CompositionAv1Encoder::LibSvtAv1 => arguments.extend([
                    "-preset".to_owned(),
                    "8".to_owned(),
                    "-crf".to_owned(),
                    output.video_quality.to_string(),
                ]),
                CompositionAv1Encoder::LibAomAv1 => arguments.extend([
                    "-cpu-used".to_owned(),
                    "6".to_owned(),
                    "-crf".to_owned(),
                    output.video_quality.to_string(),
                ]),
            }
            arguments.extend([
                "-b:v".to_owned(),
                "0".to_owned(),
                "-pix_fmt".to_owned(),
                "yuv420p".to_owned(),
            ]);
        }
        CompositionExportProfile::Mov { profile } => arguments.extend([
            "-c:v".to_owned(),
            "prores_ks".to_owned(),
            "-profile:v".to_owned(),
            profile.ffmpeg_value().to_string(),
            "-qscale:v".to_owned(),
            output.video_quality.to_string(),
            "-pix_fmt".to_owned(),
            "yuv422p10le".to_owned(),
        ]),
    }

    arguments.extend(["-c:a".to_owned()]);
    match output.profile {
        CompositionExportProfile::Mp4 { .. } => arguments.extend([
            "aac".to_owned(),
            "-b:a".to_owned(),
            DEFAULT_AUDIO_BITRATE.to_owned(),
        ]),
        CompositionExportProfile::Webm { .. } => arguments.extend([
            "libopus".to_owned(),
            "-b:a".to_owned(),
            DEFAULT_AUDIO_BITRATE.to_owned(),
        ]),
        CompositionExportProfile::Mov { .. } => arguments.push("pcm_s16le".to_owned()),
    }
    arguments.extend([
        "-ar".to_owned(),
        AUDIO_SAMPLE_RATE.to_string(),
        "-ac".to_owned(),
        "2".to_owned(),
        "-filter_complex_threads".to_owned(),
        parallel_jobs.to_string(),
        "-threads:v".to_owned(),
        parallel_jobs.to_string(),
        "-t".to_owned(),
        output_duration,
    ]);
    if matches!(output.profile, CompositionExportProfile::Mp4 { .. }) {
        arguments.extend(["-movflags".to_owned(), "+faststart".to_owned()]);
    }
    arguments.push(destination.to_string_lossy().into_owned());
}

#[derive(Clone, Copy)]
enum VisualClip<'a> {
    Video(&'a VideoClip),
    Image(&'a ImageClip),
    Text(&'a TextClip),
}

impl<'a> VisualClip<'a> {
    fn id(self) -> &'a str {
        match self {
            Self::Video(clip) => clip.id.as_str(),
            Self::Image(clip) => clip.id.as_str(),
            Self::Text(clip) => clip.id.as_str(),
        }
    }

    fn source_id(self) -> Option<&'a SourceId> {
        match self {
            Self::Video(clip) => Some(&clip.source_id),
            Self::Image(clip) => Some(&clip.source_id),
            Self::Text(_) => None,
        }
    }

    fn timeline_start_tick(self) -> u64 {
        match self {
            Self::Video(clip) => clip.placement.timeline_start_tick,
            Self::Image(clip) => clip.timeline_start_tick,
            Self::Text(clip) => clip.timeline_start_tick,
        }
    }

    fn timeline_end_tick(self) -> Result<u64> {
        match self {
            Self::Video(clip) => Ok(clip.placement.timeline_end_tick()?),
            Self::Image(clip) => clip
                .timeline_start_tick
                .checked_add(clip.duration_ticks)
                .ok_or_else(|| anyhow!("image clip {} overflows the timeline", clip.id.as_str())),
            Self::Text(clip) => Ok(clip.timeline_end_tick),
        }
    }

    fn transform(self) -> &'a TransformSpec {
        match self {
            Self::Video(clip) => &clip.transform,
            Self::Image(clip) => &clip.transform,
            Self::Text(clip) => &clip.transform,
        }
    }

    fn opacity(self) -> &'a AnimatableValue {
        match self {
            Self::Video(clip) => &clip.opacity,
            Self::Image(clip) => &clip.opacity,
            Self::Text(clip) => &clip.opacity,
        }
    }

    fn blend_mode(self) -> BlendMode {
        match self {
            Self::Video(clip) => clip.blend_mode,
            Self::Image(clip) => clip.blend_mode,
            Self::Text(_) => BlendMode::Normal,
        }
    }
}

struct VisualSlice<'a> {
    primary: Vec<&'a VideoClip>,
    primary_muted: bool,
    transitions: &'a [ClipTransition],
    /// Bottom-to-top render order. Composition track index zero is the topmost
    /// visual layer, so tracks above the primary are traversed in reverse.
    overlays: Vec<VisualClip<'a>>,
}

fn visual_slice(composition: &Composition) -> Result<VisualSlice<'_>> {
    let active_visual: Vec<_> = composition
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            _ => false,
        })
        .collect();
    let (primary_index, primary_track) = active_visual
        .last()
        .copied()
        .ok_or_else(|| anyhow!("composition export requires a visible primary video track"))?;
    let CompositionTrack::Video {
        clips,
        transitions,
        muted,
        ..
    } = primary_track
    else {
        bail!("the bottom visible visual track must be the primary video track");
    };
    let mut primary: Vec<_> = clips.iter().filter(|clip| clip.enabled).collect();
    if primary.is_empty() {
        bail!("the primary video track must contain enabled clips");
    }
    primary.sort_by(|left, right| {
        left.placement
            .timeline_start_tick
            .cmp(&right.placement.timeline_start_tick)
            .then_with(|| left.id.as_str().cmp(right.id.as_str()))
    });
    for clip in &primary {
        if clip.transform != TransformSpec::default()
            || clip.opacity != AnimatableValue::constant(1.0)
            || clip.blend_mode != BlendMode::Normal
            || !clip.effects.is_empty()
        {
            bail!(
                "primary video clip {} must use neutral visual properties",
                clip.id.as_str()
            );
        }
    }

    let mut overlays = Vec::new();
    for track in composition.tracks[..primary_index].iter().rev() {
        match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                transitions,
                ..
            } => {
                if !transitions.is_empty() {
                    bail!("transitions on overlay video tracks are not supported");
                }
                let mut active: Vec<_> = clips
                    .iter()
                    .filter(|clip| clip.enabled)
                    .map(VisualClip::Video)
                    .collect();
                sort_visual_clips(&mut active)?;
                overlays.extend(active);
            }
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => {
                let mut active: Vec<_> = clips
                    .iter()
                    .filter(|clip| clip.enabled)
                    .map(VisualClip::Image)
                    .collect();
                sort_visual_clips(&mut active)?;
                overlays.extend(active);
            }
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => {
                let mut active: Vec<_> = clips
                    .iter()
                    .filter(|clip| clip.enabled)
                    .map(VisualClip::Text)
                    .collect();
                sort_visual_clips(&mut active)?;
                overlays.extend(active);
            }
            _ => {}
        }
    }
    Ok(VisualSlice {
        primary,
        primary_muted: *muted,
        transitions,
        overlays,
    })
}

fn sort_visual_clips(clips: &mut [VisualClip<'_>]) -> Result<()> {
    clips.sort_by(|left, right| {
        left.timeline_start_tick()
            .cmp(&right.timeline_start_tick())
            .then_with(|| left.id().cmp(right.id()))
    });
    let mut previous_end = 0;
    for clip in clips {
        if clip.timeline_start_tick() < previous_end {
            bail!(
                "visual clips overlap within their track before {}",
                clip.id()
            );
        }
        previous_end = clip.timeline_end_tick()?;
    }
    Ok(())
}

fn compile_visual_overlays<'a>(
    composition: &Composition,
    clips: &[VisualClip<'a>],
    primary_duration: u64,
) -> Result<Vec<CompiledVisual<'a>>> {
    let mut compiled = Vec::with_capacity(clips.len());
    let mut keyframe_points = 0_usize;
    for clip in clips {
        if clip.timeline_end_tick()? > primary_duration {
            bail!("visual clip {} extends beyond the primary video", clip.id());
        }
        let (source_width, source_height) = match clip.source_id() {
            Some(source_id) => {
                let source = composition
                    .sources
                    .get(source_id)
                    .expect("composition validation checked the source");
                (source.width, source.height)
            }
            None => (composition.canvas.width, composition.canvas.height),
        };
        let duration_ticks = clip
            .timeline_end_tick()?
            .checked_sub(clip.timeline_start_tick())
            .ok_or_else(|| anyhow!("visual clip {} has invalid timing", clip.id()))?;
        let media_anchor = !matches!(clip, VisualClip::Text(_));
        let transform = compile_transform(
            clip.transform(),
            clip.id(),
            source_width,
            source_height,
            clip.timeline_start_tick(),
            duration_ticks,
            composition.time_base,
            media_anchor,
            &mut keyframe_points,
        )?;
        let opacity = compile_animatable(
            clip.opacity(),
            "visual opacity",
            "T",
            0.0,
            duration_ticks,
            composition.time_base,
            0.0,
            1.0,
            true,
            &mut keyframe_points,
        )?;
        let mut masks = Vec::new();
        if let VisualClip::Video(video) = clip {
            let mut saw_mask = false;
            for effect in &video.effects {
                match effect {
                    VideoEffect::ChromaKey { .. } if saw_mask => bail!(
                        "video clip {} must place chroma key effects before shape masks",
                        clip.id()
                    ),
                    VideoEffect::ChromaKey { similarity, .. } if *similarity >= 0.000_01 => {}
                    VideoEffect::ChromaKey { .. } => bail!(
                        "video clip {} has chroma similarity below FFmpeg's minimum",
                        clip.id()
                    ),
                    VideoEffect::Mask {
                        shape: MaskShape::Linear,
                        ..
                    } => bail!("Linear mask semantics are not supported by composition export"),
                    VideoEffect::Mask {
                        shape,
                        x,
                        y,
                        width,
                        height,
                        feather,
                        inverted,
                    } => {
                        saw_mask = true;
                        masks.push(CompiledMask {
                            shape: *shape,
                            x: compile_animatable(
                                x,
                                "mask x",
                                "T",
                                0.0,
                                duration_ticks,
                                composition.time_base,
                                0.0,
                                1.0,
                                true,
                                &mut keyframe_points,
                            )?,
                            y: compile_animatable(
                                y,
                                "mask y",
                                "T",
                                0.0,
                                duration_ticks,
                                composition.time_base,
                                0.0,
                                1.0,
                                true,
                                &mut keyframe_points,
                            )?,
                            width: compile_animatable(
                                width,
                                "mask width",
                                "T",
                                0.0,
                                duration_ticks,
                                composition.time_base,
                                0.0,
                                2.0,
                                false,
                                &mut keyframe_points,
                            )?,
                            height: compile_animatable(
                                height,
                                "mask height",
                                "T",
                                0.0,
                                duration_ticks,
                                composition.time_base,
                                0.0,
                                2.0,
                                false,
                                &mut keyframe_points,
                            )?,
                            feather: *feather,
                            inverted: *inverted,
                        });
                    }
                }
            }
        }
        compiled.push(CompiledVisual {
            clip: *clip,
            transform,
            opacity,
            masks,
        });
    }
    if keyframe_points > MAX_ACTIVE_KEYFRAME_POINTS {
        bail!("composition has too many active visual keyframes");
    }
    Ok(compiled)
}

#[derive(Debug, Clone)]
struct CompiledValue {
    expression: String,
    minimum: f64,
    maximum: f64,
    constant: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct RotationLayout {
    pad_width: u32,
    pad_height: u32,
    output_dimension: u32,
}

#[derive(Debug, Clone)]
struct CompiledTransform {
    x: CompiledValue,
    y: CompiledValue,
    scale_x: CompiledValue,
    scale_y: CompiledValue,
    rotation_degrees: CompiledValue,
    filter_anchor_x: f64,
    filter_anchor_y: f64,
    overlay_anchor_x: f64,
    overlay_anchor_y: f64,
    rotation_layout: Option<RotationLayout>,
}

#[derive(Debug, Clone)]
struct CompiledMask {
    shape: MaskShape,
    x: CompiledValue,
    y: CompiledValue,
    width: CompiledValue,
    height: CompiledValue,
    feather: f64,
    inverted: bool,
}

#[derive(Clone)]
struct CompiledVisual<'a> {
    clip: VisualClip<'a>,
    transform: CompiledTransform,
    opacity: CompiledValue,
    masks: Vec<CompiledMask>,
}

#[allow(clippy::too_many_arguments)]
fn compile_transform(
    transform: &TransformSpec,
    clip_id: &str,
    source_width: u32,
    source_height: u32,
    timeline_start_tick: u64,
    duration_ticks: u64,
    composition_time_base: u32,
    media_anchor: bool,
    keyframe_points: &mut usize,
) -> Result<CompiledTransform> {
    let start_seconds = timeline_start_tick as f64 / composition_time_base as f64;
    let x = compile_animatable(
        &transform.x,
        "transform x",
        "t",
        start_seconds,
        duration_ticks,
        composition_time_base,
        -MAX_VISUAL_POSITION,
        MAX_VISUAL_POSITION,
        true,
        keyframe_points,
    )?;
    let y = compile_animatable(
        &transform.y,
        "transform y",
        "t",
        start_seconds,
        duration_ticks,
        composition_time_base,
        -MAX_VISUAL_POSITION,
        MAX_VISUAL_POSITION,
        true,
        keyframe_points,
    )?;
    let scale_x = compile_animatable(
        &transform.scale_x,
        "transform scale x",
        "t",
        0.0,
        duration_ticks,
        composition_time_base,
        0.01,
        16.0,
        true,
        keyframe_points,
    )?;
    let scale_y = compile_animatable(
        &transform.scale_y,
        "transform scale y",
        "t",
        0.0,
        duration_ticks,
        composition_time_base,
        0.01,
        16.0,
        true,
        keyframe_points,
    )?;
    let rotation_degrees = compile_animatable(
        &transform.rotation_degrees,
        "transform rotation",
        "t",
        0.0,
        duration_ticks,
        composition_time_base,
        -3_600.0,
        3_600.0,
        true,
        keyframe_points,
    )?;
    let scaled_width = source_width as f64 * scale_x.maximum;
    let scaled_height = source_height as f64 * scale_y.maximum;
    if scaled_width < 1.0
        || scaled_height < 1.0
        || scaled_width > MAX_VISUAL_FILTER_DIMENSION
        || scaled_height > MAX_VISUAL_FILTER_DIMENSION
    {
        bail!("visual clip {clip_id} has unsafe transformed dimensions");
    }

    let filter_anchor_x = if media_anchor {
        transform.anchor_x
    } else {
        0.5
    };
    let filter_anchor_y = if media_anchor {
        transform.anchor_y
    } else {
        0.5
    };
    let rotates = rotation_degrees.minimum.abs() > f64::EPSILON
        || rotation_degrees.maximum.abs() > f64::EPSILON;
    let rotation_layout = if rotates {
        let pad_width =
            2.0 * (filter_anchor_x * scaled_width).max((1.0 - filter_anchor_x) * scaled_width);
        let pad_height =
            2.0 * (filter_anchor_y * scaled_height).max((1.0 - filter_anchor_y) * scaled_height);
        let pad_width = bounded_dimension(pad_width.max(scaled_width), clip_id)?;
        let pad_height = bounded_dimension(pad_height.max(scaled_height), clip_id)?;
        let output_dimension =
            bounded_dimension((pad_width as f64).hypot(pad_height as f64), clip_id)?;
        Some(RotationLayout {
            pad_width,
            pad_height,
            output_dimension,
        })
    } else {
        None
    };
    let (overlay_anchor_x, overlay_anchor_y) = if rotation_layout.is_some() || !media_anchor {
        (0.5, 0.5)
    } else {
        (transform.anchor_x, transform.anchor_y)
    };
    Ok(CompiledTransform {
        x,
        y,
        scale_x,
        scale_y,
        rotation_degrees,
        filter_anchor_x,
        filter_anchor_y,
        overlay_anchor_x,
        overlay_anchor_y,
        rotation_layout,
    })
}

fn bounded_dimension(value: f64, clip_id: &str) -> Result<u32> {
    if !value.is_finite() || value < 1.0 || value.ceil() > MAX_VISUAL_FILTER_DIMENSION {
        bail!("visual clip {clip_id} has unsafe transformed dimensions");
    }
    Ok(value.ceil() as u32)
}

#[allow(clippy::too_many_arguments)]
fn compile_animatable(
    value: &AnimatableValue,
    label: &str,
    time_variable: &str,
    offset_seconds: f64,
    clip_duration_ticks: u64,
    composition_time_base: u32,
    minimum: f64,
    maximum: f64,
    inclusive_minimum: bool,
    keyframe_points: &mut usize,
) -> Result<CompiledValue> {
    let valid = |candidate: f64| {
        candidate.is_finite()
            && candidate <= maximum
            && if inclusive_minimum {
                candidate >= minimum
            } else {
                candidate > minimum
            }
    };
    let compiled = match value {
        AnimatableValue::Constant { value } => {
            if !valid(*value) {
                bail!("{label} is outside the supported range");
            }
            CompiledValue {
                expression: decimal(*value),
                minimum: *value,
                maximum: *value,
                constant: Some(*value),
            }
        }
        AnimatableValue::Keyframes { track } => {
            *keyframe_points = keyframe_points
                .checked_add(track.keyframes.len())
                .ok_or_else(|| anyhow!("visual keyframe count overflow"))?;
            let last = track
                .keyframes
                .last()
                .expect("composition validation checked keyframes");
            if u128::from(last.tick) * u128::from(composition_time_base)
                > u128::from(clip_duration_ticks) * u128::from(track.time_base)
            {
                bail!("{label} keyframes extend beyond the clip-local duration");
            }
            if !track.keyframes.iter().all(|keyframe| valid(keyframe.value)) {
                bail!("{label} keyframes are outside the supported range");
            }
            let expression = FfmpegKeyframeAdapter::new(track)
                .expression_shifted(time_variable, offset_seconds)
                .with_context(|| format!("cannot compile {label} keyframes"))?;
            let (minimum, maximum) = track.keyframes.iter().fold(
                (f64::INFINITY, f64::NEG_INFINITY),
                |(minimum, maximum), keyframe| {
                    (minimum.min(keyframe.value), maximum.max(keyframe.value))
                },
            );
            CompiledValue {
                expression,
                minimum,
                maximum,
                constant: None,
            }
        }
    };
    if compiled.expression.len() > MAX_FILTER_EXPRESSION_BYTES {
        bail!("{label} keyframe expression exceeds the bounded compiler budget");
    }
    Ok(compiled)
}

fn validate_text_resources(
    clips: &[VisualClip<'_>],
    resources: &BTreeMap<CompositionClipId, CompositionTextResource>,
) -> Result<()> {
    let expected: BTreeSet<_> = clips
        .iter()
        .filter_map(|clip| match clip {
            VisualClip::Text(text) => Some(text.id.clone()),
            VisualClip::Video(_) | VisualClip::Image(_) => None,
        })
        .collect();
    let actual: BTreeSet<_> = resources.keys().cloned().collect();
    if actual != expected {
        bail!("text render resources must exactly match active text clips");
    }
    for (id, resource) in resources {
        if resource.text_file.as_os_str().is_empty()
            || resource.font_file.as_os_str().is_empty()
            || resource.text_file == resource.font_file
        {
            bail!("text clip {} has invalid render resources", id.as_str());
        }
    }
    Ok(())
}

fn visual_clip_filter(
    clip: VisualClip<'_>,
    input_label: &str,
    output_label: &str,
    time_base: u32,
    fps_milli: u32,
    compiled: &CompiledVisual<'_>,
) -> Result<String> {
    let mut filters = match clip {
        VisualClip::Video(video) => {
            let mut filters = source_video_filters(
                video,
                video.placement.source_in_tick as f64,
                video.placement.source_out_tick as f64,
                fps_milli,
                time_base,
            )?;
            filters.insert(1, "settb=AVTB".to_owned());
            filters
        }
        VisualClip::Image(image) => vec![
            format!("trim=duration={}", seconds(image.duration_ticks, time_base)),
            "settb=AVTB".to_owned(),
            "setpts=PTS-STARTPTS".to_owned(),
            format!("fps=fps={fps_milli}/1000"),
        ],
        VisualClip::Text(_) => bail!("text clips require explicit text render resources"),
    };

    if let VisualClip::Video(video) = clip {
        filters.extend(playback_frame_filters(
            video,
            fps_milli,
            video.placement.timeline_duration_ticks()?,
            time_base,
        ));
    }

    if let VisualClip::Video(video) = clip {
        for effect in &video.effects {
            match effect {
                VideoEffect::ChromaKey {
                    color,
                    similarity,
                    softness,
                    spill,
                } => {
                    filters.push(format!(
                        "chromakey=color={}:similarity={}:blend={}",
                        ffmpeg_rgb(color.red, color.green, color.blue),
                        decimal(*similarity),
                        decimal(*softness)
                    ));
                    if *spill > 0.0 {
                        let screen = if color.green >= color.blue {
                            "green"
                        } else {
                            "blue"
                        };
                        filters.push(format!("despill=type={screen}:mix={}", decimal(*spill)));
                    }
                }
                VideoEffect::Mask { .. } => {}
            }
        }
    }

    filters.push("format=pix_fmts=rgba".to_owned());
    for mask in &compiled.masks {
        filters.push(mask_filter(mask)?);
    }
    append_visual_transform(&mut filters, &compiled.transform);
    filters.push(opacity_filter(&compiled.opacity)?);
    filters.push(format!(
        "setpts=PTS+{}/TB",
        seconds(clip.timeline_start_tick(), time_base)
    ));
    Ok(format!(
        "[{input_label}]{}[{output_label}]",
        filters.join(",")
    ))
}

fn source_video_filters(
    clip: &VideoClip,
    source_start_tick: f64,
    source_end_tick: f64,
    fps_milli: u32,
    time_base: u32,
) -> Result<Vec<String>> {
    let filters = match clip.playback_mode {
        PlaybackMode::Forward => {
            let mut filters = vec![format!(
                "trim=start={}:end={}",
                fractional_seconds(source_start_tick, time_base),
                fractional_seconds(source_end_tick, time_base)
            )];
            append_stabilization(&mut filters, clip.stabilization, fps_milli);
            filters.push(video_timing_filter(&clip.placement, time_base)?);
            filters
        }
        PlaybackMode::Reverse => {
            let mut filters = vec![
                format!(
                    "trim=start={}:end={}",
                    fractional_seconds(source_start_tick, time_base),
                    fractional_seconds(source_end_tick, time_base)
                ),
                // Bound the number of frames retained by FFmpeg's buffered
                // reverse independently of an untrusted source frame rate.
                format!("fps=fps={fps_milli}/1000"),
            ];
            append_deshake(&mut filters, clip.stabilization);
            filters.extend([
                "reverse".to_owned(),
                video_timing_filter(&clip.placement, time_base)?,
            ]);
            filters
        }
        PlaybackMode::Freeze { source_tick } => vec![
            format!("trim=start={}", seconds(source_tick, time_base)),
            "trim=end_frame=1".to_owned(),
            "setpts=PTS-STARTPTS".to_owned(),
        ],
    };
    Ok(filters)
}

fn video_timing_filter(placement: &ClipPlacement, time_base: u32) -> Result<String> {
    let Some(segments) = placement.speed_ramp_segments()? else {
        return Ok(format!(
            "setpts=(PTS-STARTPTS)/{}",
            decimal(placement.speed)
        ));
    };
    let progress = "((PTS-STARTPTS)*TB)";
    let mut expression = seconds(
        segments
            .last()
            .expect("validated speed ramp has segments")
            .timeline_end_tick,
        time_base,
    );
    for segment in segments.iter().rev() {
        let source_start = seconds(segment.source_start_tick, time_base);
        let source_end = seconds(segment.source_end_tick, time_base);
        let source_duration = seconds(
            segment.source_end_tick - segment.source_start_tick,
            time_base,
        );
        let timeline_start = seconds(segment.timeline_start_tick, time_base);
        let timeline_duration = seconds(
            segment.timeline_end_tick - segment.timeline_start_tick,
            time_base,
        );
        let local_source = format!("({progress}-{source_start})");
        let fraction = match segment.interpolation {
            SpeedRampInterpolation::Hold | SpeedRampInterpolation::Linear
                if (segment.end_speed - segment.start_speed).abs()
                    <= f64::EPSILON * segment.start_speed.max(segment.end_speed) =>
            {
                format!("({local_source}/{source_duration})")
            }
            SpeedRampInterpolation::Hold => {
                format!("({local_source}/{source_duration})")
            }
            SpeedRampInterpolation::Linear => {
                let start_speed = precise_decimal(segment.start_speed);
                let end_speed = precise_decimal(segment.end_speed);
                let speed_delta = precise_decimal(segment.end_speed - segment.start_speed);
                format!(
                    "(log(({start_speed}+({speed_delta})*{local_source}/{source_duration})/\
                     {start_speed})/log({end_speed}/{start_speed}))"
                )
            }
        };
        let mapped = format!("({timeline_start}+{timeline_duration}*{fraction})");
        expression = format!("if(lt({progress},{source_end}),{mapped},{expression})");
    }
    let expression = format!("({expression})/TB");
    bounded_expression(&expression, "speed ramp video")?;
    Ok(format!("setpts='{expression}'"))
}

fn precise_decimal(value: f64) -> String {
    if value == 0.0 {
        "0.000000000000".to_owned()
    } else {
        format!("{value:.12}")
    }
}

fn append_stabilization(
    filters: &mut Vec<String>,
    stabilization: StabilizationSpec,
    fps_milli: u32,
) {
    if stabilization.is_enabled() {
        filters.push(format!("fps=fps={fps_milli}/1000"));
        append_deshake(filters, stabilization);
    }
}

fn append_deshake(filters: &mut Vec<String>, stabilization: StabilizationSpec) {
    if let StabilizationSpec::Deshake { radius_x, radius_y } = stabilization {
        filters.push(format!(
            "deshake=rx={radius_x}:ry={radius_y}:edge=mirror:blocksize=8:\
             contrast=20:search=exhaustive"
        ));
    }
}

fn playback_frame_filters(
    clip: &VideoClip,
    fps_milli: u32,
    duration_ticks: u64,
    time_base: u32,
) -> Vec<String> {
    match clip.playback_mode {
        PlaybackMode::Forward | PlaybackMode::Reverse
            if clip.placement.speed_ramp.is_some()
                && clip.frame_interpolation == FrameInterpolation::Duplicate =>
        {
            let duration = seconds(duration_ticks, time_base);
            vec![
                format!("tpad=stop_mode=clone:stop_duration={duration}"),
                format!("fps=fps={fps_milli}/1000"),
                format!("trim=duration={duration}"),
                "setpts=PTS-STARTPTS".to_owned(),
            ]
        }
        PlaybackMode::Forward | PlaybackMode::Reverse => video_frame_filters(
            clip.frame_interpolation,
            fps_milli,
            duration_ticks,
            time_base,
        ),
        PlaybackMode::Freeze { .. } => {
            let duration = seconds(duration_ticks, time_base);
            vec![
                format!("tpad=stop_mode=clone:stop_duration={duration}"),
                format!("fps=fps={fps_milli}/1000"),
                format!("trim=duration={duration}"),
                "setpts=PTS-STARTPTS".to_owned(),
            ]
        }
    }
}

fn video_frame_filters(
    interpolation: FrameInterpolation,
    fps_milli: u32,
    duration_ticks: u64,
    time_base: u32,
) -> Vec<String> {
    match interpolation {
        FrameInterpolation::Duplicate => vec![format!("fps=fps={fps_milli}/1000")],
        FrameInterpolation::OpticalFlow => {
            let duration = seconds(duration_ticks, time_base);
            vec![
                format!("tpad=stop_mode=clone:stop_duration={duration}"),
                format!(
                    "minterpolate=fps={fps_milli}/1000:mi_mode=mci:mc_mode=aobmc:\
                     me_mode=bilat:me=epzs:mb_size=16:search_param=32:vsbmc=1:\
                     scd=fdiff:scd_threshold=10"
                ),
                format!("trim=duration={duration}"),
                "setpts=PTS-STARTPTS".to_owned(),
            ]
        }
    }
}

fn text_clip_filter(
    clip: &TextClip,
    resource: &CompositionTextResource,
    output_label: &str,
    time_base: u32,
    canvas: CanvasSpec,
    compiled: &CompiledVisual<'_>,
) -> Result<String> {
    let duration_ticks = clip
        .timeline_end_tick
        .checked_sub(clip.timeline_start_tick)
        .ok_or_else(|| anyhow!("invalid text clip duration"))?;
    let style = &clip.style;
    let mut filters = vec![
        format!(
            "color=c=black@0.0:s={}x{}:r={}/1000:d={}",
            canvas.width,
            canvas.height,
            canvas.fps_milli,
            seconds(duration_ticks, time_base)
        ),
        "format=pix_fmts=rgba".to_owned(),
        format!(
            "drawtext=fontfile={}:textfile={}:reload=0:expansion=none:fontsize={}:fontcolor={}:\
             box=1:boxcolor={}:borderw={}:bordercolor={}:shadowx={}:shadowy={}:\
             shadowcolor={}:fix_bounds=1:x='(w/2)-{}*text_w':y='(h/2)-{}*text_h'",
            escape_filter_value(&resource.font_file, "font")?,
            escape_filter_value(&resource.text_file, "text")?,
            decimal(style.font_size),
            ffmpeg_rgba(style.color),
            ffmpeg_rgba(style.background),
            decimal(style.stroke_width.round()),
            ffmpeg_rgba(style.stroke),
            decimal(style.shadow_x.round()),
            decimal(style.shadow_y.round()),
            ffmpeg_rgba(style.shadow),
            decimal(clip.transform.anchor_x),
            decimal(clip.transform.anchor_y),
        ),
    ];
    append_visual_transform(&mut filters, &compiled.transform);
    filters.extend([
        opacity_filter(&compiled.opacity)?,
        "settb=AVTB".to_owned(),
        format!(
            "setpts=PTS-STARTPTS+{}/TB",
            seconds(clip.timeline_start_tick, time_base)
        ),
    ]);
    Ok(format!("{}[{output_label}]", filters.join(",")))
}

fn append_visual_transform(filters: &mut Vec<String>, transform: &CompiledTransform) {
    match (transform.scale_x.constant, transform.scale_y.constant) {
        (Some(scale_x), Some(scale_y)) => filters.push(format!(
            "scale=iw*{}:ih*{}:eval=init",
            decimal(scale_x),
            decimal(scale_y)
        )),
        _ => filters.push(format!(
            "scale=w='max(1,iw*({}))':h='max(1,ih*({}))':eval=frame",
            transform.scale_x.expression, transform.scale_y.expression
        )),
    }
    if let Some(layout) = transform.rotation_layout {
        filters.push(format!(
            "pad={}:{}:{}/2-{}*iw:{}/2-{}*ih:color=black@0:eval=frame",
            layout.pad_width,
            layout.pad_height,
            layout.pad_width,
            decimal(transform.filter_anchor_x),
            layout.pad_height,
            decimal(transform.filter_anchor_y),
        ));
        filters.push(format!(
            "rotate=angle='({})*PI/180':ow={}:oh={}:c=none",
            transform.rotation_degrees.expression, layout.output_dimension, layout.output_dimension,
        ));
    }
}

fn opacity_filter(opacity: &CompiledValue) -> Result<String> {
    if let Some(value) = opacity.constant {
        return Ok(format!("colorchannelmixer=aa={}", decimal(value)));
    }
    let expression = format!("alpha(X,Y)*({})", opacity.expression);
    bounded_expression(&expression, "visual opacity")?;
    Ok(format!(
        "geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='{expression}'"
    ))
}

fn mask_filter(mask: &CompiledMask) -> Result<String> {
    let pixel_x = "((X+0.5)/W)";
    let pixel_y = "((Y+0.5)/H)";
    let half_width = format!("(({})/2)", mask.width.expression);
    let half_height = format!("(({})/2)", mask.height.expression);
    let alpha = match mask.shape {
        MaskShape::Rectangle if mask.feather == 0.0 => format!(
            "lte(abs({pixel_x}-({})),{half_width})*lte(abs({pixel_y}-({})),{half_height})",
            mask.x.expression, mask.y.expression
        ),
        MaskShape::Rectangle => format!(
            "clip(min(({half_width}-abs({pixel_x}-({})))/({half_width}*{}),\
             ({half_height}-abs({pixel_y}-({})))/({half_height}*{})),0,1)",
            mask.x.expression,
            decimal(mask.feather),
            mask.y.expression,
            decimal(mask.feather),
        ),
        MaskShape::Ellipse => {
            let distance = format!(
                "sqrt(pow(({pixel_x}-({}))/{half_width},2)+pow(({pixel_y}-({}))/{half_height},2))",
                mask.x.expression, mask.y.expression
            );
            if mask.feather == 0.0 {
                format!("lte({distance},1)")
            } else {
                format!("clip((1-({distance}))/{},0,1)", decimal(mask.feather))
            }
        }
        MaskShape::Linear => bail!("Linear mask semantics are not supported by composition export"),
    };
    let alpha = if mask.inverted {
        format!("1-({alpha})")
    } else {
        alpha
    };
    let expression = format!("alpha(X,Y)*({alpha})");
    bounded_expression(&expression, "shape mask")?;
    Ok(format!(
        "geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='{expression}'"
    ))
}

fn bounded_expression(expression: &str, label: &str) -> Result<()> {
    if expression.len() > MAX_FILTER_EXPRESSION_BYTES {
        bail!("{label} expression exceeds the bounded compiler budget");
    }
    Ok(())
}

/// Escape one path for an FFmpeg filter option. The result is one quoted
/// filtergraph token; it is not shell escaping because argv is passed directly.
fn escape_filter_value(path: &Path, label: &str) -> Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| anyhow!("{label} resource path is not valid UTF-8"))?;
    if value.is_empty() {
        bail!("{label} resource path is empty");
    }
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if matches!(character, '\\' | '\'' | ':' | ',' | ';' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(format!("'{escaped}'"))
}

/// x/y are pixel offsets from canvas center; anchor is normalized within the
/// transformed layer. Keeping this expression in FFmpeg also handles the
/// dimension change introduced by rotation.
fn overlay_coordinate(offset: &str, anchor: f64, main: &str, overlay: &str) -> String {
    format!("({main}/2+({offset}))-{}*{overlay}", decimal(anchor))
}

fn blend_mode_name(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "normal",
        BlendMode::Multiply => "multiply",
        BlendMode::Screen => "screen",
        BlendMode::Overlay => "overlay",
        BlendMode::Darken => "darken",
        BlendMode::Lighten => "lighten",
        BlendMode::Difference => "difference",
        BlendMode::Addition => "addition",
    }
}

fn transition_filter_name(kind: TransitionKind) -> &'static str {
    match kind {
        TransitionKind::Dissolve => "fade",
        TransitionKind::FadeBlack => "fadeblack",
        TransitionKind::WipeLeft => "wipeleft",
        TransitionKind::WipeRight => "wiperight",
        TransitionKind::SlideLeft => "slideleft",
        TransitionKind::SlideRight => "slideright",
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct SourceHandles {
    source_head_ticks: u64,
    source_tail_ticks: u64,
}

struct TransitionPlan<'a> {
    /// Indexed by the incoming clip. Index zero is always empty.
    boundaries: Vec<Option<&'a ClipTransition>>,
    handles: Vec<SourceHandles>,
}

impl TransitionPlan<'_> {
    fn has_transitions(&self) -> bool {
        self.boundaries.iter().any(Option::is_some)
    }
}

fn transition_plan<'a>(
    composition: &Composition,
    clips: &[&VideoClip],
    transitions: &'a [ClipTransition],
) -> Result<TransitionPlan<'a>> {
    let indexes: BTreeMap<_, _> = clips
        .iter()
        .enumerate()
        .map(|(index, clip)| (&clip.id, index))
        .collect();
    let mut boundaries = vec![None; clips.len()];
    let mut handles = vec![SourceHandles::default(); clips.len()];
    let mut head_occupancy = vec![0_u64; clips.len()];
    let mut tail_occupancy = vec![0_u64; clips.len()];

    for transition in transitions {
        let (Some(&from_index), Some(&to_index)) = (
            indexes.get(&transition.from_clip_id),
            indexes.get(&transition.to_clip_id),
        ) else {
            // A transition whose endpoint clip is disabled is inactive too.
            continue;
        };
        if from_index.checked_add(1) != Some(to_index) || boundaries[to_index].is_some() {
            bail!(
                "transition {} must connect one unique adjacent primary boundary",
                transition.id.as_str()
            );
        }
        let before_edit = transition.duration_ticks / 2;
        let after_edit = transition.duration_ticks - before_edit;
        if transition.duration_ticks == 0
            || clips[from_index].placement.timeline_end_tick()?
                != clips[to_index].placement.timeline_start_tick
        {
            bail!("transition {} has invalid timing", transition.id.as_str());
        }

        let from = clips[from_index];
        let to = clips[to_index];
        let available_tail = composition.sources[&from.source_id]
            .duration_ticks
            .saturating_sub(from.placement.source_out_tick) as f64;
        let required_tail = after_edit as f64 * from.placement.speed;
        let available_head = to.placement.source_in_tick as f64;
        let required_head = before_edit as f64 * to.placement.speed;
        if available_tail + f64::EPSILON < required_tail
            || available_head + f64::EPSILON < required_head
        {
            bail!(
                "transition {} does not have exact source handles",
                transition.id.as_str()
            );
        }

        handles[from_index].source_tail_ticks = after_edit;
        handles[to_index].source_head_ticks = before_edit;
        tail_occupancy[from_index] = before_edit;
        head_occupancy[to_index] = after_edit;
        boundaries[to_index] = Some(transition);
    }

    for (index, clip) in clips.iter().enumerate() {
        let occupied = head_occupancy[index]
            .checked_add(tail_occupancy[index])
            .ok_or_else(|| anyhow!("transition window overflow"))?;
        if clip.placement.timeline_duration_ticks()? < occupied {
            bail!(
                "transition windows overlap inside primary clip {}",
                clip.id.as_str()
            );
        }
    }
    Ok(TransitionPlan {
        boundaries,
        handles,
    })
}

fn validate_optical_flow_workload(
    composition: &Composition,
    visual: &VisualSlice<'_>,
    transitions: &TransitionPlan<'_>,
) -> Result<()> {
    let mut total_pixel_frames = 0_u128;
    for (index, clip) in visual.primary.iter().enumerate() {
        if clip.frame_interpolation != FrameInterpolation::OpticalFlow {
            continue;
        }
        let handles = transitions.handles[index];
        let duration_ticks = clip
            .placement
            .timeline_duration_ticks()?
            .checked_add(handles.source_head_ticks)
            .and_then(|value| value.checked_add(handles.source_tail_ticks))
            .ok_or_else(|| anyhow!("optical-flow duration overflow"))?;
        validate_optical_flow_work(
            composition.canvas.width,
            composition.canvas.height,
            duration_ticks,
            composition,
            clip.id.as_str(),
            &mut total_pixel_frames,
        )?;
    }
    for clip in &visual.overlays {
        let VisualClip::Video(clip) = clip else {
            continue;
        };
        if clip.frame_interpolation != FrameInterpolation::OpticalFlow {
            continue;
        }
        let source = &composition.sources[&clip.source_id];
        validate_optical_flow_work(
            source.width,
            source.height,
            clip.placement.timeline_duration_ticks()?,
            composition,
            clip.id.as_str(),
            &mut total_pixel_frames,
        )?;
    }
    Ok(())
}

fn validate_optical_flow_work(
    width: u32,
    height: u32,
    duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    if width > MAX_OPTICAL_FLOW_EDGE
        || height > MAX_OPTICAL_FLOW_EDGE
        || pixels > MAX_OPTICAL_FLOW_PIXELS
    {
        bail!("optical flow clip {clip_id} exceeds the dimension budget");
    }
    if duration_ticks > MAX_OPTICAL_FLOW_CLIP_TICKS {
        bail!("optical flow clip {clip_id} exceeds the duration budget");
    }
    let denominator = u128::from(composition.time_base) * 1_000;
    let frame_numerator = u128::from(duration_ticks) * u128::from(composition.canvas.fps_milli);
    let output_frames = frame_numerator
        .checked_add(denominator - 1)
        .ok_or_else(|| anyhow!("optical-flow frame count overflow"))?
        / denominator;
    let work = pixels
        .checked_mul(output_frames)
        .ok_or_else(|| anyhow!("optical-flow work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow!("optical-flow work overflow"))?;
    if *total_pixel_frames > MAX_OPTICAL_FLOW_PIXEL_FRAMES {
        bail!("composition exceeds the optical-flow work budget");
    }
    Ok(())
}

fn validate_reverse_workload(composition: &Composition, visual: &VisualSlice<'_>) -> Result<()> {
    let mut total_pixel_frames = 0_u128;
    for clip in visual
        .primary
        .iter()
        .copied()
        .chain(visual.overlays.iter().filter_map(|clip| match clip {
            VisualClip::Video(video) => Some(*video),
            VisualClip::Image(_) | VisualClip::Text(_) => None,
        }))
    {
        if clip.playback_mode != PlaybackMode::Reverse {
            continue;
        }
        let source = &composition.sources[&clip.source_id];
        let source_duration_ticks = clip
            .placement
            .source_out_tick
            .checked_sub(clip.placement.source_in_tick)
            .ok_or_else(|| anyhow!("reverse source duration underflow"))?;
        validate_reverse_work(
            source.width,
            source.height,
            source_duration_ticks,
            composition,
            clip.id.as_str(),
            &mut total_pixel_frames,
        )?;
    }
    Ok(())
}

fn validate_reverse_work(
    width: u32,
    height: u32,
    source_duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    if width > MAX_REVERSE_EDGE || height > MAX_REVERSE_EDGE || pixels > MAX_REVERSE_PIXELS {
        bail!("reverse clip {clip_id} exceeds the dimension budget");
    }
    if u128::from(source_duration_ticks) * u128::from(DEFAULT_TIME_BASE)
        > u128::from(MAX_REVERSE_CLIP_TICKS) * u128::from(composition.time_base)
    {
        bail!("reverse clip {clip_id} exceeds the buffered duration budget");
    }
    let denominator = u128::from(composition.time_base) * 1_000;
    let frame_numerator =
        u128::from(source_duration_ticks) * u128::from(composition.canvas.fps_milli);
    let buffered_frames = frame_numerator
        .checked_add(denominator - 1)
        .ok_or_else(|| anyhow!("reverse frame count overflow"))?
        / denominator;
    let work = pixels
        .checked_mul(buffered_frames)
        .ok_or_else(|| anyhow!("reverse work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow!("reverse work overflow"))?;
    if *total_pixel_frames > MAX_REVERSE_PIXEL_FRAMES {
        bail!("composition exceeds the reverse buffering work budget");
    }
    Ok(())
}

fn validate_stabilization_workload(
    composition: &Composition,
    visual: &VisualSlice<'_>,
    transitions: &TransitionPlan<'_>,
) -> Result<()> {
    let mut total_pixel_frames = 0_u128;
    for (index, clip) in visual.primary.iter().enumerate() {
        if !clip.stabilization.is_enabled() {
            continue;
        }
        let handles = transitions.handles[index];
        let timeline_duration_ticks = clip
            .placement
            .timeline_duration_ticks()?
            .checked_add(handles.source_head_ticks)
            .and_then(|value| value.checked_add(handles.source_tail_ticks))
            .ok_or_else(|| anyhow!("stabilization duration overflow"))?;
        let source_duration_ticks = if clip.placement.speed_ramp.is_some() {
            clip.placement.source_span_ticks()
        } else {
            scaled_source_duration_ticks(
                timeline_duration_ticks,
                clip.placement.speed,
                "stabilization",
            )?
        };
        let source = &composition.sources[&clip.source_id];
        validate_stabilization_work(
            source.width,
            source.height,
            source_duration_ticks,
            composition,
            clip.id.as_str(),
            &mut total_pixel_frames,
        )?;
    }
    for clip in &visual.overlays {
        let VisualClip::Video(clip) = clip else {
            continue;
        };
        if !clip.stabilization.is_enabled() {
            continue;
        }
        let source = &composition.sources[&clip.source_id];
        validate_stabilization_work(
            source.width,
            source.height,
            clip.placement.source_out_tick - clip.placement.source_in_tick,
            composition,
            clip.id.as_str(),
            &mut total_pixel_frames,
        )?;
    }
    Ok(())
}

fn scaled_source_duration_ticks(
    timeline_duration_ticks: u64,
    speed: f64,
    label: &str,
) -> Result<u64> {
    let duration = timeline_duration_ticks as f64 * speed;
    if !duration.is_finite() || duration < 1.0 || duration > u64::MAX as f64 {
        bail!("{label} source duration is invalid");
    }
    Ok(duration.ceil() as u64)
}

fn validate_stabilization_work(
    width: u32,
    height: u32,
    source_duration_ticks: u64,
    composition: &Composition,
    clip_id: &str,
    total_pixel_frames: &mut u128,
) -> Result<()> {
    let pixels = u128::from(width) * u128::from(height);
    if width > MAX_STABILIZATION_EDGE
        || height > MAX_STABILIZATION_EDGE
        || pixels > MAX_STABILIZATION_PIXELS
    {
        bail!("stabilization clip {clip_id} exceeds the dimension budget");
    }
    if u128::from(source_duration_ticks) * u128::from(DEFAULT_TIME_BASE)
        > u128::from(MAX_STABILIZATION_CLIP_TICKS) * u128::from(composition.time_base)
    {
        bail!("stabilization clip {clip_id} exceeds the duration budget");
    }
    let denominator = u128::from(composition.time_base) * 1_000;
    let frame_numerator =
        u128::from(source_duration_ticks) * u128::from(composition.canvas.fps_milli);
    let frames = frame_numerator
        .checked_add(denominator - 1)
        .ok_or_else(|| anyhow!("stabilization frame count overflow"))?
        / denominator;
    let work = pixels
        .checked_mul(frames)
        .ok_or_else(|| anyhow!("stabilization work overflow"))?;
    *total_pixel_frames = total_pixel_frames
        .checked_add(work)
        .ok_or_else(|| anyhow!("stabilization work overflow"))?;
    if *total_pixel_frames > MAX_STABILIZATION_PIXEL_FRAMES {
        bail!("composition exceeds the stabilization work budget");
    }
    Ok(())
}

fn validate_speed_ramp_budget(visual: &VisualSlice<'_>, audio: &[&AudioClip]) -> Result<()> {
    let mut segments = 0_usize;
    for placement in visual
        .primary
        .iter()
        .map(|clip| &clip.placement)
        .chain(visual.overlays.iter().filter_map(|clip| match clip {
            VisualClip::Video(clip) => Some(&clip.placement),
            VisualClip::Image(_) | VisualClip::Text(_) => None,
        }))
        .chain(audio.iter().map(|clip| &clip.placement))
    {
        if let Some(ramp) = placement.speed_ramp_segments()? {
            segments = segments
                .checked_add(ramp.len())
                .ok_or_else(|| anyhow!("speed-ramp segment count overflow"))?;
        }
    }
    if segments > MAX_ACTIVE_SPEED_RAMP_SEGMENTS {
        bail!("composition has too many active speed-ramp segments");
    }
    Ok(())
}

fn validate_gapless_video(clips: &[&VideoClip]) -> Result<u64> {
    let mut cursor = 0_u64;
    for clip in clips {
        if clip.placement.timeline_start_tick != cursor {
            bail!(
                "visible video track has a gap or overlap before clip {}",
                clip.id.as_str()
            );
        }
        cursor = clip.placement.timeline_end_tick()?;
    }
    Ok(cursor)
}

fn active_audio_clips(composition: &Composition) -> Vec<&AudioClip> {
    let has_solo = composition.tracks.iter().any(|track| {
        matches!(
            track,
            CompositionTrack::Audio {
                muted: false,
                solo: true,
                ..
            }
        )
    });
    let mut clips = Vec::new();
    for track in &composition.tracks {
        if let CompositionTrack::Audio {
            muted,
            solo,
            clips: track_clips,
            ..
        } = track
        {
            if *muted || (has_solo && !*solo) {
                continue;
            }
            clips.extend(track_clips.iter().filter(|clip| {
                clip.enabled
                    && clip
                        .placement
                        .speed_ramp
                        .as_ref()
                        .is_none_or(|ramp| ramp.audio_policy != SpeedRampAudioPolicy::Mute)
            }));
        }
    }
    clips
}

#[derive(Debug, Clone)]
struct CompiledAudioAutomation {
    gain: CompiledValue,
    pan: CompiledValue,
}

#[derive(Debug, Clone)]
struct CompiledAudioClip<'a> {
    clip: &'a AudioClip,
    automation: CompiledAudioAutomation,
}

fn compile_primary_source_audio(
    composition: &Composition,
    clips: &[&VideoClip],
    track_muted: bool,
    keyframe_points: &mut usize,
) -> Result<Vec<Option<CompiledAudioAutomation>>> {
    clips
        .iter()
        .map(|clip| {
            let source = composition
                .sources
                .get(&clip.source_id)
                .expect("composition validation checked the source");
            if track_muted
                || !clip.source_audio_enabled
                || !source.has_audio
                || matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                || clip
                    .placement
                    .speed_ramp
                    .as_ref()
                    .is_some_and(|ramp| ramp.audio_policy == SpeedRampAudioPolicy::Mute)
            {
                return Ok(None);
            }
            let duration_ticks = clip.placement.timeline_duration_ticks()?;
            compile_audio_automation(
                &clip.audio_gain,
                &clip.audio_pan,
                duration_ticks,
                composition.time_base,
                clip.id.as_str(),
                "source audio",
                keyframe_points,
            )
            .map(Some)
        })
        .collect()
}

fn compile_audio_slice<'a>(
    clips: &[&'a AudioClip],
    video_duration_ticks: u64,
    time_base: u32,
    keyframe_points: &mut usize,
) -> Result<Vec<CompiledAudioClip<'a>>> {
    let mut compiled = Vec::with_capacity(clips.len());
    for &clip in clips {
        if clip.placement.timeline_end_tick()? > video_duration_ticks {
            bail!(
                "audio clip {} extends beyond the primary video track",
                clip.id.as_str()
            );
        }
        let duration_ticks = clip.placement.timeline_duration_ticks()?;
        compiled.push(CompiledAudioClip {
            clip,
            automation: compile_audio_automation(
                &clip.gain,
                &clip.pan,
                duration_ticks,
                time_base,
                clip.id.as_str(),
                "audio",
                keyframe_points,
            )?,
        });
    }
    Ok(compiled)
}

#[allow(clippy::too_many_arguments)]
fn compile_audio_automation(
    gain: &AnimatableValue,
    pan: &AnimatableValue,
    duration_ticks: u64,
    time_base: u32,
    clip_id: &str,
    label: &str,
    keyframe_points: &mut usize,
) -> Result<CompiledAudioAutomation> {
    Ok(CompiledAudioAutomation {
        gain: compile_animatable(
            gain,
            &format!("{label} gain for clip {clip_id}"),
            "t",
            0.0,
            duration_ticks,
            time_base,
            0.0,
            MAX_AUDIO_GAIN,
            true,
            keyframe_points,
        )?,
        pan: compile_animatable(
            pan,
            &format!("{label} pan for clip {clip_id}"),
            "t",
            0.0,
            duration_ticks,
            time_base,
            -1.0,
            1.0,
            true,
            keyframe_points,
        )?,
    })
}

#[derive(Debug)]
struct AudioFilterOptions<'a> {
    automation: &'a CompiledAudioAutomation,
    timeline_start_tick: Option<u64>,
    fade_in_ticks: u64,
    fade_out_ticks: u64,
}

#[allow(clippy::too_many_arguments)]
fn media_audio_filter(
    input_label: &str,
    placement: &ClipPlacement,
    duration_ticks: u64,
    time_base: u32,
    output_label: &str,
    reverse: bool,
    options: Option<AudioFilterOptions<'_>>,
) -> Result<String> {
    if placement.speed_ramp.is_some() {
        return speed_ramp_audio_filter(
            input_label,
            placement,
            duration_ticks,
            time_base,
            output_label,
            reverse,
            options,
        );
    }
    let duration = seconds(duration_ticks, time_base);
    let mut filters = vec![
        format!(
            "atrim=start={}:end={}",
            seconds(placement.source_in_tick, time_base),
            seconds(placement.source_out_tick, time_base)
        ),
        "asetpts=PTS-STARTPTS".to_owned(),
    ];
    if reverse {
        filters.push("areverse".to_owned());
    }
    filters.extend(atempo_filters(placement.speed)?);
    filters.extend([
        format!("aresample={AUDIO_SAMPLE_RATE}:async=1:first_pts=0"),
        format!("aformat=sample_fmts=fltp:sample_rates={AUDIO_SAMPLE_RATE}:channel_layouts=stereo"),
        format!("atrim=duration={duration}"),
        "asetpts=PTS-STARTPTS".to_owned(),
    ]);

    append_audio_post_filters(&mut filters, options, duration_ticks, time_base)?;

    Ok(format!(
        "[{input_label}]{}[{output_label}]",
        filters.join(",")
    ))
}

#[allow(clippy::too_many_arguments)]
fn speed_ramp_audio_filter(
    input_label: &str,
    placement: &ClipPlacement,
    duration_ticks: u64,
    time_base: u32,
    output_label: &str,
    reverse: bool,
    options: Option<AudioFilterOptions<'_>>,
) -> Result<String> {
    let ramp = placement
        .speed_ramp
        .as_ref()
        .expect("speed-ramp audio path has a ramp");
    if ramp.audio_policy != SpeedRampAudioPolicy::PreservePitch {
        bail!("muted speed-ramp audio must compile as timeline silence");
    }
    let segments = placement
        .speed_ramp_segments()?
        .expect("speed-ramp audio path has validated segments");
    if segments.last().map(|segment| segment.timeline_end_tick) != Some(duration_ticks) {
        bail!("speed-ramp audio duration disagrees with clip placement");
    }

    let split_inputs = (0..segments.len())
        .map(|index| format!("[{output_label}_ramp_in_{index}]"))
        .collect::<String>();
    let mut input_filters = vec![
        format!(
            "atrim=start={}:end={}",
            seconds(placement.source_in_tick, time_base),
            seconds(placement.source_out_tick, time_base)
        ),
        "asetpts=PTS-STARTPTS".to_owned(),
    ];
    if reverse {
        input_filters.push("areverse".to_owned());
    }
    input_filters.push(format!("asplit={}", segments.len()));
    let mut graph = vec![format!(
        "[{input_label}]{}{split_inputs}",
        input_filters.join(",")
    )];

    let mut concat_inputs = String::new();
    for (index, segment) in segments.iter().enumerate() {
        let source_ticks = segment.source_end_tick - segment.source_start_tick;
        let output_ticks = segment.timeline_end_tick - segment.timeline_start_tick;
        let effective_speed = source_ticks as f64 / output_ticks as f64;
        let mut filters = vec![
            format!(
                "atrim=start={}:end={}",
                seconds(segment.source_start_tick, time_base),
                seconds(segment.source_end_tick, time_base)
            ),
            "asetpts=PTS-STARTPTS".to_owned(),
        ];
        filters.extend(atempo_filters_with_rounding_tolerance(effective_speed)?);
        filters.extend([
            format!("aresample={AUDIO_SAMPLE_RATE}:async=1:first_pts=0"),
            format!(
                "aformat=sample_fmts=fltp:sample_rates={AUDIO_SAMPLE_RATE}:channel_layouts=stereo"
            ),
            format!("atrim=duration={}", seconds(output_ticks, time_base)),
            "asetpts=PTS-STARTPTS".to_owned(),
        ]);
        let segment_label = format!("{output_label}_ramp_{index}");
        graph.push(format!(
            "[{output_label}_ramp_in_{index}]{}[{segment_label}]",
            filters.join(",")
        ));
        concat_inputs.push_str(&format!("[{segment_label}]"));
    }

    let mut final_filters = vec![
        format!("atrim=duration={}", seconds(duration_ticks, time_base)),
        "asetpts=PTS-STARTPTS".to_owned(),
    ];
    append_audio_post_filters(&mut final_filters, options, duration_ticks, time_base)?;
    graph.push(format!(
        "{concat_inputs}concat=n={}:v=0:a=1,{}[{output_label}]",
        segments.len(),
        final_filters.join(",")
    ));
    Ok(graph.join(";"))
}

fn append_audio_post_filters(
    filters: &mut Vec<String>,
    options: Option<AudioFilterOptions<'_>>,
    duration_ticks: u64,
    time_base: u32,
) -> Result<()> {
    let Some(options) = options else {
        return Ok(());
    };
    append_audio_automation(filters, options.automation)?;
    if options.fade_in_ticks > 0 {
        filters.push(format!(
            "afade=t=in:st=0:d={}",
            seconds(options.fade_in_ticks, time_base)
        ));
    }
    if options.fade_out_ticks > 0 {
        let fade_start = duration_ticks
            .checked_sub(options.fade_out_ticks)
            .ok_or_else(|| anyhow!("audio fade exceeds clip duration"))?;
        filters.push(format!(
            "afade=t=out:st={}:d={}",
            seconds(fade_start, time_base),
            seconds(options.fade_out_ticks, time_base)
        ));
    }
    if let Some(timeline_start_tick) = options.timeline_start_tick {
        filters.push(format!(
            "adelay={}S:all=1",
            ticks_to_audio_samples(timeline_start_tick, time_base)
        ));
    }
    Ok(())
}

fn append_audio_automation(
    filters: &mut Vec<String>,
    automation: &CompiledAudioAutomation,
) -> Result<()> {
    if let Some(gain) = automation.gain.constant {
        filters.push(format!("volume={}", decimal(gain)));
    } else {
        bounded_expression(&automation.gain.expression, "audio gain")?;
        filters.push(format!(
            "volume=volume='{}':eval=frame:precision=double",
            automation.gain.expression
        ));
    }

    if let Some(pan) = automation.pan.constant {
        if pan != 0.0 {
            let left = if pan > 0.0 { 1.0 - pan } else { 1.0 };
            let right = if pan < 0.0 { 1.0 + pan } else { 1.0 };
            filters.push(format!(
                "pan=stereo|c0={}*c0|c1={}*c1",
                decimal(left),
                decimal(right)
            ));
        }
    } else {
        let pan = &automation.pan.expression;
        let expression = format!("val(0)*(1-max(({pan}),0))|val(1)*(1+min(({pan}),0))");
        bounded_expression(&expression, "audio pan")?;
        filters.push(format!("aeval=exprs='{expression}':channel_layout=stereo"));
    }
    Ok(())
}

fn atempo_filters(speed: f64) -> Result<Vec<String>> {
    if !speed.is_finite() || !(0.05..=16.0).contains(&speed) {
        bail!("audio speed must be in 0.05..=16");
    }
    Ok(build_atempo_filters(speed))
}

fn atempo_filters_with_rounding_tolerance(speed: f64) -> Result<Vec<String>> {
    if !speed.is_finite() || !(0.049..=16.01).contains(&speed) {
        bail!("rounded speed-ramp audio tempo exceeds 0.05..=16 tolerance");
    }
    Ok(build_atempo_filters(speed))
}

fn build_atempo_filters(speed: f64) -> Vec<String> {
    let mut filters = Vec::new();
    let mut remaining = speed;
    while remaining > 2.0 + f64::EPSILON {
        filters.push("atempo=2.000000".to_owned());
        remaining /= 2.0;
    }
    while remaining < 0.5 - f64::EPSILON {
        filters.push("atempo=0.500000".to_owned());
        remaining /= 0.5;
    }
    if (remaining - 1.0).abs() > f64::EPSILON {
        filters.push(format!("atempo={}", decimal(remaining)));
    }
    filters
}

#[derive(Debug, Default)]
struct SourcePads {
    video: VecDeque<String>,
    audio: VecDeque<String>,
}

#[derive(Debug, Default)]
struct SourcePadPools(BTreeMap<SourceId, SourcePads>);

impl SourcePadPools {
    fn take_video(&mut self, source_id: &SourceId) -> Result<String> {
        self.0
            .get_mut(source_id)
            .and_then(|pads| pads.video.pop_front())
            .ok_or_else(|| anyhow!("missing video pad for source {}", source_id.as_str()))
    }

    fn take_audio(&mut self, source_id: &SourceId) -> Result<String> {
        self.0
            .get_mut(source_id)
            .and_then(|pads| pads.audio.pop_front())
            .ok_or_else(|| anyhow!("missing audio pad for source {}", source_id.as_str()))
    }
}

fn build_source_pads(
    input_indexes: &BTreeMap<SourceId, usize>,
    video_uses: &BTreeMap<SourceId, usize>,
    audio_uses: &BTreeMap<SourceId, usize>,
    graph: &mut Vec<String>,
) -> SourcePadPools {
    let mut pools = BTreeMap::new();
    for (source_id, input_index) in input_indexes {
        let video = build_media_pads(
            *input_index,
            "v",
            "split",
            *video_uses.get(source_id).unwrap_or(&0),
            graph,
        );
        let audio = build_media_pads(
            *input_index,
            "a",
            "asplit",
            *audio_uses.get(source_id).unwrap_or(&0),
            graph,
        );
        pools.insert(source_id.clone(), SourcePads { video, audio });
    }
    SourcePadPools(pools)
}

fn build_media_pads(
    input_index: usize,
    stream: &str,
    split_filter: &str,
    uses: usize,
    graph: &mut Vec<String>,
) -> VecDeque<String> {
    if uses == 0 {
        return VecDeque::new();
    }
    if uses == 1 {
        return VecDeque::from([format!("{input_index}:{stream}")]);
    }
    let labels: Vec<_> = (0..uses)
        .map(|use_index| format!("input_{input_index}_{stream}_{use_index}"))
        .collect();
    graph.push(format!(
        "[{input_index}:{stream}]{split_filter}={uses}{}",
        labels
            .iter()
            .map(|label| format!("[{label}]"))
            .collect::<String>()
    ));
    labels.into()
}

fn increment(counts: &mut BTreeMap<SourceId, usize>, source_id: &SourceId) {
    *counts.entry(source_id.clone()).or_default() += 1;
}

fn ticks_to_audio_samples(ticks: u64, time_base: u32) -> u64 {
    let numerator = ticks as u128 * AUDIO_SAMPLE_RATE as u128;
    let rounded = (numerator + (time_base as u128 / 2)) / time_base as u128;
    rounded.min(u64::MAX as u128) as u64
}

fn seconds(ticks: u64, time_base: u32) -> String {
    decimal(ticks as f64 / time_base as f64)
}

fn fractional_seconds(ticks: f64, time_base: u32) -> String {
    decimal(ticks / time_base as f64)
}

fn decimal(value: f64) -> String {
    format!("{value:.6}")
}

fn ffmpeg_rgb(red: f64, green: f64, blue: f64) -> String {
    let byte = |value: f64| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("0x{:02X}{:02X}{:02X}", byte(red), byte(green), byte(blue))
}

fn ffmpeg_rgba(color: crate::domain::composition::Rgba) -> String {
    format!(
        "{}@{}",
        ffmpeg_rgb(color.red, color.green, color.blue),
        decimal(color.alpha)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{
        AudioClip, CanvasSpec, ClipPlacement, ClipTransition, CompositionClipId, CompositionSource,
        ImageClip, MaskShape, Rgba, SourceKind, TextClip, TextStyle, TrackId, TransformSpec,
        TransitionId,
    };
    use crate::domain::keyframes::{Interpolation, Keyframe, KeyframeTrack};
    use crate::ports::CompositionProResProfile;

    fn id(value: &str) -> SourceId {
        SourceId::parse(value).unwrap()
    }

    fn source(value: &str, kind: SourceKind, has_audio: bool) -> CompositionSource {
        CompositionSource {
            id: id(value),
            kind,
            duration_ticks: 10_000_000,
            width: if kind == SourceKind::Audio { 0 } else { 1_280 },
            height: if kind == SourceKind::Audio { 0 } else { 720 },
            has_audio,
        }
    }

    fn placement(start: u64, source_in: u64, source_out: u64) -> ClipPlacement {
        ClipPlacement {
            timeline_start_tick: start,
            source_in_tick: source_in,
            source_out_tick: source_out,
            speed: 1.0,
            speed_ramp: None,
        }
    }

    fn video_clip(value: &str, source: &str, placement: ClipPlacement) -> VideoClip {
        VideoClip {
            id: CompositionClipId::parse(value).unwrap(),
            source_id: id(source),
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

    fn audio_clip(value: &str, source: &str, placement: ClipPlacement) -> AudioClip {
        AudioClip {
            id: CompositionClipId::parse(value).unwrap(),
            source_id: id(source),
            placement,
            gain: AnimatableValue::constant(0.5),
            pan: AnimatableValue::constant(0.0),
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            enabled: true,
        }
    }

    fn speed_ramp(
        interpolation: SpeedRampInterpolation,
        points: &[(u64, f64)],
        audio_policy: SpeedRampAudioPolicy,
    ) -> crate::domain::composition::SpeedRampSpec {
        crate::domain::composition::SpeedRampSpec {
            interpolation,
            points: points
                .iter()
                .map(
                    |(source_progress_tick, speed)| crate::domain::composition::SpeedRampPoint {
                        source_progress_tick: *source_progress_tick,
                        speed: *speed,
                    },
                )
                .collect(),
            audio_policy,
        }
    }

    fn first_slice_composition() -> Composition {
        let mut composition = Composition::new(CanvasSpec::default());
        for source in [
            source("video-a", SourceKind::Video, true),
            source("video-b", SourceKind::Video, false),
            source("music", SourceKind::Audio, true),
        ] {
            composition.sources.insert(source.id.clone(), source);
        }
        composition.tracks = vec![
            CompositionTrack::Video {
                id: TrackId::parse("video-main").unwrap(),
                name: "Video".to_owned(),
                hidden: false,
                muted: false,
                locked: false,
                clips: vec![
                    video_clip("clip-a", "video-a", placement(0, 1_000_000, 3_000_000)),
                    video_clip("clip-b", "video-b", placement(2_000_000, 0, 3_000_000)),
                ],
                transitions: Vec::new(),
            },
            CompositionTrack::Audio {
                id: TrackId::parse("music-track").unwrap(),
                name: "Music".to_owned(),
                muted: false,
                solo: false,
                locked: false,
                clips: vec![audio_clip(
                    "music-clip",
                    "music",
                    placement(1_000_000, 0, 4_000_000),
                )],
            },
        ];
        composition
    }

    fn inputs() -> BTreeMap<SourceId, PathBuf> {
        BTreeMap::from([
            (id("video-a"), PathBuf::from("/media/a.mp4")),
            (id("video-b"), PathBuf::from("/media/b.mp4")),
            (id("music"), PathBuf::from("/media/music.wav")),
        ])
    }

    fn layered_composition(image_blend: BlendMode) -> Composition {
        let mut composition = first_slice_composition();
        for source in [
            source("overlay", SourceKind::Video, false),
            source("image", SourceKind::Image, false),
        ] {
            composition.sources.insert(source.id.clone(), source);
        }

        let mut overlay = video_clip("overlay-clip", "overlay", placement(500_000, 0, 1_000_000));
        overlay.transform.x = AnimatableValue::constant(12.0);
        overlay.transform.y = AnimatableValue::constant(-8.0);
        overlay.transform.scale_x = AnimatableValue::constant(0.5);
        overlay.transform.scale_y = AnimatableValue::constant(0.75);
        overlay.transform.rotation_degrees = AnimatableValue::constant(15.0);
        overlay.opacity = AnimatableValue::constant(0.6);
        overlay.blend_mode = BlendMode::Multiply;
        overlay.effects.push(VideoEffect::ChromaKey {
            color: Rgba {
                red: 0.0,
                green: 1.0,
                blue: 0.0,
                alpha: 1.0,
            },
            similarity: 0.12,
            softness: 0.04,
            spill: 0.25,
        });

        let image_transform = TransformSpec {
            x: AnimatableValue::constant(-20.0),
            scale_x: AnimatableValue::constant(0.25),
            scale_y: AnimatableValue::constant(0.25),
            ..TransformSpec::default()
        };
        let image = ImageClip {
            id: CompositionClipId::parse("image-clip").unwrap(),
            source_id: id("image"),
            timeline_start_tick: 2_000_000,
            duration_ticks: 1_000_000,
            transform: image_transform,
            opacity: AnimatableValue::constant(0.8),
            blend_mode: image_blend,
            enabled: true,
        };

        composition.tracks.insert(
            0,
            CompositionTrack::Image {
                id: TrackId::parse("image-layer").unwrap(),
                name: "Image".to_owned(),
                hidden: false,
                locked: false,
                clips: vec![image],
            },
        );
        composition.tracks.insert(
            1,
            CompositionTrack::Video {
                id: TrackId::parse("video-overlay").unwrap(),
                name: "Overlay".to_owned(),
                hidden: false,
                muted: false,
                locked: false,
                clips: vec![overlay],
                transitions: Vec::new(),
            },
        );
        composition
    }

    fn layered_inputs() -> BTreeMap<SourceId, PathBuf> {
        let mut resolved = inputs();
        resolved.insert(id("overlay"), PathBuf::from("/media/overlay.mp4"));
        resolved.insert(id("image"), PathBuf::from("/media/logo.png"));
        resolved
    }

    fn text_transition_composition(kind: TransitionKind) -> Composition {
        let mut composition = first_slice_composition();
        let CompositionTrack::Video {
            clips, transitions, ..
        } = &mut composition.tracks[0]
        else {
            unreachable!()
        };
        clips[1].placement.source_in_tick = 1_000_000;
        clips[1].placement.source_out_tick = 4_000_000;
        transitions.push(ClipTransition {
            id: TransitionId::parse("transition-a-b").unwrap(),
            from_clip_id: clips[0].id.clone(),
            to_clip_id: clips[1].id.clone(),
            duration_ticks: 500_000,
            kind,
        });
        let transform = TransformSpec {
            x: AnimatableValue::constant(24.0),
            y: AnimatableValue::constant(-16.0),
            scale_x: AnimatableValue::constant(0.75),
            scale_y: AnimatableValue::constant(0.75),
            rotation_degrees: AnimatableValue::constant(5.0),
            ..TransformSpec::default()
        };
        composition.tracks.insert(
            0,
            CompositionTrack::Text {
                id: TrackId::parse("title-track").unwrap(),
                name: "Title".to_owned(),
                hidden: false,
                locked: false,
                clips: vec![TextClip {
                    id: CompositionClipId::parse("title-clip").unwrap(),
                    timeline_start_tick: 500_000,
                    timeline_end_tick: 1_500_000,
                    text: "Привет, мир!\n字幕".into(),
                    style: TextStyle {
                        font_family: "Noto Sans".into(),
                        font_size: 56.0,
                        color: Rgba {
                            red: 1.0,
                            green: 0.8,
                            blue: 0.2,
                            alpha: 1.0,
                        },
                        background: Rgba {
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            alpha: 0.5,
                        },
                        stroke: Rgba::BLACK,
                        stroke_width: 2.0,
                        shadow: Rgba {
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                            alpha: 0.75,
                        },
                        shadow_x: 3.0,
                        shadow_y: 4.0,
                    },
                    transform,
                    opacity: AnimatableValue::constant(0.8),
                    enabled: true,
                }],
            },
        );
        composition
    }

    fn text_resources() -> BTreeMap<CompositionClipId, CompositionTextResource> {
        BTreeMap::from([(
            CompositionClipId::parse("title-clip").unwrap(),
            CompositionTextResource {
                text_file: PathBuf::from("/tmp/title:one.txt"),
                font_file: PathBuf::from("/fonts/Noto Sans.ttf"),
            },
        )])
    }

    fn animated(
        interpolation: Interpolation,
        end_tick: u64,
        start: f64,
        end: f64,
    ) -> AnimatableValue {
        AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                interpolation,
                vec![
                    Keyframe {
                        tick: 0,
                        value: start,
                    },
                    Keyframe {
                        tick: end_tick,
                        value: end,
                    },
                ],
            )
            .unwrap(),
        }
    }

    #[test]
    fn compiles_multi_input_concat_silence_and_audio_mix_golden() {
        let command = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &first_slice_composition(),
            21,
            3,
        )
        .unwrap();

        assert_eq!(command.expected_duration_seconds, 5.0);
        assert!(command.read_only_files.is_empty());
        assert_eq!(
            &command.arguments[..7],
            [
                "-y",
                "-i",
                "/media/music.wav",
                "-i",
                "/media/a.mp4",
                "-i",
                "/media/b.mp4",
            ]
        );
        let graph_index = command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap();
        let graph = &command.arguments[graph_index + 1];
        let expected = concat!(
            "[1:v]trim=start=1.000000:end=3.000000,setpts=(PTS-STARTPTS)/1.000000,",
            "scale=1920:1080:force_original_aspect_ratio=decrease,",
            "pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x000000,",
            "setsar=1,fps=fps=30000/1000,format=pix_fmts=yuv420p,settb=AVTB[video_clip_0];",
            "[1:a]atrim=start=1.000000:end=3.000000,asetpts=PTS-STARTPTS,",
            "aresample=48000:async=1:first_pts=0,",
            "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,",
            "atrim=duration=2.000000,asetpts=PTS-STARTPTS,",
            "volume=1.000000[source_audio_0];",
            "[2:v]trim=start=0.000000:end=3.000000,setpts=(PTS-STARTPTS)/1.000000,",
            "scale=1920:1080:force_original_aspect_ratio=decrease,",
            "pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=0x000000,",
            "setsar=1,fps=fps=30000/1000,format=pix_fmts=yuv420p,settb=AVTB[video_clip_1];",
            "anullsrc=channel_layout=stereo:sample_rate=48000,",
            "atrim=duration=3.000000,asetpts=PTS-STARTPTS[source_audio_1];",
            "[video_clip_0][source_audio_0][video_clip_1][source_audio_1]",
            "concat=n=2:v=1:a=1[vout][source_audio];",
            "[0:a]atrim=start=0.000000:end=4.000000,asetpts=PTS-STARTPTS,",
            "aresample=48000:async=1:first_pts=0,",
            "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,",
            "atrim=duration=4.000000,asetpts=PTS-STARTPTS,volume=0.500000,",
            "adelay=48000S:all=1[audio_clip_0];",
            "[source_audio][audio_clip_0]",
            "amix=inputs=2:duration=first:dropout_transition=0:normalize=0,",
            "alimiter=limit=0.950000,atrim=duration=5.000000,",
            "asetpts=PTS-STARTPTS[aout]"
        );
        assert_eq!(graph, expected);
        assert_eq!(
            &command.arguments[graph_index + 2..],
            [
                "-map",
                "[vout]",
                "-map",
                "[aout]",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "21",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-filter_complex_threads",
                "3",
                "-threads:v",
                "3",
                "-t",
                "5.000000",
                "-movflags",
                "+faststart",
                "/renders/result.mp4",
            ]
        );
    }

    fn delivery_tail(
        profile: CompositionExportProfile,
        video_quality: u32,
        av1_encoder: Option<CompositionAv1Encoder>,
        destination: &str,
    ) -> Vec<String> {
        let command = build_composition_ffmpeg_command_for_output(
            &inputs(),
            Path::new(destination),
            &first_slice_composition(),
            CompositionExportSpec {
                profile,
                video_quality,
                av1_encoder,
            },
            3,
        )
        .unwrap();
        let start = command
            .arguments
            .iter()
            .position(|argument| argument == "-c:v")
            .unwrap();
        command.arguments[start..].to_vec()
    }

    fn expected_delivery_tail(
        video: &[&str],
        audio: &[&str],
        faststart: bool,
        destination: &str,
    ) -> Vec<String> {
        let mut expected = video
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        expected.extend(audio.iter().map(|value| (*value).to_owned()));
        expected.extend(
            [
                "-ar",
                "48000",
                "-ac",
                "2",
                "-filter_complex_threads",
                "3",
                "-threads:v",
                "3",
                "-t",
                "5.000000",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        if faststart {
            expected.extend(["-movflags".to_owned(), "+faststart".to_owned()]);
        }
        expected.push(destination.to_owned());
        expected
    }

    #[test]
    fn compiles_exact_delivery_profile_arguments_and_rejects_mismatched_contracts() {
        let h265 = CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H265,
        };
        assert_eq!(
            delivery_tail(h265, 25, None, "/renders/h265.mp4"),
            expected_delivery_tail(
                &[
                    "-c:v", "libx265", "-preset", "veryfast", "-crf", "25", "-tag:v", "hvc1",
                    "-pix_fmt", "yuv420p",
                ],
                &["-c:a", "aac", "-b:a", "192k"],
                true,
                "/renders/h265.mp4",
            )
        );

        let vp9 = CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Vp9,
        };
        assert_eq!(
            delivery_tail(vp9, 32, None, "/renders/vp9.webm"),
            expected_delivery_tail(
                &[
                    "-c:v",
                    "libvpx-vp9",
                    "-deadline",
                    "good",
                    "-cpu-used",
                    "2",
                    "-crf",
                    "32",
                    "-b:v",
                    "0",
                    "-row-mt",
                    "1",
                    "-pix_fmt",
                    "yuv420p",
                ],
                &["-c:a", "libopus", "-b:a", "192k"],
                false,
                "/renders/vp9.webm",
            )
        );

        let av1 = CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Av1,
        };
        assert_eq!(
            delivery_tail(
                av1,
                32,
                Some(CompositionAv1Encoder::LibSvtAv1),
                "/renders/svt.webm",
            ),
            expected_delivery_tail(
                &[
                    "-c:v",
                    "libsvtav1",
                    "-preset",
                    "8",
                    "-crf",
                    "32",
                    "-b:v",
                    "0",
                    "-pix_fmt",
                    "yuv420p",
                ],
                &["-c:a", "libopus", "-b:a", "192k"],
                false,
                "/renders/svt.webm",
            )
        );
        assert_eq!(
            delivery_tail(
                av1,
                32,
                Some(CompositionAv1Encoder::LibAomAv1),
                "/renders/aom.webm",
            ),
            expected_delivery_tail(
                &[
                    "-c:v",
                    "libaom-av1",
                    "-cpu-used",
                    "6",
                    "-crf",
                    "32",
                    "-b:v",
                    "0",
                    "-pix_fmt",
                    "yuv420p",
                ],
                &["-c:a", "libopus", "-b:a", "192k"],
                false,
                "/renders/aom.webm",
            )
        );

        for (profile, ffmpeg_profile) in [
            (CompositionProResProfile::Proxy, "0"),
            (CompositionProResProfile::Lt, "1"),
            (CompositionProResProfile::Standard, "2"),
            (CompositionProResProfile::Hq, "3"),
        ] {
            let destination = format!("/renders/prores-{ffmpeg_profile}.mov");
            assert_eq!(
                delivery_tail(
                    CompositionExportProfile::Mov { profile },
                    9,
                    None,
                    &destination,
                ),
                expected_delivery_tail(
                    &[
                        "-c:v",
                        "prores_ks",
                        "-profile:v",
                        ffmpeg_profile,
                        "-qscale:v",
                        "9",
                        "-pix_fmt",
                        "yuv422p10le",
                    ],
                    &["-c:a", "pcm_s16le"],
                    false,
                    &destination,
                )
            );
        }

        let unresolved = build_composition_ffmpeg_command_for_output(
            &inputs(),
            Path::new("/renders/unresolved.webm"),
            &first_slice_composition(),
            CompositionExportSpec {
                profile: av1,
                video_quality: 32,
                av1_encoder: None,
            },
            1,
        )
        .unwrap_err();
        assert!(unresolved.to_string().contains("resolved encoder"));
        let mismatched_extension = build_composition_ffmpeg_command_for_output(
            &inputs(),
            Path::new("/renders/wrong.mp4"),
            &first_slice_composition(),
            CompositionExportSpec {
                profile: vp9,
                video_quality: 32,
                av1_encoder: None,
            },
            1,
        )
        .unwrap_err();
        assert!(mismatched_extension.to_string().contains("requires .webm"));
    }

    #[test]
    fn compiles_primary_and_overlay_optical_flow_with_exact_clip_local_timing() {
        let mut composition = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        let CompositionTrack::Video {
            clips, transitions, ..
        } = &mut composition.tracks[2]
        else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        clips[1].placement.timeline_start_tick = 4_000_000;
        clips[1].placement.source_in_tick = 500_000;
        clips[1].placement.source_out_tick = 3_500_000;
        transitions.push(ClipTransition {
            id: TransitionId::parse("optical-transition").unwrap(),
            from_clip_id: clips[0].id.clone(),
            to_clip_id: clips[1].id.clone(),
            duration_ticks: 500_000,
            kind: TransitionKind::Dissolve,
        });

        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/optical-flow.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert_eq!(
            graph.matches("minterpolate=fps=30000/1000").count(),
            2,
            "{graph}"
        );
        assert!(
            graph.contains(
                "setpts=(PTS-STARTPTS)/0.500000,scale=1920:1080:\
                 force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:\
                 color=0x000000,setsar=1,tpad=stop_mode=clone:stop_duration=4.250000,\
                 minterpolate=fps=30000/1000"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "trim=start=0.000000:end=1.000000,settb=AVTB,\
                 setpts=(PTS-STARTPTS)/0.500000,tpad=stop_mode=clone:\
                 stop_duration=2.000000,minterpolate=fps=30000/1000"
            ),
            "{graph}"
        );
        assert!(graph.contains("trim=duration=4.250000,setpts=PTS-STARTPTS"));
        assert!(graph.contains("trim=duration=2.000000,setpts=PTS-STARTPTS"));
        assert!(graph.contains("xfade=transition=fade:duration=0.500000:offset=3.750000"));

        let ordinary = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/ordinary.mp4"),
            &first_slice_composition(),
            23,
            1,
        )
        .unwrap();
        let ordinary_graph = &ordinary.arguments[ordinary
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(!ordinary_graph.contains("minterpolate"), "{ordinary_graph}");
        assert!(!ordinary_graph.contains("tpad="), "{ordinary_graph}");
    }

    #[test]
    fn compiles_primary_overlay_reverse_and_audio_speed_ramps_with_exact_boundaries() {
        let mut composition = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[2] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        clips[0].placement.speed_ramp = Some(speed_ramp(
            SpeedRampInterpolation::Hold,
            &[(0, 1.0), (1_000_000, 2.0), (2_000_000, 2.0)],
            SpeedRampAudioPolicy::PreservePitch,
        ));
        clips[1].placement.timeline_start_tick = 1_500_000;

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].placement.speed_ramp = Some(speed_ramp(
            SpeedRampInterpolation::Linear,
            &[(0, 0.5), (1_000_000, 2.0)],
            SpeedRampAudioPolicy::Mute,
        ));

        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[3] else {
            unreachable!();
        };
        clips[0].placement.speed_ramp = Some(speed_ramp(
            SpeedRampInterpolation::Hold,
            &[(0, 1.0), (2_000_000, 2.0), (4_000_000, 2.0)],
            SpeedRampAudioPolicy::PreservePitch,
        ));

        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/speed-ramp.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        assert_eq!(command.expected_duration_seconds, 4.5);
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "trim=start=1.000000:end=3.000000,fps=fps=30000/1000,reverse,\
             setpts='(if(lt(((PTS-STARTPTS)*TB),1.000000)"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("(0.000000+1.000000*((((PTS-STARTPTS)*TB)-0.000000)/1.000000))"),
            "{graph}"
        );
        assert!(graph.contains("(1.000000+0.500000*"), "{graph}");
        assert!(graph.contains("1.500000)))/TB'"), "{graph}");
        assert!(
            graph.contains(
                "atrim=start=1.000000:end=3.000000,asetpts=PTS-STARTPTS,areverse,\
             asplit=2[source_audio_0_ramp_in_0][source_audio_0_ramp_in_1]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "[source_audio_0_ramp_in_1]atrim=start=1.000000:end=2.000000,\
             asetpts=PTS-STARTPTS,atempo=2.000000,\
             aresample=48000:async=1:first_pts=0"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "[source_audio_0_ramp_0][source_audio_0_ramp_1]concat=n=2:v=0:a=1,\
             atrim=duration=1.500000,asetpts=PTS-STARTPTS,volume=1.000000[source_audio_0]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("log((0.500000000000+(1.500000000000)"),
            "{graph}"
        );
        assert!(
            graph.contains(
                "atrim=start=0.000000:end=4.000000,asetpts=PTS-STARTPTS,\
             asplit=2[audio_clip_0_ramp_in_0][audio_clip_0_ramp_in_1]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "[audio_clip_0_ramp_0][audio_clip_0_ramp_1]concat=n=2:v=0:a=1,\
             atrim=duration=3.000000,asetpts=PTS-STARTPTS,volume=0.500000,\
             adelay=48000S:all=1[audio_clip_0]"
            ),
            "{graph}"
        );
    }

    #[test]
    fn muted_speed_ramp_audio_compiles_timeline_silence_without_resolving_audio_source() {
        let mut composition = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed_ramp = Some(speed_ramp(
            SpeedRampInterpolation::Hold,
            &[(0, 1.0), (2_000_000, 1.0)],
            SpeedRampAudioPolicy::Mute,
        ));
        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].placement.speed_ramp = Some(speed_ramp(
            SpeedRampInterpolation::Hold,
            &[(0, 1.0), (4_000_000, 1.0)],
            SpeedRampAudioPolicy::Mute,
        ));
        let mut resolved = inputs();
        resolved.remove(&id("music"));
        let command = build_composition_ffmpeg_command(
            &resolved,
            Path::new("/renders/muted-ramp.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        assert!(!command
            .arguments
            .iter()
            .any(|argument| argument == "/media/music.wav"));
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(graph.contains(
            "anullsrc=channel_layout=stereo:sample_rate=48000,\
             atrim=duration=2.000000,asetpts=PTS-STARTPTS[source_audio_0]"
        ));
        assert!(!graph.contains("audio_clip_0"));
    }

    #[test]
    fn rejects_more_than_the_active_speed_ramp_segment_budget() {
        let mut composition = first_slice_composition();
        let points = (0_u64..32)
            .map(|index| (index * 1_000, 1.0))
            .collect::<Vec<_>>();
        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips.clear();
        for index in 0_u64..9 {
            let mut clip = audio_clip(
                &format!("ramp-budget-{index}"),
                "music",
                placement(index * 31_000, 0, 31_000),
            );
            clip.placement.speed_ramp = Some(speed_ramp(
                SpeedRampInterpolation::Hold,
                &points,
                SpeedRampAudioPolicy::PreservePitch,
            ));
            clips.push(clip);
        }
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/ramp-budget.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("too many active speed-ramp segments"),
            "{error:#}"
        );
    }

    #[test]
    fn compiles_reverse_and_freeze_with_deterministic_filter_order_and_silence() {
        let mut reverse = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut reverse.tracks[1] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        let CompositionTrack::Video { clips, .. } = &mut reverse.tracks[2] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;

        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/reverse.mp4"),
            &reverse,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert_eq!(graph.matches(",reverse,").count(), 2, "{graph}");
        assert_eq!(graph.matches(",areverse,").count(), 1, "{graph}");
        assert!(
            graph.contains(
                "trim=start=1.000000:end=3.000000,fps=fps=30000/1000,reverse,\
                 setpts=(PTS-STARTPTS)/1.000000"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "trim=start=0.000000:end=1.000000,settb=AVTB,fps=fps=30000/1000,reverse,\
                 setpts=(PTS-STARTPTS)/0.500000,tpad=stop_mode=clone:stop_duration=2.000000,\
                 minterpolate=fps=30000/1000"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "atrim=start=1.000000:end=3.000000,asetpts=PTS-STARTPTS,areverse,\
                 aresample=48000"
            ),
            "{graph}"
        );

        let mut freeze = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut freeze.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze {
            source_tick: 1_500_000,
        };
        let command = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/freeze.mp4"),
            &freeze,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "trim=start=1.500000,trim=end_frame=1,setpts=PTS-STARTPTS,scale=1920:1080"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "tpad=stop_mode=clone:stop_duration=2.000000,fps=fps=30000/1000,\
                 trim=duration=2.000000,setpts=PTS-STARTPTS"
            ),
            "{graph}"
        );
        assert!(graph.contains("anullsrc=channel_layout=stereo"), "{graph}");
        assert!(!graph.contains("areverse"), "{graph}");
        assert!(!graph.contains("[1:a]"), "{graph}");

        let mut freeze_overlay = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut freeze_overlay.tracks[1] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/freeze-overlay.mp4"),
            &freeze_overlay,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "trim=start=0.000000,settb=AVTB,trim=end_frame=1,setpts=PTS-STARTPTS,\
                 tpad=stop_mode=clone:stop_duration=1.000000,fps=fps=30000/1000,\
                 trim=duration=1.000000,setpts=PTS-STARTPTS"
            ),
            "{graph}"
        );
    }

    #[test]
    fn reverse_compiler_enforces_buffering_dimensions_duration_and_total_work() {
        let mut oversized = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut oversized.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        oversized.sources.get_mut(&id("video-a")).unwrap().width = 3_840;
        oversized.sources.get_mut(&id("video-a")).unwrap().height = 2_160;
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/reverse.mp4"),
            &oversized,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("dimension budget"), "{error:#}");

        let mut too_long = first_slice_composition();
        too_long
            .sources
            .get_mut(&id("video-a"))
            .unwrap()
            .duration_ticks = 40_000_000;
        let CompositionTrack::Video { clips, .. } = &mut too_long.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        clips[0].placement.source_out_tick = 31_000_001;
        clips[1].placement.timeline_start_tick = 30_000_001;
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/reverse.mp4"),
            &too_long,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("buffered duration budget"),
            "{error:#}"
        );

        let mut total = first_slice_composition();
        total.canvas.fps_milli = 60_000;
        for source_id in [id("video-a"), id("video-b")] {
            let source = total.sources.get_mut(&source_id).unwrap();
            source.width = 1_920;
            source.height = 1_080;
        }
        let CompositionTrack::Video { clips, .. } = &mut total.tracks[0] else {
            unreachable!();
        };
        for (index, clip) in clips.iter_mut().enumerate() {
            clip.playback_mode = PlaybackMode::Reverse;
            clip.placement.timeline_start_tick = index as u64 * 10_000_000;
            clip.placement.source_in_tick = 0;
            clip.placement.source_out_tick = 10_000_000;
        }
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/reverse.mp4"),
            &total,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("buffering work budget"),
            "{error:#}"
        );
    }

    #[test]
    fn compiles_primary_and_overlay_stabilization_before_playback_and_optical_flow() {
        let mut composition = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].stabilization = StabilizationSpec::Deshake {
            radius_x: 16,
            radius_y: 32,
        };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[2] else {
            unreachable!();
        };
        clips[0].stabilization = StabilizationSpec::Deshake {
            radius_x: 32,
            radius_y: 16,
        };
        clips[0].playback_mode = PlaybackMode::Reverse;
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        clips[1].placement.timeline_start_tick = 4_000_000;

        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/stabilized.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert_eq!(graph.matches("deshake=").count(), 2, "{graph}");
        assert!(
            graph.contains(
                "trim=start=1.000000:end=3.000000,fps=fps=30000/1000,\
                 deshake=rx=32:ry=16:edge=mirror:blocksize=8:contrast=20:search=exhaustive,\
                 reverse,setpts=(PTS-STARTPTS)/0.500000,scale=1920:1080"
            ),
            "{graph}"
        );
        assert!(
            graph.contains(
                "trim=start=0.000000:end=1.000000,settb=AVTB,fps=fps=30000/1000,\
                 deshake=rx=16:ry=32:edge=mirror:blocksize=8:contrast=20:search=exhaustive,\
                 setpts=(PTS-STARTPTS)/0.500000,tpad=stop_mode=clone:stop_duration=2.000000,\
                 minterpolate=fps=30000/1000"
            ),
            "{graph}"
        );

        let ordinary = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/ordinary.mp4"),
            &first_slice_composition(),
            23,
            1,
        )
        .unwrap();
        let graph = &ordinary.arguments[ordinary
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(!graph.contains("deshake"), "{graph}");

        let mut transitioned = text_transition_composition(TransitionKind::Dissolve);
        let CompositionTrack::Video { clips, .. } = &mut transitioned.tracks[1] else {
            unreachable!();
        };
        clips[0].stabilization = StabilizationSpec::Deshake {
            radius_x: 16,
            radius_y: 16,
        };
        let command = build_composition_ffmpeg_command_with_text_resources(
            &inputs(),
            &text_resources(),
            Path::new("/renders/stabilized-transition.mp4"),
            &transitioned,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "trim=start=1.000000:end=3.250000,fps=fps=30000/1000,\
                 deshake=rx=16:ry=16"
            ),
            "{graph}"
        );
        assert!(graph.contains("xfade=transition=fade:duration=0.500000"));
    }

    #[test]
    fn stabilization_compiler_enforces_dimension_duration_and_work_budgets() {
        let enabled = StabilizationSpec::Deshake {
            radius_x: 64,
            radius_y: 64,
        };
        let mut oversized = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut oversized.tracks[0] else {
            unreachable!();
        };
        clips[0].stabilization = enabled;
        let source = oversized.sources.get_mut(&id("video-a")).unwrap();
        source.width = 3_840;
        source.height = 2_160;
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/stabilized.mp4"),
            &oversized,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("dimension budget"), "{error:#}");

        let mut too_long = first_slice_composition();
        too_long
            .sources
            .get_mut(&id("video-a"))
            .unwrap()
            .duration_ticks = 20_000_000;
        let CompositionTrack::Video { clips, .. } = &mut too_long.tracks[0] else {
            unreachable!();
        };
        clips[0].stabilization = enabled;
        clips[0].placement.source_in_tick = 0;
        clips[0].placement.source_out_tick = 15_000_001;
        clips[1].placement.timeline_start_tick = 15_000_001;
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/stabilized.mp4"),
            &too_long,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duration budget"), "{error:#}");

        let mut total = first_slice_composition();
        total.canvas.fps_milli = 60_000;
        for source_id in [id("video-a"), id("video-b")] {
            let source = total.sources.get_mut(&source_id).unwrap();
            source.width = 1_920;
            source.height = 1_080;
        }
        let CompositionTrack::Video { clips, .. } = &mut total.tracks[0] else {
            unreachable!();
        };
        for (index, clip) in clips.iter_mut().enumerate() {
            clip.stabilization = enabled;
            clip.placement.timeline_start_tick = index as u64 * 5_000_000;
            clip.placement.source_in_tick = 0;
            clip.placement.source_out_tick = 5_000_000;
        }
        let error = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/stabilized.mp4"),
            &total,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("stabilization work budget"),
            "{error:#}"
        );
    }

    #[test]
    fn optical_flow_compiler_enforces_dimension_duration_and_total_work_budgets() {
        let mut oversized = first_slice_composition();
        oversized.canvas.width = 3_840;
        oversized.canvas.height = 2_160;
        let CompositionTrack::Video { clips, .. } = &mut oversized.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        clips[1].placement.timeline_start_tick = 4_000_000;
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/oversized.mp4"),
            &oversized,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("dimension budget"), "{error:#}");

        let mut too_long = first_slice_composition();
        too_long.tracks.pop();
        let CompositionTrack::Video { clips, .. } = &mut too_long.tracks[0] else {
            unreachable!();
        };
        clips.truncate(1);
        clips[0].placement.source_in_tick = 0;
        clips[0].placement.source_out_tick = 7_000_000;
        clips[0].placement.speed = 0.05;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/too-long.mp4"),
            &too_long,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duration budget"), "{error:#}");

        let mut too_much_work = first_slice_composition();
        too_much_work.tracks.pop();
        let CompositionTrack::Video { clips, .. } = &mut too_much_work.tracks[0] else {
            unreachable!();
        };
        for (index, clip) in clips.iter_mut().enumerate() {
            clip.placement.source_in_tick = 0;
            clip.placement.source_out_tick = 4_000_000;
            clip.placement.speed = 0.05;
            clip.placement.timeline_start_tick = index as u64 * 80_000_000;
            clip.frame_interpolation = FrameInterpolation::OpticalFlow;
        }
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/too-much-work.mp4"),
            &too_much_work,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("work budget"), "{error:#}");
    }

    #[test]
    fn compiles_source_and_independent_audio_automation_golden() {
        let mut composition = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].audio_gain = animated(Interpolation::EaseIn, 2_000_000, 0.25, 1.0);
        clips[0].audio_pan = animated(Interpolation::EaseOut, 2_000_000, -1.0, 1.0);
        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].gain = animated(Interpolation::EaseInOut, 4_000_000, 0.2, 0.8);
        clips[0].pan = animated(Interpolation::Hold, 4_000_000, -0.5, 0.5);
        clips[0].fade_in_ticks = 250_000;
        clips[0].fade_out_ticks = 500_000;

        let command = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "volume=volume='if(lt(t,0),0.25,if(lt(t,2),0.25+(1-0.25)*pow(((t-0)/2),3),1))':eval=frame:precision=double"
            ),
            "{graph}"
        );
        assert!(graph.contains("1-pow(1-((t-0)/2),3)"), "{graph}");
        assert_eq!(graph.matches("aeval=exprs=").count(), 2, "{graph}");
        assert!(
            graph.contains(
                "volume=volume='if(lt(t,0),0.2,if(lt(t,4),0.2+(0.8-0.2)*if(lt(((t-0)/4),0.5)"
            ),
            "{graph}"
        );
        assert!(graph.contains("afade=t=in:st=0:d=0.250000"), "{graph}");
        assert!(
            graph.contains("afade=t=out:st=3.500000:d=0.500000"),
            "{graph}"
        );
        assert!(
            graph.contains("adelay=48000S:all=1[audio_clip_0]"),
            "{graph}"
        );
    }

    #[test]
    fn muted_or_disabled_primary_source_audio_compiles_silence_without_audio_pads() {
        for disable_clip in [false, true] {
            let mut composition = first_slice_composition();
            composition.tracks.pop();
            let CompositionTrack::Video { muted, clips, .. } = &mut composition.tracks[0] else {
                unreachable!();
            };
            *muted = !disable_clip;
            clips[0].source_audio_enabled = !disable_clip;
            if disable_clip {
                clips[0].source_audio_enabled = false;
            }
            clips[0].audio_gain = animated(Interpolation::Linear, 2_000_001, 0.0, 1.0);

            let command = build_composition_ffmpeg_command(
                &inputs(),
                Path::new("/renders/result.mp4"),
                &composition,
                23,
                1,
            )
            .unwrap();
            let graph = &command.arguments[command
                .arguments
                .iter()
                .position(|argument| argument == "-filter_complex")
                .unwrap()
                + 1];
            assert!(!graph.contains("[0:a]"), "{graph}");
            assert_eq!(graph.matches("anullsrc=").count(), 2, "{graph}");
        }
    }

    #[test]
    fn rejects_active_audio_automation_outside_duration_and_ranges() {
        let mut composition = first_slice_composition();
        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].gain = animated(Interpolation::Linear, 4_000_001, 0.0, 1.0);
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("clip-local duration"),
            "{error:#}"
        );

        let mut composition = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].audio_pan = animated(Interpolation::Linear, 2_000_000, -1.0, 1.01);
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("supported range"), "{error:#}");
    }

    #[test]
    fn rejects_more_than_the_active_audio_keyframe_budget() {
        let mut composition = first_slice_composition();
        let points = (0_u64..32)
            .map(|tick| Keyframe {
                tick,
                value: tick as f64 / 31.0,
            })
            .collect::<Vec<_>>();
        let gain = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(1_000_000, Interpolation::Linear, points.clone()).unwrap(),
        };
        let pan = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                Interpolation::Linear,
                points
                    .into_iter()
                    .map(|point| Keyframe {
                        tick: point.tick,
                        value: point.value * 2.0 - 1.0,
                    })
                    .collect(),
            )
            .unwrap(),
        };
        let CompositionTrack::Audio { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        *clips = (0_u64..33)
            .map(|index| AudioClip {
                id: CompositionClipId::parse(format!("budget-audio-{index}")).unwrap(),
                source_id: id("music"),
                placement: placement(index * 100_000, 0, 100_000),
                gain: gain.clone(),
                pan: pan.clone(),
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                enabled: true,
            })
            .collect();
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("too many active audio keyframes"),
            "{error:#}"
        );
    }

    #[test]
    fn compiles_unicode_textfile_and_exact_handle_transition_golden() {
        let composition = text_transition_composition(TransitionKind::Dissolve);
        let resources = text_resources();
        let command = build_composition_ffmpeg_command_with_text_resources(
            &inputs(),
            &resources,
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(!graph.contains("Привет"), "text leaked into filtergraph");
        assert!(
            graph.contains("trim=start=1.000000:end=3.250000"),
            "{graph}"
        );
        assert!(
            graph.contains("trim=start=0.750000:end=4.000000"),
            "{graph}"
        );
        assert!(graph.contains(
            "[video_clip_0][video_clip_1]xfade=transition=fade:duration=0.500000:offset=1.750000[base_video]"
        ), "{graph}");
        assert!(
            graph.contains(
                "drawtext=fontfile='/fonts/Noto Sans.ttf':textfile='/tmp/title\\:one.txt':reload=0:expansion=none"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("fontsize=56.000000:fontcolor=0xFFCC33@1.000000"),
            "{graph}"
        );
        assert!(
            graph.contains("box=1:boxcolor=0x000000@0.500000:borderw=2.000000"),
            "{graph}"
        );
        assert!(
            graph.contains("shadowx=3.000000:shadowy=4.000000"),
            "{graph}"
        );
        assert!(graph.contains("scale=iw*0.750000:ih*0.750000"), "{graph}");
        assert!(graph.contains("colorchannelmixer=aa=0.800000"), "{graph}");
        assert_eq!(
            command.read_only_files,
            vec![
                PathBuf::from("/fonts/Noto Sans.ttf"),
                PathBuf::from("/tmp/title:one.txt")
            ]
        );
    }

    #[test]
    fn maps_all_supported_transition_kinds_and_rejects_missing_handles() {
        for (kind, filter) in [
            (TransitionKind::Dissolve, "fade"),
            (TransitionKind::FadeBlack, "fadeblack"),
            (TransitionKind::WipeLeft, "wipeleft"),
            (TransitionKind::WipeRight, "wiperight"),
            (TransitionKind::SlideLeft, "slideleft"),
            (TransitionKind::SlideRight, "slideright"),
        ] {
            let composition = text_transition_composition(kind);
            let command = build_composition_ffmpeg_command_with_text_resources(
                &inputs(),
                &text_resources(),
                Path::new("/renders/result.mp4"),
                &composition,
                23,
                1,
            )
            .unwrap();
            let graph = &command.arguments[command
                .arguments
                .iter()
                .position(|argument| argument == "-filter_complex")
                .unwrap()
                + 1];
            assert!(
                graph.contains(&format!("xfade=transition={filter}:")),
                "{kind:?}: {graph}"
            );
        }

        let mut missing = text_transition_composition(TransitionKind::Dissolve);
        let CompositionTrack::Video { clips, .. } = &mut missing.tracks[1] else {
            unreachable!()
        };
        clips[1].placement.source_in_tick = 0;
        assert!(missing.validate().is_err());
    }

    #[test]
    fn text_resources_are_exact_and_text_opacity_keyframes_compile_safely() {
        let mut composition = text_transition_composition(TransitionKind::Dissolve);
        let error = build_composition_ffmpeg_command(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("text render resources"));

        let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] else {
            unreachable!()
        };
        clips[0].opacity = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                Interpolation::Linear,
                vec![
                    Keyframe {
                        tick: 0,
                        value: 0.5,
                    },
                    Keyframe {
                        tick: 1_000_000,
                        value: 1.0,
                    },
                ],
            )
            .unwrap(),
        };
        let command = build_composition_ffmpeg_command_with_text_resources(
            &inputs(),
            &text_resources(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains("a='alpha(X,Y)*(if(lt(T,0),0.5,if(lt(T,1),0.5+(1-0.5)*((T-0)/1),1)))'"),
            "{graph}"
        );
    }

    #[test]
    fn repeated_source_is_split_for_each_video_and_audio_consumer() {
        let mut composition = first_slice_composition();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[1].source_id = id("video-a");
        composition.tracks.pop();

        let args = build_composition_ffmpeg_args(
            &inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &args[args
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains("[0:v]split=2[input_0_v_0][input_0_v_1]"),
            "{graph}"
        );
        assert!(
            graph.contains("[0:a]asplit=2[input_0_a_0][input_0_a_1]"),
            "{graph}"
        );
    }

    #[test]
    fn rejects_video_gaps_and_overlaps() {
        for start in [1_500_000, 2_500_000] {
            let mut composition = first_slice_composition();
            let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
                unreachable!();
            };
            clips[1].placement.timeline_start_tick = start;
            let error = build_composition_ffmpeg_command(
                &inputs(),
                Path::new("/renders/result.mp4"),
                &composition,
                23,
                1,
            )
            .unwrap_err();
            assert!(error.to_string().contains("gap or overlap"), "{error:#}");
        }
    }

    #[test]
    fn compiles_static_video_and_looped_image_layers_golden() {
        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &layered_composition(BlendMode::Screen),
            23,
            1,
        )
        .unwrap();
        let logo = command
            .arguments
            .iter()
            .position(|argument| argument == "/media/logo.png")
            .unwrap();
        assert_eq!(
            &command.arguments[logo - 5..logo],
            ["-loop", "1", "-framerate", "30000/1000", "-i"]
        );
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains(
                "chromakey=color=0x00FF00:similarity=0.120000:blend=0.040000,\
                 despill=type=green:mix=0.250000"
            ),
            "{graph}"
        );
        assert!(graph.contains("scale=iw*0.500000:ih*0.750000:eval=init"));
        assert!(graph
            .contains("pad=640:540:640/2-0.500000*iw:540/2-0.500000*ih:color=black@0:eval=frame"));
        assert!(graph.contains("rotate=angle='(15.000000)*PI/180':ow=838:oh=838:c=none"));
        assert!(graph.contains("colorchannelmixer=aa=0.600000"));
        assert!(graph.contains("blend=all_mode=multiply"));
        assert!(graph.contains("blend=all_mode=screen"));
        assert!(graph.contains("alphaextract[layer_mask_0]"));
        assert!(graph.contains("maskedmerge[composite_1]"));
    }

    #[test]
    fn emits_every_supported_static_blend_mode() {
        for (mode, ffmpeg) in [
            (BlendMode::Normal, "normal"),
            (BlendMode::Multiply, "multiply"),
            (BlendMode::Screen, "screen"),
            (BlendMode::Overlay, "overlay"),
            (BlendMode::Darken, "darken"),
            (BlendMode::Lighten, "lighten"),
            (BlendMode::Difference, "difference"),
            (BlendMode::Addition, "addition"),
        ] {
            let args = build_composition_ffmpeg_args(
                &layered_inputs(),
                Path::new("/renders/result.mp4"),
                &layered_composition(mode),
                23,
                1,
            )
            .unwrap();
            let graph = &args[args
                .iter()
                .position(|argument| argument == "-filter_complex")
                .unwrap()
                + 1];
            assert!(
                graph.contains(&format!("blend=all_mode={ffmpeg}")),
                "{graph}"
            );
        }
    }

    #[test]
    fn compiles_shape_masks_and_visual_keyframes_but_rejects_linear_masks() {
        let mut masked = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut masked.tracks[1] else {
            unreachable!();
        };
        clips[0].effects.push(VideoEffect::Mask {
            shape: MaskShape::Rectangle,
            x: AnimatableValue::constant(0.5),
            y: AnimatableValue::constant(0.5),
            width: AnimatableValue::constant(0.5),
            height: AnimatableValue::constant(0.5),
            feather: 0.1,
            inverted: false,
        });
        clips[0].transform.x = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                Interpolation::EaseInOut,
                vec![
                    Keyframe {
                        tick: 0,
                        value: -20.0,
                    },
                    Keyframe {
                        tick: 500_000,
                        value: 20.0,
                    },
                ],
            )
            .unwrap(),
        };
        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &masked,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains("geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*(clip(min("),
            "{graph}"
        );
        assert!(graph.contains("eval=frame:eof_action=pass"), "{graph}");
        assert!(graph.contains("pow("), "{graph}");

        let mut animated = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut animated.tracks[1] else {
            unreachable!();
        };
        clips[0].effects.push(VideoEffect::Mask {
            shape: MaskShape::Linear,
            x: AnimatableValue::constant(0.5),
            y: AnimatableValue::constant(0.5),
            width: AnimatableValue::constant(0.5),
            height: AnimatableValue::constant(0.5),
            feather: 0.0,
            inverted: false,
        });
        let error = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &animated,
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Linear mask"), "{error:#}");

        let mut reordered = layered_composition(BlendMode::Normal);
        let CompositionTrack::Video { clips, .. } = &mut reordered.tracks[1] else {
            unreachable!();
        };
        clips[0].effects.insert(
            0,
            VideoEffect::Mask {
                shape: MaskShape::Rectangle,
                x: AnimatableValue::constant(0.5),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.5),
                height: AnimatableValue::constant(0.5),
                feather: 0.0,
                inverted: false,
            },
        );
        let error = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &reordered,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("before shape masks"),
            "{error:#}"
        );

        let mut oversized = layered_composition(BlendMode::Normal);
        let CompositionTrack::Image { clips, .. } = &mut oversized.tracks[0] else {
            unreachable!();
        };
        clips[0].transform.scale_x = AnimatableValue::constant(16.0);
        let error = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &oversized,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("unsafe transformed dimensions"),
            "{error:#}"
        );
    }

    #[test]
    fn compiles_every_visual_animation_site_and_ellipse_inversion_golden() {
        let mut composition = layered_composition(BlendMode::Normal);
        let CompositionTrack::Image { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].transform.x = animated(Interpolation::EaseIn, 1_000_000, -30.0, 30.0);
        clips[0].transform.y = animated(Interpolation::EaseOut, 1_000_000, 20.0, -20.0);
        clips[0].transform.scale_x = animated(Interpolation::Linear, 1_000_000, 0.2, 0.4);
        clips[0].transform.scale_y = animated(Interpolation::Hold, 1_000_000, 0.2, 0.4);
        clips[0].transform.rotation_degrees =
            animated(Interpolation::EaseInOutCubic, 1_000_000, -15.0, 45.0);
        clips[0].opacity = animated(Interpolation::EaseInOut, 1_000_000, 0.1, 0.9);

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        clips[0].effects.push(VideoEffect::Mask {
            shape: MaskShape::Ellipse,
            x: animated(Interpolation::Linear, 1_000_000, 0.25, 0.75),
            y: animated(Interpolation::Hold, 1_000_000, 0.5, 0.6),
            width: animated(Interpolation::EaseIn, 1_000_000, 0.2, 0.8),
            height: animated(Interpolation::EaseOut, 1_000_000, 0.3, 0.9),
            feather: 0.2,
            inverted: true,
        });

        let command = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        let graph = &command.arguments[command
            .arguments
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(
            graph.contains("scale=w='max(1,iw*(if(lt(t,0),0.2"),
            "{graph}"
        );
        assert!(graph.contains("rotate=angle='(if(lt(t,0),-15"), "{graph}");
        assert!(graph.contains("*PI/180':ow="), "{graph}");
        assert!(graph.contains("a='alpha(X,Y)*(if(lt(T,0),0.1"), "{graph}");
        assert!(
            graph.contains("overlay=x='(main_w/2+(if(lt((t-2),0),-30"),
            "{graph}"
        );
        assert!(graph.contains("eval=frame:eof_action=pass"), "{graph}");
        assert!(graph.contains("alpha(X,Y)*(1-(clip((1-(sqrt("), "{graph}");
        assert!(graph.contains("pow("), "{graph}");
    }

    #[test]
    fn rejects_visual_keyframes_past_the_clip_local_duration() {
        let mut composition = layered_composition(BlendMode::Normal);
        let CompositionTrack::Image { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].opacity = animated(Interpolation::Linear, 1_000_001, 0.0, 1.0);
        let error = build_composition_ffmpeg_command(
            &layered_inputs(),
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("clip-local duration"),
            "{error:#}"
        );
    }

    #[test]
    fn rejects_more_than_the_active_visual_keyframe_budget() {
        let mut composition = first_slice_composition();
        let image_source = source("budget-image", SourceKind::Image, false);
        composition
            .sources
            .insert(image_source.id.clone(), image_source);
        let one_point = |value| AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                1_000_000,
                Interpolation::Hold,
                vec![Keyframe { tick: 0, value }],
            )
            .unwrap(),
        };
        let clips = (0_u64..410)
            .map(|index| ImageClip {
                id: CompositionClipId::parse(format!("budget-image-{index}")).unwrap(),
                source_id: id("budget-image"),
                timeline_start_tick: index * 10_000,
                duration_ticks: 10_000,
                transform: TransformSpec {
                    x: one_point(0.0),
                    y: one_point(0.0),
                    scale_x: one_point(0.25),
                    scale_y: one_point(0.25),
                    rotation_degrees: one_point(0.0),
                    anchor_x: 0.5,
                    anchor_y: 0.5,
                },
                opacity: one_point(1.0),
                blend_mode: BlendMode::Normal,
                enabled: true,
            })
            .collect();
        composition.tracks.insert(
            0,
            CompositionTrack::Image {
                id: TrackId::parse("budget-track").unwrap(),
                name: "Budget".into(),
                hidden: false,
                locked: false,
                clips,
            },
        );
        let mut resolved = inputs();
        resolved.insert(id("budget-image"), PathBuf::from("/media/budget.png"));
        let error = build_composition_ffmpeg_command(
            &resolved,
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("too many active visual keyframes"),
            "{error:#}"
        );
    }

    #[test]
    fn muted_audio_track_does_not_require_or_compile_its_input() {
        let mut composition = first_slice_composition();
        let CompositionTrack::Audio { muted, .. } = &mut composition.tracks[1] else {
            unreachable!();
        };
        *muted = true;
        let mut resolved = inputs();
        resolved.remove(&id("music"));

        let args = build_composition_ffmpeg_args(
            &resolved,
            Path::new("/renders/result.mp4"),
            &composition,
            23,
            1,
        )
        .unwrap();
        assert!(!args.iter().any(|argument| argument == "/media/music.wav"));
        let graph = &args[args
            .iter()
            .position(|argument| argument == "-filter_complex")
            .unwrap()
            + 1];
        assert!(graph.contains("[source_audio]atrim=duration=5.000000"));
        assert!(!graph.contains("amix="));
    }

    #[test]
    fn reports_the_missing_source_id_before_compilation() {
        let mut resolved = inputs();
        resolved.remove(&id("video-b"));
        let error = build_composition_ffmpeg_command(
            &resolved,
            Path::new("/renders/result.mp4"),
            &first_slice_composition(),
            23,
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("video-b"), "{error:#}");
    }
}
