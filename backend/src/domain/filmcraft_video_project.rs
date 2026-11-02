//! Filmcraft layer applicability after parameter conversion.
//!
//! Upstream render/src/lib.rs excludes graphic layers from the media effect stack.
//! Keep both ordered groups so callers can preserve layers on media items and use
//! them as source content on graphic items. This does not parse a Filmcraft project.
use super::{
    composition::VideoEffect,
    graphic_clip::{GraphicClip, GraphicContent, GraphicLayer},
};
use anyhow::Result;

/// Attach a converted scalar curve using the source parameter spelling.
/// Curve times remain on their authored clock; the render caller selects that clock.
/// Lumetri scalar controls are routed into their native grading sections.
pub fn with_scalar_animation(
    effect: VideoEffect,
    source_parameter: &str,
    curve: super::composition::AnimatableValue,
) -> Result<VideoEffect> {
    anyhow::ensure!(
        !source_parameter.is_empty()
            && source_parameter
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "invalid source scalar parameter name"
    );
    let mut channel = String::new();
    let mut uppercase = false;
    for c in source_parameter.chars() {
        if c == '_' {
            uppercase = true;
        } else {
            channel.push(if uppercase { c.to_ascii_uppercase() } else { c });
            uppercase = false;
        }
    }
    let mut wire = serde_json::to_value(effect)?;
    let kind = wire["kind"]
        .as_str()
        .expect("tagged video effect")
        .to_string();
    let scalar = match kind.as_str() {
        "color_tint" => Some("amount"),
        "gaussian_blur" | "gaussian_blur_legacy" | "vr_blur" => Some("blurriness"),
        "sharpen" | "vr_sharpen" | "vr_denoise" | "edge_feather" => Some("amount"),
        "camera_blur" => Some("percent"),
        "color_pass" | "color_replace" => Some("similarity"),
        "gamma_correction" => Some("gamma"),
        "color_invert" | "find_edges" => Some("blend"),
        "posterize" => Some("levels"),
        "replicate" => Some("count"),
        "median" => Some("radius"),
        "compound_blur" => Some("max"),
        "reduce_interlace_flicker" => Some("softness"),
        "alpha_adjust" => Some("opacity"),
        _ => None,
    };
    let curve = serde_json::to_value(curve)?;
    let retained_path;
    if let Some(scalar) = scalar {
        anyhow::ensure!(
            source_parameter == scalar,
            "unsupported scalar channel for {kind}"
        );
        wire["animation"] = curve.clone();
        retained_path = "/animation".to_string();
    } else if kind == "lumetri" {
        let (section, target) = lumetri_scalar_target(source_parameter).ok_or_else(|| {
            anyhow::anyhow!("unsupported Lumetri scalar channel: {source_parameter}")
        })?;
        let parameters = wire["parameters"]
            .as_object_mut()
            .expect("Lumetri parameters");
        let animation = parameters
            .entry("animation")
            .or_insert_with(|| serde_json::json!({}));
        if animation.is_null() {
            *animation = serde_json::json!({});
        }
        let animation = animation
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("invalid Lumetri animation"))?;
        if section.is_empty() {
            animation.insert(target.into(), curve.clone());
            retained_path = format!("/parameters/animation/{target}");
        } else {
            let channels = animation
                .entry(section)
                .or_insert_with(|| serde_json::json!({}));
            channels
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("invalid Lumetri {section} animation"))?
                .insert(target.into(), curve.clone());
            retained_path = format!("/parameters/animation/{section}/{target}");
        }
    } else {
        let object = wire.as_object_mut().expect("tagged video effect");
        let animation = object
            .entry("animation")
            .or_insert_with(|| serde_json::json!({}));
        if animation.is_null() {
            *animation = serde_json::json!({});
        }
        animation
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("effect animation is not a channel map"))?
            .insert(channel.clone(), curve.clone());
        retained_path = format!("/animation/{channel}");
    }
    let effect: VideoEffect = serde_json::from_value(wire)?;
    effect.validate()?;
    let restored = serde_json::to_value(&effect)?;
    anyhow::ensure!(
        restored.pointer(&retained_path) == Some(&curve),
        "scalar animation was not retained"
    );
    Ok(effect)
}

/// Attach an actual serialized Filmcraft Float Param without resampling its keys.
/// The receiving clip must select EffectTiming::FilmcraftSource for evaluation.
pub fn with_source_scalar_animation(
    effect: VideoEffect,
    source_parameter: &str,
    parameter: &serde_json::Value,
) -> Result<VideoEffect> {
    with_scalar_animation(
        effect,
        source_parameter,
        super::filmcraft_scalar_curve::convert(parameter)?,
    )
}

/// Preserve explicit point curves for offsets, generator geometry and color wheels.
/// Other point controls require an explicit native channel binding.
pub fn with_source_point_animation(
    effect: VideoEffect,
    source_parameter: &str,
    parameter: &serde_json::Value,
) -> Result<VideoEffect> {
    with_source_point_curves(
        effect,
        source_parameter,
        super::filmcraft_component_curve::point(parameter)?,
    )
}

fn with_source_point_curves(
    effect: VideoEffect,
    source_parameter: &str,
    curves: [super::composition::AnimatableValue; 2],
) -> Result<VideoEffect> {
    anyhow::ensure!(
        match &effect {
            VideoEffect::Move { .. } => matches!(source_parameter, "from" | "to"),
            VideoEffect::GraphicText { .. } => matches!(source_parameter, "position" | "anchor"),
            VideoEffect::GraphicShape { .. } =>
                matches!(source_parameter, "size" | "position" | "anchor"),
            VideoEffect::Transform { .. } => matches!(source_parameter, "anchor" | "position"),
            VideoEffect::Timecode { .. }
            | VideoEffect::ClipName { .. }
            | VideoEffect::SimpleText { .. }
            | VideoEffect::MetadataBurnin { .. } => source_parameter == "position",
            VideoEffect::LightingEffects { .. } => matches!(
                source_parameter,
                "l1_center" | "l2_center" | "l3_center" | "l4_center" | "l5_center"
            ),
            VideoEffect::CornerPin { .. } => matches!(
                source_parameter,
                "upper_left" | "upper_right" | "lower_left" | "lower_right"
            ),
            VideoEffect::Mirror { .. }
            | VideoEffect::Twirl { .. }
            | VideoEffect::TwirlLegacy { .. }
            | VideoEffect::Magnify { .. }
            | VideoEffect::Spherize { .. }
            | VideoEffect::RgbSplit { .. }
            | VideoEffect::VolumetricRays { .. }
            | VideoEffect::Grow { .. }
            | VideoEffect::Shrink { .. }
            | VideoEffect::Spin { .. }
            | VideoEffect::FocusBlur { .. } => source_parameter == "center",
            VideoEffect::Offset { .. } => source_parameter == "shift",
            VideoEffect::TurbulentDisplace { .. }
            | VideoEffect::RoughenEdges { .. }
            | VideoEffect::AutoReframe { .. }
            | VideoEffect::CellPattern { .. } => source_parameter == "offset",
            VideoEffect::Ramp { .. }
            | VideoEffect::Gradient { .. }
            | VideoEffect::Lightning { .. } => matches!(source_parameter, "start" | "end"),
            VideoEffect::Circle { .. }
            | VideoEffect::LensFlare { .. }
            | VideoEffect::Ellipse { .. } => source_parameter == "center",
            VideoEffect::Checkerboard { .. } => source_parameter == "anchor",
            VideoEffect::PaintBucket { .. } => source_parameter == "point",
            VideoEffect::WriteOn { .. } => source_parameter == "brush",
            VideoEffect::Lumetri { .. } => matches!(
                source_parameter,
                "wheel_shadows" | "wheel_midtones" | "wheel_highlights"
            ),
            _ => false,
        },
        "unsupported source point control"
    );
    let [x, y] = curves;
    let effect = with_scalar_animation(effect, &format!("{source_parameter}_x"), x)?;
    with_scalar_animation(effect, &format!("{source_parameter}_y"), y)
}

/// Import points after the original project loader's geometry-dependent baseline
/// resolution. Authored keys remain unchanged and use the source clock.
pub fn with_source_point_animation_in_geometry(
    effect: VideoEffect,
    source_effect: &str,
    source_parameter: &str,
    parameter: &serde_json::Value,
    frame: (u32, u32),
    source_size: (u32, u32),
) -> Result<VideoEffect> {
    let resolved = super::filmcraft_component_curve::resolve_point_baseline(
        parameter,
        source_effect,
        source_parameter,
        frame,
        source_size,
    )?;
    let keyed_auto = resolved
        .get("keyframes")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|keys| {
            keys.iter().any(|key| {
                ["x", "y"].iter().any(|axis| {
                    key.pointer(&format!("/value/Vec2/{axis}"))
                        .is_some_and(serde_json::Value::is_null)
                })
            })
        });
    if !keyed_auto {
        return with_source_point_animation(effect, source_parameter, &resolved);
    }
    if matches!(source_effect, "graphic_text" | "graphic_shape") {
        let curves = if source_parameter == "position" {
            let fallback = [f64::from(frame.0) / 2., f64::from(frame.1) / 2.];
            super::filmcraft_component_curve::point_with_runtime_fallback(&resolved, fallback)?;
            let mut coupled = resolved.clone();
            for key in coupled["keyframes"].as_array_mut().expect("checked keys") {
                if ["x", "y"].iter().any(|axis| {
                    key.pointer(&format!("/value/Vec2/{axis}"))
                        .is_some_and(serde_json::Value::is_null)
                }) {
                    key["value"]["Vec2"]["x"] = serde_json::Value::Null;
                    key["value"]["Vec2"]["y"] = serde_json::Value::Null;
                }
            }
            super::filmcraft_component_curve::point_with_runtime_fallback(&coupled, fallback)?
        } else if source_effect == "graphic_shape" && source_parameter == "size" {
            // Original eval_layer applies NaN.max(0), independently per axis.
            super::filmcraft_component_curve::point_with_runtime_fallback(&resolved, [0., 0.])?
        } else {
            anyhow::bail!("automatic graphic anchor produces a nonfinite original transform");
        };
        return with_source_point_curves(effect, source_parameter, curves);
    }
    anyhow::ensure!(
        !matches!(source_effect, "motion" | "lumetri"),
        "automatic keyed points require a dedicated layer evaluator"
    );
    let (fx, fy) = match (source_effect, source_parameter) {
        ("move", _) | ("roughen_edges" | "auto_reframe" | "cell_pattern", "offset") => (0., 0.),
        ("corner_pin", "upper_left") => (0., 0.),
        ("corner_pin", "upper_right") => (1., 0.),
        ("corner_pin", "lower_left") => (0., 1.),
        ("corner_pin", "lower_right") => (1., 1.),
        ("gradient", "start") => (0.5, 0.),
        ("gradient", "end") => (0.5, 1.),
        ("lightning", "start") => (0.1, 0.1),
        ("lightning", "end") => (0.9, 0.9),
        ("checkerboard", "anchor") => (0., 0.),
        ("volumetric_rays", "center") => (0.5, 0.25),
        _ => (0.5, 0.5),
    };
    let curves = super::filmcraft_component_curve::point_with_runtime_fallback(
        &resolved,
        [f64::from(frame.0) * fx, f64::from(frame.1) * fy],
    )?;
    with_source_point_curves(effect, source_parameter, curves)
}

/// Resolve and import the complete point-parameter group of an effect instance.
/// Loading only one point leaves other automatic defaults unresolved (for
/// example Ramp's end uses frame height at project load, not runtime centre).
pub fn with_source_point_parameters_in_geometry(
    mut effect: VideoEffect,
    source_effect: &str,
    parameters: &serde_json::Value,
    frame: (u32, u32),
    source_size: (u32, u32),
) -> Result<VideoEffect> {
    let points = parameters
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid source point parameters"))?;
    for (name, parameter) in points {
        effect = with_source_point_animation_in_geometry(
            effect,
            source_effect,
            name,
            parameter,
            frame,
            source_size,
        )?;
    }
    Ok(effect)
}

/// Attach original RGBA curves to native color correction, keying and generator channels.
/// Preserve alpha and all other channels; evaluate using FilmcraftSource time.
pub fn with_source_color_animation(
    effect: VideoEffect,
    source_parameter: &str,
    parameter: &serde_json::Value,
) -> Result<VideoEffect> {
    if matches!(&effect, VideoEffect::Lumetri { .. }) {
        anyhow::ensure!(
            matches!(source_parameter, "shadow_tint" | "highlight_tint"),
            "unsupported source Lumetri color control"
        );
        let curves = super::filmcraft_component_curve::color(parameter)?;
        let mut effect = effect;
        for (suffix, curve) in ["red", "green", "blue", "alpha"].into_iter().zip(curves) {
            effect = with_scalar_animation(effect, &format!("{source_parameter}_{suffix}"), curve)?;
        }
        return Ok(effect);
    }
    let prefix = match (&effect, source_parameter) {
        (VideoEffect::ColorTint { .. }, "black") => "black",
        (VideoEffect::ColorTint { .. }, "white") => "white",
        (VideoEffect::ColorReplace { .. }, "target") => "target",
        (VideoEffect::ColorReplace { .. }, "replace") => "replace",
        (VideoEffect::ChangeToColor { .. }, "from") => "from",
        (VideoEffect::ChangeToColor { .. }, "to") => "to",
        (
            VideoEffect::ColorPass { .. }
            | VideoEffect::ColorKey { .. }
            | VideoEffect::LeaveColor { .. },
            "color",
        ) => "",
        (VideoEffect::UltraKey { .. }, "key_color") => "",
        (VideoEffect::GraphicText { .. } | VideoEffect::GraphicShape { .. }, "fill_color") => {
            "fill"
        }
        (VideoEffect::GraphicText { .. } | VideoEffect::GraphicShape { .. }, "stroke_color") => {
            "stroke"
        }
        (VideoEffect::GraphicText { .. } | VideoEffect::GraphicShape { .. }, "stroke2_color") => {
            "stroke2"
        }
        (
            VideoEffect::GraphicText { .. } | VideoEffect::GraphicShape { .. },
            "background_color",
        ) => "background",
        (VideoEffect::GraphicText { .. } | VideoEffect::GraphicShape { .. }, "shadow_color") => {
            "shadow"
        }
        (
            VideoEffect::SimpleText { .. }
            | VideoEffect::VignetteEffect { .. }
            | VideoEffect::WriteOn { .. },
            "color",
        ) => "color",
        (VideoEffect::VideoLimiter { .. }, "warning_color") => "",
        (VideoEffect::Lightning { .. }, "outside") => "outside",
        (VideoEffect::Lightning { .. }, "inside") => "inside",
        (VideoEffect::LightingEffects { .. }, "l1_color") => "l1Color",
        (VideoEffect::LightingEffects { .. }, "l2_color") => "l2Color",
        (VideoEffect::LightingEffects { .. }, "l3_color") => "l3Color",
        (VideoEffect::LightingEffects { .. }, "l4_color") => "l4Color",
        (VideoEffect::LightingEffects { .. }, "l5_color") => "l5Color",
        (VideoEffect::LightingEffects { .. }, "ambient_color") => "ambientColor",
        (
            VideoEffect::Grid { .. }
            | VideoEffect::LensFlare { .. }
            | VideoEffect::LogoCutout { .. }
            | VideoEffect::Spacer { .. }
            | VideoEffect::Stroke { .. }
            | VideoEffect::MetadataBurnin { .. }
            | VideoEffect::Checkerboard { .. }
            | VideoEffect::PaintBucket { .. },
            "color",
        ) => "color",
        (VideoEffect::Gradient { .. }, "start_color") => "startColor",
        (VideoEffect::Gradient { .. }, "end_color") => "endColor",
        (VideoEffect::Ellipse { .. }, "inside") => "inside",
        (VideoEffect::Ellipse { .. }, "outside") => "outside",
        (VideoEffect::RoundedCrop { .. }, "border_color") => "border",
        (VideoEffect::RoughenEdges { .. }, "edge_color") => "edgeColor",
        (
            VideoEffect::EchoGlow { .. }
            | VideoEffect::EdgeGlow { .. }
            | VideoEffect::Glint { .. }
            | VideoEffect::VolumetricRays { .. }
            | VideoEffect::WonderGlow { .. }
            | VideoEffect::LongShadow { .. }
            | VideoEffect::BevelAlpha { .. },
            "color",
        ) => "color",
        (VideoEffect::LightLeaks { .. }, "c1") => "c1",
        (VideoEffect::LightLeaks { .. }, "c2") => "c2",
        (VideoEffect::AlphaGlow { .. }, "start_color") => "startColor",
        (VideoEffect::AlphaGlow { .. }, "end_color") => "endColor",
        (VideoEffect::VrGlow { .. }, "tint") => "tint",
        (VideoEffect::VrColorGradients { .. }, "c1") => "c1",
        (VideoEffect::VrColorGradients { .. }, "c2") => "c2",
        (VideoEffect::VrColorGradients { .. }, "c3") => "c3",
        (VideoEffect::FourColorGradient { .. }, "c1") => "c1",
        (VideoEffect::FourColorGradient { .. }, "c2") => "c2",
        (VideoEffect::FourColorGradient { .. }, "c3") => "c3",
        (VideoEffect::FourColorGradient { .. }, "c4") => "c4",
        (VideoEffect::Ramp { .. }, "start_color") => "startColor",
        (VideoEffect::Ramp { .. }, "end_color") => "endColor",
        (
            VideoEffect::Circle { .. }
            | VideoEffect::DropShadow { .. }
            | VideoEffect::Strobe { .. },
            "color",
        ) => "color",
        _ => anyhow::bail!("unsupported source color control"),
    };
    let curves = super::filmcraft_component_curve::color(parameter)?;
    let field = if matches!(
        &effect,
        VideoEffect::FourColorGradient { .. }
            | VideoEffect::VrColorGradients { .. }
            | VideoEffect::Ramp { .. }
            | VideoEffect::Circle { .. }
            | VideoEffect::DropShadow { .. }
            | VideoEffect::Strobe { .. }
            | VideoEffect::EchoGlow { .. }
            | VideoEffect::EdgeGlow { .. }
            | VideoEffect::Glint { .. }
            | VideoEffect::VolumetricRays { .. }
            | VideoEffect::WonderGlow { .. }
            | VideoEffect::LongShadow { .. }
            | VideoEffect::BevelAlpha { .. }
            | VideoEffect::LightLeaks { .. }
            | VideoEffect::AlphaGlow { .. }
            | VideoEffect::VrGlow { .. }
            | VideoEffect::Grid { .. }
            | VideoEffect::LensFlare { .. }
            | VideoEffect::LogoCutout { .. }
            | VideoEffect::Spacer { .. }
            | VideoEffect::Stroke { .. }
            | VideoEffect::MetadataBurnin { .. }
            | VideoEffect::Checkerboard { .. }
            | VideoEffect::PaintBucket { .. }
            | VideoEffect::Gradient { .. }
            | VideoEffect::Ellipse { .. }
            | VideoEffect::RoundedCrop { .. }
            | VideoEffect::RoughenEdges { .. }
            | VideoEffect::SimpleText { .. }
            | VideoEffect::VignetteEffect { .. }
            | VideoEffect::WriteOn { .. }
            | VideoEffect::VideoLimiter { .. }
            | VideoEffect::Lightning { .. }
            | VideoEffect::LightingEffects { .. }
            | VideoEffect::GraphicText { .. }
            | VideoEffect::GraphicShape { .. }
    ) {
        "animation"
    } else {
        "colorAnimation"
    };
    let suffixes = if matches!(
        &effect,
        VideoEffect::EchoGlow { .. }
            | VideoEffect::EdgeGlow { .. }
            | VideoEffect::Glint { .. }
            | VideoEffect::VolumetricRays { .. }
            | VideoEffect::WonderGlow { .. }
            | VideoEffect::LightLeaks { .. }
            | VideoEffect::LogoCutout { .. }
            | VideoEffect::VignetteEffect { .. }
            | VideoEffect::LightingEffects { .. }
            | VideoEffect::VideoLimiter { .. }
    ) {
        ["Red", "Green", "Blue", "Alpha"]
    } else {
        ["R", "G", "B", "A"]
    };
    let mut wire = serde_json::to_value(effect)?;
    if wire.get(field).is_none_or(serde_json::Value::is_null) {
        wire[field] = serde_json::json!({});
    }
    let mut retained = Vec::new();
    for (suffix, curve) in suffixes.into_iter().zip(curves) {
        let channel = if prefix.is_empty() {
            suffix.to_ascii_lowercase()
        } else {
            format!("{prefix}{suffix}")
        };
        let curve = serde_json::to_value(curve)?;
        wire[field][&channel] = curve.clone();
        retained.push((channel, curve));
    }
    let effect: VideoEffect = serde_json::from_value(wire)?;
    effect.validate()?;
    let restored = serde_json::to_value(&effect)?;
    for (channel, curve) in retained {
        anyhow::ensure!(
            restored[field][channel] == curve,
            "source color channel not retained"
        );
    }
    Ok(effect)
}

/// Source scalar controls and point-component bindings for native SDR grading.
/// HDR white/specular/curve range require HDR grading and are deliberately refused.
fn lumetri_scalar_target(source: &str) -> Option<(&'static str, &'static str)> {
    Some(match source {
        "shadow_tint_red" => ("creative", "shadowRed"),
        "shadow_tint_green" => ("creative", "shadowGreen"),
        "shadow_tint_blue" => ("creative", "shadowBlue"),
        "shadow_tint_alpha" => ("creative", "shadowAlpha"),
        "highlight_tint_red" => ("creative", "highlightRed"),
        "highlight_tint_green" => ("creative", "highlightGreen"),
        "highlight_tint_blue" => ("creative", "highlightBlue"),
        "highlight_tint_alpha" => ("creative", "highlightAlpha"),
        "temperature" => ("basic", "temperature"),
        "tint" => ("basic", "tint"),
        "exposure" => ("basic", "exposure"),
        "contrast" => ("basic", "contrast"),
        "highlights" => ("basic", "highlights"),
        "shadows" => ("basic", "shadows"),
        "whites" => ("basic", "whites"),
        "blacks" => ("basic", "blacks"),
        "saturation" => ("basic", "saturation"),
        "look_intensity" => ("grade", "intensity"),
        "faded_film" => ("creative", "fadedFilm"),
        "sharpen" => ("", "sharpen"),
        "vibrance" => ("creative", "vibrance"),
        "creative_sat" => ("creative", "saturation"),
        "wheel_shadows_x" => ("grade", "shadowsX"),
        "wheel_shadows_y" => ("grade", "shadowsY"),
        "wheel_midtones_x" => ("grade", "midtonesX"),
        "wheel_midtones_y" => ("grade", "midtonesY"),
        "wheel_highlights_x" => ("grade", "highlightsX"),
        "wheel_highlights_y" => ("grade", "highlightsY"),
        "wheel_shadows_l" => ("grade", "shadowsLightness"),
        "wheel_midtones_l" => ("grade", "midtonesLightness"),
        "wheel_highlights_l" => ("grade", "highlightsLightness"),
        "hsl_hue" => ("secondary", "hue"),
        "hsl_hue_range" => ("secondary", "hueRange"),
        "hsl_sat_min" => ("secondary", "saturationMin"),
        "hsl_luma_min" => ("secondary", "lightnessMin"),
        "hsl_luma_max" => ("secondary", "lightnessMax"),
        "hsl_soft" => ("secondary", "soften"),
        "hsl_denoise" => ("secondary", "denoise"),
        "hsl_blur" => ("secondary", "blur"),
        "hsl_temp" => ("secondary", "temperature"),
        "hsl_tint" => ("secondary", "tint"),
        "hsl_sat" => ("secondary", "saturation"),
        "hsl_hue_shift" => ("secondary", "hueShift"),
        "vignette_amount" => ("vignette", "amount"),
        "vignette_midpoint" => ("vignette", "midpoint"),
        "vignette_roundness" => ("vignette", "roundness"),
        "vignette_feather" => ("vignette", "feather"),
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectPartition {
    pub media_effects: Vec<VideoEffect>,
    pub graphic: GraphicClip,
}

/// Partition enabled, non-intrinsic effects whose parameters have been converted.
/// Generated layer identifiers retain the original slot index, including gaps.
pub fn partition_adapted_effects(effects: Vec<VideoEffect>) -> Result<EffectPartition> {
    let mut result = EffectPartition {
        media_effects: vec![],
        graphic: GraphicClip::default(),
    };
    for (slot, effect) in effects.into_iter().enumerate() {
        effect.validate()?;
        let content = match effect {
            VideoEffect::GraphicShape {
                parameters,
                animation,
            } => GraphicContent::Shape {
                parameters,
                animation,
            },
            VideoEffect::GraphicText {
                parameters,
                animation,
                source_text_animation,
            } => GraphicContent::Text {
                parameters,
                animation,
                source_text_animation,
            },
            effect => {
                result.media_effects.push(effect);
                continue;
            }
        };
        result.graphic.layers.push(GraphicLayer {
            uid: u64::try_from(slot)?,
            enabled: true,
            content,
            original_parameters: None,
            pin: None,
        });
    }
    result.graphic.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_adaptation_preserves_other_curves_and_rejects_unknown_channels() {
        use super::super::composition::AnimatableValue;
        for (kind, parameter, value) in [
            ("gaussian_blur", "blurriness", 20.),
            ("brightness_contrast", "brightness", 0.5),
            ("lumetri", "temperature", 1.),
        ] {
            let mut wire = serde_json::json!({"kind":kind,"parameters":{}});
            if kind == "brightness_contrast" {
                wire["animation"] =
                    serde_json::json!({"contrast":{"mode":"constant","value":0.25}});
            }
            if kind == "lumetri" {
                wire["parameters"]["animation"] =
                    serde_json::json!({"basic":{"exposure":{"mode":"constant","value":2.0}}});
            }
            let original: VideoEffect = serde_json::from_value(wire).unwrap();
            assert!(with_scalar_animation(
                original.clone(),
                "not_a_channel",
                AnimatableValue::constant(value)
            )
            .is_err());
            let adapted =
                with_scalar_animation(original, parameter, AnimatableValue::constant(value))
                    .unwrap();
            let saved = serde_json::to_value(adapted).unwrap();
            if kind == "brightness_contrast" {
                assert_eq!(saved["animation"]["contrast"]["value"], 0.25);
            }
            if kind == "lumetri" {
                assert_eq!(
                    saved["parameters"]["animation"]["basic"]["exposure"]["value"],
                    2.0
                );
            }
        }
        assert!(with_scalar_animation(
            VideoEffect::HorizontalFlip,
            "amount",
            AnimatableValue::constant(1.)
        )
        .is_err());
    }
    #[test]
    fn all_source_sdr_lumetri_scalar_curves_coexist_and_hdr_is_refused() {
        use super::super::composition::AnimatableValue;
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-transition-lumetri-animation.json"
        ))
        .unwrap();
        let mut effect: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":"lumetri","parameters":{}})).unwrap();
        let mut expected = std::collections::BTreeMap::new();
        for case in oracle["cases"].as_array().unwrap() {
            let parameter = case["parameter"].as_str().unwrap();
            if expected.contains_key(parameter) {
                continue;
            }
            let curve: AnimatableValue = serde_json::from_value(serde_json::json!({
                "mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear",
                "keyframes":[{"tick":18000000,"value":case["low"]},{"tick":32000000,"value":case["high"]}]}
            })).unwrap();
            effect = with_scalar_animation(effect, parameter, curve.clone()).unwrap();
            expected.insert(parameter.to_string(), serde_json::to_value(curve).unwrap());
            let saved = serde_json::to_value(&effect).unwrap();
            for (source, curve) in &expected {
                let (section, target) = lumetri_scalar_target(source).unwrap();
                let path = if section.is_empty() {
                    format!("/parameters/animation/{target}")
                } else {
                    format!("/parameters/animation/{section}/{target}")
                };
                assert_eq!(
                    saved.pointer(&path),
                    Some(curve),
                    "neighbor curve lost: {source}"
                );
            }
        }
        assert_eq!(expected.len(), 33);
        assert_eq!(
            effect,
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap()
        );
        if let VideoEffect::Lumetri { parameters } = &effect {
            parameters
                .sample(
                    super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(
                        31 * 254016000000,
                    ),
                )
                .unwrap();
        } else {
            unreachable!();
        }
        for parameter in ["hdr_white", "hdr_specular", "curves_hdr_range", "unknown"] {
            assert!(with_scalar_animation(
                effect.clone(),
                parameter,
                AnimatableValue::constant(1.)
            )
            .is_err());
        }
        assert!(with_scalar_animation(effect, "exposure", AnimatableValue::constant(6.)).is_err());
    }
    #[test]
    fn serialized_source_scalar_curves_reach_native_lumetri_without_clock_rounding() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-scalar-param.json"
        ))
        .unwrap();
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2646);
        let baseline: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":"lumetri","parameters":{}})).unwrap();
        for (index, case) in cases.iter().enumerate() {
            let effect =
                with_source_scalar_animation(baseline.clone(), "temperature", &case["parameter"])
                    .unwrap();
            let restored: VideoEffect =
                serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
            assert_eq!(effect, restored);
            let VideoEffect::Lumetri { parameters } = restored else {
                unreachable!()
            };
            let at = case["at"].as_str().unwrap().parse().unwrap();
            let sampled = parameters
                .sample(
                    super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(
                        at,
                    ),
                )
                .unwrap();
            let expected = f64::from_bits(case["bits"].as_u64().unwrap()) as f32;
            assert_eq!(
                sampled.basic.temperature.to_bits(),
                expected.to_bits(),
                "source Lumetri scalar {index}"
            );
        }
    }
    #[test]
    fn source_point_curves_reach_all_graphic_points_at_exact_ticks() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == "point")
        {
            for kind in ["graphic_text", "graphic_shape"] {
                let mut effect: VideoEffect =
                    serde_json::from_value(serde_json::json!({"kind":kind,"parameters":{}}))
                        .unwrap();
                let controls = if kind == "graphic_text" {
                    vec!["position", "anchor"]
                } else {
                    vec!["size", "position", "anchor"]
                };
                for control in &controls {
                    effect =
                        with_source_point_animation(effect, control, &case["parameter"]).unwrap();
                }
                let saved = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
                assert_eq!(effect, saved);
                let graphic = partition_adapted_effects(vec![saved]).unwrap().graphic;
                let snapshot = graphic
                    .snapshot_original_ticks(
                        (80, 64),
                        case["at"].as_str().unwrap().parse().unwrap(),
                    )
                    .unwrap();
                let points = match &snapshot.layers[0].content {
                    GraphicContent::Text { parameters: p, .. } => {
                        vec![p.position.unwrap(), p.anchor]
                    }
                    GraphicContent::Shape { parameters: p, .. } => {
                        vec![p.size, p.position.unwrap(), p.anchor]
                    }
                };
                let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
                for point in points {
                    assert_eq!(point.map(f64::to_bits).as_slice(), expected);
                    count += 1;
                }
            }
        }
        assert_eq!(count, 8820);
    }
    #[test]
    fn source_rgba_curves_reach_all_graphic_colors_at_exact_ticks() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == "color")
        {
            for kind in ["graphic_text", "graphic_shape"] {
                let mut effect: VideoEffect =
                    serde_json::from_value(serde_json::json!({"kind":kind,"parameters":{}}))
                        .unwrap();
                for control in [
                    "fill_color",
                    "stroke_color",
                    "stroke2_color",
                    "background_color",
                    "shadow_color",
                ] {
                    effect =
                        with_source_color_animation(effect, control, &case["parameter"]).unwrap();
                }
                let restored: VideoEffect =
                    serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
                assert_eq!(effect, restored);
                let graphic = partition_adapted_effects(vec![restored]).unwrap().graphic;
                let ticks = case["at"].as_str().unwrap().parse().unwrap();
                let snapshot = graphic.snapshot_original_ticks((80, 64), ticks).unwrap();
                let colors = match &snapshot.layers[0].content {
                    GraphicContent::Text {
                        parameters: p,
                        animation,
                        ..
                    } => {
                        assert!(animation.is_none());
                        [
                            p.fill_color,
                            p.stroke_color,
                            p.stroke2_color,
                            p.background_color,
                            p.shadow_color,
                        ]
                    }
                    GraphicContent::Shape {
                        parameters: p,
                        animation,
                    } => {
                        assert!(animation.is_none());
                        [
                            p.fill_color,
                            p.stroke_color,
                            p.stroke2_color,
                            p.background_color,
                            p.shadow_color,
                        ]
                    }
                };
                let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
                for color in colors {
                    assert_eq!(color.map(|v| u64::from(v.to_bits())).as_slice(), expected);
                }
                count += 5;
            }
        }
        assert_eq!(count, 17640);
    }
    #[test]
    fn source_rgba_curves_reach_lumetri_split_tone_with_alpha() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let baseline: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":"lumetri","parameters":{}})).unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["kind"] == "color")
        {
            let effect =
                with_source_color_animation(baseline.clone(), "shadow_tint", &case["parameter"])
                    .unwrap();
            let effect =
                with_source_color_animation(effect, "highlight_tint", &case["parameter"]).unwrap();
            let saved: VideoEffect =
                serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
            assert_eq!(effect, saved);
            let VideoEffect::Lumetri { parameters } = saved else {
                unreachable!()
            };
            let at = case["at"].as_str().unwrap().parse().unwrap();
            let p = parameters
                .sample(
                    super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(
                        at,
                    ),
                )
                .unwrap()
                .creative;
            let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
            for color in [
                [p.shadow_red, p.shadow_green, p.shadow_blue, p.shadow_alpha],
                [
                    p.highlight_red,
                    p.highlight_green,
                    p.highlight_blue,
                    p.highlight_alpha,
                ],
            ] {
                assert_eq!(color.map(|v| u64::from(v.to_bits())).as_slice(), expected);
            }
            count += 1;
        }
        assert_eq!(count, 1764);
    }
    #[test]
    fn source_rgba_curves_reach_both_tint_endpoints_without_losing_alpha() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let baseline: VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"color_tint", "parameters":{"amount":25},
            "animation":{"mode":"constant","value":30}
        }))
        .unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["kind"] == "color")
        {
            let effect =
                with_source_color_animation(baseline.clone(), "black", &case["parameter"]).unwrap();
            let effect = with_source_color_animation(effect, "white", &case["parameter"]).unwrap();
            let saved: VideoEffect =
                serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
            assert_eq!(effect, saved);
            let VideoEffect::ColorTint {
                parameters,
                animation: Some(amount),
                color_animation: Some(colors),
            } = saved
            else {
                unreachable!()
            };
            assert_eq!(colors.len(), 8);
            assert_eq!(
                amount,
                super::super::composition::AnimatableValue::constant(30.)
            );
            let at = case["at"].as_str().unwrap().parse().unwrap();
            let sampled = super::super::color_tint::sample_colors(
                parameters,
                &colors,
                super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(at),
            )
            .unwrap();
            let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(
                sampled
                    .black
                    .map(|value| u64::from(value.to_bits()))
                    .as_slice(),
                expected
            );
            assert_eq!(
                sampled
                    .white
                    .map(|value| u64::from(value.to_bits()))
                    .as_slice(),
                expected
            );
            assert_eq!(sampled.amount, 25.);
            count += 1;
        }
        assert_eq!(count, 1764);
        assert!(
            with_source_color_animation(baseline.clone(), "unknown", &serde_json::json!({}))
                .is_err()
        );
        assert!(with_source_color_animation(
            VideoEffect::HorizontalFlip,
            "black",
            &serde_json::json!({})
        )
        .is_err());
        assert!(with_source_color_animation(
            baseline,
            "white",
            &serde_json::json!({"value":{"Color":[0,0,0]}})
        )
        .is_err());
    }
    #[test]
    fn source_point_curves_reach_move_without_coordinate_or_time_rounding() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let baseline: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":"move","parameters":{}})).unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["kind"] == "point")
        {
            for control in ["from", "to"] {
                let effect =
                    with_source_point_animation(baseline.clone(), control, &case["parameter"])
                        .unwrap();
                let saved: VideoEffect =
                    serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
                assert_eq!(effect, saved);
                let VideoEffect::Move {
                    parameters,
                    animation: Some(animation),
                } = saved
                else {
                    unreachable!()
                };
                let at = case["at"].as_str().unwrap().parse().unwrap();
                let sampled = super::super::move_effect::sample_animation(
                    parameters,
                    &animation,
                    super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(
                        at,
                    ),
                )
                .unwrap();
                let actual = if control == "from" {
                    sampled.from
                } else {
                    sampled.to
                };
                let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
                assert_eq!(actual.map(f64::to_bits).as_slice(), expected);
                if control == "from" {
                    assert_eq!(sampled.to, [0., 0.]);
                } else {
                    assert_eq!(sampled.from, [-100., 0.]);
                }
            }
            count += 1;
        }
        assert_eq!(count, 1764);
        assert!(with_source_point_animation(
            VideoEffect::HorizontalFlip,
            "from",
            &serde_json::json!({})
        )
        .is_err());
    }
    #[test]
    fn source_point_curves_reach_lumetri_wheels_with_f64_coordinates() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        let baseline: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":"lumetri","parameters":{}})).unwrap();
        let mut count = 0;
        for case in oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["kind"] == "point")
        {
            for control in ["wheel_shadows", "wheel_midtones", "wheel_highlights"] {
                let effect =
                    with_source_point_animation(baseline.clone(), control, &case["parameter"])
                        .unwrap();
                let saved: VideoEffect =
                    serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
                assert_eq!(effect, saved);
                let VideoEffect::Lumetri { parameters } = saved else {
                    unreachable!()
                };
                let at = case["at"].as_str().unwrap().parse().unwrap();
                let sampled=parameters.sample(super::super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(at)).unwrap();
                let wheel = match control {
                    "wheel_shadows" => sampled.grade.shadows,
                    "wheel_midtones" => sampled.grade.midtones,
                    _ => sampled.grade.highlights,
                };
                let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
                assert_eq!([wheel.x.to_bits(), wheel.y.to_bits()].as_slice(), expected);
                assert_eq!(wheel.lightness, 0.);
            }
            count += 1;
        }
        assert_eq!(count, 1764);
    }
    #[test]
    fn media_and_graphic_orders_and_authored_animation_are_retained() {
        let shape: VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"graphic_shape", "parameters":{"name":"Authored shape","size":[8,4]},
            "animation":{"opacity":{"mode":"keyframes","track":{"timeBase":1000000,
                "interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}}}
        })).unwrap();
        let text: VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"graphic_text", "parameters":{}
        }))
        .unwrap();
        let result = partition_adapted_effects(vec![
            VideoEffect::HorizontalFlip,
            shape.clone(),
            VideoEffect::VerticalFlip,
            text.clone(),
        ])
        .unwrap();
        assert_eq!(
            result.media_effects,
            vec![VideoEffect::HorizontalFlip, VideoEffect::VerticalFlip]
        );
        assert_eq!(
            result
                .graphic
                .layers
                .iter()
                .map(|layer| layer.uid)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        let VideoEffect::GraphicShape {
            parameters,
            animation,
        } = shape
        else {
            unreachable!()
        };
        assert_eq!(
            result.graphic.layers[0].content,
            GraphicContent::Shape {
                parameters,
                animation
            }
        );
        let VideoEffect::GraphicText {
            parameters,
            animation,
            source_text_animation,
        } = text
        else {
            unreachable!()
        };
        assert_eq!(
            result.graphic.layers[1].content,
            GraphicContent::Text {
                parameters,
                animation,
                source_text_animation
            }
        );
        let restored: GraphicClip =
            serde_json::from_value(serde_json::to_value(&result.graphic).unwrap()).unwrap();
        assert_eq!(restored, result.graphic);
    }
}
