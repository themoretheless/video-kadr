//! Stage 1: multi-clip concat and transitions. Owned by the transitions agent.
//!
//! Reads `spec.composition()` (clips with their in/out points, per-clip speed
//! and volume, and the transition that opens each clip) and, when `clips` is
//! absent, `spec.segment_transition()` for plain `segments`. It adds one input
//! per distinct clip source, trims each clip, and stitches the pieces with
//! `xfade` on the video branch and `acrossfade` on the audio branch, leaving
//! the concatenated pads as the plan's terminal labels.
//!
//! Every piece contributes exactly one video pad and, while the plan has an
//! audio branch, exactly one audio pad. A muted piece (or a source without an
//! audio stream) contributes synthesised silence instead of a real stream, so
//! `concat` never sees a pad count mismatch and the branches stay in sync.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::composition::{TransitionKind, TransitionSpec};
use crate::domain::edit::{EditSpec, TimeRange};
use crate::domain::filter_graph::{FilterGraph, MediaKind};

use super::{ComplexPlan, InputSpec, RenderContext};

/// Layout and rate of the synthesised silence. Matching what the encoder wants
/// keeps `concat` and `acrossfade` from resampling in the middle of the graph.
const SILENCE_LAYOUT: &str = "stereo";
const SILENCE_SAMPLE_RATE: u32 = 48_000;
/// `atempo` accepts only this range per instance, so a clip speed outside it is
/// decomposed into several stages.
const MIN_ATEMPO: f64 = 0.5;
const MAX_ATEMPO: f64 = 2.0;
/// Playback rate bounds mirrored from the domain, applied again here so a
/// hand-built spec can never spin the `atempo` decomposition.
const MIN_SPEED: f64 = 0.25;
const MAX_SPEED: f64 = 4.0;
const MAX_VOLUME: f64 = 4.0;
/// Shortest piece worth emitting: below this FFmpeg produces no frames.
const MIN_PIECE_SECONDS: f64 = 0.01;
/// A transition shorter than this is dropped in favour of a hard cut.
const MIN_EFFECTIVE_TRANSITION_SECONDS: f64 = 0.05;

/// Frame rate a multi-source timeline falls back to when the probe reports
/// none. Every clip has to share one rate before `xfade` accepts them.
const DEFAULT_COMPOSITION_FPS: f64 = 30.0;

/// Build the clip/segment concat and leave its pads as the plan's terminals.
/// An edit that drives its timeline through `trim`/`segments` alone is left
/// untouched, so the legacy single-pass and concat paths keep working.
pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    // Once any other composed stage is active the legacy concat path is out of
    // reach, so plain `segments` have to be stitched here even without a
    // transition. Otherwise the render would silently ignore the cuts.
    let force_segments = spec.uses_non_timeline_extensions();
    let Some(slices) = timeline_slices(spec, ctx.duration_seconds, force_segments)? else {
        return Ok(());
    };
    let pieces = resolve_inputs(plan, ctx, &slices)?;
    emit(plan, ctx, &pieces)?;
    plan.mark_timeline_consumed();
    Ok(())
}

/// Expected output length of the composed timeline, in seconds, or `None` when
/// this stage leaves the timeline to the legacy `trim`/`segments` path. The
/// running total subtracts the overlap every surviving transition consumes and
/// then applies the global `speed`, so progress reporting matches the render.
pub fn expected_output_seconds(spec: &EditSpec, source_duration_seconds: f64) -> Option<f64> {
    let slices = timeline_slices(
        spec,
        source_duration_seconds,
        spec.uses_non_timeline_extensions(),
    )
    .ok()
    .flatten()?;
    let first = slices.first()?;
    let mut total = first.output_duration_seconds();
    let mut previous = first;
    for slice in slices.iter().skip(1) {
        let overlap = effective_transition(previous, slice).map_or(0.0, |(_, seconds)| seconds);
        total += slice.output_duration_seconds() - overlap;
        previous = slice;
    }
    let speed = spec.timing().speed;
    let speed = if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        1.0
    };
    Some((total / speed).max(0.0))
}

/// One playable piece of the timeline, before its source is bound to an input.
/// `source_id` is `None` for the plain-segments path, where every piece comes
/// from the primary input.
#[derive(Debug, Clone, PartialEq)]
struct Slice {
    source_id: Option<String>,
    start_seconds: f64,
    end_seconds: f64,
    speed: f64,
    volume: f64,
    muted: bool,
    transition: Option<TransitionSpec>,
}

impl Slice {
    fn output_duration_seconds(&self) -> f64 {
        (self.end_seconds - self.start_seconds) / self.speed
    }
}

/// A slice with its source resolved to an FFmpeg input index.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    input_index: usize,
    slice: Slice,
}

/// Compile the requested timeline into ordered, clamped pieces.
fn timeline_slices(
    spec: &EditSpec,
    source_duration_seconds: f64,
    force_segments: bool,
) -> anyhow::Result<Option<Vec<Slice>>> {
    if let Some(composition) = spec.composition() {
        // The first clip's source is the primary input by contract, so it is
        // the only one whose in/out points can be clamped against the probe.
        let primary_id = composition
            .clips()
            .first()
            .map(|clip| clip.source_id().to_owned());
        let mut slices = Vec::with_capacity(composition.clips().len());
        for clip in composition.clips() {
            let limit = (Some(clip.source_id()) == primary_id.as_deref())
                .then_some(source_duration_seconds);
            let Some((start_seconds, end_seconds)) =
                clamped_range(clip.start_seconds(), clip.end_seconds(), limit)
            else {
                continue;
            };
            slices.push(Slice {
                source_id: Some(clip.source_id().to_owned()),
                start_seconds,
                end_seconds,
                speed: bounded(clip.speed(), MIN_SPEED, MAX_SPEED, 1.0),
                volume: bounded(clip.volume(), 0.0, MAX_VOLUME, 1.0),
                muted: clip.muted(),
                transition: clip.transition_in(),
            });
        }
        return Ok(finish(slices));
    }

    let transition = spec.segment_transition().copied();
    if transition.is_none() && !force_segments {
        return Ok(None);
    }
    let ranges = ordered_ranges(&spec.timing().segments, source_duration_seconds)?;
    // A single surviving segment has nothing to cross-fade with, so the legacy
    // concat path stays in charge and the plan is left trivial. When another
    // stage forces this one, even one segment has to be trimmed here.
    let minimum = if force_segments { 1 } else { 2 };
    if ranges.len() < minimum {
        return Ok(None);
    }
    let slices = ranges
        .into_iter()
        .map(|(start_seconds, end_seconds)| Slice {
            source_id: None,
            start_seconds,
            end_seconds,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            transition,
        })
        .collect();
    Ok(finish(slices))
}

/// Drop the transition of the opening piece (nothing precedes it) and map an
/// empty timeline back to "this stage does not apply".
fn finish(mut slices: Vec<Slice>) -> Option<Vec<Slice>> {
    let first = slices.first_mut()?;
    first.transition = None;
    Some(slices)
}

/// Sort keep-ranges, reject overlaps, clamp each to the source and drop the
/// ones that clamping left too short to render.
fn ordered_ranges(
    ranges: &[TimeRange],
    source_duration_seconds: f64,
) -> anyhow::Result<Vec<(f64, f64)>> {
    let mut ordered: Vec<(f64, f64)> = ranges
        .iter()
        .filter_map(|range| {
            clamped_range(
                range.start_seconds(),
                range.end_seconds(),
                Some(source_duration_seconds),
            )
        })
        .collect();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    for pair in ordered.windows(2) {
        if pair[1].0 < pair[0].1 {
            anyhow::bail!("timeline segments overlap and cannot be stitched");
        }
    }
    Ok(ordered)
}

/// Clamp one range into `0..=limit` and reject anything non-finite or empty.
fn clamped_range(start: f64, end: f64, limit: Option<f64>) -> Option<(f64, f64)> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let mut start = start.max(0.0);
    let mut end = end.max(0.0);
    if let Some(limit) = limit.filter(|value| value.is_finite() && *value > 0.0) {
        start = start.min(limit);
        end = end.min(limit);
    }
    (end - start > MIN_PIECE_SECONDS).then_some((start, end))
}

/// Range-guard a wire-derived scalar, falling back to `default` when it is not
/// a usable number.
fn bounded(value: f64, min: f64, max: f64, default: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

/// The transition actually emitted between two neighbours. A transition can
/// never consume more than either neighbour plays, so it is shortened to fit
/// and degrades to a hard cut once nothing meaningful is left.
fn effective_transition(previous: &Slice, current: &Slice) -> Option<(TransitionKind, f64)> {
    let transition = current.transition?;
    let room = previous
        .output_duration_seconds()
        .min(current.output_duration_seconds())
        - MIN_PIECE_SECONDS;
    let seconds = transition.duration_seconds().min(room);
    (seconds >= MIN_EFFECTIVE_TRANSITION_SECONDS).then_some((transition.kind(), seconds))
}

/// Bind every slice source to an FFmpeg input, registering one extra input per
/// distinct additional source. The first clip's source is the primary input by
/// contract, which is why an unresolved id is only tolerated there.
fn resolve_inputs(
    plan: &mut ComplexPlan,
    ctx: &RenderContext,
    slices: &[Slice],
) -> anyhow::Result<Vec<Piece>> {
    let primary_path: PathBuf = plan
        .inputs()
        .first()
        .map(|input| input.path.clone())
        .unwrap_or_default();
    let mut indices: BTreeMap<&str, usize> = BTreeMap::new();
    let mut pieces = Vec::with_capacity(slices.len());
    for (position, slice) in slices.iter().enumerate() {
        let input_index = match slice.source_id.as_deref() {
            None => 0,
            Some(id) => match indices.get(id) {
                Some(index) => *index,
                None => {
                    let index = match ctx.asset_path(id) {
                        Some(path) if path == primary_path => 0,
                        Some(path) => plan.add_input(InputSpec::source(path)),
                        // The primary input already is the first clip's source.
                        None if position == 0 => 0,
                        None => {
                            anyhow::bail!("render plan references unresolved clip source '{id}'")
                        }
                    };
                    indices.insert(id, index);
                    index
                }
            },
        };
        pieces.push(Piece {
            input_index,
            slice: slice.clone(),
        });
    }
    Ok(pieces)
}

/// Emit the per-piece trims and the stitching statements, then hand the plan
/// its new terminal pads.
fn emit(plan: &mut ComplexPlan, ctx: &RenderContext, pieces: &[Piece]) -> anyhow::Result<()> {
    let Some(first) = pieces.first() else {
        return Ok(());
    };
    let with_audio = plan.audio_label().is_some();
    // Sources other than the primary can differ in size, aspect and frame rate;
    // xfade and concat both require every input to agree on all three.
    let normalize = pieces.iter().any(|piece| piece.input_index != 0);

    let mut video_pads = Vec::with_capacity(pieces.len());
    let mut audio_pads = Vec::with_capacity(pieces.len());
    for piece in pieces {
        video_pads.push(emit_video_piece(plan, ctx, piece, normalize)?);
        if with_audio {
            audio_pads.push(emit_audio_piece(plan, ctx, piece)?);
        }
    }

    let mut video_label = video_pads[0].clone();
    let mut audio_label = audio_pads.first().cloned();
    if pieces.len() > 1 {
        if pieces
            .iter()
            .skip(1)
            .zip(pieces)
            .all(|(current, previous)| {
                effective_transition(&previous.slice, &current.slice).is_none()
            })
        {
            // No transition survives, so one N-way concat is enough.
            let (video, audio) = emit_concat(plan, &video_pads, &audio_pads);
            video_label = video;
            audio_label = audio;
        } else {
            let mut running = first.slice.output_duration_seconds();
            for (index, piece) in pieces.iter().enumerate().skip(1) {
                let previous = &pieces[index - 1].slice;
                match effective_transition(previous, &piece.slice) {
                    Some((kind, seconds)) => {
                        let offset = (running - seconds).max(0.0);
                        let label = plan.next_label("xfade");
                        plan.push(format!(
                            "[{video_label}][{}]xfade=transition={}:duration={seconds:.3}:offset={offset:.3}[{label}]",
                            video_pads[index],
                            kind.ffmpeg_name()
                        ));
                        video_label = label;
                        if let Some(current_audio) = audio_label.clone() {
                            let label = plan.next_label("acrossfade");
                            plan.push(format!(
                                "[{current_audio}][{}]acrossfade=d={seconds:.3}:c1=tri:c2=tri[{label}]",
                                audio_pads[index]
                            ));
                            audio_label = Some(label);
                        }
                        running += piece.slice.output_duration_seconds() - seconds;
                    }
                    None => {
                        let pair_video = vec![video_label.clone(), video_pads[index].clone()];
                        let pair_audio = match audio_label.clone() {
                            Some(current_audio) => {
                                vec![current_audio, audio_pads[index].clone()]
                            }
                            None => Vec::new(),
                        };
                        let (video, audio) = emit_concat(plan, &pair_video, &pair_audio);
                        video_label = video;
                        audio_label = audio;
                        running += piece.slice.output_duration_seconds();
                    }
                }
            }
        }
    }

    plan.set_video_label(video_label);
    if audio_label.is_some() {
        plan.set_audio_label(audio_label);
    }
    Ok(())
}

/// Concatenate every pad in order and return the resulting terminal labels.
fn emit_concat(
    plan: &mut ComplexPlan,
    video_pads: &[String],
    audio_pads: &[String],
) -> (String, Option<String>) {
    let count = video_pads.len();
    let with_audio = audio_pads.len() == count;
    let mut statement = String::new();
    for (index, video) in video_pads.iter().enumerate() {
        statement.push_str(&format!("[{video}]"));
        if with_audio {
            statement.push_str(&format!("[{}]", audio_pads[index]));
        }
    }
    let audio_flag = u8::from(with_audio);
    let video_label = plan.next_label("concatv");
    let audio_label = with_audio.then(|| plan.next_label("concata"));
    statement.push_str(&format!(
        "concat=n={count}:v=1:a={audio_flag}[{video_label}]"
    ));
    if let Some(label) = &audio_label {
        statement.push_str(&format!("[{label}]"));
    }
    plan.push(statement);
    (video_label, audio_label)
}

fn emit_video_piece(
    plan: &mut ComplexPlan,
    ctx: &RenderContext,
    piece: &Piece,
    normalize: bool,
) -> anyhow::Result<String> {
    let slice = &piece.slice;
    let mut filters = vec![
        format!(
            "trim=start={:.3}:end={:.3}",
            slice.start_seconds, slice.end_seconds
        ),
        "setpts=PTS-STARTPTS".to_owned(),
    ];
    if (slice.speed - 1.0).abs() > f64::EPSILON {
        filters.push(format!("setpts={:.6}*PTS", 1.0 / slice.speed));
    }
    if normalize {
        if ctx.width > 0 && ctx.height > 0 {
            filters.push(format!(
                "scale={}:{}:force_original_aspect_ratio=decrease",
                ctx.width, ctx.height
            ));
            filters.push(format!(
                "pad={}:{}:(ow-iw)/2:(oh-ih)/2",
                ctx.width, ctx.height
            ));
        }
        // `xfade` also refuses two inputs at different frame rates, so a
        // concrete rate is mandatory here even when the probe reported none.
        let fps = ctx
            .fps
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_COMPOSITION_FPS);
        filters.push(format!("fps={fps:.3}"));
        filters.push("setsar=1".to_owned());
        // `xfade` refuses two inputs whose timebases differ, and two files
        // muxed at different rates carry different ones. AVTB is the common
        // denominator FFmpeg itself uses between filters.
        filters.push("settb=AVTB".to_owned());
        filters.push("format=yuv420p".to_owned());
    }
    let label = plan.next_label("clipv");
    let chain = serialize(MediaKind::Video, &filters)?;
    plan.push(format!("[{}:v]{chain}[{label}]", piece.input_index));
    Ok(label)
}

fn emit_audio_piece(
    plan: &mut ComplexPlan,
    ctx: &RenderContext,
    piece: &Piece,
) -> anyhow::Result<String> {
    let slice = &piece.slice;
    let label = plan.next_label("clipa");
    if !input_has_audio(ctx, piece.input_index) || slice.muted || slice.volume <= 0.0 {
        // Silence keeps the pad count of both branches equal, which is what a
        // source without an audio stream used to break.
        let filters = vec![
            format!("anullsrc=channel_layout={SILENCE_LAYOUT}:sample_rate={SILENCE_SAMPLE_RATE}"),
            format!("atrim=duration={:.3}", slice.output_duration_seconds()),
            "asetpts=PTS-STARTPTS".to_owned(),
        ];
        let chain = serialize(MediaKind::Audio, &filters)?;
        plan.push(format!("{chain}[{label}]"));
        return Ok(label);
    }

    let mut filters = vec![
        format!(
            "atrim=start={:.3}:end={:.3}",
            slice.start_seconds, slice.end_seconds
        ),
        "asetpts=PTS-STARTPTS".to_owned(),
    ];
    filters.extend(atempo_chain(slice.speed));
    if (slice.volume - 1.0).abs() > f64::EPSILON {
        filters.push(format!("volume={:.3}", slice.volume));
    }
    // Every piece has to agree with the synthesised silence and with the other
    // sources before concat or acrossfade sees it.
    filters.push(format!(
        "aformat=sample_fmts=fltp:sample_rates={SILENCE_SAMPLE_RATE}:channel_layouts={SILENCE_LAYOUT}"
    ));
    let chain = serialize(MediaKind::Audio, &filters)?;
    plan.push(format!("[{}:a]{chain}[{label}]", piece.input_index));
    Ok(label)
}

/// Whether the input backing a piece is known to carry an audio stream. The
/// render context only probes the primary source, so a clip resolved from the
/// asset store is assumed to have audio; a muted clip takes the silence path
/// regardless.
fn input_has_audio(ctx: &RenderContext, input_index: usize) -> bool {
    input_index != 0 || ctx.has_audio
}

/// Decompose a playback rate into `atempo` stages inside the filter's own
/// accepted range. A rate of 1.0 produces no filter at all.
fn atempo_chain(speed: f64) -> Vec<String> {
    let mut remaining = bounded(speed, MIN_SPEED, MAX_SPEED, 1.0);
    let mut factors = Vec::new();
    while remaining > MAX_ATEMPO {
        factors.push(MAX_ATEMPO);
        remaining /= MAX_ATEMPO;
    }
    while remaining < MIN_ATEMPO {
        factors.push(MIN_ATEMPO);
        remaining /= MIN_ATEMPO;
    }
    if (remaining - 1.0).abs() > 1e-6 {
        factors.push(remaining);
    }
    factors
        .into_iter()
        .map(|factor| format!("atempo={factor:.6}"))
        .collect()
}

/// Validate and serialize a linear chain before it reaches FFmpeg, the same way
/// `ComplexPlan::chain_video` does for terminal-pad chains.
fn serialize(media: MediaKind, filters: &[String]) -> anyhow::Result<String> {
    FilterGraph::linear(media, filters)
        .and_then(|graph| graph.ffmpeg_linear_chain())
        .map_err(|error| anyhow::anyhow!("invalid {media:?} transition chain: {error}"))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::domain::artifact_graph::Fingerprint;
    use crate::domain::edit::EditExtensions;
    use crate::model::EditRequest;
    use crate::services::render::{EditPlan, SourceMediaMetadata};

    const SOURCE_SECONDS: f64 = 30.0;

    fn spec(value: Value) -> EditSpec {
        let request: EditRequest = serde_json::from_value(value).unwrap();
        let extensions = EditExtensions::from_request(&request).unwrap();
        EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1920, 1080, SOURCE_SECONDS).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap()
    }

    fn context(has_audio: bool) -> RenderContext {
        RenderContext::new(1920, 1080, SOURCE_SECONDS, has_audio, Some(30.0), 10.0)
    }

    fn clip(source_id: &str, start: f64, end: f64) -> Value {
        json!({ "sourceId": source_id, "start": start, "end": end })
    }

    fn run(value: Value, has_audio: bool) -> (ComplexPlan, EditSpec) {
        let spec = spec(value);
        let ctx = context(has_audio);
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), has_audio);
        apply(&mut plan, &spec, &ctx).unwrap();
        (plan, spec)
    }

    #[test]
    fn an_edit_without_clips_or_a_segment_transition_is_left_alone() {
        let (plan, _) = run(json!({ "videoId": "x" }), true);
        assert!(plan.is_trivial());

        // Plain segments still belong to the legacy concat path.
        let (plan, _) = run(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            true,
        );
        assert!(plan.is_trivial());
    }

    #[test]
    fn a_single_clip_is_trimmed_without_a_concat() {
        let (plan, spec) = run(
            json!({ "videoId": "x", "clips": [clip("vid_a", 2.0, 6.0)] }),
            true,
        );
        let graph = plan.render();
        assert_eq!(
            graph,
            "[0:v]trim=start=2.000:end=6.000,setpts=PTS-STARTPTS[clipv_1];\
             [0:a]atrim=start=2.000:end=6.000,asetpts=PTS-STARTPTS,\
             aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[clipa_2]"
        );
        assert_eq!(plan.video_label(), "clipv_1");
        assert_eq!(plan.audio_label(), Some("clipa_2"));
        assert!(!graph.contains("concat"), "{graph}");
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(4.0));
    }

    #[test]
    fn two_clips_without_a_transition_use_one_concat() {
        let (plan, spec) = run(
            json!({
                "videoId": "x",
                "clips": [clip("vid_a", 0.0, 4.0), clip("vid_a", 10.0, 12.0)]
            }),
            true,
        );
        let graph = plan.render();
        assert!(
            graph.contains(
                "[clipv_1][clipa_2][clipv_3][clipa_4]concat=n=2:v=1:a=1[concatv_5][concata_6]"
            ),
            "{graph}"
        );
        assert!(!graph.contains("xfade"), "{graph}");
        assert_eq!(plan.video_label(), "concatv_5");
        assert_eq!(plan.audio_label(), Some("concata_6"));
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(6.0));
    }

    #[test]
    fn two_clips_with_a_transition_cross_fade_both_branches() {
        let mut second = clip("vid_a", 10.0, 14.0);
        second["transitionIn"] = json!({ "kind": "wipeleft", "duration": 0.5 });
        let (plan, spec) = run(
            json!({ "videoId": "x", "clips": [clip("vid_a", 0.0, 4.0), second] }),
            true,
        );
        let graph = plan.render();
        // offset = running output time (4.0) minus the transition duration.
        assert!(
            graph.contains(
                "[clipv_1][clipv_3]xfade=transition=wipeleft:duration=0.500:offset=3.500[xfade_5]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("[clipa_2][clipa_4]acrossfade=d=0.500:c1=tri:c2=tri[acrossfade_6]"),
            "{graph}"
        );
        assert_eq!(plan.video_label(), "xfade_5");
        assert_eq!(plan.audio_label(), Some("acrossfade_6"));
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(7.5));
    }

    #[test]
    fn three_clips_accumulate_the_running_offset() {
        let mut second = clip("vid_a", 10.0, 14.0);
        second["transitionIn"] = json!({ "kind": "fade", "duration": 0.5 });
        let mut third = clip("vid_a", 20.0, 26.0);
        third["transitionIn"] = json!({ "kind": "circleopen", "duration": 1.0 });
        let (plan, spec) = run(
            json!({ "videoId": "x", "clips": [clip("vid_a", 0.0, 4.0), second, third] }),
            true,
        );
        let graph = plan.render();
        assert!(
            graph.contains("xfade=transition=fade:duration=0.500:offset=3.500"),
            "{graph}"
        );
        // 4.0 + 4.0 - 0.5 = 7.5 played, minus the second transition.
        assert!(
            graph.contains("xfade=transition=circleopen:duration=1.000:offset=6.500"),
            "{graph}"
        );
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(12.5));
    }

    #[test]
    fn every_documented_transition_reaches_xfade_by_name() {
        for kind in TransitionKind::ALL {
            let mut second = clip("vid_a", 10.0, 14.0);
            second["transitionIn"] = json!({ "kind": kind.wire_id(), "duration": 0.5 });
            let (plan, _) = run(
                json!({ "videoId": "x", "clips": [clip("vid_a", 0.0, 4.0), second] }),
                true,
            );
            assert!(
                plan.render()
                    .contains(&format!("xfade=transition={}:", kind.ffmpeg_name())),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_source_without_audio_never_grows_an_audio_branch() {
        let (plan, _) = run(
            json!({
                "videoId": "x",
                "clips": [clip("vid_a", 0.0, 4.0), clip("vid_a", 10.0, 12.0)]
            }),
            false,
        );
        let graph = plan.render();
        assert!(graph.contains("concat=n=2:v=1:a=0[concatv_3]"), "{graph}");
        assert!(!graph.contains("atrim"), "{graph}");
        assert!(!graph.contains("anullsrc"), "{graph}");
        assert_eq!(plan.audio_label(), None);
    }

    #[test]
    fn a_muted_clip_contributes_synthesised_silence() {
        let mut muted = clip("vid_a", 10.0, 12.0);
        muted["muted"] = json!(true);
        let (plan, _) = run(
            json!({ "videoId": "x", "clips": [clip("vid_a", 0.0, 4.0), muted] }),
            true,
        );
        let graph = plan.render();
        assert!(
            graph.contains(
                "anullsrc=channel_layout=stereo:sample_rate=48000,\
                 atrim=duration=2.000,asetpts=PTS-STARTPTS[clipa_4]"
            ),
            "{graph}"
        );
        // Both branches still carry one pad per piece.
        assert!(graph.contains("concat=n=2:v=1:a=1"), "{graph}");
    }

    #[test]
    fn a_second_source_becomes_its_own_normalized_input() {
        let ctx = context(true).with_asset("vid_b", "/private/media/b.mp4");
        let spec = spec(json!({
            "videoId": "x",
            "clips": [clip("vid_a", 0.0, 4.0), clip("vid_b", 1.0, 3.0)]
        }));
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), true);
        apply(&mut plan, &spec, &ctx).unwrap();

        assert_eq!(plan.inputs().len(), 2);
        assert_eq!(plan.inputs()[1].path, PathBuf::from("/private/media/b.mp4"));
        let graph = plan.render();
        assert!(graph.contains("[1:v]trim=start=1.000:end=3.000"), "{graph}");
        assert!(
            graph.contains("[1:a]atrim=start=1.000:end=3.000"),
            "{graph}"
        );
        // Both branches are normalized so xfade and concat can accept them.
        assert!(
            graph.contains(
                "scale=1920:1080:force_original_aspect_ratio=decrease,\
                 pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=30.000,setsar=1,settb=AVTB,\
                 format=yuv420p"
            ),
            "{graph}"
        );
    }

    #[test]
    fn an_unresolved_second_source_fails_the_render() {
        let spec = spec(json!({
            "videoId": "x",
            "clips": [clip("vid_a", 0.0, 4.0), clip("vid_b", 1.0, 3.0)]
        }));
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), true);
        assert!(apply(&mut plan, &spec, &context(true)).is_err());
    }

    #[test]
    fn clip_ranges_are_clamped_to_the_probed_source_duration() {
        let (plan, spec) = run(
            json!({ "videoId": "x", "clips": [clip("vid_a", 25.0, 99.0)] }),
            true,
        );
        assert!(
            plan.render().contains("trim=start=25.000:end=30.000"),
            "{}",
            plan.render()
        );
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(5.0));
    }

    #[test]
    fn per_clip_speed_and_volume_reach_both_branches() {
        let mut fast = clip("vid_a", 0.0, 8.0);
        fast["speed"] = json!(4.0);
        fast["volume"] = json!(0.5);
        let (plan, spec) = run(json!({ "videoId": "x", "clips": [fast] }), true);
        let graph = plan.render();
        assert!(graph.contains("setpts=0.250000*PTS"), "{graph}");
        assert!(
            graph.contains("atempo=2.000000,atempo=2.000000,volume=0.500"),
            "{graph}"
        );
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(2.0));
    }

    #[test]
    fn a_segment_transition_stitches_the_plain_segment_timeline() {
        let (plan, spec) = run(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 4.0 }, { "start": 10.0, "end": 14.0 }],
                "segmentTransition": { "kind": "slideup", "duration": 0.5 }
            }),
            true,
        );
        let graph = plan.render();
        assert!(graph.contains("[0:v]trim=start=0.000:end=4.000"), "{graph}");
        assert!(
            graph.contains("[0:v]trim=start=10.000:end=14.000"),
            "{graph}"
        );
        assert!(
            graph.contains("xfade=transition=slideup:duration=0.500:offset=3.500"),
            "{graph}"
        );
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(7.5));
    }

    #[test]
    fn a_lone_segment_leaves_the_legacy_path_in_charge() {
        let (plan, spec) = run(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 4.0 }],
                "segmentTransition": { "kind": "fade", "duration": 0.5 }
            }),
            true,
        );
        assert!(plan.is_trivial());
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), None);
    }

    #[test]
    fn overlapping_ranges_are_rejected_and_short_ones_dropped() {
        let overlapping = [
            TimeRange::new(0.0, 5.0).unwrap(),
            TimeRange::new(3.0, 8.0).unwrap(),
        ];
        assert!(ordered_ranges(&overlapping, SOURCE_SECONDS).is_err());

        // Out-of-order input is sorted, and clamping can empty a range.
        let unsorted = [
            TimeRange::new(10.0, 12.0).unwrap(),
            TimeRange::new(1.0, 2.0).unwrap(),
            TimeRange::new(40.0, 50.0).unwrap(),
        ];
        assert_eq!(
            ordered_ranges(&unsorted, SOURCE_SECONDS).unwrap(),
            vec![(1.0, 2.0), (10.0, 12.0)]
        );
    }

    #[test]
    fn a_transition_never_outlasts_the_clips_it_joins() {
        let short = |seconds: f64, transition: Option<f64>| Slice {
            source_id: None,
            start_seconds: 0.0,
            end_seconds: seconds,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            transition: transition.map(|duration| {
                TransitionSpec::from_wire(&crate::model::Transition {
                    kind: "fade".into(),
                    duration,
                })
                .unwrap()
            }),
        };
        assert_eq!(
            effective_transition(&short(4.0, None), &short(4.0, Some(0.5))),
            Some((TransitionKind::Fade, 0.5))
        );
        // The transition is shortened to what the shorter neighbour can spare.
        let clipped = effective_transition(&short(4.0, None), &short(0.3, Some(1.0)));
        assert!(
            clipped.is_some_and(|(_, seconds)| (seconds - 0.29).abs() < 1e-9),
            "{clipped:?}"
        );
        // Nothing left to fade with: fall back to a hard cut.
        assert!(effective_transition(&short(4.0, None), &short(0.05, Some(1.0))).is_none());
    }

    #[test]
    fn atempo_stays_inside_the_filter_accepted_range() {
        assert!(atempo_chain(1.0).is_empty());
        assert_eq!(
            atempo_chain(4.0),
            vec!["atempo=2.000000", "atempo=2.000000"]
        );
        assert_eq!(
            atempo_chain(0.25),
            vec!["atempo=0.500000", "atempo=0.500000"]
        );
        assert_eq!(atempo_chain(1.5), vec!["atempo=1.500000"]);
        assert_eq!(atempo_chain(f64::NAN), Vec::<String>::new());
    }

    #[test]
    fn the_global_speed_scales_the_composed_duration() {
        let spec = spec(json!({
            "videoId": "x",
            "speed": 2.0,
            "clips": [clip("vid_a", 0.0, 4.0), clip("vid_a", 10.0, 14.0)]
        }));
        assert_eq!(expected_output_seconds(&spec, SOURCE_SECONDS), Some(4.0));
    }
}
