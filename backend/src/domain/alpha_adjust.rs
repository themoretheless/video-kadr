//! Filmcraft Alpha Adjust premultiplied CPU contract, including zero-alpha pixels.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AlphaAdjust {
    pub opacity: f32,
    pub ignore: bool,
    pub invert: bool,
    pub mask_only: bool,
}
impl Default for AlphaAdjust {
    fn default() -> Self { Self { opacity: 100.0, ignore: false, invert: false, mask_only: false } }
}
impl AlphaAdjust {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.opacity.is_finite() && (0.0..=100.0).contains(&self.opacity), "Alpha Adjust opacity out of range");
        Ok(())
    }
    pub fn pixel(&self, pixel: [f32; 4]) -> [f32; 4] {
        let color = if pixel[3] <= 1e-6 { [0.0; 3] } else { [pixel[0] / pixel[3], pixel[1] / pixel[3], pixel[2] / pixel[3]] };
        let mut alpha = if self.ignore { 1.0 } else { pixel[3] };
        if self.invert { alpha = 1.0 - alpha; }
        alpha = (alpha * (self.opacity / 100.0)).clamp(0.0, 1.0);
        if self.mask_only {
            let gray = super::basic_grade::linear(alpha);
            [gray, gray, gray, 1.0]
        } else {
            [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
        }
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut |pixel| self.pixel(pixel), checkpoint)
    }
}
pub fn validate_animation(value: &super::composition::AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(super::composition::animatable_values_in(value, 0.0, 100.0, true), "Alpha Adjust opacity animation out of range");
    Ok(())
}
pub fn sample_animation(mut parameters: AlphaAdjust, value: &super::composition::AnimatableValue, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<AlphaAdjust> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(value)?;
    parameters.opacity = super::nested_composition::sample_value(value, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: AlphaAdjust, input: [f32; 4], bits: [u32; 4] }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-alpha-adjust.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 224);
        for (index, case) in oracle.cases.iter().enumerate() {
            assert_eq!(case.parameters.pixel(case.input).map(f32::to_bits), case.bits, "original Alpha Adjust case {index}");
        }
    }
    #[test]
    fn persisted_opacity_curve_preserves_all_authored_flags() {
        let effect: super::super::composition::VideoEffect = serde_json::from_value(serde_json::json!({"kind":"alpha_adjust","parameters":{"ignore":true,"invert":true,"maskOnly":true},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}}})).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let super::super::composition::VideoEffect::AlphaAdjust { parameters, animation: Some(animation) } = effect else { panic!("Alpha Adjust"); };
        let sampled = sample_animation(parameters, &animation, 500000).unwrap();
        assert_eq!(sampled.opacity, 50.0);
        assert!(sampled.ignore && sampled.invert && sampled.mask_only);
    }
    #[test]
    fn opacity_inversion_ignore_order_and_hidden_rgb() {
        let p = [0.125, 0.25, 0.5, 0.25];
        assert_eq!(AlphaAdjust { opacity: 50.0, ..Default::default() }.pixel(p), [0.0625, 0.125, 0.25, 0.125]);
        assert_eq!(AlphaAdjust { invert: true, ..Default::default() }.pixel(p), [0.375, 0.75, 1.5, 0.75]);
        assert_eq!(AlphaAdjust { ignore: true, invert: true, ..Default::default() }.pixel(p), [0.0; 4]);
        assert_eq!(AlphaAdjust { ignore: true, ..Default::default() }.pixel([1.0, 2.0, 3.0, 0.0]), [0.0, 0.0, 0.0, 1.0]);
        let gray = super::super::basic_grade::linear(0.25);
        assert_eq!(AlphaAdjust { mask_only: true, ..Default::default() }.pixel(p), [gray, gray, gray, 1.0]);
        assert!(AlphaAdjust { opacity: 101.0, ..Default::default() }.validate().is_err());
    }
}
