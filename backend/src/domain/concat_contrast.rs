//! Independently implemented scene-linear stop contrast for Concat format-2 packages.
//! Behavioral reference: concat.contrast at 78e4be1; no upstream implementation included.
use super::{composition::AnimatableValue, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConcatContrast {
    pub amount: f32,
}
impl ConcatContrast {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (-100.0..=100.0).contains(&self.amount),
            "Concat contrast amount out of range"
        );
        Ok(())
    }
    pub fn channel(&self, value: f32) -> f32 {
        if value <= 0.0 || self.amount == 0.0 {
            return value;
        }
        let exponent = 1.0 + self.amount / 100.0;
        (0.18 * (value.max(1e-10) / 0.18).powf(exponent)).min(65504.0)
    }
    pub fn apply_sampled_linear(amount:f32,frame:&mut LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        ensure!(amount.is_finite(),"nonfinite Concat contrast source value");
        let parameters=Self{amount};super::concat_working::apply_channels(frame,|value|parameters.channel(value),checkpoint)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_channels(frame, |input| self.channel(input), checkpoint)
    }
}
pub fn validate_concat_keys(keys:&[super::concat_effect_upgrade::ParamKey])->Result<()>{
    ensure!(keys.len()<=4096,"too many Concat contrast source keys");
    ensure!(keys.iter().all(|key|key.at.is_finite()&&key.value.is_finite()&&(key.derived||(-100. ..=100.).contains(&key.value))&&key.ease.iter().all(|v|v.is_finite())),"invalid Concat contrast source key");
    Ok(())
}
pub fn sample_concat_keys(parameters:ConcatContrast,keys:&[super::concat_effect_upgrade::ParamKey],progress:f64)->Result<f32>{
    parameters.validate()?;validate_concat_keys(keys)?;
    let value=super::concat_animation::PreparedTrack::new(keys)?.value_at(progress,parameters.amount as f64)? as f32;
    ensure!(value.is_finite(),"nonfinite Concat contrast source result");Ok(value)
}
pub fn validate_animation(value: &AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, -100.0, 100.0, true),
        "Concat contrast animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut p: ConcatContrast,
    value: &AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ConcatContrast> {
    p.validate()?;
    validate_animation(value)?;
    p.amount = super::nested_composition::sample_value(value, time.into())? as f32;
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::super::concat_working::texture_half;
    use super::*;
    #[test]
    fn stop_contrast_math_and_half_float_alpha_boundary() {
        for amount in [-100.0, -50.0, 0.0, 40.0, 100.0] {
            let p = ConcatContrast { amount };
            p.validate().unwrap();
            assert_eq!(p.channel(0.0), 0.0);
            assert_eq!(p.channel(-0.1), -0.1);
            assert!((p.channel(0.18) - 0.18).abs() < 1e-7);
        }
        let p = ConcatContrast { amount: 100.0 };
        assert!((p.channel(0.36) - 0.72).abs() < 1e-6);
        assert!(p.channel(2.0) > 2.0);
        assert_eq!(p.channel(65504.0), 65504.0);
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[0.18, 0.09, 0.0, 0.5], &mut || Ok(())).unwrap();
        p.apply_linear(&mut frame, &mut || Ok(())).unwrap();
        assert_eq!(frame.pixels()[0][3], 0.5);
        assert_eq!(frame.pixels()[0][0], 0.36010742);
        assert_eq!(frame.pixels()[0][2], 0.0);
    }
    #[test]
    fn wire_validation_and_native_effect_admission() {
        for amount in [-101.0, 101.0, f32::NAN, f32::INFINITY] {
            assert!(ConcatContrast { amount }.validate().is_err());
        }
        let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_contrast","parameters":{"amount":40},"animation":{"mode":"constant","value":-20}})).unwrap();
        effect.validate().unwrap();
        assert!(serde_json::from_value::<ConcatContrast>(serde_json::json!({"drive":1})).is_err());
    }
    #[test]
    fn native_half_float_stack_keeps_extended_light_and_samples_clip_animation() {
        let make = |amount: f32| super::super::composition::VideoEffect::ConcatContrast {
            parameters: ConcatContrast { amount },
            animation: None,
                        concat_keys: None,
        };
        let original = [0.36, 2.0, -0.1, 1.0];
        let mut frame = LinearColorFrame::from_working(1, 1, &original, &mut || Ok(())).unwrap();
        super::super::flat_surface::apply_effect_stack_working(
            &mut frame,
            &[make(100.0), make(-50.0)],
            0,
            &[],
            None,
            &mut || Ok(()),
        )
        .unwrap();
        for (actual, expected) in
            frame.pixels()[0]
                .iter()
                .zip([0.35986328, 1.9990234, -0.099975586, 1.0])
        {
            assert!((actual - expected).abs() < 1e-5);
        }
        let animation:AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-100},{"tick":1000000,"value":100}]}})).unwrap();
        for (time, expected) in [(0, -100.0), (500000, 0.0), (1000000, 100.0)] {
            assert_eq!(
                sample_animation(ConcatContrast::default(), &animation, time)
                    .unwrap()
                    .amount,
                expected
            );
        }
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[0.36, 0.18, 0.0, 1.0], &mut || Ok(())).unwrap();
        let effect = super::super::composition::VideoEffect::ConcatContrast {
            parameters: ConcatContrast::default(),
            animation: Some(animation),
            concat_keys: None,
        };
        super::super::flat_surface::apply_effect_stack_working(
            &mut frame,
            &[effect],
            1000000,
            &[],
            None,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(frame.pixels()[0][0], 0.72021484);
        assert_eq!(frame.pixels()[0][1], 0.18005371);
    }
    #[test]
    fn original_metal_shader_oracle() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            amounts: Vec<f32>,
            effective_input: [f32; 4],
            gpu: [f32; 4],
            tolerance: [f32; 4],
        }
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/concat-contrast-gpu.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(fixture["adapter"]["backend"], "Metal");
        assert_eq!(fixture["upstreamProbes"], 5);
        let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
        assert_eq!(cases.len(), 320);
        let mut worst = 0.0f32;
        for (index, case) in cases.into_iter().enumerate() {
            let alpha = case.effective_input[3];
            assert!(alpha > 0.0 && alpha <= 1.0);
            let premultiplied = [
                case.effective_input[0] * alpha,
                case.effective_input[1] * alpha,
                case.effective_input[2] * alpha,
                alpha,
            ];
            let mut frame =
                LinearColorFrame::from_working(1, 1, &premultiplied, &mut || Ok(())).unwrap();
            let effects = case
                .amounts
                .iter()
                .map(
                    |amount| super::super::composition::VideoEffect::ConcatContrast {
                        parameters: ConcatContrast { amount: *amount },
                        animation: None,
                        concat_keys: None,
                    },
                )
                .collect::<Vec<_>>();
            super::super::flat_surface::apply_effect_stack_working(
                &mut frame,
                &effects,
                0,
                &[],
                None,
                &mut || Ok(()),
            )
            .unwrap();
            let got = frame.pixels()[0];
            assert_eq!(got[3], case.gpu[3], "alpha case{index}");
            for channel in 0..3 {
                let allowed = case.tolerance[channel];
                assert!(
                    allowed.is_finite()
                        && allowed > 0.0
                        && allowed <= case.gpu[channel].abs() * 0.002 + 1.01e-6
                );
                let actual = got[channel] / alpha;
                let error = (actual - case.gpu[channel]).abs();
                worst = worst.max(error / allowed);
                assert!(error<=allowed,"original Metal case{index}, channel{channel}, amounts{:?}, input{:?}: native{actual} GPU{} error{error} allowed{allowed}",case.amounts,case.effective_input,case.gpu[channel]);
            }
        }
        eprintln!("original Metal contrast:320 cases, max tolerance fraction={worst}");
    }
    #[test]
    fn metal_texture_rounding_underflow_is_not_recovered_by_an_inverse_pass() {
        assert_eq!(texture_half(5.6e-8), 0.0);
        assert_eq!(texture_half(-5.6e-8), -0.0);
        assert_eq!(texture_half(0.18), 0.17993164);
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[0.0001, 0.18, 0.0, 1.0], &mut || Ok(()))
                .unwrap();
        for amount in [100.0, -50.0] {
            ConcatContrast { amount }
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
        }
        assert_eq!(frame.pixels()[0][0], 0.0);
        assert_eq!(frame.pixels()[0][3], 1.0);
    }
}
