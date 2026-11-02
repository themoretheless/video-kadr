//! Filmcraft display-distance Color Replace CPU contract.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorReplace {
    pub similarity: f32,
    pub solid: bool,
    pub target: [f32; 4],
    pub replace: [f32; 4],
}
impl Default for ColorReplace {
    fn default() -> Self {
        Self { similarity: 10.0, solid: false, target: [1.0, 0.0, 0.0, 1.0], replace: [0.0, 0.0, 1.0, 1.0] }
    }
}
impl ColorReplace {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.target.iter().chain(self.replace.iter()).all(|v| v.is_finite()), "nonfinite Color Replace endpoint");
        ensure!(self.similarity.is_finite() && (0.0..=100.0).contains(&self.similarity), "Color Replace similarity out of range");
        Ok(())
    }
    pub fn linear_rgb(&self, input: [f32; 3]) -> [f32; 3] {
        let encoded = input.map(|v| super::basic_grade::display(v.max(0.0)));
        let distance = ((encoded[0] - self.target[0]).powi(2) + (encoded[1] - self.target[1]).powi(2) + (encoded[2] - self.target[2]).powi(2)).sqrt();
        let similarity = self.similarity / 100.0 * 1.2;
        let low = similarity * 0.85;
        let t = ((distance - low) / (similarity.max(1e-4) - low).max(1e-6)).clamp(0.0, 1.0);
        let weight = 1.0 - t * t * (3.0 - 2.0 * t);
        if weight <= 0.0 { return input; }
        let replacement = if self.solid {
            [self.replace[0], self.replace[1], self.replace[2]]
        } else {
            let hsl = super::tone_curves::rgb_to_hsl(self.replace[0], self.replace[1], self.replace[2]);
            let lightness = super::tone_curves::rgb_to_hsl(encoded[0], encoded[1], encoded[2])[2];
            super::tone_curves::hsl_to_rgb(hsl[0], hsl[1], lightness)
        };
        std::array::from_fn(|i| super::basic_grade::linear((encoded[i] + (replacement[i] - encoded[i]) * weight).clamp(0.0, 1.0)))
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        self.validate()?;
        frame.map_premultiplied(&mut |mut pixel| {
            let a = pixel[3];
            if a > 1e-6 {
                let rgb = self.linear_rgb([pixel[0] / a, pixel[1] / a, pixel[2] / a]);
                for i in 0..3 { pixel[i] = rgb[i] * a; }
            }
            pixel
        }, checkpoint)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: ColorReplace, input: [f32; 4], bits: [u32; 4] }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-replace.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(), 144);
        for (index, case) in oracle.cases.iter().enumerate() {
            let mut actual = case.input;
            let a = actual[3];
            if a > 1e-6 {
                let rgb = case.parameters.linear_rgb([actual[0] / a, actual[1] / a, actual[2] / a]);
                for i in 0..3 { actual[i] = rgb[i] * a; }
            }
            assert_eq!(actual.map(f32::to_bits), case.bits, "original Color Replace case {index}");
        }
    }
    #[test]
    fn persisted_independent_endpoint_curves_and_similarity_preserve_solid() {
        let effect: super::super::composition::VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"color_replace","parameters":{"solid":true},
            "animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}},
            "colorAnimation":{"targetR":{"mode":"constant","value":0.25},"replaceG":{"mode":"constant","value":0.75},"replaceA":{"mode":"constant","value":0}}
        })).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let super::super::composition::VideoEffect::ColorReplace { parameters, animation: Some(animation), color_animation: Some(colors) } = effect else { panic!("Color Replace"); };
        let sampled = sample_colors(sample_animation(parameters, &animation, 500000).unwrap(), &colors, 500000).unwrap();
        assert_eq!(sampled.similarity, 50.0);
        assert_eq!(sampled.target, [0.25, 0.0, 0.0, 1.0]);
        assert_eq!(sampled.replace, [0.0, 0.75, 1.0, 0.0]);
        assert!(sampled.solid);
    }
    #[test]
    fn replacement_retains_lightness_unless_solid_and_ignores_endpoint_alpha() {
        let red = super::super::basic_grade::linear(0.5);
        let effect = ColorReplace { target: [0.5, 0.0, 0.0, 0.0], replace: [0.0, 0.0, 1.0, 0.0], ..Default::default() };
        let result = effect.linear_rgb([red, 0.0, 0.0]);
        assert!(result[0].abs() < 1e-6 && result[1].abs() < 1e-6 && (result[2] - red).abs() < 1e-6);
        assert_eq!(ColorReplace { solid: true, ..effect }.linear_rgb([red, 0.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_eq!(effect.linear_rgb([0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]);
        assert!(ColorReplace { similarity: 101.0, ..effect }.validate().is_err());
    }
}

pub fn validate_animation(value:&super::composition::AnimatableValue)->Result<()> {
 value.validate()?;ensure!(super::composition::animatable_values_in(value,0.0,100.0,true),"Color Replace similarity animation out of range");Ok(())
}
pub fn sample_animation(mut parameters:ColorReplace,value:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorReplace> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(value)?;parameters.similarity=super::nested_composition::sample_value(value,time)? as f32;parameters.validate()?;Ok(parameters)
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum ReplaceColorChannel {TargetR,TargetG,TargetB,TargetA,ReplaceR,ReplaceG,ReplaceB,ReplaceA}
pub type ReplaceColorAnimation=std::collections::BTreeMap<ReplaceColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&ReplaceColorAnimation)->Result<()> {
    for value in animation.values() {value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Color Replace endpoint animation exceeds finite f32 range");}
    Ok(())
}
pub fn sample_colors(mut parameters:ColorReplace,animation:&ReplaceColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<ColorReplace> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    validate_color_animation(animation)?;
    for (channel,value) in animation {
        let value=super::nested_composition::sample_value(value,time)? as f32;
        let (color,index)=match channel {
            ReplaceColorChannel::TargetR=>(&mut parameters.target,0),ReplaceColorChannel::TargetG=>(&mut parameters.target,1),
            ReplaceColorChannel::TargetB=>(&mut parameters.target,2),ReplaceColorChannel::TargetA=>(&mut parameters.target,3),
            ReplaceColorChannel::ReplaceR=>(&mut parameters.replace,0),ReplaceColorChannel::ReplaceG=>(&mut parameters.replace,1),
            ReplaceColorChannel::ReplaceB=>(&mut parameters.replace,2),ReplaceColorChannel::ReplaceA=>(&mut parameters.replace,3),
        };color[index]=value;
    }
    parameters.validate()?;Ok(parameters)
}
