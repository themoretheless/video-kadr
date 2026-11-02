//! Filmcraft Luma Key: display-encoded linear Rec709 luma scales premultiplied RGBA.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LumaKey { pub threshold: f32, pub cutoff: f32 }
impl LumaKey {
    pub fn validate(&self) -> Result<()> {
        ensure!([self.threshold, self.cutoff].iter().all(|v| v.is_finite() && (0.0..=100.0).contains(v)), "Luma Key parameter out of range");
        Ok(())
    }
    pub fn pixel(&self, mut pixel: [f32; 4]) -> [f32; 4] {
        if pixel[3] <= 0.0 { return pixel; }
        let threshold = self.threshold / 100.0;
        let cutoff = self.cutoff / 100.0;
        let luma = super::basic_grade::display(super::black_white::gray([pixel[0] / pixel[3], pixel[1] / pixel[3], pixel[2] / pixel[3]]).max(0.0));
        let alpha = if luma <= cutoff { 0.0 } else if luma >= threshold.max(cutoff + 1e-3) { 1.0 } else { (luma - cutoff) / (threshold - cutoff).max(1e-3) };
        for value in &mut pixel { *value *= alpha; }
        pixel
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut |pixel| self.pixel(pixel), checkpoint)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LumaChannel { Threshold, Cutoff }
pub type LumaAnimation = std::collections::BTreeMap<LumaChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &LumaAnimation) -> Result<()> {
    for value in animation.values() {
        value.validate()?;
        ensure!(super::composition::animatable_values_in(value, 0.0, 100.0, true), "Luma Key animation out of range");
    }
    Ok(())
}
pub fn sample_animation(mut parameters: LumaKey, animation: &LumaAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<LumaKey> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, time)? as f32;
        match channel { LumaChannel::Threshold => parameters.threshold = value, LumaChannel::Cutoff => parameters.cutoff = value }
    }
    parameters.validate()?;
    Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: LumaKey, input: [f32; 4], bits: [u32; 4] }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-luma-key.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 72);
        for (index, case) in oracle.cases.iter().enumerate() {
            assert_eq!(case.parameters.pixel(case.input).map(f32::to_bits), case.bits, "original Luma Key case {index}");
        }
    }
    #[test]
    fn persisted_threshold_and_cutoff_curves_sample_independently() {
        let effect: super::super::composition::VideoEffect = serde_json::from_value(serde_json::json!({"kind":"luma_key","parameters":{},"animation":{"threshold":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}},"cutoff":{"mode":"constant","value":75}}})).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let super::super::composition::VideoEffect::LumaKey { parameters, animation: Some(animation) } = effect else { panic!("Luma Key"); };
        let sampled = sample_animation(parameters, &animation, 500000).unwrap();
        assert_eq!(sampled, LumaKey { threshold: 50.0, cutoff: 75.0 });
        sampled.validate().unwrap();
    }
    #[test]
    fn threshold_cutoff_crossing_and_existing_alpha() {
        let linear = super::super::basic_grade::linear(0.5);
        let p = [linear * 0.25, linear * 0.25, linear * 0.25, 0.25];
        assert_eq!(LumaKey::default().pixel(p), p);
        let result = LumaKey { threshold: 100.0, cutoff: 0.0 }.pixel(p);
        assert!((result[3] - 0.125).abs() < 1e-6);
        assert_eq!(LumaKey { threshold: 25.0, cutoff: 75.0 }.pixel(p), [0.0; 4]);
        assert_eq!(LumaKey::default().pixel([1.0, 2.0, 3.0, 0.0]), [1.0, 2.0, 3.0, 0.0]);
        assert!(LumaKey { cutoff: 101.0, ..Default::default() }.validate().is_err());
    }
}
