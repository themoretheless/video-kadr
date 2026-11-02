//! Filmcraft Simple Text parameters and native linear working-frame application.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{linear_color_frame::LinearColorFrame, text_burnin::draw_text};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SimpleText {
    pub text: String,
    pub position: [Option<f64>; 2],
    pub size: f32,
    pub font: u32,
    pub alignment: u32,
    pub color: [f32; 4],
    pub bg_opacity: f32,
}
impl Default for SimpleText {
    fn default() -> Self {
        Self {
            text: "Simple Text".into(),
            position: [None, None],
            size: 64.,
            font: 0,
            alignment: 1,
            color: [1.; 4],
            bg_opacity: 0.,
        }
    }
}
impl SimpleText {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.font <= 1 && self.alignment <= 2,
            "invalid Simple Text choice"
        );
        ensure!(
            self.size.is_finite() && (1.0..=1000.0).contains(&self.size),
            "invalid Simple Text size"
        );
        ensure!(
            self.position.iter().flatten().all(|v| v.is_finite()),
            "invalid Simple Text position"
        );
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid Simple Text color"
        );
        ensure!(
            self.bg_opacity.is_finite() && (0.0..=100.0).contains(&self.bg_opacity),
            "invalid Simple Text background"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        pixel_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            pixel_scale.is_finite() && pixel_scale > 0.0,
            "invalid Simple Text pixel scale"
        );
        let (w, h) = frame.dimensions();
        let position = [
            self.position[0].map_or(w as f64 * 0.5, |v| v * pixel_scale as f64),
            self.position[1].map_or(h as f64 * 0.5, |v| v * pixel_scale as f64),
        ];
        draw_text(
            frame,
            &self.text,
            ["Inter", "JetBrains Mono"][self.font as usize],
            position,
            (self.size * pixel_scale).max(2.0),
            self.alignment,
            self.color,
            self.bg_opacity / 100.0,
            checkpoint,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SimpleTextChannel {
    PositionX,
    PositionY,
    Size,
    ColorR,
    ColorG,
    ColorB,
    ColorA,
    BgOpacity,
}
pub type SimpleTextAnimation =
    std::collections::BTreeMap<SimpleTextChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &SimpleTextAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            SimpleTextChannel::PositionX | SimpleTextChannel::PositionY => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            SimpleTextChannel::Size => (1., 1000.),
            SimpleTextChannel::BgOpacity => (0., 100.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Simple Text animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut parameters: SimpleText,
    animation: &SimpleTextAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<SimpleText> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            SimpleTextChannel::PositionX => parameters.position[0] = Some(value),
            SimpleTextChannel::PositionY => parameters.position[1] = Some(value),
            SimpleTextChannel::Size => parameters.size = value as f32,
            SimpleTextChannel::ColorR => parameters.color[0] = value as f32,
            SimpleTextChannel::ColorG => parameters.color[1] = value as f32,
            SimpleTextChannel::ColorB => parameters.color[2] = value as f32,
            SimpleTextChannel::ColorA => parameters.color[3] = value as f32,
            SimpleTextChannel::BgOpacity => parameters.bg_opacity = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eight_local_channels_preserve_text_choices_and_roundtrip() {
        let values = [
            ("positionX", 12.25),
            ("positionY", -7.75),
            ("size", 20.25),
            ("colorR", 0.2),
            ("colorG", 0.4),
            ("colorB", 0.6),
            ("colorA", 0.8),
            ("bgOpacity", 37.5),
        ];
        let mut map = serde_json::Map::new();
        for (name, value) in values {
            map.insert(name.into(), serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-0.1},{"tick":750,"value":value+0.1}]}}));
        }
        let animation: SimpleTextAnimation =
            serde_json::from_value(serde_json::Value::Object(map)).unwrap();
        let base = SimpleText {
            text: "Привет עולם".into(),
            font: 1,
            alignment: 2,
            ..Default::default()
        };
        let result = sample(base.clone(), &animation, 500000).unwrap();
        assert_eq!(result.position, [Some(12.25), Some(-7.75)]);
        assert_eq!(result.size, 20.25);
        assert_eq!(result.color, [0.2, 0.4, 0.6, 0.8]);
        assert_eq!(result.bg_opacity, 37.5);
        assert_eq!(
            (&result.text, result.font, result.alignment),
            (&base.text, base.font, base.alignment)
        );
        assert_eq!(
            result,
            serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap()
        );
        assert_eq!(
            animation,
            serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap()
        );
        assert_eq!(base.position, [None, None]);
        let invalid: SimpleTextAnimation =
            serde_json::from_value(serde_json::json!({"colorA":{"mode":"constant","value":1.01}}))
                .unwrap();
        assert!(validate_animation(&invalid).is_err());
        assert!(SimpleText {
            size: 0.,
            ..base.clone()
        }
        .validate()
        .is_err());
        assert!(SimpleText {
            position: [Some(f64::INFINITY), None],
            ..base
        }
        .validate()
        .is_err());
    }
}
