//! Filmcraft Color Key CPU behavior, including the authored but unused Edge Thin.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorKey {
    pub color: [f32; 4],
    pub tolerance: f32,
    pub thin: f32,
    pub feather: f32,
}
impl Default for ColorKey {
    fn default() -> Self { Self { color: [0.0, 0.0, 1.0, 1.0], tolerance: 0.0, thin: 0.0, feather: 0.0 } }
}
pub(crate) fn ycbcr(rgb: [f32; 3]) -> [f32; 3] {
    let kr = 0.2126f32;
    let kb = 0.0722f32;
    let kg = 1.0 - kr - kb;
    let y = kr * rgb[0] + kg * rgb[1] + kb * rgb[2];
    [y, (rgb[2] - y) / (2.0 * (1.0 - kb)), (rgb[0] - y) / (2.0 * (1.0 - kr))]
}
impl ColorKey {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.color.iter().all(|v| v.is_finite()), "nonfinite Color Key color");
        ensure!(self.tolerance.is_finite() && (0.0..=255.0).contains(&self.tolerance), "Color Key tolerance out of range");
        ensure!(self.thin.is_finite() && (-5.0..=5.0).contains(&self.thin), "Color Key thin out of range");
        ensure!(self.feather.is_finite() && (0.0..=50.0).contains(&self.feather), "Color Key feather out of range");
        Ok(())
    }
    pub fn pixel(&self, pixel: [f32; 4]) -> [f32; 4] {
        if pixel[3] <= 0.0 { return pixel; }
        let key = ycbcr([self.color[0], self.color[1], self.color[2]]);
        let encoded = [pixel[0] / pixel[3], pixel[1] / pixel[3], pixel[2] / pixel[3]].map(|v| super::basic_grade::display(v.max(0.0)));
        let signal = ycbcr(encoded);
        let distance = ((signal[1] - key[1]).powi(2) + (signal[2] - key[2]).powi(2)).sqrt() + (signal[0] - key[0]).abs() * 0.15;
        let tolerance = self.tolerance / 255.0 * 0.6 + 0.01;
        let softness = self.feather / 50.0 * 0.2 + 0.01;
        let alpha = pixel[3] * ((distance - tolerance) / softness).clamp(0.0, 1.0);
        let linear = encoded.map(|v| super::basic_grade::linear(v.clamp(0.0, 1.0)));
        [linear[0] * alpha, linear[1] * alpha, linear[2] * alpha, alpha]
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut |pixel| self.pixel(pixel), checkpoint)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyChannel { Tolerance, Thin, Feather }
pub type KeyAnimation = std::collections::BTreeMap<KeyChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &KeyAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (min, max) = match channel { KeyChannel::Tolerance => (0.0, 255.0), KeyChannel::Thin => (-5.0, 5.0), KeyChannel::Feather => (0.0, 50.0) };
        ensure!(super::composition::animatable_values_in(value, min, max, true), "Color Key animation out of range");
    }
    Ok(())
}
pub fn sample_animation(mut parameters: ColorKey, animation: &KeyAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<ColorKey> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, time)? as f32;
        match channel { KeyChannel::Tolerance => parameters.tolerance = value, KeyChannel::Thin => parameters.thin = value, KeyChannel::Feather => parameters.feather = value }
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
        struct Case { parameters: ColorKey, input: [f32; 4], bits: [u32; 4] }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-key.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 504);
        for (index, case) in oracle.cases.iter().enumerate() {
            assert_eq!(case.parameters.pixel(case.input).map(f32::to_bits), case.bits, "original Color Key case {index}");
        }
    }
    #[test]
    fn independent_scalar_color_curves_keep_authored_thin_and_roundtrip() {
        let effect: super::super::composition::VideoEffect = serde_json::from_value(serde_json::json!({"kind":"color_key","parameters":{"thin":-5},"animation":{"tolerance":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":255}]}},"feather":{"mode":"constant","value":25}},"colorAnimation":{"g":{"mode":"constant","value":0.8},"a":{"mode":"constant","value":0}}})).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let super::super::composition::VideoEffect::ColorKey { parameters, animation: Some(animation), color_animation: Some(colors) } = effect else { panic!("Color Key"); };
        let sampled = sample_color(sample_animation(parameters, &animation, 500000).unwrap(), &colors, 500000).unwrap();
        assert_eq!(sampled.tolerance, 127.5);
        assert_eq!(sampled.feather, 25.0);
        assert_eq!(sampled.thin, -5.0);
        assert_eq!(sampled.color, [0.0, 0.8, 1.0, 0.0]);
    }
    #[test]
    fn selected_blue_cut_and_authored_thin_does_not_change_source_behavior() {
        let effect = ColorKey::default();
        assert_eq!(effect.pixel([0.0, 0.0, 0.25, 0.25]), [0.0; 4]);
        let p = [0.125, 0.25, 0.5, 0.25];
        assert_eq!(ColorKey { thin: 5.0, ..effect }.pixel(p), effect.pixel(p));
        assert_eq!(effect.pixel([1.0, 2.0, 3.0, 0.0]), [1.0, 2.0, 3.0, 0.0]);
        assert!(ColorKey { tolerance: 256.0, ..effect }.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum KeyColorChannel {R,G,B,A}
pub type KeyColorAnimation=std::collections::BTreeMap<KeyColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&KeyColorAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Color Key animation exceeds finite f32 range");}Ok(())
}
pub fn sample_color(mut parameters:ColorKey,animation:&KeyColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorKey> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_color_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;let index=match channel {KeyColorChannel::R=>0,KeyColorChannel::G=>1,KeyColorChannel::B=>2,KeyColorChannel::A=>3};parameters.color[index]=value;}
 parameters.validate()?;Ok(parameters)
}
