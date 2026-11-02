//! Independent scene-linear Concat Exposure behavior; original package at78e4be1.
use super::{composition::AnimatableValue, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConcatExposure {
    pub stops: f32,
}
impl ConcatExposure {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.stops.is_finite() && (-4.0..=4.0).contains(&self.stops),
            "Concat exposure stops out of range"
        );
        Ok(())
    }
    pub fn channel(&self, value: f32) -> f32 {
        value * self.stops.exp2()
    }
    pub fn apply_sampled_linear(stops:f32,frame:&mut LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        ensure!(stops.is_finite(),"nonfinite Concat exposure source value");
        let gain=stops.exp2();ensure!(gain.is_finite(),"Concat exposure source gain overflow");
        super::concat_working::apply_channels(frame,|value|value*gain,checkpoint)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_channels(frame, |v| self.channel(v), checkpoint)
    }
}
pub fn validate_concat_keys(keys:&[super::concat_effect_upgrade::ParamKey])->Result<()>{
    ensure!(keys.len()<=4096,"too many Concat exposure source keys");
    ensure!(keys.iter().all(|key|key.at.is_finite()&&key.value.is_finite()&&(key.derived||(-4. ..=4.).contains(&key.value))&&key.ease.iter().all(|v|v.is_finite())),"invalid Concat exposure source key");
    Ok(())
}
pub fn sample_concat_keys(parameters:ConcatExposure,keys:&[super::concat_effect_upgrade::ParamKey],progress:f64)->Result<f32>{
    parameters.validate()?;validate_concat_keys(keys)?;
    let value=super::concat_animation::PreparedTrack::new(keys)?.value_at(progress,parameters.stops as f64)? as f32;
    ensure!(value.is_finite()&&value.exp2().is_finite(),"nonfinite Concat exposure source result");Ok(value)
}
pub fn validate_animation(value: &AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, -4.0, 4.0, true),
        "Concat exposure animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut p: ConcatExposure,
    value: &AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ConcatExposure> {
    p.validate()?;
    validate_animation(value)?;
    p.stops = super::nested_composition::sample_value(value, time.into())? as f32;
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exposure_stops_range_and_clip_animation() {
        assert_eq!(ConcatExposure { stops: 1.0 }.channel(4.0), 8.0);
        assert_eq!(ConcatExposure { stops: -1.0 }.channel(-0.5), -0.25);
        for stops in [-4.01, 4.01, f32::NAN, f32::INFINITY] {
            assert!(ConcatExposure { stops }.validate().is_err());
        }
        let value:AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-4.0},{"tick":1000000,"value":4.0}]}})).unwrap();
        assert_eq!(
            sample_animation(ConcatExposure::default(), &value, 500000)
                .unwrap()
                .stops,
            0.0
        );
        let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_exposure","parameters":{"stops":1.5},"animation":value})).unwrap();
        effect.validate().unwrap();
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[0.2, 0.1, -0.05, 0.5], &mut || Ok(())).unwrap();
        super::super::flat_surface::apply_effect_stack_working(
            &mut frame,
            &[effect],
            1000000,
            &[],
            None,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(frame.pixels()[0][3], 0.5);
        assert!(frame.pixels()[0][0] > 3.0);
        assert!(frame.pixels()[0][2] < 0.0);
    }
    #[test]
    fn original_metal_shader_oracle() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            stops: Vec<f32>,
            effective_input: [f32; 4],
            gpu: [f32; 4],
            tolerance: [f32; 4],
        }
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/concat-exposure-gpu.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(fixture["adapter"]["backend"], "Metal");
        assert_eq!(fixture["upstreamProbes"], 6);
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
                .stops
                .iter()
                .map(
                    |amount| super::super::composition::VideoEffect::ConcatExposure {
                        parameters: ConcatExposure { stops: *amount },
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
                assert!(error<=allowed,"original Metal case{index}, channel{channel}, stops{:?}, input{:?}: native{actual} GPU{} error{error} allowed{allowed}",case.stops,case.effective_input,case.gpu[channel]);
            }
        }
        eprintln!("original Metal exposure:320 cases, max tolerance fraction={worst}");
    }
}
