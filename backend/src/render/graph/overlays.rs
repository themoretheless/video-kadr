//! Stage 8: image and video overlays. Owned by the overlays agent.
//!
//! Reads `spec.overlays()`, adds one plan input per overlay asset (still
//! images looped for the whole output), scales and rotates each layer, applies
//! `colorkey`/`chromakey` when the layer is keyed, fades it in and out over
//! its own window, and composites it with `overlay` at the normalized
//! position. Video overlays with audio enabled hand their branch to the audio
//! mixer stage.

use std::path::Path;

use crate::domain::edit::{EditSpec, Rotation};
use crate::domain::overlay::{OverlayKind, OverlaySpec};

use super::{ComplexPlan, InputSpec, RenderContext};

/// An overlay layer smaller than one macroblock is indistinguishable from a
/// rounding artefact, and `scale=0:...` is rejected by FFmpeg outright.
const MIN_LAYER_PIXELS: u32 = 2;
/// Upper bound for every pixel dimension derived from a normalized value, so a
/// clamped-but-large ratio can never overflow the `f64` -> `u32` cast.
const MAX_LAYER_PIXELS: u32 = 16_384;
/// Positions may sit off-frame (validated to -2..3 of the frame), so the
/// offsets stay signed and are bounded on the same scale as the dimensions.
const MAX_OFFSET_PIXELS: i64 = 65_536;

/// One overlay audio branch published for the audio mixer stage.
///
/// `overlays::apply` never touches the audio side of the plan: it only adds the
/// input and reports where the pad lives. `audio_mix` is expected to place the
/// pad on the output timeline (`start_seconds` / `end_seconds`) and apply
/// `volume` before summing it with `amix`.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayAudioPad {
    /// Index into `spec.overlays()`, so a mixer can report a useful error.
    pub overlay_index: usize,
    /// Index returned by `ComplexPlan::add_input` for the overlay asset.
    pub input_index: usize,
    /// Ready-to-use pad name, `"<input_index>:a"`, without the brackets.
    pub pad: String,
    /// Validated 0..4 linear gain from the wire `audio.volume`.
    pub volume: f64,
    /// Where the overlay starts on the OUTPUT timeline, seconds.
    pub start_seconds: f64,
    /// Where it ends, or `None` for "runs to the end of the output".
    pub end_seconds: Option<f64>,
}

/// Stage entry point used by `graph::compose`. The audio branches of video
/// overlays are dropped here; call `apply_with_audio` to receive them.
pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    apply_with_audio(plan, spec, ctx).map(|_| ())
}

/// Same as `apply`, but returns the audio pads of every video overlay whose
/// `audio.enabled` is set, in overlay order.
pub fn apply_with_audio(
    plan: &mut ComplexPlan,
    spec: &EditSpec,
    ctx: &RenderContext,
) -> anyhow::Result<Vec<OverlayAudioPad>> {
    let overlays = spec.overlays();
    if overlays.is_empty() {
        return Ok(Vec::new());
    }

    let (frame_width, frame_height) = output_frame_size(spec, ctx);
    let output_duration = if ctx.output_duration_seconds.is_finite() {
        ctx.output_duration_seconds.max(0.0)
    } else {
        0.0
    };
    let mut pads = Vec::new();

    for (index, overlay) in overlays.iter().enumerate() {
        let path = ctx.require_asset(overlay.asset_id())?;
        // The path becomes an argv element, never filter text, but a non-UTF-8
        // path would still reach FFmpeg as a lossy string. Reject it here.
        ensure_representable(path)?;
        let input = match overlay.kind() {
            // A still has a single frame, so it has to be looped for the whole
            // output; a video overlay carries its own timeline.
            OverlayKind::Image => InputSpec::still(path),
            OverlayKind::Video => InputSpec::source(path),
        };
        let input_index = plan.add_input(input);

        let layer = layer_label(
            plan,
            overlay,
            input_index,
            frame_width,
            frame_height,
            output_duration,
        );
        composite(
            plan,
            overlay,
            &layer,
            frame_width,
            frame_height,
            output_duration,
        );

        if let Some(audio) = overlay.audio().filter(|audio| audio.enabled()) {
            pads.push(OverlayAudioPad {
                overlay_index: index,
                input_index,
                pad: format!("{input_index}:a"),
                volume: audio.volume(),
                start_seconds: overlay.start_seconds(),
                end_seconds: overlay.end_seconds(),
            });
        }
    }

    Ok(pads)
}

/// Build the per-overlay preparation chain and return the pad it ends on.
fn layer_label(
    plan: &mut ComplexPlan,
    overlay: &OverlaySpec,
    input_index: usize,
    frame_width: u32,
    frame_height: u32,
    output_duration: f64,
) -> String {
    let mut filters: Vec<String> = Vec::new();
    // Every later step (keying, rotation fill, opacity, alpha fades) needs a
    // real alpha plane, and an opaque source has none.
    filters.push("format=rgba".into());

    if let Some(key) = overlay.chroma_key() {
        // `colorkey` works in RGB, which is exactly the format forced above;
        // `chromakey` would need a YUV detour for the same result.
        filters.push(format!(
            "colorkey={}:similarity={:.3}:blend={:.3}",
            key.color().ffmpeg_color(),
            key.similarity(),
            key.blend()
        ));
    }

    let width = to_pixels(overlay.width() * f64::from(frame_width));
    let height = match overlay.height() {
        Some(height) => to_pixels(height * f64::from(frame_height)).to_string(),
        // A null height keeps the overlay's own aspect ratio.
        None => "-1".to_owned(),
    };
    filters.push(format!("scale={width}:{height}"));

    let rotation = overlay.rotation_degrees();
    if rotation.abs() > 1e-6 {
        let radians = rotation.to_radians();
        // `c=none` fills the corners the rotation exposes with transparency
        // instead of black, and rotw/roth grow the layer to fit the rotation.
        filters.push(format!(
            "rotate={radians:.6}:c=none:ow=rotw({radians:.6}):oh=roth({radians:.6})"
        ));
    }

    let opacity = overlay.opacity();
    if opacity < 1.0 - 1e-6 {
        filters.push(format!("colorchannelmixer=aa={opacity:.3}"));
    }

    // Shift the layer onto the output timeline before the fades, so their
    // `st=` values are expressed in output seconds like every other stage.
    let start = overlay.start_seconds();
    if start > 1e-6 {
        filters.push(format!("setpts=PTS-STARTPTS+{start:.3}/TB"));
    }

    let end = layer_end(overlay, output_duration);
    let fade_in = overlay.fade_in_seconds();
    if fade_in > 1e-6 {
        filters.push(format!("fade=t=in:st={start:.3}:d={fade_in:.3}:alpha=1"));
    }
    let fade_out = overlay.fade_out_seconds();
    if let Some(end) = end.filter(|end| fade_out > 1e-6 && end - fade_out > start) {
        filters.push(format!(
            "fade=t=out:st={:.3}:d={fade_out:.3}:alpha=1",
            end - fade_out
        ));
    }

    let label = plan.next_label("overlay");
    plan.push(format!("[{input_index}:v]{}[{label}]", filters.join(",")));
    label
}

/// Composite the prepared layer onto the current terminal video pad.
fn composite(
    plan: &mut ComplexPlan,
    overlay: &OverlaySpec,
    layer: &str,
    frame_width: u32,
    frame_height: u32,
    output_duration: f64,
) {
    let (x, y) = overlay.position();
    let x = to_offset(x * f64::from(frame_width));
    let y = to_offset(y * f64::from(frame_height));
    // `eof_action=pass` lets a picture-in-picture clip shorter than the main
    // video simply disappear instead of freezing on its last frame.
    let mut options = format!("overlay=x={x}:y={y}:format=auto:eof_action=pass");
    if let Some(enable) = enable_expression(overlay, output_duration) {
        options.push_str(&format!(":enable='{enable}'"));
    }

    let current = plan.video_label().to_owned();
    let label = plan.next_label("composite");
    plan.push(format!("[{current}][{layer}]{options}[{label}]"));
    plan.set_video_label(label);
}

/// End of the overlay window on the output timeline. An overlay without an
/// explicit end runs until the output does; a zero-length output leaves it open.
fn layer_end(overlay: &OverlaySpec, output_duration: f64) -> Option<f64> {
    match overlay.end_seconds() {
        Some(end) => Some(end),
        None if output_duration > overlay.start_seconds() => Some(output_duration),
        None => None,
    }
}

/// `enable=` value for the timed window, or `None` when the overlay is on for
/// the whole output and the filter does not need gating at all.
fn enable_expression(overlay: &OverlaySpec, output_duration: f64) -> Option<String> {
    let start = overlay.start_seconds();
    match (overlay.end_seconds(), start > 1e-6) {
        (Some(end), _) => Some(format!("between(t,{start:.3},{end:.3})")),
        (None, true) if output_duration > start => {
            Some(format!("between(t,{start:.3},{output_duration:.3})"))
        }
        (None, true) => Some(format!("gte(t,{start:.3})")),
        (None, false) => None,
    }
}

/// The frame size overlays are positioned against.
///
/// `RenderContext` only carries the primary source dimensions, so the geometry
/// the earlier stages emit (crop, rotate, scale, pad) is replayed here to reach
/// the frame size that exists at stage 8.
fn output_frame_size(spec: &EditSpec, ctx: &RenderContext) -> (u32, u32) {
    let geometry = spec.geometry();
    let (mut width, mut height) = (ctx.width.max(1), ctx.height.max(1));

    if let Some(crop) = geometry.crop {
        width = crop.width.max(1);
        height = crop.height.max(1);
    }
    if matches!(
        geometry.rotation,
        Rotation::Clockwise90 | Rotation::Clockwise270
    ) {
        std::mem::swap(&mut width, &mut height);
    }
    if let Some(scale) = geometry.scale {
        // A negative scale dimension is FFmpeg's "derive from the aspect
        // ratio" marker, so the other side drives it.
        let (source_width, source_height) = (f64::from(width), f64::from(height));
        let (scaled_width, scaled_height) = match (scale.width, scale.height) {
            (w, h) if w > 0 && h > 0 => (w as u32, h as u32),
            (w, _) if w > 0 => (
                w as u32,
                to_pixels(f64::from(w) * source_height / source_width),
            ),
            (_, h) if h > 0 => (
                to_pixels(f64::from(h) * source_width / source_height),
                h as u32,
            ),
            _ => (width, height),
        };
        width = scaled_width;
        height = scaled_height;
    }
    if let Some(aspect) = geometry.pad_aspect {
        let target_width = f64::from(aspect.width.max(1));
        let target_height = f64::from(aspect.height.max(1));
        let (current_width, current_height) = (f64::from(width), f64::from(height));
        width = to_pixels(
            (current_width.max(current_height * target_width / target_height) / 2.0).ceil() * 2.0,
        );
        height = to_pixels(
            (current_height.max(current_width * target_height / target_width) / 2.0).ceil() * 2.0,
        );
    }

    (width.max(MIN_LAYER_PIXELS), height.max(MIN_LAYER_PIXELS))
}

/// Turn a normalized dimension into an even pixel count. Every input is already
/// range-clamped by the domain, but a non-finite value would still poison the
/// cast, so it collapses to the minimum instead.
fn to_pixels(value: f64) -> u32 {
    if !value.is_finite() {
        return MIN_LAYER_PIXELS;
    }
    let rounded = value
        .round()
        .clamp(f64::from(MIN_LAYER_PIXELS), f64::from(MAX_LAYER_PIXELS)) as u32;
    (rounded & !1).max(MIN_LAYER_PIXELS)
}

/// Signed pixel offset for an overlay corner, which may legitimately be
/// negative when the layer hangs off the frame.
fn to_offset(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(-MAX_OFFSET_PIXELS as f64, MAX_OFFSET_PIXELS as f64) as i64
}

/// FFmpeg receives paths as argv elements, so they need no filtergraph
/// escaping, but they do have to survive the trip as text.
fn ensure_representable(path: &Path) -> anyhow::Result<()> {
    if path.to_str().is_none() {
        anyhow::bail!("overlay asset path is not valid UTF-8");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    const LOGO: &str = "ast_0123456789abcdef";
    const CLIP: &str = "ast_fedcba9876543210";

    fn spec_from(request: Value) -> EditSpec {
        let request: crate::model::EditRequest = serde_json::from_value(request).unwrap();
        let extensions = crate::domain::edit::EditExtensions::from_request(&request).unwrap();
        crate::services::render::EditPlan::compile(
            crate::domain::artifact_graph::Fingerprint::digest(b"source"),
            request,
            crate::services::render::SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap()
    }

    fn context() -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, true, Some(30.0), 10.0)
            .with_asset(LOGO, "/private/assets/logo.png")
            .with_asset(CLIP, "/private/assets/pip.mp4")
    }

    fn plan() -> ComplexPlan {
        ComplexPlan::new(InputSpec::source("/in.mp4"), true)
    }

    fn run(request: Value) -> (ComplexPlan, Vec<OverlayAudioPad>) {
        let spec = spec_from(request);
        let ctx = context();
        let mut plan = plan();
        let pads = apply_with_audio(&mut plan, &spec, &ctx).unwrap();
        (plan, pads)
    }

    #[test]
    fn an_edit_without_overlays_emits_nothing() {
        let (plan, pads) = run(json!({ "videoId": "x" }));
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
        assert!(pads.is_empty());
    }

    #[test]
    fn an_image_overlay_loops_its_input_and_composites_at_pixel_offsets() {
        let (plan, pads) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": LOGO,
                "kind": "image",
                "x": 0.05, "y": 0.1,
                "width": 0.25
            }]
        }));

        assert_eq!(plan.inputs().len(), 2);
        assert!(plan.inputs()[1].loop_still);
        assert_eq!(
            plan.inputs()[1].path,
            std::path::PathBuf::from("/private/assets/logo.png")
        );
        assert_eq!(
            plan.render(),
            "[1:v]format=rgba,scale=480:-1[overlay_1];\
             [0:v][overlay_1]overlay=x=96:y=108:format=auto:eof_action=pass[composite_2]"
        );
        assert_eq!(plan.video_label(), "composite_2");
        assert_eq!(plan.audio_label(), Some("0:a"));
        assert!(pads.is_empty());
    }

    #[test]
    fn a_video_overlay_is_not_looped_and_publishes_its_audio_pad() {
        let (plan, pads) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": CLIP,
                "kind": "video",
                "x": 0.6, "y": 0.6,
                "width": 0.3, "height": 0.3,
                "start": 2.0, "end": 6.0,
                "audio": { "enabled": true, "volume": 0.8 }
            }]
        }));

        assert!(!plan.inputs()[1].loop_still);
        assert!(plan.render().contains("scale=576:324"));
        assert_eq!(pads.len(), 1);
        assert_eq!(
            pads[0],
            OverlayAudioPad {
                overlay_index: 0,
                input_index: 1,
                pad: "1:a".to_owned(),
                volume: 0.8,
                start_seconds: 2.0,
                end_seconds: Some(6.0),
            }
        );
    }

    #[test]
    fn a_disabled_or_image_audio_branch_publishes_no_pad() {
        let (_, pads) = run(json!({
            "videoId": "x",
            "overlays": [
                {
                    "assetId": CLIP, "kind": "video", "x": 0.0, "y": 0.0, "width": 0.5,
                    "audio": { "enabled": false, "volume": 1.0 }
                },
                {
                    "assetId": LOGO, "kind": "image", "x": 0.0, "y": 0.0, "width": 0.5,
                    "audio": { "enabled": true, "volume": 1.0 }
                }
            ]
        }));
        assert!(pads.is_empty());
    }

    #[test]
    fn a_timed_overlay_gets_an_enable_window_and_alpha_fades() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": LOGO,
                "kind": "image",
                "x": 0.0, "y": 0.0,
                "width": 0.5,
                "start": 2.0, "end": 6.0,
                "fadeIn": 0.5, "fadeOut": 1.0
            }]
        }));

        let rendered = plan.render();
        assert!(rendered.contains("setpts=PTS-STARTPTS+2.000/TB"));
        assert!(rendered.contains("fade=t=in:st=2.000:d=0.500:alpha=1"));
        assert!(rendered.contains("fade=t=out:st=5.000:d=1.000:alpha=1"));
        assert!(rendered.contains(":enable='between(t,2.000,6.000)'"));
    }

    #[test]
    fn an_open_ended_overlay_is_gated_against_the_output_duration() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": LOGO, "kind": "image",
                "x": 0.0, "y": 0.0, "width": 0.5,
                "start": 3.0, "fadeOut": 2.0
            }]
        }));

        let rendered = plan.render();
        assert!(rendered.contains(":enable='between(t,3.000,10.000)'"));
        assert!(rendered.contains("fade=t=out:st=8.000:d=2.000:alpha=1"));
    }

    #[test]
    fn chroma_key_and_opacity_become_colorkey_and_alpha_mixing() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": CLIP,
                "kind": "video",
                "x": 0.0, "y": 0.0,
                "width": 1.0,
                "opacity": 0.4,
                "rotation": 30.0,
                "chromaKey": { "color": "#00FF00", "similarity": 0.12, "blend": 0.05 }
            }]
        }));

        let rendered = plan.render();
        assert!(rendered.contains("format=rgba,colorkey=0x00FF00:similarity=0.120:blend=0.050"));
        assert!(rendered.contains("rotate=0.523599:c=none:ow=rotw(0.523599):oh=roth(0.523599)"));
        assert!(rendered.contains("colorchannelmixer=aa=0.400"));
        // The key runs before the resize, so it sees unresampled edges.
        let key = rendered.find("colorkey").unwrap();
        let scale = rendered.find("scale=").unwrap();
        assert!(key < scale);
    }

    #[test]
    fn overlay_geometry_follows_the_scaled_output_frame() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "scale": { "w": 640, "h": 360 },
            "overlays": [{
                "assetId": LOGO, "kind": "image",
                "x": 0.5, "y": 0.5, "width": 0.5, "height": 0.5
            }]
        }));

        let rendered = plan.render();
        assert!(rendered.contains("scale=320:180"));
        assert!(rendered.contains("overlay=x=320:y=180"));
    }

    #[test]
    fn a_negative_position_stays_a_signed_offset() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": LOGO, "kind": "image",
                "x": -0.1, "y": -0.25, "width": 0.5
            }]
        }));
        assert!(plan.render().contains("overlay=x=-192:y=-270"));
    }

    #[test]
    fn several_overlays_stack_in_declaration_order() {
        let (plan, _) = run(json!({
            "videoId": "x",
            "overlays": [
                { "assetId": LOGO, "kind": "image", "x": 0.0, "y": 0.0, "width": 0.2 },
                { "assetId": CLIP, "kind": "video", "x": 0.5, "y": 0.5, "width": 0.2 }
            ]
        }));

        assert_eq!(plan.inputs().len(), 3);
        let rendered = plan.render();
        assert!(rendered.contains("[0:v][overlay_1]"));
        assert!(rendered.contains("[composite_2][overlay_3]"));
        assert_eq!(plan.video_label(), "composite_4");
    }

    #[test]
    fn an_unresolved_asset_fails_the_render_instead_of_dropping_the_layer() {
        let spec = spec_from(json!({
            "videoId": "x",
            "overlays": [{
                "assetId": "ast_ffffffffffffffff",
                "kind": "image",
                "x": 0.0, "y": 0.0, "width": 0.5
            }]
        }));
        let mut plan = plan();
        assert!(apply(&mut plan, &spec, &context()).is_err());
    }
}
