//! Filmcraft ProcAmp signal math (pinned gpufx.rs, MIT).
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ProcAmp {
    pub brightness: f32,
    pub contrast: f32,
    pub hue: f32,
    pub saturation: f32,
}
impl Default for ProcAmp {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 100.0,
            hue: 0.0,
            saturation: 100.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcAmpChannel {
    Brightness,
    Contrast,
    Hue,
    Saturation,
}
pub type ProcAmpAnimation =
    std::collections::BTreeMap<ProcAmpChannel, super::composition::AnimatableValue>;
impl ProcAmp {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.hue.is_finite(), "nonfinite ProcAmp hue");
        for (value, min, max) in [
            (self.brightness, -100.0, 100.0),
            (self.contrast, 0.0, 200.0),
            (self.saturation, 0.0, 200.0),
        ] {
            ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "ProcAmp parameter out of range"
            );
        }
        Ok(())
    }
    pub fn signal(&self, input: [f32; 3]) -> [f32; 3] {
        let br = self.brightness / 100.0 * 0.4;
        let co = self.contrast / 100.0;
        let hue = self.hue / 360.0;
        let sat = self.saturation / 100.0;
        let mut hsl = super::tone_curves::rgb_to_hsl(input[0], input[1], input[2]);
        hsl[0] = (hsl[0] + hue).rem_euclid(1.0);
        hsl[1] = (hsl[1] * sat).clamp(0.0, 1.0);
        super::tone_curves::hsl_to_rgb(hsl[0], hsl[1], hsl[2]).map(|v| (v - 0.5) * co + 0.5 + br)
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_signal(&mut |input, _, _| Ok(self.signal(input)), checkpoint)
    }
}
pub fn validate_animation(animation: &ProcAmpAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let bounds = match channel {
            ProcAmpChannel::Brightness => Some((-100.0, 100.0)),
            ProcAmpChannel::Contrast | ProcAmpChannel::Saturation => Some((0.0, 200.0)),
            ProcAmpChannel::Hue => None,
        };
        if let Some((min, max)) = bounds {
            ensure!(
                super::composition::animatable_values_in(value, min, max, true),
                "ProcAmp animation out of range"
            );
        }
    }
    Ok(())
}
pub fn sample_animation(
    mut parameters: ProcAmp,
    animation: &ProcAmpAnimation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ProcAmp> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, value) in animation {
        let sampled = super::nested_composition::sample_value(value, time)? as f32;
        *match channel {
            ProcAmpChannel::Brightness => &mut parameters.brightness,
            ProcAmpChannel::Contrast => &mut parameters.contrast,
            ProcAmpChannel::Hue => &mut parameters.hue,
            ProcAmpChannel::Saturation => &mut parameters.saturation,
        } = sampled;
    }
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_parameters_match_independently_compiled_source_renderer_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: ProcAmp,
            input: [f32; 3],
            bits: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Oracle {
            commit: String,
            cases: Vec<Case>,
        }
        let oracle: Oracle =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-proc-amp.json"))
                .unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 96);
        for (index, case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();
            let encoded = case
                .input
                .map(|v| super::super::basic_grade::display(v.max(0.0)));
            let actual = case
                .parameters
                .signal(encoded)
                .map(|v| super::super::basic_grade::linear(v.clamp(0.0, 1.0)).to_bits());
            assert_eq!(
                actual, case.bits,
                "source case{index}: {:?}",
                case.parameters
            );
        }
    }
    #[test]
    fn all_source_channels_animate_and_unknown_channels_refuse() {
        let animation:ProcAmpAnimation=serde_json::from_value(serde_json::json!({
   "brightness":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-100.0},{"tick":1000000,"value":100.0}]}},
   "contrast":{"mode":"constant","value":50.0},"saturation":{"mode":"constant","value":200.0},"hue":{"mode":"constant","value":1080.0}
  })).unwrap();
        assert_eq!(
            sample_animation(ProcAmp::default(), &animation, 500000).unwrap(),
            ProcAmp {
                brightness: 0.0,
                contrast: 50.0,
                saturation: 200.0,
                hue: 1080.0
            }
        );
        let bad: ProcAmpAnimation = serde_json::from_value(
            serde_json::json!({"contrast":{"mode":"constant","value":201.0}}),
        )
        .unwrap();
        assert!(validate_animation(&bad).is_err());
        assert!(
            serde_json::from_value::<ProcAmpAnimation>(
                serde_json::json!({"gamma":{"mode":"constant","value":10.0}})
            )
            .is_err()
        );
        assert!(
            ProcAmp {
                hue: f32::INFINITY,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        let effect: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"proc_amp","parameters":{},"animation":animation}),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<super::super::composition::VideoEffect>(
                serde_json::to_value(&effect).unwrap()
            )
            .unwrap(),
            effect
        );
    }
}
