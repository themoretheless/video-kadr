//! Filmcraft obsolete Timecode and Clip Name effects; MIT attribution in docs/licenses.
use super::{
    linear_color_frame::LinearColorFrame, metadata_burnin::BurninContext,
    text_burnin::draw_clock_label,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Timecode {
    pub position: [Option<f64>; 2],
    pub size: f32,
    pub opacity: f32,
}
impl Default for Timecode {
    fn default() -> Self {
        Self {
            position: [None, None],
            size: 15.,
            opacity: 100.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipName {
    pub position: [Option<f64>; 2],
    pub size: f32,
}
impl Default for ClipName {
    fn default() -> Self {
        Self {
            position: [None, None],
            size: 15.,
        }
    }
}
fn validate(position: [Option<f64>; 2], size: f32) -> Result<()> {
    ensure!(
        position.iter().flatten().all(|v| v.is_finite()),
        "invalid label position"
    );
    ensure!(
        size.is_finite() && (1.0..=100.0).contains(&size),
        "invalid label size"
    );
    Ok(())
}
fn apply(
    frame: &mut LinearColorFrame,
    text: &str,
    family: &str,
    position: [Option<f64>; 2],
    size: f32,
    opacity: f32,
    scale: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    ensure!(scale.is_finite() && scale > 0., "invalid label pixel scale");
    let (w, h) = frame.dimensions();
    let position = [
        position[0].map_or(w as f64 / 2., |v| v * scale as f64),
        position[1].map_or(h as f64 / 2., |v| v * scale as f64),
    ];
    draw_clock_label(
        frame,
        text,
        family,
        position,
        (size / 100. * h as f32 * 0.5).max(6.),
        opacity,
        checkpoint,
    )
}
impl Timecode {
    pub fn validate(&self) -> Result<()> {
        validate(self.position, self.size)?;
        ensure!(
            self.opacity.is_finite() && (0.0..=100.0).contains(&self.opacity),
            "invalid timecode opacity"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        context: BurninContext<'_>,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        apply(
            frame,
            context.sequence_timecode,
            "JetBrains Mono",
            self.position,
            self.size,
            (self.opacity / 100.).clamp(0., 1.) * 0.8,
            scale,
            checkpoint,
        )
    }
}
impl ClipName {
    pub fn validate(&self) -> Result<()> {
        validate(self.position, self.size)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        context: BurninContext<'_>,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        apply(
            frame,
            context.clip_name,
            "Inter",
            self.position,
            self.size,
            0.8,
            scale,
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_clock_label_working_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-clock-label.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 144);
        for (index, c) in cases.iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c["width"].as_u64().unwrap() as usize,
                    height: c["height"].as_u64().unwrap() as usize,
                    rgba: serde_json::from_value(c["rgba"].clone()).unwrap(),
                },
                &mut || Ok(()),
            )
            .unwrap();
            let position = if c["autoPosition"].as_bool().unwrap() {
                [None, None]
            } else {
                let p: [f64; 2] = serde_json::from_value(c["position"].clone()).unwrap();
                p.map(Some)
            };
            let size = c["size"].as_f64().unwrap() as f32;
            let scale = c["pixelScale"].as_f64().unwrap() as f32;
            let context = BurninContext {
                sequence_timecode: c["timecode"].as_str().unwrap(),
                media_timecode: None,
                clip_name: c["clipName"].as_str().unwrap(),
                file_name: None,
                sequence_name: None,
                frame_rate: None,
                seconds: 0.5,
            };
            if c["kind"] == "timecode" {
                Timecode {
                    position,
                    size,
                    opacity: c["opacity"].as_f64().unwrap() as f32,
                }
                .apply_linear(&mut frame, context, scale, &mut || Ok(()))
                .unwrap();
            } else {
                ClipName { position, size }
                    .apply_linear(&mut frame, context, scale, &mut || Ok(()))
                    .unwrap();
            }
            let actual: Vec<u32> = frame
                .pixels()
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect();
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(actual, expected, "clock label case {index}");
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimecodeChannel {
    PositionX,
    PositionY,
    Size,
    Opacity,
}
pub type TimecodeAnimation =
    std::collections::BTreeMap<TimecodeChannel, super::composition::AnimatableValue>;
pub fn validate_timecode_animation(animation: &TimecodeAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            TimecodeChannel::PositionX | TimecodeChannel::PositionY => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            TimecodeChannel::Size => (1., 100.),
            TimecodeChannel::Opacity => (0., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "label animation out of range"
        );
    }
    Ok(())
}
pub fn sample_timecode(
    mut parameters: Timecode,
    animation: &TimecodeAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Timecode> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_timecode_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            TimecodeChannel::PositionX => parameters.position[0] = Some(value),
            TimecodeChannel::PositionY => parameters.position[1] = Some(value),
            TimecodeChannel::Size => parameters.size = value as f32,
            TimecodeChannel::Opacity => parameters.opacity = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipNameChannel {
    PositionX,
    PositionY,
    Size,
}
pub type ClipNameAnimation =
    std::collections::BTreeMap<ClipNameChannel, super::composition::AnimatableValue>;
pub fn validate_clip_name_animation(animation: &ClipNameAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            ClipNameChannel::PositionX | ClipNameChannel::PositionY => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            ClipNameChannel::Size => (1., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "label animation out of range"
        );
    }
    Ok(())
}
pub fn sample_clip_name(
    mut parameters: ClipName,
    animation: &ClipNameAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ClipName> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_clip_name_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            ClipNameChannel::PositionX => parameters.position[0] = Some(value),
            ClipNameChannel::PositionY => parameters.position[1] = Some(value),
            ClipNameChannel::Size => parameters.size = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn signed_local_curves_roundtrip_bounds_and_effect_wire() {
        let mut channels = serde_json::Map::new();
        for (name, value) in [
            ("positionX", 12.25),
            ("positionY", -7.75),
            ("size", 37.5),
            ("opacity", 42.125),
        ] {
            channels.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":-250,"value":value-0.125},{"tick":1250,"value":value+0.125}]}}));
        }
        let timecode: TimecodeAnimation =
            serde_json::from_value(serde_json::Value::Object(channels.clone())).unwrap();
        let base = Timecode::default();
        let sampled = sample_timecode(base.clone(), &timecode, 500000).unwrap();
        assert_eq!(
            sampled,
            Timecode {
                position: [Some(12.25), Some(-7.75)],
                size: 37.5,
                opacity: 42.125
            }
        );
        assert_eq!(base, Timecode::default());
        assert_eq!(
            timecode,
            serde_json::from_value(serde_json::to_value(&timecode).unwrap()).unwrap()
        );
        assert!(
            serde_json::from_value::<ClipNameAnimation>(serde_json::Value::Object(
                channels.clone()
            ))
            .is_err()
        );
        channels.remove("opacity");
        let clip: ClipNameAnimation =
            serde_json::from_value(serde_json::Value::Object(channels)).unwrap();
        assert_eq!(
            sample_clip_name(ClipName::default(), &clip, 500000).unwrap(),
            ClipName {
                position: [Some(12.25), Some(-7.75)],
                size: 37.5
            }
        );
        for bad in [-0.1, 100.1] {
            let animation: TimecodeAnimation = serde_json::from_value(
                serde_json::json!({"opacity":{"mode":"constant","value":bad}}),
            )
            .unwrap();
            assert!(validate_timecode_animation(&animation).is_err());
        }
        for bad in [0., 100.1] {
            let animation: ClipNameAnimation =
                serde_json::from_value(serde_json::json!({"size":{"mode":"constant","value":bad}}))
                    .unwrap();
            assert!(validate_clip_name_animation(&animation).is_err());
        }
        for kind in ["timecode", "clip_name"] {
            let value = serde_json::json!({"kind":kind,"parameters":{},"animation":{"size":{"mode":"constant","value":37.5}}});
            let effect: super::super::composition::VideoEffect =
                serde_json::from_value(value.clone()).unwrap();

            assert_eq!(
                serde_json::to_value(&effect).unwrap()["animation"],
                value["animation"]
            );
        }
    }
}
