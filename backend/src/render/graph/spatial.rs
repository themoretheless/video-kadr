//! Stage 2: 360 reframing, lens correction and stabilization. Owned by the
//! spatial agent.
//!
//! Reads `spec.reframe360()`, `spec.lens_correction()` and `spec.stabilize()`.
//! It emits `v360` for spherical/fisheye sources, `lenscorrection` for barrel
//! distortion, and either `deshake` (fast) or the two-pass `vidstabdetect` +
//! `vidstabtransform` (precise). Runs before the existing geometry stage
//! because it changes what the frame contains, not just how it is cropped.
//!
//! Inside the stage the order is optics, then framing, then stabilization:
//! lens distortion is undone on the raw pixels, the sphere is reframed, and the
//! resulting flat framing is what gets smoothed. `prepass_command` replays the
//! first two steps so the analysis pass sees exactly the frames the render pass
//! will transform.
//!
//! `v360` takes no per-frame expressions for `yaw`/`pitch`/`roll`/`*_fov`, but
//! it does accept runtime commands for them, so an animated track is emitted as
//! a `sendcmd` program with the `[expr]` flag and the filter is asked to reset
//! its rotation every frame (commands are otherwise applied incrementally).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::domain::edit::EditSpec;
use crate::domain::keyframes::{FfmpegKeyframeAdapter, KeyframeTrack};
use crate::domain::spatial::{
    LensCorrectionSpec, OutputProjection, Reframe360Spec, StabilizeMode, StabilizeSpec,
};

use super::{ComplexPlan, RenderContext};

/// Reserved `RenderContext` asset key holding the vidstab transform file. The
/// analysis pass writes it and the render pass reads it, so registering it once
/// is what keeps the two passes pointing at the same file.
pub const TRANSFORMS_CONTEXT_KEY: &str = "spatial:vidstab-transforms";

/// Named `v360` instance, so `sendcmd` has a target to address.
const REFRAME_INSTANCE: &str = "v360@reframe";
/// Time variable of a `sendcmd` `[expr]` argument, in seconds.
const SENDCMD_TIME_VARIABLE: &str = "T";
/// `sendcmd` rejects an open-ended interval. One week outlives any render this
/// service accepts and keeps the emitted string independent of a duration the
/// upstream stages are still free to change.
const SENDCMD_INTERVAL_END: &str = "604800";

/// Equirectangular footage is 2:1. Outside this band the source is not a
/// sphere and reframing it would silently produce a smeared render.
const EQUIRECT_ASPECT: f64 = 2.0;
const EQUIRECT_ASPECT_TOLERANCE: f64 = 0.2;

const MIN_FOV_DEGREES: f64 = 1.0;
const MAX_FOV_DEGREES: f64 = 360.0;
const DEFAULT_RECTILINEAR_FOV_DEGREES: f64 = 90.0;
const DEFAULT_FISHEYE_FOV_DEGREES: f64 = 180.0;
/// `v360` accepts a half turn either way; the domain allows a full one.
const MAX_V360_ANGLE_DEGREES: f64 = 180.0;

const MIN_DESHAKE_RADIUS: f64 = 4.0;
const MAX_DESHAKE_RADIUS: f64 = 64.0;
/// FFmpeg only accepts a `deshake` radius that is a multiple of 16.
const DESHAKE_RADIUS_STEP: f64 = 16.0;
const MIN_VIDSTAB_SHAKINESS: f64 = 1.0;
const MAX_VIDSTAB_SHAKINESS: f64 = 10.0;
const VIDSTAB_ACCURACY: u32 = 15;

/// Failures this stage reports instead of emitting a graph that would render
/// something wrong. They are wrapped in `anyhow` and can be downcast by the
/// HTTP adapter to pick a status code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpatialError {
    /// The client asked for an equirectangular input the probe contradicts.
    NotEquirectangular { width: u32, height: u32 },
    /// Two-pass stabilization without a transform file registered in the
    /// render context. See `TRANSFORMS_CONTEXT_KEY`.
    MissingTransformsPath,
    /// Transform files are indexed by frame number, so a timeline the earlier
    /// stages restructured would desynchronize the two passes.
    PreciseNeedsUncutTimeline,
    /// A path that cannot be spelled inside a filter option.
    UnrepresentablePath,
}

impl fmt::Display for SpatialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEquirectangular { width, height } => write!(
                formatter,
                "360 reframing needs an equirectangular source, but the probe reports {width}x{height}"
            ),
            Self::MissingTransformsPath => write!(
                formatter,
                "two-pass stabilization needs a transform file registered as '{TRANSFORMS_CONTEXT_KEY}'"
            ),
            Self::PreciseNeedsUncutTimeline => write!(
                formatter,
                "two-pass stabilization cannot run on a timeline built from clips or segments"
            ),
            Self::UnrepresentablePath => {
                write!(formatter, "stabilization path is not valid UTF-8")
            }
        }
    }
}

impl std::error::Error for SpatialError {}

/// Everything the job runner needs to run the vidstab analysis pass before the
/// render pass. The runner is not ours to edit, so this is exposed rather than
/// scheduled here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StabilizePrepass {
    /// FFmpeg argv, without the binary name.
    pub args: Vec<String>,
    /// File the pass writes and the render pass reads back. Always the path
    /// registered under `TRANSFORMS_CONTEXT_KEY`.
    pub transforms_path: PathBuf,
}

pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    let mut filters = optics_filters(spec, ctx)?;
    let frame = reframed_size(spec, ctx);

    if let Some(stabilize) = spec.stabilize() {
        match stabilize.mode() {
            // `deshake` corrects translation only, so its horizon lock request
            // has nothing to act on and is deliberately ignored.
            StabilizeMode::Fast => {
                filters.push(deshake_filter(stabilize));
                filters.extend(punch_in_filters(frame, stabilize.zoom_percent()));
            }
            StabilizeMode::Precise => {
                if spec.composition().is_some() || !spec.timing().segments.is_empty() {
                    return Err(SpatialError::PreciseNeedsUncutTimeline.into());
                }
                let transforms = ctx
                    .asset_path(TRANSFORMS_CONTEXT_KEY)
                    .ok_or(SpatialError::MissingTransformsPath)?;
                filters.push(vidstabtransform_filter(stabilize, transforms)?);
            }
        }
    }

    plan.chain_video(&filters, "spatial")
}

/// The analysis pass for `mode=precise`, or `None` when the edit does not ask
/// for two-pass stabilization. The caller must run this to completion before
/// the render command, with the same working directory and the same input.
pub fn prepass_command(
    plan: &ComplexPlan,
    spec: &EditSpec,
    ctx: &RenderContext,
) -> anyhow::Result<Option<StabilizePrepass>> {
    let Some(stabilize) = spec.stabilize() else {
        return Ok(None);
    };
    if stabilize.mode() != StabilizeMode::Precise {
        return Ok(None);
    }
    if spec.composition().is_some() || !spec.timing().segments.is_empty() {
        return Err(SpatialError::PreciseNeedsUncutTimeline.into());
    }
    let transforms = ctx
        .asset_path(TRANSFORMS_CONTEXT_KEY)
        .ok_or(SpatialError::MissingTransformsPath)?;
    let input = plan
        .inputs()
        .first()
        .ok_or(SpatialError::UnrepresentablePath)?
        .path
        .to_str()
        .ok_or(SpatialError::UnrepresentablePath)?
        .to_owned();

    let mut filters = optics_filters(spec, ctx)?;
    filters.push(vidstabdetect_filter(stabilize, transforms)?);

    let mut args: Vec<String> = vec![
        "-y".to_owned(),
        "-hide_banner".to_owned(),
        "-v".to_owned(),
        "error".to_owned(),
    ];
    // The transform file is indexed by frame number, so the analysis pass has
    // to see the same input-side trim the render pass applies.
    if let Some(trim) = spec.timing().trim {
        args.push("-ss".to_owned());
        args.push(format!("{:.3}", trim.start_seconds()));
        args.push("-t".to_owned());
        args.push(format!("{:.3}", trim.duration_seconds()));
    }
    args.push("-i".to_owned());
    args.push(input);
    args.push("-an".to_owned());
    args.push("-vf".to_owned());
    args.push(filters.join(","));
    args.push("-f".to_owned());
    args.push("null".to_owned());
    args.push("-".to_owned());

    Ok(Some(StabilizePrepass {
        args,
        transforms_path: transforms.to_path_buf(),
    }))
}

/// Lens correction plus 360 reframing: the part of the stage both the render
/// pass and the analysis pass have to agree on.
fn optics_filters(spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<Vec<String>> {
    let mut filters = Vec::new();
    if let Some(lens) = spec.lens_correction() {
        filters.push(lens_filter(lens));
    }
    if let Some(reframe) = spec.reframe360() {
        guard_projection(reframe, ctx)?;
        let built = reframe_filters(reframe)?;
        filters.extend(built.sendcmd);
        filters.push(built.v360);
    }
    Ok(filters)
}

/// Frame size stabilization operates on: the reframe output when there is one,
/// otherwise the probed source size.
fn reframed_size(spec: &EditSpec, ctx: &RenderContext) -> (u32, u32) {
    match spec.reframe360() {
        Some(reframe) => reframe.output_size(),
        None => (ctx.width, ctx.height),
    }
}

/// Refuse to reframe a source that is not plausibly a sphere. An unknown probe
/// size is not evidence either way, so it passes.
fn guard_projection(reframe: &Reframe360Spec, ctx: &RenderContext) -> Result<(), SpatialError> {
    use crate::domain::spatial::InputProjection;

    if reframe.input_projection() != InputProjection::Equirect {
        return Ok(());
    }
    if ctx.width == 0 || ctx.height == 0 {
        return Ok(());
    }
    let aspect = f64::from(ctx.width) / f64::from(ctx.height);
    if (aspect - EQUIRECT_ASPECT).abs() > EQUIRECT_ASPECT_TOLERANCE {
        return Err(SpatialError::NotEquirectangular {
            width: ctx.width,
            height: ctx.height,
        });
    }
    Ok(())
}

fn lens_filter(lens: &LensCorrectionSpec) -> String {
    format!("lenscorrection=k1={:.4}:k2={:.4}", lens.k1(), lens.k2())
}

struct ReframeFilters {
    /// Present only when at least one track is animated.
    sendcmd: Option<String>,
    v360: String,
}

fn reframe_filters(reframe: &Reframe360Spec) -> anyhow::Result<ReframeFilters> {
    let (width, height) = reframe.output_size();
    let mut options = vec![
        format!("input={}", reframe.input_projection().ffmpeg_name()),
        format!("output={}", reframe.output_projection().ffmpeg_name()),
        format!("w={width}"),
        format!("h={height}"),
    ];
    let mut commands: Vec<(&'static str, String)> = Vec::new();

    // Horizon lock compensates roll by cancelling it: without camera IMU
    // metadata a zero roll is the only framing that keeps the horizon level.
    let roll = if reframe.horizon_lock() {
        None
    } else {
        reframe.roll()
    };
    for (parameter, track) in [
        ("yaw", reframe.yaw()),
        ("pitch", reframe.pitch()),
        ("roll", roll),
    ] {
        options.push(format!(
            "{parameter}={:.3}",
            wrap_degrees(static_value(track, 0.0))
        ));
        if let Some(expression) = animated_expression(track)? {
            commands.push((parameter, wrap_degrees_expression(&expression)));
        }
    }

    if let Some(default_fov) = default_fov(reframe.output_projection()) {
        // `v360` takes the two axes separately; the vertical one follows the
        // requested output aspect so the reframe is not stretched.
        let vertical_ratio = if width == 0 {
            1.0
        } else {
            f64::from(height) / f64::from(width)
        };
        let horizontal = clamp_fov(static_value(reframe.fov(), default_fov));
        options.push(format!("h_fov={horizontal:.3}"));
        options.push(format!(
            "v_fov={:.3}",
            clamp_fov(horizontal * vertical_ratio)
        ));
        if let Some(expression) = animated_expression(reframe.fov())? {
            commands.push(("h_fov", clamp_fov_expression(&expression)));
            commands.push((
                "v_fov",
                clamp_fov_expression(&format!("({expression})*{vertical_ratio:.6}")),
            ));
        }
    }

    // Rotation commands accumulate frame over frame unless the filter resets
    // its rotation, and an animated track means absolute angles.
    if !commands.is_empty() {
        options.push("reset_rot=1".to_owned());
    }

    Ok(ReframeFilters {
        sendcmd: (!commands.is_empty())
            .then(|| sendcmd_filter(&commands))
            .transpose()?,
        v360: format!("{REFRAME_INSTANCE}={}", options.join(":")),
    })
}

/// Serialize the per-frame command program. Commands inside one interval are
/// separated by a bare `,`; a `,` inside an argument has to reach `sendcmd`
/// escaped, and the filtergraph parser eats one backslash inside the quoted
/// option value, hence the doubled one.
fn sendcmd_filter(commands: &[(&'static str, String)]) -> anyhow::Result<String> {
    let mut rendered = Vec::with_capacity(commands.len());
    for (parameter, expression) in commands {
        if expression.contains('\'') || expression.contains('\\') {
            anyhow::bail!("keyframe expression carries a quote or escape character");
        }
        rendered.push(format!(
            "[expr] {REFRAME_INSTANCE} {parameter} {}",
            expression.replace(',', "\\\\,")
        ));
    }
    Ok(format!(
        "sendcmd=c='0-{SENDCMD_INTERVAL_END} {}'",
        rendered.join(",")
    ))
}

/// The value a track holds when it is not animated, and the frame-zero value
/// when it is. A missing track falls back to `default`.
fn static_value(track: Option<&KeyframeTrack<f64>>, default: f64) -> f64 {
    track
        .and_then(|track| track.keyframes.first())
        .map_or(default, |keyframe| keyframe.value)
}

/// An expression over `T`, or `None` when the track has fewer than two points
/// and a plain option is enough.
fn animated_expression(track: Option<&KeyframeTrack<f64>>) -> anyhow::Result<Option<String>> {
    let Some(track) = track.filter(|track| track.keyframes.len() > 1) else {
        return Ok(None);
    };
    let expression = FfmpegKeyframeAdapter::new(track)
        .expression(SENDCMD_TIME_VARIABLE)
        .map_err(|error| anyhow::anyhow!("invalid spatial keyframe track: {error}"))?;
    Ok(Some(expression))
}

/// Fold an angle into the half turn `v360` accepts. Rotation is periodic, so
/// this is an identity on the sphere rather than a clamp.
fn wrap_degrees(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let full_turn = 2.0 * MAX_V360_ANGLE_DEGREES;
    let shifted = (value + MAX_V360_ANGLE_DEGREES).rem_euclid(full_turn);
    shifted - MAX_V360_ANGLE_DEGREES
}

fn wrap_degrees_expression(expression: &str) -> String {
    // `mod` keeps the sign of its left operand, so the result is folded twice.
    let full_turn = 2.0 * MAX_V360_ANGLE_DEGREES;
    format!(
        "(mod(mod({expression}+{MAX_V360_ANGLE_DEGREES:.0},{full_turn:.0})+{full_turn:.0},{full_turn:.0})-{MAX_V360_ANGLE_DEGREES:.0})"
    )
}

fn clamp_fov(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_RECTILINEAR_FOV_DEGREES;
    }
    value.clamp(MIN_FOV_DEGREES, MAX_FOV_DEGREES)
}

fn clamp_fov_expression(expression: &str) -> String {
    format!("clip({expression},{MIN_FOV_DEGREES:.0},{MAX_FOV_DEGREES:.0})")
}

/// Field of view only means something for the projections that sample a window
/// out of the sphere; a full equirectangular output ignores it.
fn default_fov(projection: OutputProjection) -> Option<f64> {
    match projection {
        OutputProjection::Equirect => None,
        OutputProjection::Fisheye => Some(DEFAULT_FISHEYE_FOV_DEGREES),
        OutputProjection::Flat | OutputProjection::Stereographic | OutputProjection::Pannini => {
            Some(DEFAULT_RECTILINEAR_FOV_DEGREES)
        }
    }
}

fn deshake_filter(stabilize: &StabilizeSpec) -> String {
    // Smoothing is a 1..100 dial; `deshake` wants a search radius in pixels.
    // FFmpeg rejects a radius that is not a multiple of 16 ("rx must be a
    // multiple of 16"), so the dial is quantized onto that grid.
    let radius = (stabilize.smoothing() / 100.0 * MAX_DESHAKE_RADIUS)
        .clamp(MIN_DESHAKE_RADIUS, MAX_DESHAKE_RADIUS);
    let radius = ((radius / DESHAKE_RADIUS_STEP).round() * DESHAKE_RADIUS_STEP)
        .clamp(DESHAKE_RADIUS_STEP, MAX_DESHAKE_RADIUS) as u32;
    format!("deshake=rx={radius}:ry={radius}:edge=mirror")
}

/// `deshake` has no zoom of its own, so the crop-in that hides its borders is
/// an explicit scale-up followed by a centred crop back to the frame size.
fn punch_in_filters(frame: (u32, u32), zoom_percent: f64) -> Vec<String> {
    let (width, height) = frame;
    if zoom_percent <= 1e-6 || width == 0 || height == 0 {
        return Vec::new();
    }
    let factor = 1.0 + zoom_percent / 100.0;
    let scaled_width = even_dimension(f64::from(width) * factor);
    let scaled_height = even_dimension(f64::from(height) * factor);
    if scaled_width <= width || scaled_height <= height {
        return Vec::new();
    }
    vec![
        format!("scale=w={scaled_width}:h={scaled_height}"),
        format!("crop=w={width}:h={height}"),
    ]
}

fn even_dimension(value: f64) -> u32 {
    if !value.is_finite() {
        return 2;
    }
    let rounded = value.round().clamp(2.0, f64::from(u32::MAX / 2)) as u32;
    rounded & !1
}

fn vidstabdetect_filter(
    stabilize: &StabilizeSpec,
    transforms: &Path,
) -> Result<String, SpatialError> {
    let shakiness = (stabilize.smoothing() / 10.0)
        .round()
        .clamp(MIN_VIDSTAB_SHAKINESS, MAX_VIDSTAB_SHAKINESS) as u32;
    Ok(format!(
        "vidstabdetect=result={}:shakiness={shakiness}:accuracy={VIDSTAB_ACCURACY}",
        escape_filter_path(transforms)?
    ))
}

fn vidstabtransform_filter(
    stabilize: &StabilizeSpec,
    transforms: &Path,
) -> Result<String, SpatialError> {
    let smoothing = stabilize.smoothing().round().max(1.0) as u32;
    let mut options = vec![
        format!("input={}", escape_filter_path(transforms)?),
        format!("smoothing={smoothing}"),
        // Rotation correction is what levels the horizon; without the lock the
        // stabilizer only takes out translation.
        format!(
            "maxangle={}",
            if stabilize.horizon_lock() { "-1" } else { "0" }
        ),
        "interpol=bilinear".to_owned(),
    ];
    if stabilize.zoom_percent() > 1e-6 {
        options.push(format!("zoom={:.3}", stabilize.zoom_percent()));
        // An explicit crop-in and the optimal-zoom search would fight.
        options.push("optzoom=0".to_owned());
    }
    Ok(format!("vidstabtransform={}", options.join(":")))
}

/// Filtergraph escaping for one quoted option value. This is not shell
/// escaping: the command is still handed over as an argv vector.
fn escape_filter_path(path: &Path) -> Result<String, SpatialError> {
    let value = path.to_str().ok_or(SpatialError::UnrepresentablePath)?;
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if matches!(character, '\\' | '\'' | ':' | ',' | ';' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(format!("'{escaped}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::artifact_graph::Fingerprint;
    use crate::domain::edit::EditExtensions;
    use crate::model::EditRequest;
    use crate::render::graph::InputSpec;
    use crate::services::render::{EditPlan, SourceMediaMetadata};

    /// Build a validated spec from a wire request, on a 2:1 equirect source.
    fn spec_on(width: u32, height: u32, value: serde_json::Value) -> EditSpec {
        let request: EditRequest = serde_json::from_value(value).expect("valid request");
        let extensions = EditExtensions::from_request(&request).expect("valid extensions");
        EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(width, height, 10.0).expect("valid probe"),
        )
        .expect("valid plan")
        .edit
        .with_extensions(extensions)
        .expect("valid extensions")
    }

    fn spec(value: serde_json::Value) -> EditSpec {
        spec_on(3_840, 1_920, value)
    }

    fn context(width: u32, height: u32) -> RenderContext {
        RenderContext::new(width, height, 10.0, true, Some(30.0), 10.0)
    }

    fn equirect_context() -> RenderContext {
        context(3_840, 1_920)
    }

    fn plan() -> ComplexPlan {
        ComplexPlan::new(InputSpec::source("/media/source.mp4"), true)
    }

    fn rendered(spec: &EditSpec, ctx: &RenderContext) -> String {
        let mut plan = plan();
        apply(&mut plan, spec, ctx).expect("stage applies");
        plan.render()
    }

    fn reframe(output: &str) -> serde_json::Value {
        serde_json::json!({
            "videoId": "vid_1",
            "reframe360": {
                "inputProjection": "equirect",
                "outputProjection": output,
                "outputWidth": 1920,
                "outputHeight": 1080,
                "horizonLock": false
            }
        })
    }

    #[test]
    fn an_edit_without_spatial_fields_leaves_the_plan_trivial() {
        let mut plan = plan();
        apply(
            &mut plan,
            &spec(serde_json::json!({ "videoId": "vid_1" })),
            &equirect_context(),
        )
        .expect("stage applies");
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn every_projection_pair_maps_to_its_v360_tokens() {
        for (output, expected) in [
            ("flat", "output=flat:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:h_fov=90.000:v_fov=50.625"),
            ("equirect", "output=e:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000"),
            ("fisheye", "output=fisheye:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:h_fov=180.000:v_fov=101.250"),
            ("stereographic", "output=sg:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:h_fov=90.000:v_fov=50.625"),
            ("pannini", "output=pannini:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:h_fov=90.000:v_fov=50.625"),
        ] {
            let graph = rendered(&spec(reframe(output)), &equirect_context());
            assert_eq!(
                graph,
                format!("[0:v]v360@reframe=input=e:{expected}[spatial_1]"),
                "projection {output}"
            );
        }

        // A fisheye input is not held to the 2:1 rule and keeps its own token.
        let mut request = reframe("flat");
        request["reframe360"]["inputProjection"] = serde_json::json!("dfisheye");
        let graph = rendered(&spec_on(1_920, 1_080, request), &context(1_920, 1_080));
        assert!(graph.contains("v360@reframe=input=dfisheye:output=flat:"));
    }

    #[test]
    fn a_single_point_track_stays_a_static_option() {
        let mut request = reframe("flat");
        request["reframe360"]["yaw"] = serde_json::json!([{ "t": 0.0, "v": 45.0 }]);
        let graph = rendered(&spec(request), &equirect_context());
        assert!(graph.contains(":yaw=45.000:"), "{graph}");
        assert!(!graph.contains("sendcmd"), "{graph}");
        assert!(!graph.contains("reset_rot"), "{graph}");
    }

    #[test]
    fn an_animated_track_becomes_a_sendcmd_program() {
        let mut request = reframe("flat");
        request["reframe360"]["yaw"] = serde_json::json!([
            { "t": 0.0, "v": 0.0, "interp": "linear" },
            { "t": 2.0, "v": 90.0, "interp": "linear" },
        ]);
        let graph = rendered(&spec(request), &equirect_context());

        assert_eq!(
            graph,
            "[0:v]sendcmd=c='0-604800 [expr] v360@reframe yaw \
             (mod(mod(if(lt(T\\\\,0)\\\\,0\\\\,if(lt(T\\\\,2)\\\\,0+(90-0)*((T-0)/2)\\\\,90))+180\\\\,360)+360\\\\,360)-180)',\
             v360@reframe=input=e:output=flat:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:\
             h_fov=90.000:v_fov=50.625:reset_rot=1[spatial_1]"
        );
    }

    #[test]
    fn an_animated_fov_drives_both_axes() {
        let mut request = reframe("flat");
        request["reframe360"]["fov"] = serde_json::json!([
            { "t": 0.0, "v": 60.0, "interp": "linear" },
            { "t": 1.0, "v": 120.0, "interp": "linear" },
        ]);
        let graph = rendered(&spec(request), &equirect_context());

        assert!(graph.contains(":h_fov=60.000:v_fov=33.750:"), "{graph}");
        assert!(
            graph.contains("[expr] v360@reframe h_fov clip(if(lt(T\\\\,0)"),
            "{graph}"
        );
        assert!(
            graph.contains(")*0.562500\\\\,1\\\\,360)"),
            "vertical axis follows the output aspect: {graph}"
        );
        assert!(graph.contains(":reset_rot=1"), "{graph}");
    }

    #[test]
    fn angles_beyond_a_half_turn_are_folded_not_clipped() {
        let mut request = reframe("flat");
        request["reframe360"]["pitch"] = serde_json::json!([{ "t": 0.0, "v": 270.0 }]);
        let graph = rendered(&spec(request), &equirect_context());
        assert!(graph.contains(":pitch=-90.000:"), "{graph}");
        assert_eq!(wrap_degrees(-270.0), 90.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
    }

    #[test]
    fn horizon_lock_cancels_roll_entirely() {
        let mut request = reframe("flat");
        request["reframe360"]["horizonLock"] = serde_json::json!(true);
        request["reframe360"]["roll"] = serde_json::json!([
            { "t": 0.0, "v": 30.0, "interp": "linear" },
            { "t": 1.0, "v": 60.0, "interp": "linear" },
        ]);
        let graph = rendered(&spec(request), &equirect_context());

        assert!(graph.contains(":roll=0.000:"), "{graph}");
        assert!(!graph.contains("sendcmd"), "{graph}");
    }

    #[test]
    fn a_non_spherical_source_is_refused_before_it_renders() {
        let error = apply(&mut plan(), &spec(reframe("flat")), &context(1_920, 1_080))
            .expect_err("guard rejects a 16:9 source");
        assert_eq!(
            error.downcast_ref::<SpatialError>(),
            Some(&SpatialError::NotEquirectangular {
                width: 1_920,
                height: 1_080
            })
        );

        // An unknown probe size is not evidence against the client's claim.
        assert!(apply(&mut plan(), &spec(reframe("flat")), &context(0, 0)).is_ok());
    }

    #[test]
    fn lens_correction_emits_both_coefficients() {
        let graph = rendered(
            &spec(serde_json::json!({
                "videoId": "vid_1",
                "lensCorrection": { "k1": -0.227, "k2": 0.0125 }
            })),
            &equirect_context(),
        );
        assert_eq!(graph, "[0:v]lenscorrection=k1=-0.2270:k2=0.0125[spatial_1]");

        // Identity coefficients never reach the graph.
        let identity = rendered(
            &spec(serde_json::json!({
                "videoId": "vid_1",
                "lensCorrection": { "k1": 0.0, "k2": 0.0 }
            })),
            &equirect_context(),
        );
        assert_eq!(identity, "");
    }

    #[test]
    fn fast_stabilization_is_a_single_deshake_pass() {
        let graph = rendered(
            &spec(serde_json::json!({
                "videoId": "vid_1",
                "stabilize": { "mode": "fast", "smoothing": 50.0, "zoom": 5.0 }
            })),
            &equirect_context(),
        );
        assert_eq!(
            graph,
            "[0:v]deshake=rx=32:ry=32:edge=mirror,scale=w=4032:h=2016,crop=w=3840:h=1920[spatial_1]"
        );

        // Without a crop-in there is nothing to scale. The smallest dial still
        // quantizes up to the 16 px grid FFmpeg demands.
        let plain = rendered(
            &spec(serde_json::json!({
                "videoId": "vid_1",
                "stabilize": { "mode": "fast", "smoothing": 1.0 }
            })),
            &equirect_context(),
        );
        assert_eq!(plain, "[0:v]deshake=rx=16:ry=16:edge=mirror[spatial_1]");
    }

    #[test]
    fn fast_stabilization_crops_into_the_reframed_size() {
        let mut request = reframe("flat");
        request["stabilize"] = serde_json::json!({ "mode": "fast", "zoom": 10.0 });
        let graph = rendered(&spec(request), &equirect_context());
        assert!(
            graph.contains(",scale=w=2112:h=1188,crop=w=1920:h=1080["),
            "{graph}"
        );
    }

    #[test]
    fn precise_stabilization_reads_the_registered_transform_file() {
        let spec = spec(serde_json::json!({
            "videoId": "vid_1",
            "stabilize": { "mode": "precise", "smoothing": 24.0, "zoom": 3.0,
                           "horizonLock": true }
        }));
        let ctx = equirect_context().with_asset(TRANSFORMS_CONTEXT_KEY, "/work/job:1/vidstab.trf");
        assert_eq!(
            rendered(&spec, &ctx),
            "[0:v]vidstabtransform=input='/work/job\\:1/vidstab.trf':smoothing=24:\
             maxangle=-1:interpol=bilinear:zoom=3.000:optzoom=0[spatial_1]"
        );

        // Rotation correction is what the horizon lock switches on.
        let unlocked = spec_on(
            3_840,
            1_920,
            serde_json::json!({
                "videoId": "vid_1",
                "stabilize": { "mode": "precise", "smoothing": 24.0 }
            }),
        );
        assert!(rendered(&unlocked, &ctx).contains(":maxangle=0:"));
    }

    #[test]
    fn precise_stabilization_without_a_transform_file_is_a_typed_error() {
        let spec = spec(serde_json::json!({
            "videoId": "vid_1",
            "stabilize": { "mode": "precise" }
        }));
        let error = apply(&mut plan(), &spec, &equirect_context())
            .expect_err("two-pass needs its prepass output");
        assert_eq!(
            error.downcast_ref::<SpatialError>(),
            Some(&SpatialError::MissingTransformsPath)
        );
    }

    #[test]
    fn precise_stabilization_refuses_a_recut_timeline() {
        let spec = spec(serde_json::json!({
            "videoId": "vid_1",
            "segments": [{ "start": 0.0, "end": 2.0 }, { "start": 4.0, "end": 6.0 }],
            "stabilize": { "mode": "precise" }
        }));
        let ctx = equirect_context().with_asset(TRANSFORMS_CONTEXT_KEY, "/work/vidstab.trf");
        assert_eq!(
            apply(&mut plan(), &spec, &ctx)
                .expect_err("frame indices would desynchronize")
                .downcast_ref::<SpatialError>(),
            Some(&SpatialError::PreciseNeedsUncutTimeline)
        );
    }

    #[test]
    fn the_prepass_replays_the_optics_and_the_input_trim() {
        let mut request = reframe("flat");
        request["trim"] = serde_json::json!({ "start": 1.5, "end": 6.5 });
        request["lensCorrection"] = serde_json::json!({ "k1": -0.2, "k2": 0.0 });
        request["stabilize"] = serde_json::json!({ "mode": "precise", "smoothing": 40.0 });
        let spec = spec(request);
        let ctx = equirect_context().with_asset(TRANSFORMS_CONTEXT_KEY, "/work/vidstab.trf");

        let prepass = prepass_command(&plan(), &spec, &ctx)
            .expect("prepass builds")
            .expect("precise mode has a prepass");
        assert_eq!(prepass.transforms_path, PathBuf::from("/work/vidstab.trf"));
        assert_eq!(
            prepass.args,
            vec![
                "-y",
                "-hide_banner",
                "-v",
                "error",
                "-ss",
                "1.500",
                "-t",
                "5.000",
                "-i",
                "/media/source.mp4",
                "-an",
                "-vf",
                "lenscorrection=k1=-0.2000:k2=0.0000,v360@reframe=input=e:output=flat:w=1920:\
                 h=1080:yaw=0.000:pitch=0.000:roll=0.000:h_fov=90.000:v_fov=50.625,\
                 vidstabdetect=result='/work/vidstab.trf':shakiness=4:accuracy=15",
                "-f",
                "null",
                "-",
            ]
        );
    }

    #[test]
    fn only_precise_mode_has_a_prepass() {
        let ctx = equirect_context().with_asset(TRANSFORMS_CONTEXT_KEY, "/work/vidstab.trf");
        assert!(prepass_command(
            &plan(),
            &spec(serde_json::json!({ "videoId": "vid_1" })),
            &ctx
        )
        .expect("no stabilization")
        .is_none());
        let fast = spec(serde_json::json!({
            "videoId": "vid_1",
            "stabilize": { "mode": "fast" }
        }));
        assert!(prepass_command(&plan(), &fast, &ctx)
            .expect("single pass")
            .is_none());
    }

    #[test]
    fn the_whole_stage_stacks_optics_before_stabilization() {
        let mut request = reframe("stereographic");
        request["lensCorrection"] = serde_json::json!({ "k1": 0.1, "k2": -0.05 });
        request["stabilize"] = serde_json::json!({ "mode": "fast", "smoothing": 100.0 });
        let graph = rendered(&spec(request), &equirect_context());
        assert_eq!(
            graph,
            "[0:v]lenscorrection=k1=0.1000:k2=-0.0500,\
             v360@reframe=input=e:output=sg:w=1920:h=1080:yaw=0.000:pitch=0.000:roll=0.000:\
             h_fov=90.000:v_fov=50.625,deshake=rx=64:ry=64:edge=mirror[spatial_1]"
        );
    }
}
