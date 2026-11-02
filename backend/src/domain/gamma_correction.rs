//! Filmcraft Gamma Correction source signal math (MIT; pinned gpufx.rs).
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GammaCorrection {
    pub gamma: f32,
}
impl Default for GammaCorrection {
    fn default() -> Self {
        Self { gamma: 10.0 }
    }
}
impl GammaCorrection {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.gamma.is_finite() && (1.0..=28.0).contains(&self.gamma),
            "Gamma Correction parameter out of range"
        );
        Ok(())
    }
    pub fn signal(&self, value: f32) -> f32 {
        value.max(0.0).powf(self.gamma / 10.0)
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_signal(
            &mut |input, _, _| Ok(input.map(|value| self.signal(value))),
            checkpoint,
        )
    }
}
pub fn validate_animation(value: &super::composition::AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, 1.0, 28.0, true),
        "Gamma Correction animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut parameters: GammaCorrection,
    animation: &super::composition::AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<GammaCorrection> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.gamma = super::nested_composition::sample_value(animation, time)? as f32;
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
            parameters: GammaCorrection,
            input: [f32; 3],
            bits: [u32; 3],
        }
        #[derive(Deserialize)]
        struct Oracle {
            commit: String,
            cases: Vec<Case>,
        }
        let oracle: Oracle =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gamma.json"))
                .unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 24);
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
    fn source_units_animation_and_persistence() {
        assert_eq!(GammaCorrection { gamma: 20.0 }.signal(0.5), 0.25);
        let animation=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.0},{"tick":1000000,"value":28.0}]}})).unwrap();
        assert_eq!(
            sample_animation(GammaCorrection::default(), &animation, 500000)
                .unwrap()
                .gamma,
            14.5
        );
        let invalid =
            serde_json::from_value(serde_json::json!({"mode":"constant","value":28.1})).unwrap();
        assert!(validate_animation(&invalid).is_err());
        assert!(GammaCorrection { gamma: f32::NAN }.validate().is_err());
        let effect: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"gamma_correction","parameters":{},"animation":animation}),
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
