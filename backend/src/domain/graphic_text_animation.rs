//! Filmcraft graphic text clip-local animation channels.
use super::{composition::AnimatableValue, graphic_text::GraphicTextParameters};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicTextChannel {
    Size,
    Tracking,
    Leading,
    BaselineShift,
    BoxWidth,
    BoxHeight,

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
pub type GraphicTextAnimation = std::collections::BTreeMap<GraphicTextChannel, AnimatableValue>;
pub fn validate_animation(animation: &GraphicTextAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            GraphicTextChannel::Size => (1., 2000.),
            GraphicTextChannel::Tracking => (-1000., 10000.),
            GraphicTextChannel::Leading => (-5000., 5000.),
            GraphicTextChannel::BaselineShift => (-5000., 5000.),
            GraphicTextChannel::BoxWidth => (0., 100000.),
            GraphicTextChannel::BoxHeight => (0., 100000.),

            GraphicTextChannel::PositionX => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::PositionY => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::AnchorX => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::AnchorY => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::FillR => (0.0, 1.0),
            GraphicTextChannel::FillG => (0.0, 1.0),
            GraphicTextChannel::FillB => (0.0, 1.0),
            GraphicTextChannel::FillA => (0.0, 1.0),
            GraphicTextChannel::StrokeR => (0.0, 1.0),
            GraphicTextChannel::StrokeG => (0.0, 1.0),
            GraphicTextChannel::StrokeB => (0.0, 1.0),
            GraphicTextChannel::StrokeA => (0.0, 1.0),
            GraphicTextChannel::Stroke2R => (0.0, 1.0),
            GraphicTextChannel::Stroke2G => (0.0, 1.0),
            GraphicTextChannel::Stroke2B => (0.0, 1.0),
            GraphicTextChannel::Stroke2A => (0.0, 1.0),
            GraphicTextChannel::BackgroundR => (0.0, 1.0),
            GraphicTextChannel::BackgroundG => (0.0, 1.0),
            GraphicTextChannel::BackgroundB => (0.0, 1.0),
            GraphicTextChannel::BackgroundA => (0.0, 1.0),
            GraphicTextChannel::ShadowR => (0.0, 1.0),
            GraphicTextChannel::ShadowG => (0.0, 1.0),
            GraphicTextChannel::ShadowB => (0.0, 1.0),
            GraphicTextChannel::ShadowA => (0.0, 1.0),
            GraphicTextChannel::StrokeWidth => (0.0, 1000.0),
            GraphicTextChannel::Stroke2Width => (0.0, 1000.0),
            GraphicTextChannel::BackgroundOpacity => (0.0, 100.0),
            GraphicTextChannel::BackgroundSize => (0.0, 1000.0),
            GraphicTextChannel::BackgroundRadius => (0.0, 1000.0),
            GraphicTextChannel::ShadowOpacity => (0.0, 100.0),
            GraphicTextChannel::ShadowAngle => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::ShadowDistance => (0.0, 1000.0),
            GraphicTextChannel::ShadowSize => (0.0, 1000.0),
            GraphicTextChannel::ShadowBlur => (0.0, 1000.0),
            GraphicTextChannel::Scale => (0.0, 100000.0),
            GraphicTextChannel::ScaleWidth => (0.0, 100000.0),
            GraphicTextChannel::Rotation => (-3.4028234663852886e+38, 3.4028234663852886e+38),
            GraphicTextChannel::Opacity => (0.0, 100.0),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "graphic text animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    parameters: GraphicTextParameters,
    animation: &GraphicTextAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
    canvas: (u32, u32),
) -> Result<GraphicTextParameters> {
    let local = local.into();
    sample_with(parameters, animation, canvas, |track| local.sample_track(track))
}
pub fn sample_signed(
    parameters: GraphicTextParameters,
    animation: &GraphicTextAnimation,
    local: i128,
    canvas: (u32, u32),
) -> Result<GraphicTextParameters> {
    sample_seconds(parameters, animation, local as f64 / 1_000_000., canvas)
}
pub fn sample_seconds(
    parameters: GraphicTextParameters,
    animation: &GraphicTextAnimation,
    seconds: f64,
    canvas: (u32, u32),
) -> Result<GraphicTextParameters> {
    sample_with(parameters, animation, canvas, |track| track.sample_seconds(seconds))
}
fn sample_with(
    mut parameters: GraphicTextParameters,
    animation: &GraphicTextAnimation,
    canvas: (u32, u32),
    sample_track: impl Fn(&super::keyframes::KeyframeTrack<f64>) -> Result<f64, super::keyframes::KeyframeError>,
) -> Result<GraphicTextParameters> {
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
            GraphicTextChannel::Size => {
                parameters.size = value as f32;
            }
            GraphicTextChannel::Tracking => {
                parameters.tracking = value as f32;
            }
            GraphicTextChannel::Leading => {
                parameters.leading = value as f32;
            }
            GraphicTextChannel::BaselineShift => {
                parameters.baseline_shift = value as f32;
            }
            GraphicTextChannel::BoxWidth => {
                parameters.box_width = value as f32;
            }
            GraphicTextChannel::BoxHeight => {
                parameters.box_height = value as f32;
            }
            GraphicTextChannel::PositionX => {
                parameters
                    .position
                    .get_or_insert([canvas.0 as f64 / 2., canvas.1 as f64 / 2.])[0] = value;
            }
            GraphicTextChannel::PositionY => {
                parameters
                    .position
                    .get_or_insert([canvas.0 as f64 / 2., canvas.1 as f64 / 2.])[1] = value;
            }
            GraphicTextChannel::AnchorX => {
                parameters.anchor[0] = value;
            }
            GraphicTextChannel::AnchorY => {
                parameters.anchor[1] = value;
            }
            GraphicTextChannel::FillR => {
                parameters.fill_color[0] = value as f32;
            }
            GraphicTextChannel::FillG => {
                parameters.fill_color[1] = value as f32;
            }
            GraphicTextChannel::FillB => {
                parameters.fill_color[2] = value as f32;
            }
            GraphicTextChannel::FillA => {
                parameters.fill_color[3] = value as f32;
            }
            GraphicTextChannel::StrokeR => {
                parameters.stroke_color[0] = value as f32;
            }
            GraphicTextChannel::StrokeG => {
                parameters.stroke_color[1] = value as f32;
            }
            GraphicTextChannel::StrokeB => {
                parameters.stroke_color[2] = value as f32;
            }
            GraphicTextChannel::StrokeA => {
                parameters.stroke_color[3] = value as f32;
            }
            GraphicTextChannel::Stroke2R => {
                parameters.stroke2_color[0] = value as f32;
            }
            GraphicTextChannel::Stroke2G => {
                parameters.stroke2_color[1] = value as f32;
            }
            GraphicTextChannel::Stroke2B => {
                parameters.stroke2_color[2] = value as f32;
            }
            GraphicTextChannel::Stroke2A => {
                parameters.stroke2_color[3] = value as f32;
            }
            GraphicTextChannel::BackgroundR => {
                parameters.background_color[0] = value as f32;
            }
            GraphicTextChannel::BackgroundG => {
                parameters.background_color[1] = value as f32;
            }
            GraphicTextChannel::BackgroundB => {
                parameters.background_color[2] = value as f32;
            }
            GraphicTextChannel::BackgroundA => {
                parameters.background_color[3] = value as f32;
            }
            GraphicTextChannel::ShadowR => {
                parameters.shadow_color[0] = value as f32;
            }
            GraphicTextChannel::ShadowG => {
                parameters.shadow_color[1] = value as f32;
            }
            GraphicTextChannel::ShadowB => {
                parameters.shadow_color[2] = value as f32;
            }
            GraphicTextChannel::ShadowA => {
                parameters.shadow_color[3] = value as f32;
            }
            GraphicTextChannel::StrokeWidth => {
                parameters.stroke_width = value as f32;
            }
            GraphicTextChannel::Stroke2Width => {
                parameters.stroke2_width = value as f32;
            }
            GraphicTextChannel::BackgroundOpacity => {
                parameters.background_opacity = value as f32;
            }
            GraphicTextChannel::BackgroundSize => {
                parameters.background_size = value as f32;
            }
            GraphicTextChannel::BackgroundRadius => {
                parameters.background_radius = value as f32;
            }
            GraphicTextChannel::ShadowOpacity => {
                parameters.shadow_opacity = value as f32;
            }
            GraphicTextChannel::ShadowAngle => {
                parameters.shadow_angle = value as f32;
            }
            GraphicTextChannel::ShadowDistance => {
                parameters.shadow_distance = value as f32;
            }
            GraphicTextChannel::ShadowSize => {
                parameters.shadow_size = value as f32;
            }
            GraphicTextChannel::ShadowBlur => {
                parameters.shadow_blur = value as f32;
            }
            GraphicTextChannel::Scale => {
                parameters.scale = value as f32;
            }
            GraphicTextChannel::ScaleWidth => {
                parameters.scale_width = value as f32;
            }
            GraphicTextChannel::Rotation => {
                parameters.rotation = value as f32;
            }
            GraphicTextChannel::Opacity => {
                parameters.opacity = value as f32;
            }
        }
    }
    parameters.validate()?;
    Ok(parameters)
}
