//! Filmcraft Posterize signal math (MIT), gpufx.rs at 5231852443363f001c3f6b396dd9b1e6461ae2be.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Posterize {
    pub levels: f32,
}
impl Default for Posterize {
    fn default() -> Self { Self { levels: 7.0 } }
}
impl Posterize {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.levels.is_finite() && (2.0..=255.0).contains(&self.levels), "Posterize level out of range");
        Ok(())
    }
    pub fn signal(&self, value: f32) -> f32 {
        let n = self.levels.max(2.0) - 1.0;
        (value * n).round() / n
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.validate()?;
        frame.map_signal(&mut |input, _, _| Ok(input.map(|value| self.signal(value))), checkpoint)
    }
}
pub fn validate_animation(value: &super::composition::AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(super::composition::animatable_values_in(value, 2.0, 255.0, true), "Posterize animation out of range");
    Ok(())
}
pub fn sample_animation(mut parameters: Posterize, animation: &super::composition::AnimatableValue, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Posterize> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.levels = super::nested_composition::sample_value(animation, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_compiled_source_renderer_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: Posterize,
            input: [f32; 3],
            bits: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Oracle {
            commit: String,
            cases: Vec<Case>,
        }
        let oracle: Oracle =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-posterize.json"))
                .unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 30);
        for (index, case) in oracle.cases.iter().enumerate() {
            case.parameters.validate().unwrap();
            let actual = case.input.map(|v| {
                super::super::basic_grade::linear(
                    case.parameters
                        .signal(super::super::basic_grade::display(v.max(0.0)))
                        .clamp(0.0, 1.0),
                )
                .to_bits()
            });
            assert_eq!(actual, case.bits, "source case{index}");
        }
    }
    #[test]
    fn quantization_boundaries_and_fractional_levels() {
        assert_eq!(Posterize::default().levels, 7.0);
        let binary = Posterize { levels: 2.0 };
        assert_eq!(binary.signal(0.499), 0.0);
        assert_eq!(binary.signal(0.5), 1.0);
        assert_eq!(binary.signal(1.0), 1.0);
        let fractional = Posterize { levels: 3.5 };
        assert_eq!(fractional.signal(0.5), 0.4);
        assert_eq!(fractional.signal(1.0), 1.2);
        for levels in [1.0, 256.0, f32::NAN, f32::INFINITY] {
            assert!(Posterize { levels }.validate().is_err());
        }
    }
}
