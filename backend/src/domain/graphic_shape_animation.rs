//! Filmcraft graphic shape clip-local animation channels.
use super::{composition::AnimatableValue, graphic_shape::GraphicShape};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicShapeChannel {
    SizeX,
    SizeY,
    PositionX,
    PositionY,
    AnchorX,
    AnchorY,
    FillR,
    FillG,
    FillB,
    FillA,
    StrokeR,
    StrokeG,
    StrokeB,
    StrokeA,
    Stroke2R,
    Stroke2G,
    Stroke2B,
    Stroke2A,
    BackgroundR,
    BackgroundG,
    BackgroundB,
    BackgroundA,
    ShadowR,
    ShadowG,
    ShadowB,
    ShadowA,
    Sides,
    CornerRadius,
    StrokeWidth,
    Stroke2Width,
    BackgroundOpacity,
    BackgroundSize,
    BackgroundRadius,
    ShadowOpacity,
    ShadowAngle,
    ShadowDistance,
    ShadowSize,
    ShadowBlur,
    Scale,
    ScaleWidth,
    Rotation,
    Opacity,
}
pub type GraphicShapeAnimation = std::collections::BTreeMap<GraphicShapeChannel, AnimatableValue>;
pub fn validate_animation(animation: &GraphicShapeAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            GraphicShapeChannel::SizeX => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::SizeY => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::PositionX => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::PositionY => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::AnchorX => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::AnchorY => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::FillR => (0.0, 1.0),
            GraphicShapeChannel::FillG => (0.0, 1.0),
            GraphicShapeChannel::FillB => (0.0, 1.0),
            GraphicShapeChannel::FillA => (0.0, 1.0),
            GraphicShapeChannel::StrokeR => (0.0, 1.0),
            GraphicShapeChannel::StrokeG => (0.0, 1.0),
            GraphicShapeChannel::StrokeB => (0.0, 1.0),
            GraphicShapeChannel::StrokeA => (0.0, 1.0),
            GraphicShapeChannel::Stroke2R => (0.0, 1.0),
            GraphicShapeChannel::Stroke2G => (0.0, 1.0),
            GraphicShapeChannel::Stroke2B => (0.0, 1.0),
            GraphicShapeChannel::Stroke2A => (0.0, 1.0),
            GraphicShapeChannel::BackgroundR => (0.0, 1.0),
            GraphicShapeChannel::BackgroundG => (0.0, 1.0),
            GraphicShapeChannel::BackgroundB => (0.0, 1.0),
            GraphicShapeChannel::BackgroundA => (0.0, 1.0),
            GraphicShapeChannel::ShadowR => (0.0, 1.0),
            GraphicShapeChannel::ShadowG => (0.0, 1.0),
            GraphicShapeChannel::ShadowB => (0.0, 1.0),
            GraphicShapeChannel::ShadowA => (0.0, 1.0),
            GraphicShapeChannel::Sides => (3.0, 64.0),
            GraphicShapeChannel::CornerRadius => (0.0, 10000.0),
            GraphicShapeChannel::StrokeWidth => (0.0, 1000.0),
            GraphicShapeChannel::Stroke2Width => (0.0, 1000.0),
            GraphicShapeChannel::BackgroundOpacity => (0.0, 100.0),
            GraphicShapeChannel::BackgroundSize => (0.0, 1000.0),
            GraphicShapeChannel::BackgroundRadius => (0.0, 1000.0),
            GraphicShapeChannel::ShadowOpacity => (0.0, 100.0),
            GraphicShapeChannel::ShadowAngle => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::ShadowDistance => (0.0, 1000.0),
            GraphicShapeChannel::ShadowSize => (0.0, 1000.0),
            GraphicShapeChannel::ShadowBlur => (0.0, 1000.0),
            GraphicShapeChannel::Scale => (0.0, 100000.0),
            GraphicShapeChannel::ScaleWidth => (0.0, 100000.0),
            GraphicShapeChannel::Rotation => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicShapeChannel::Opacity => (0.0, 100.0),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "graphic shape animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    parameters: GraphicShape,
    animation: &GraphicShapeAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
    canvas: (u32, u32),
) -> Result<GraphicShape> {
    let local = local.into();
    sample_with(parameters, animation, canvas, |track| local.sample_track(track))
}
pub fn sample_signed(
    parameters: GraphicShape,
    animation: &GraphicShapeAnimation,
    local: i128,
    canvas: (u32, u32),
) -> Result<GraphicShape> {
    sample_seconds(parameters, animation, local as f64 / 1_000_000., canvas)
}
pub fn sample_seconds(
    parameters: GraphicShape,
    animation: &GraphicShapeAnimation,
    seconds: f64,
    canvas: (u32, u32),
) -> Result<GraphicShape> {
    sample_with(parameters, animation, canvas, |track| track.sample_seconds(seconds))
}
fn sample_with(
    mut parameters: GraphicShape,
    animation: &GraphicShapeAnimation,
    canvas: (u32, u32),
    sample_track: impl Fn(&super::keyframes::KeyframeTrack<f64>) -> Result<f64, super::keyframes::KeyframeError>,
) -> Result<GraphicShape> {
    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = match curve {
            super::composition::AnimatableValue::Constant { value } => *value,
            super::composition::AnimatableValue::Keyframes { track } => {
                sample_track(track)?
            }
        };
        match channel {
            GraphicShapeChannel::SizeX => {
                parameters.size[0] = value;
            }
            GraphicShapeChannel::SizeY => {
                parameters.size[1] = value;
            }
            GraphicShapeChannel::PositionX => {
                parameters
                    .position
                    .get_or_insert([canvas.0 as f64 / 2., canvas.1 as f64 / 2.])[0] = value;
            }
            GraphicShapeChannel::PositionY => {
                parameters
                    .position
                    .get_or_insert([canvas.0 as f64 / 2., canvas.1 as f64 / 2.])[1] = value;
            }
            GraphicShapeChannel::AnchorX => {
                parameters.anchor[0] = value;
            }
            GraphicShapeChannel::AnchorY => {
                parameters.anchor[1] = value;
            }
            GraphicShapeChannel::FillR => {
                parameters.fill_color[0] = value as f32;
            }
            GraphicShapeChannel::FillG => {
                parameters.fill_color[1] = value as f32;
            }
            GraphicShapeChannel::FillB => {
                parameters.fill_color[2] = value as f32;
            }
            GraphicShapeChannel::FillA => {
                parameters.fill_color[3] = value as f32;
            }
            GraphicShapeChannel::StrokeR => {
                parameters.stroke_color[0] = value as f32;
            }
            GraphicShapeChannel::StrokeG => {
                parameters.stroke_color[1] = value as f32;
            }
            GraphicShapeChannel::StrokeB => {
                parameters.stroke_color[2] = value as f32;
            }
            GraphicShapeChannel::StrokeA => {
                parameters.stroke_color[3] = value as f32;
            }
            GraphicShapeChannel::Stroke2R => {
                parameters.stroke2_color[0] = value as f32;
            }
            GraphicShapeChannel::Stroke2G => {
                parameters.stroke2_color[1] = value as f32;
            }
            GraphicShapeChannel::Stroke2B => {
                parameters.stroke2_color[2] = value as f32;
            }
            GraphicShapeChannel::Stroke2A => {
                parameters.stroke2_color[3] = value as f32;
            }
            GraphicShapeChannel::BackgroundR => {
                parameters.background_color[0] = value as f32;
            }
            GraphicShapeChannel::BackgroundG => {
                parameters.background_color[1] = value as f32;
            }
            GraphicShapeChannel::BackgroundB => {
                parameters.background_color[2] = value as f32;
            }
            GraphicShapeChannel::BackgroundA => {
                parameters.background_color[3] = value as f32;
            }
            GraphicShapeChannel::ShadowR => {
                parameters.shadow_color[0] = value as f32;
            }
            GraphicShapeChannel::ShadowG => {
                parameters.shadow_color[1] = value as f32;
            }
            GraphicShapeChannel::ShadowB => {
                parameters.shadow_color[2] = value as f32;
            }
            GraphicShapeChannel::ShadowA => {
                parameters.shadow_color[3] = value as f32;
            }
            GraphicShapeChannel::Sides => {
                parameters.sides = value as f32;
            }
            GraphicShapeChannel::CornerRadius => {
                parameters.corner_radius = value as f32;
            }
            GraphicShapeChannel::StrokeWidth => {
                parameters.stroke_width = value as f32;
            }
            GraphicShapeChannel::Stroke2Width => {
                parameters.stroke2_width = value as f32;
            }
            GraphicShapeChannel::BackgroundOpacity => {
                parameters.background_opacity = value as f32;
            }
            GraphicShapeChannel::BackgroundSize => {
                parameters.background_size = value as f32;
            }
            GraphicShapeChannel::BackgroundRadius => {
                parameters.background_radius = value as f32;
            }
            GraphicShapeChannel::ShadowOpacity => {
                parameters.shadow_opacity = value as f32;
            }
            GraphicShapeChannel::ShadowAngle => {
                parameters.shadow_angle = value as f32;
            }
            GraphicShapeChannel::ShadowDistance => {
                parameters.shadow_distance = value as f32;
            }
            GraphicShapeChannel::ShadowSize => {
                parameters.shadow_size = value as f32;
            }
            GraphicShapeChannel::ShadowBlur => {
                parameters.shadow_blur = value as f32;
            }
            GraphicShapeChannel::Scale => {
                parameters.scale = value as f32;
            }
            GraphicShapeChannel::ScaleWidth => {
                parameters.scale_width = value as f32;
            }
            GraphicShapeChannel::Rotation => {
                parameters.rotation = value as f32;
            }
            GraphicShapeChannel::Opacity => {
                parameters.opacity = value as f32;
            }
        }
    }
    parameters.validate()?;
    Ok(parameters)
}
