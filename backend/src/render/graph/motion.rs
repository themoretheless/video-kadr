//! Stage 6: keyframed transform and speed ramps. Owned by the motion agent.
//!
//! Reads `spec.motion()` (zoom, panX, panY and rotation keyframe tracks) and
//! `spec.speed_ramps()`. Keyframe tracks become FFmpeg expressions over the
//! frame timestamp through `domain::keyframes::FfmpegKeyframeAdapter`.
//!
//! Why `zoompan` and not `scale`+`crop`: a per-frame zoom needs a filter whose
//! *geometry* is re-evaluated every frame. `crop` only re-evaluates `x` and `y`
//! (`eval=frame`); its `w`/`h` are configuration-time, because a filter link
//! carries one fixed frame size. `scale` has the same constraint on the way
//! out. So a scale+crop pair can animate the pan but not the zoom, and
//! `zoompan` is the only filter that takes a zoom expression at all.
//!
//! `zoompan` places its window on integer pixels of its own output, so a slow
//! pan visibly steps one pixel at a time. The mitigation here is to let
//! `zoompan` render at `SUPERSAMPLE`x the frame size and to scale back down
//! afterwards, which turns that step into a sub-pixel one for the price of a
//! single extra `scale`. `zoompan` also re-stamps its output at a constant
//! frame rate, so the chain normalizes to that rate with an explicit `fps`
//! first; without it the video would silently play at 25 fps.

use crate::domain::edit::EditSpec;
use crate::domain::keyframes::{FfmpegKeyframeAdapter, Interpolation, KeyframeTrack};
use crate::domain::motion::{MotionSpec, MAX_RAMP_SPEED, MIN_RAMP_SPEED};

use super::{ComplexPlan, RenderContext};

/// Same cap the wire conversion enforces, re-checked here so a hand-built spec
/// cannot hand the graph an unbounded expression.
const MAX_TRACK_POINTS: usize = 64;
/// Values closer than this to the identity are treated as "not animated".
const IDENTITY_EPSILON: f64 = 1e-6;
/// `zoompan` renders at this multiple of the frame size to make its integer
/// window placement sub-pixel, then the frame is scaled back down.
const SUPERSAMPLE: u32 = 2;
/// Skip supersampling rather than build an intermediate no scaler will take.
const MAX_INTERMEDIATE_EDGE: u32 = 8192;
/// Used when the render context carries no frame rate. `zoompan` insists on a
/// constant rate and its own default (25) would retime the whole render.
const DEFAULT_MOTION_FPS: f64 = 30.0;
/// A single `atempo` stage is only well defined inside this range.
const ATEMPO_MIN: f64 = 0.5;
const ATEMPO_MAX: f64 = 2.0;

/// Typed rejection of a malformed track. The wire conversion already validates
/// everything here; this is the second gate for specs built in process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionError {
    EmptyTrack(&'static str),
    OversizedTrack(&'static str),
    NonFiniteTrack(&'static str),
    UnsortedTrack(&'static str),
    InvalidTimeBase(&'static str),
    Expression(&'static str),
    UnknownFrameSize,
}

impl std::fmt::Display for MotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTrack(track) => write!(formatter, "motion track '{track}' has no keyframes"),
            Self::OversizedTrack(track) => write!(
                formatter,
                "motion track '{track}' exceeds {MAX_TRACK_POINTS} keyframes"
            ),
            Self::NonFiniteTrack(track) => {
                write!(formatter, "motion track '{track}' has a non-finite value")
            }
            Self::UnsortedTrack(track) => write!(
                formatter,
                "motion track '{track}' is unsorted or has duplicate times"
            ),
            Self::InvalidTimeBase(track) => {
                write!(formatter, "motion track '{track}' has a zero time base")
            }
            Self::Expression(track) => write!(
                formatter,
                "motion track '{track}' cannot be compiled to an expression"
            ),
            Self::UnknownFrameSize => {
                write!(
                    formatter,
                    "animated reframe needs a known source frame size"
                )
            }
        }
    }
}

impl std::error::Error for MotionError {}

pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    // Speed first: motion keyframes are documented on the OUTPUT timeline, so
    // the transform has to read timestamps that the ramp already retimed.
    if let Some(ramps) = spec.speed_ramps() {
        apply_speed_ramps(plan, ramps)?;
    }
    if let Some(motion) = spec.motion() {
        apply_transform(plan, motion, ctx)?;
    }
    Ok(())
}

/// Ken Burns / animated reframe: rotation, then the zoom+pan window.
fn apply_transform(
    plan: &mut ComplexPlan,
    motion: &MotionSpec,
    ctx: &RenderContext,
) -> anyhow::Result<()> {
    let rotation = animated(motion.rotation(), "rotation", 0.0)?;
    let zoom = animated(motion.zoom(), "zoom", 1.0)?;
    let pan_x = animated(motion.pan_x(), "panX", 0.0)?;
    let pan_y = animated(motion.pan_y(), "panY", 0.0)?;

    let mut filters: Vec<String> = Vec::new();
    // Rotate before the window so a punch-in can hide the exposed corners.
    if let Some(track) = rotation {
        let expression = expression_for(track, "t", "rotation")?;
        filters.push(format!(
            "rotate=a='({expression})*PI/180':fillcolor=black:bilinear=1"
        ));
    }
    if zoom.is_some() || pan_x.is_some() || pan_y.is_some() {
        filters.extend(window_filters(zoom, pan_x, pan_y, ctx)?);
    }
    plan.chain_video(&filters, "motion")
}

/// The `fps` + `zoompan` + `scale` triple that carries zoom and pan.
fn window_filters(
    zoom: Option<&KeyframeTrack<f64>>,
    pan_x: Option<&KeyframeTrack<f64>>,
    pan_y: Option<&KeyframeTrack<f64>>,
    ctx: &RenderContext,
) -> anyhow::Result<Vec<String>> {
    if ctx.width == 0 || ctx.height == 0 {
        return Err(MotionError::UnknownFrameSize.into());
    }
    let supersample =
        if ctx.width.max(ctx.height).saturating_mul(SUPERSAMPLE) <= MAX_INTERMEDIATE_EDGE {
            SUPERSAMPLE
        } else {
            1
        };
    let window_width = ctx.width.saturating_mul(supersample);
    let window_height = ctx.height.saturating_mul(supersample);
    let fps = ctx
        .fps
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(DEFAULT_MOTION_FPS)
        .clamp(1.0, 240.0);

    // `zoompan` scales its input by `z` and crops the output window out of the
    // result, so the zoom that just fills the window is the supersample factor.
    let zoom_expression = match zoom {
        Some(track) => format!(
            "{}*({})",
            format_number(f64::from(supersample)),
            expression_for(track, "ot", "zoom")?
        ),
        None => format_number(f64::from(supersample)),
    };
    // Pan is normalized to the headroom the current zoom leaves: -1 pins the
    // window to the left/top edge, +1 to the right/bottom one, 0 centres it.
    // At zoom 1 there is no headroom, so a pan track alone changes nothing.
    let pan_expression = |track: Option<&KeyframeTrack<f64>>,
                          axis: &'static str,
                          span: &str|
     -> anyhow::Result<String> {
        Ok(match track {
            Some(track) => format!("({span})/2*(1+({}))", expression_for(track, "ot", axis)?),
            None => format!("({span})/2"),
        })
    };
    let x = pan_expression(pan_x, "panX", "iw*zoom-ow")?;
    let y = pan_expression(pan_y, "panY", "ih*zoom-oh")?;

    let mut filters = vec![
        format!("fps={fps:.3}"),
        format!(
            "zoompan=z='{zoom_expression}':x='{x}':y='{y}':d=1:s={window_width}x{window_height}:fps={fps:.3}"
        ),
    ];
    if supersample != 1 {
        filters.push(format!("scale={}:{}", ctx.width, ctx.height));
    }
    Ok(filters)
}

/// Compile the ramp track into a piecewise `setpts` plus the matching audio.
fn apply_speed_ramps(plan: &mut ComplexPlan, track: &KeyframeTrack<f64>) -> anyhow::Result<()> {
    guard_track(track, "speedRamps")?;
    let points = ramp_points(track);
    if points
        .iter()
        .all(|(_, speed)| (speed - 1.0).abs() <= IDENTITY_EPSILON)
    {
        return Ok(());
    }
    let regions = ramp_regions(&points, track.interpolation);
    plan.chain_video(
        &[format!("setpts='({})/TB'", setpts_expression(&regions))],
        "ramp",
    )?;
    apply_audio_ramp(plan, &regions)
}

/// Ramp keyframes as `(seconds, clamped speed)`. Clamping to the documented
/// range is also what keeps every later division away from zero.
fn ramp_points(track: &KeyframeTrack<f64>) -> Vec<(f64, f64)> {
    track
        .keyframes
        .iter()
        .map(|keyframe| {
            (
                keyframe.tick as f64 / f64::from(track.time_base),
                keyframe.value.clamp(MIN_RAMP_SPEED, MAX_RAMP_SPEED),
            )
        })
        .collect()
}

/// One piece of the piecewise timing map, in SOURCE seconds.
#[derive(Debug, Clone, PartialEq)]
struct Region {
    start: f64,
    /// `None` on the tail region, which runs to the end of the stream.
    end: Option<f64>,
    /// Output time already spent before this region starts.
    output_start: f64,
    /// Output time this region produces, `None` on the tail region.
    output_duration: Option<f64>,
    /// `setpts` value expression for a timestamp inside this region.
    expression: String,
    /// Constant speed that produces the same output duration. This is what the
    /// audio branch gets, because `atempo` cannot take an expression.
    effective_speed: f64,
}

/// Split the ramp into regions and give each one its exact timing map.
///
/// Inside a region the speed is either constant (a `hold` track, the lead-in
/// before the first keyframe, and the tail after the last one) or linear. For a
/// linear speed `s(T) = s0 + k*(T-t0)` the output time is the exact integral
/// `log(s(T)/s0)/k`, so the video really ramps instead of stepping. A `smooth`
/// track is compiled with the same linear form: the cubic ease has no
/// elementary integral, and subdividing it would multiply the expression size.
fn ramp_regions(points: &[(f64, f64)], interpolation: Interpolation) -> Vec<Region> {
    let mut regions: Vec<Region> = Vec::new();
    let mut output_start = 0.0;
    let (first_time, first_speed) = points[0];
    if first_time > IDENTITY_EPSILON {
        regions.push(constant_region(0.0, Some(first_time), 0.0, first_speed));
        output_start = first_time / first_speed;
    }
    for window in points.windows(2) {
        let (start, start_speed) = window[0];
        let (end, end_speed) = window[1];
        let span = end - start;
        // `KeyframeTrack::new` sorts and rejects duplicate ticks, so this only
        // fires for a hand-built track that skipped the constructor.
        if span <= 0.0 {
            continue;
        }
        let holds = interpolation == Interpolation::Hold
            || (end_speed - start_speed).abs() <= IDENTITY_EPSILON;
        let region = if holds {
            constant_region(start, Some(end), output_start, start_speed)
        } else {
            let slope = (end_speed - start_speed) / span;
            let duration = (end_speed.ln() - start_speed.ln()) / slope;
            Region {
                start,
                end: Some(end),
                output_start,
                output_duration: Some(duration),
                expression: format!(
                    "{}+log(({}+{}*(T-{}))/{})/{}",
                    format_number(output_start),
                    format_number(start_speed),
                    format_number(slope),
                    format_number(start),
                    format_number(start_speed),
                    format_number(slope)
                ),
                effective_speed: span / duration,
            }
        };
        output_start += region.output_duration.unwrap_or_default();
        regions.push(region);
    }
    let (last_time, last_speed) = points[points.len() - 1];
    regions.push(Region {
        start: last_time,
        end: None,
        output_start,
        output_duration: None,
        expression: constant_expression(output_start, last_time, last_speed),
        effective_speed: last_speed,
    });
    regions
}

fn constant_region(start: f64, end: Option<f64>, output_start: f64, speed: f64) -> Region {
    Region {
        start,
        end,
        output_start,
        output_duration: end.map(|end| (end - start) / speed),
        expression: constant_expression(output_start, start, speed),
        effective_speed: speed,
    }
}

fn constant_expression(output_start: f64, start: f64, speed: f64) -> String {
    format!(
        "{}+(T-{})/{}",
        format_number(output_start),
        format_number(start),
        format_number(speed)
    )
}

/// Nest the regions into one `if(lt(T,..),..,..)` chain, tail innermost.
fn setpts_expression(regions: &[Region]) -> String {
    let mut expression = regions[regions.len() - 1].expression.clone();
    for region in regions[..regions.len() - 1].iter().rev() {
        let Some(end) = region.end else {
            continue;
        };
        expression = format!(
            "if(lt(T,{}),{},{expression})",
            format_number(end),
            region.expression
        );
    }
    expression
}

/// Audio side of a ramp. `atempo` takes a number, never an expression, so the
/// honest fallback is to cut the audio at the region boundaries, give each
/// region the constant tempo that reproduces its exact output duration, and
/// concatenate. Pitch correction is kept; the ramp inside a region is not,
/// which is audible only on a long region with a steep slope.
fn apply_audio_ramp(plan: &mut ComplexPlan, regions: &[Region]) -> anyhow::Result<()> {
    let Some(source) = plan.audio_label().map(str::to_owned) else {
        return Ok(());
    };
    if regions.len() == 1 {
        return plan.chain_audio(&atempo_chain(regions[0].effective_speed), "ramp");
    }
    let branches: Vec<String> = (0..regions.len())
        .map(|_| plan.next_label("ramp_branch"))
        .collect();
    plan.push(format!(
        "[{source}]asplit={}{}",
        regions.len(),
        pad_list(&branches)
    ));
    let mut parts: Vec<String> = Vec::with_capacity(regions.len());
    for (region, branch) in regions.iter().zip(&branches) {
        let mut filters = vec![match region.end {
            Some(end) => format!(
                "atrim=start={}:end={}",
                format_number(region.start),
                format_number(end)
            ),
            None => format!("atrim=start={}", format_number(region.start)),
        }];
        filters.push("asetpts=PTS-STARTPTS".to_owned());
        filters.extend(atempo_chain(region.effective_speed));
        let part = plan.next_label("ramp_part");
        plan.push(format!("[{branch}]{}[{part}]", filters.join(",")));
        parts.push(part);
    }
    let joined = plan.next_label("ramp_audio");
    plan.push(format!(
        "{}concat=n={}:v=0:a=1[{joined}]",
        pad_list(&parts),
        parts.len()
    ));
    plan.set_audio_label(Some(joined));
    Ok(())
}

fn pad_list(labels: &[String]) -> String {
    labels
        .iter()
        .map(|label| format!("[{label}]"))
        .collect::<Vec<_>>()
        .join("")
}

/// A single `atempo` only covers 0.5..2.0, and the ramp range is 0.25..4, so a
/// factor outside it becomes two stages of its square root.
fn atempo_chain(speed: f64) -> Vec<String> {
    let speed = if speed.is_finite() {
        speed.clamp(MIN_RAMP_SPEED, MAX_RAMP_SPEED)
    } else {
        1.0
    };
    if (ATEMPO_MIN..=ATEMPO_MAX).contains(&speed) {
        return vec![format!("atempo={speed:.6}")];
    }
    let stage = speed.sqrt().clamp(ATEMPO_MIN, ATEMPO_MAX);
    vec![format!("atempo={stage:.6}"), format!("atempo={stage:.6}")]
}

/// Return the track only when it actually animates away from `identity`.
fn animated<'a>(
    track: Option<&'a KeyframeTrack<f64>>,
    name: &'static str,
    identity: f64,
) -> Result<Option<&'a KeyframeTrack<f64>>, MotionError> {
    let Some(track) = track else {
        return Ok(None);
    };
    guard_track(track, name)?;
    if track
        .keyframes
        .iter()
        .all(|keyframe| (keyframe.value - identity).abs() <= IDENTITY_EPSILON)
    {
        return Ok(None);
    }
    Ok(Some(track))
}

/// Reject anything the expression compiler cannot safely consume: an empty or
/// oversized track, a non-finite value, an unsorted or duplicated time, or a
/// zero time base that would divide by zero while converting ticks to seconds.
fn guard_track(track: &KeyframeTrack<f64>, name: &'static str) -> Result<(), MotionError> {
    if track.time_base == 0 {
        return Err(MotionError::InvalidTimeBase(name));
    }
    if track.keyframes.is_empty() {
        return Err(MotionError::EmptyTrack(name));
    }
    if track.keyframes.len() > MAX_TRACK_POINTS {
        return Err(MotionError::OversizedTrack(name));
    }
    if track
        .keyframes
        .iter()
        .any(|keyframe| !keyframe.value.is_finite())
    {
        return Err(MotionError::NonFiniteTrack(name));
    }
    if track
        .keyframes
        .windows(2)
        .any(|window| window[0].tick >= window[1].tick)
    {
        return Err(MotionError::UnsortedTrack(name));
    }
    Ok(())
}

fn expression_for(
    track: &KeyframeTrack<f64>,
    time_variable: &str,
    name: &'static str,
) -> Result<String, MotionError> {
    FfmpegKeyframeAdapter::new(track)
        .expression(time_variable)
        .map_err(|_| MotionError::Expression(name))
}

/// Same compact rendering the keyframe adapter uses, so a hand-written segment
/// and an adapter-built one read alike inside one expression.
fn format_number(value: f64) -> String {
    let value = if value == -0.0 { 0.0 } else { value };
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::artifact_graph::Fingerprint;
    use crate::domain::edit::EditExtensions;
    use crate::domain::keyframes::Keyframe;
    use crate::render::graph::InputSpec;
    use crate::services::render::{EditPlan, SourceMediaMetadata};

    fn plan() -> ComplexPlan {
        ComplexPlan::new(InputSpec::source("/in.mp4"), true)
    }

    fn context() -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, true, Some(30.0), 10.0)
    }

    fn spec(value: serde_json::Value) -> EditSpec {
        let mut request_value = serde_json::json!({ "videoId": "x" });
        if let (Some(target), Some(source)) = (request_value.as_object_mut(), value.as_object()) {
            for (key, item) in source {
                target.insert(key.clone(), item.clone());
            }
        }
        let request: crate::model::EditRequest = serde_json::from_value(request_value).unwrap();
        let extensions = EditExtensions::from_request(&request).unwrap();
        EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap()
    }

    fn track(points: &[(u64, f64)], interpolation: Interpolation) -> KeyframeTrack<f64> {
        KeyframeTrack {
            time_base: 1_000,
            interpolation,
            keyframes: points
                .iter()
                .map(|(tick, value)| Keyframe {
                    tick: *tick,
                    value: *value,
                })
                .collect(),
        }
    }

    #[test]
    fn identity_tracks_emit_nothing() {
        let spec = spec(serde_json::json!({
            "motion": {
                "zoom": [{ "t": 0.0, "v": 1.0 }, { "t": 4.0, "v": 1.0 }],
                "panX": [{ "t": 0.0, "v": 0.0 }],
                "rotation": [{ "t": 0.0, "v": 0.0 }, { "t": 2.0, "v": 0.0 }],
            },
            "speedRamps": [{ "t": 0.0, "v": 1.0 }, { "t": 3.0, "v": 1.0 }],
        }));
        assert!(spec.motion().is_some() && spec.speed_ramps().is_some());

        let mut plan = plan();
        apply(&mut plan, &spec, &context()).unwrap();
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn a_two_point_zoom_track_emits_the_expected_window_expression() {
        let spec = spec(serde_json::json!({
            "motion": { "zoom": [{ "t": 0.0, "v": 1.0 }, { "t": 4.0, "v": 2.0 }] },
        }));
        let mut plan = plan();
        apply(&mut plan, &spec, &context()).unwrap();

        assert_eq!(
            plan.render(),
            "[0:v]fps=30.000,zoompan=z='2*(if(lt(ot,0),1,if(lt(ot,4),1+(2-1)*((ot-0)/4),2)))'\
             :x='(iw*zoom-ow)/2':y='(ih*zoom-oh)/2':d=1:s=3840x2160:fps=30.000,\
             scale=1920:1080[motion_1]"
        );
        assert_eq!(plan.video_label(), "motion_1");
        assert_eq!(plan.audio_label(), Some("0:a"));
    }

    #[test]
    fn a_pan_track_offsets_the_window_by_the_available_headroom() {
        let spec = spec(serde_json::json!({
            "motion": {
                "zoom": [{ "t": 0.0, "v": 2.0 }],
                "panX": [{ "t": 0.0, "v": -1.0 }, { "t": 2.0, "v": 1.0 }],
                "rotation": [{ "t": 0.0, "v": 0.0 }, { "t": 2.0, "v": 90.0 }],
            },
        }));
        let mut plan = plan();
        apply(&mut plan, &spec, &context()).unwrap();

        let graph = plan.render();
        assert!(graph.contains(
            "rotate=a='(if(lt(t,0),0,if(lt(t,2),0+(90-0)*((t-0)/2),90)))*PI/180'\
             :fillcolor=black:bilinear=1"
        ));
        assert!(graph.contains(
            "x='(iw*zoom-ow)/2*(1+(if(lt(ot,0),-1,if(lt(ot,2),-1+(1--1)*((ot-0)/2),1))))'"
        ));
        assert!(graph.contains("y='(ih*zoom-oh)/2'"));
        // A single-point zoom track is still a punch-in, so the window stays.
        assert!(graph.contains("z='2*(if(lt(ot,0),2,2))'"));
    }

    #[test]
    fn a_three_point_ramp_emits_the_expected_piecewise_setpts() {
        let spec = spec(serde_json::json!({
            "speedRamps": [
                { "t": 0.0, "v": 1.0, "interp": "hold" },
                { "t": 2.0, "v": 2.0, "interp": "hold" },
                { "t": 4.0, "v": 0.5, "interp": "hold" },
            ],
        }));
        let mut plan = plan();
        apply(&mut plan, &spec, &context()).unwrap();

        let graph = plan.render();
        // Holds are exact: 0..2 at 1x fills 2 s of output, 2..4 at 2x adds 1 s.
        assert!(graph.contains(
            "[0:v]setpts='(if(lt(T,2),0+(T-0)/1,if(lt(T,4),2+(T-2)/2,3+(T-4)/0.5)))/TB'[ramp_1]"
        ));
        // Audio cannot take the expression, so each region gets its own tempo.
        assert!(graph.contains("[0:a]asplit=3[ramp_branch_2][ramp_branch_3][ramp_branch_4]"));
        assert!(graph
            .contains("[ramp_branch_2]atrim=start=0:end=2,asetpts=PTS-STARTPTS,atempo=1.000000"));
        assert!(graph
            .contains("[ramp_branch_3]atrim=start=2:end=4,asetpts=PTS-STARTPTS,atempo=2.000000"));
        assert!(graph.contains("[ramp_branch_4]atrim=start=4,asetpts=PTS-STARTPTS,atempo=0.500000"));
        assert!(graph.contains("concat=n=3:v=0:a=1[ramp_audio_8]"));
        assert_eq!(plan.audio_label(), Some("ramp_audio_8"));
    }

    #[test]
    fn a_linear_ramp_integrates_the_speed_and_keeps_the_audio_in_sync() {
        let spec = spec(serde_json::json!({
            "speedRamps": [{ "t": 0.0, "v": 1.0 }, { "t": 4.0, "v": 2.0 }],
        }));
        let mut plan = plan();
        apply(&mut plan, &spec, &context()).unwrap();

        let graph = plan.render();
        // s(T) = 1 + 0.25*T, so the output time is log(s(T)/1)/0.25.
        assert!(graph
            .contains("setpts='(if(lt(T,4),0+log((1+0.25*(T-0))/1)/0.25,2.772589+(T-4)/2))/TB'"));
        // 4 input seconds become 4*ln(2)/1 = 2.772589 output seconds, so the
        // region's constant-tempo equivalent is 4/2.772589 = 1.442695.
        assert!(graph.contains("atempo=1.442695"));
    }

    #[test]
    fn a_ramp_beyond_a_single_atempo_stage_is_split_into_two() {
        assert_eq!(atempo_chain(1.5), vec!["atempo=1.500000".to_owned()]);
        assert_eq!(
            atempo_chain(4.0),
            vec!["atempo=2.000000".to_owned(), "atempo=2.000000".to_owned()]
        );
        assert_eq!(
            atempo_chain(0.25),
            vec!["atempo=0.500000".to_owned(), "atempo=0.500000".to_owned()]
        );
        // Out-of-range and non-finite values are clamped, never divided by.
        assert_eq!(atempo_chain(f64::NAN), vec!["atempo=1.000000".to_owned()]);
        assert_eq!(
            atempo_chain(99.0),
            vec!["atempo=2.000000".to_owned(), "atempo=2.000000".to_owned()]
        );
    }

    #[test]
    fn a_silent_source_ramps_video_only() {
        let spec = spec(serde_json::json!({
            "speedRamps": [{ "t": 0.0, "v": 2.0 }],
        }));
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), false);
        apply(&mut plan, &spec, &context()).unwrap();

        assert_eq!(plan.render(), "[0:v]setpts='(0+(T-0)/2)/TB'[ramp_1]");
        assert_eq!(plan.audio_label(), None);
    }

    #[test]
    fn malformed_tracks_are_rejected_with_a_typed_error() {
        let cases: Vec<(KeyframeTrack<f64>, MotionError)> = vec![
            (
                track(&[(1_000, 1.0), (500, 2.0)], Interpolation::Linear),
                MotionError::UnsortedTrack("speedRamps"),
            ),
            (
                track(&[(0, 1.0), (0, 2.0)], Interpolation::Linear),
                MotionError::UnsortedTrack("speedRamps"),
            ),
            (
                track(&[(0, f64::NAN)], Interpolation::Linear),
                MotionError::NonFiniteTrack("speedRamps"),
            ),
            (
                track(&[], Interpolation::Linear),
                MotionError::EmptyTrack("speedRamps"),
            ),
        ];
        for (malformed, expected) in cases {
            let mut plan = plan();
            let error = apply_speed_ramps(&mut plan, &malformed).unwrap_err();
            assert_eq!(error.downcast_ref::<MotionError>(), Some(&expected));
            assert!(plan.is_trivial());
        }

        let mut zero_base = track(&[(0, 1.0)], Interpolation::Linear);
        zero_base.time_base = 0;
        assert_eq!(
            guard_track(&zero_base, "zoom"),
            Err(MotionError::InvalidTimeBase("zoom"))
        );

        let long = track(
            &(0..=64)
                .map(|index| (index * 1_000, 1.5))
                .collect::<Vec<_>>(),
            Interpolation::Linear,
        );
        assert_eq!(
            guard_track(&long, "zoom"),
            Err(MotionError::OversizedTrack("zoom"))
        );
    }

    #[test]
    fn an_unknown_frame_size_is_rejected_instead_of_guessed() {
        let spec = spec(serde_json::json!({
            "motion": { "zoom": [{ "t": 0.0, "v": 1.5 }] },
        }));
        let mut plan = plan();
        let ctx = RenderContext::new(0, 0, 10.0, true, None, 10.0);
        let error = apply(&mut plan, &spec, &ctx).unwrap_err();
        assert_eq!(
            error.downcast_ref::<MotionError>(),
            Some(&MotionError::UnknownFrameSize)
        );
    }

    #[test]
    fn a_missing_output_rate_falls_back_to_a_constant_one() {
        let spec = spec(serde_json::json!({
            "motion": { "zoom": [{ "t": 0.0, "v": 1.5 }] },
        }));
        let mut plan = plan();
        let ctx = RenderContext::new(1920, 1080, 10.0, true, None, 10.0);
        apply(&mut plan, &spec, &ctx).unwrap();
        assert!(plan.render().starts_with("[0:v]fps=30.000,zoompan="));
    }

    #[test]
    fn a_source_too_large_to_supersample_renders_at_frame_size() {
        let spec = spec(serde_json::json!({
            "motion": { "zoom": [{ "t": 0.0, "v": 1.5 }] },
        }));
        let mut plan = plan();
        let ctx = RenderContext::new(7680, 4320, 10.0, true, Some(24.0), 10.0);
        apply(&mut plan, &spec, &ctx).unwrap();

        let graph = plan.render();
        assert!(graph.contains("s=7680x4320"));
        assert!(graph.contains("z='1*(if(lt(ot,0),1.5,1.5))'"));
        assert!(!graph.contains("scale="));
    }
}
