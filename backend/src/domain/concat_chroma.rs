//! Independent Concat display-space chroma behavior, with local Metal pass precision.
use super::{composition::AnimatableValue, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
pub(crate) fn display(v: f32) -> f32 {
    v.signum() * v.abs().powf(1.0 / 2.4)
}
pub(crate) fn linear(v: f32) -> f32 {
    v.signum() * v.abs().powf(2.4)
}
fn saturate(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
    rgb.map(|v| y * (1.0 - factor) + v * factor)
}
fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub(crate) fn skin(rgb: [f32; 3]) -> f32 {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let lo = rgb[0].min(rgb[1]).min(rgb[2]);
    let d = hi - lo;
    let hue = if d < 0.0001 {
        0.0
    } else {
        let h = if hi == rgb[0] {
            (rgb[1] - rgb[2]) / d
        } else if hi == rgb[1] {
            2.0 + (rgb[2] - rgb[0]) / d
        } else {
            4.0 + (rgb[0] - rgb[1]) / d
        };
        let f = h / 6.0;
        (f - f.floor()) * 360.0
    };
    let f = (hue - 25.0) / 360.0 + 0.5;
    let distance = ((f - f.floor()) * 360.0 - 180.0).abs();
    (1.0 - smooth(20.0, 40.0, distance)) * smooth(0.0, 0.25, d)
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Saturation {
    pub amount: f32,
}
impl Saturation {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (-100.0..=100.0).contains(&self.amount),
            "Concat saturation out of range"
        );
        Ok(())
    }
    pub fn signal(&self, input: [f32; 3]) -> [f32; 3] {
        let s = self.amount / 100.0;
        let factor = if s > 0.0 {
            1.0 + 2.0 * s
        } else {
            (1.0 + s) * (1.0 + s)
        };
        saturate(input.map(display), factor).map(linear)
    }
    pub fn apply_sampled_linear(amount:f32,frame:&mut LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        ensure!(amount.is_finite(),"nonfinite Concat saturation source value");
        let parameters=Self{amount};super::concat_working::apply_rgb(frame,|rgb|parameters.signal(rgb),checkpoint)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(frame, |rgb| self.signal(rgb), checkpoint)
    }
}
pub fn validate_concat_keys(keys:&[super::concat_effect_upgrade::ParamKey])->Result<()>{
    ensure!(keys.len()<=4096,"too many Concat saturation source keys");
    ensure!(keys.iter().all(|key|key.at.is_finite()&&key.value.is_finite()&&(key.derived||(-100. ..=100.).contains(&key.value))&&key.ease.iter().all(|v|v.is_finite())),"invalid Concat saturation source key");
    Ok(())
}
pub fn sample_concat_keys(parameters:Saturation,keys:&[super::concat_effect_upgrade::ParamKey],progress:f64)->Result<f32>{
    parameters.validate()?;validate_concat_keys(keys)?;
    let value=super::concat_animation::PreparedTrack::new(keys)?.value_at(progress,parameters.amount as f64)? as f32;
    ensure!(value.is_finite(),"nonfinite Concat saturation source result");Ok(value)
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vibrance {
    pub amount: f32,
    pub intensity: f32,
}
impl Default for Vibrance {
    fn default() -> Self {
        Self {
            amount: 0.7,
            intensity: 100.0,
        }
    }
}
impl Vibrance {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite()
                && (0.1..=2.0).contains(&self.amount)
                && self.intensity.is_finite()
                && (0.0..=100.0).contains(&self.intensity),
            "Concat vibrance out of range"
        );
        Ok(())
    }
    pub fn signal(&self, input: [f32; 3]) -> [f32; 3] {
        if self.intensity == 0.0 {
            return input;
        }
        let rgb = input.map(display);
        let chroma = rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2]);
        let boosted = saturate(rgb, 1.0 + self.amount * (1.0 - chroma).max(0.0));
        let keep = skin(rgb) * 0.6;
        let graded: [f32; 3] = std::array::from_fn(|i| boosted[i] * (1.0 - keep) + rgb[i] * keep);
        let light = graded.map(linear);
        let mix = self.intensity / 100.0;
        std::array::from_fn(|i| input[i] * (1.0 - mix) + light[i] * mix)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(frame, |rgb| self.signal(rgb), checkpoint)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VibranceChannel {
    Amount,
    Intensity,
}
pub type VibranceAnimation = std::collections::BTreeMap<VibranceChannel, AnimatableValue>;
fn valid(value: &AnimatableValue, low: f64, high: f64) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, low, high, true),
        "Concat chroma animation out of range"
    );
    Ok(())
}
pub fn validate_saturation_animation(value: &AnimatableValue) -> Result<()> {
    valid(value, -100.0, 100.0)
}
pub fn sample_saturation(
    mut p: Saturation,
    value: &AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Saturation> {
    p.validate()?;
    validate_saturation_animation(value)?;
    p.amount = super::nested_composition::sample_value(value, time.into())? as f32;
    p.validate()?;
    Ok(p)
}
pub fn validate_vibrance_animation(values: &VibranceAnimation) -> Result<()> {
    for (channel, value) in values {
        match channel {
            VibranceChannel::Amount => valid(value, 0.1, 2.0)?,
            VibranceChannel::Intensity => valid(value, 0.0, 100.0)?,
        }
    }
    Ok(())
}
pub fn sample_vibrance(
    mut p: Vibrance,
    values: &VibranceAnimation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Vibrance> {
    let time = time.into();
    p.validate()?;
    validate_vibrance_animation(values)?;
    for (channel, value) in values {
        let v = super::nested_composition::sample_value(value, time)? as f32;
        match channel {
            VibranceChannel::Amount => p.amount = v,
            VibranceChannel::Intensity => p.intensity = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_saturation_vibrance_ranges_and_independent_curves() {
        let grey = Saturation { amount: -100.0 }.signal([0.6, 0.3, 0.1]);
        for value in grey {
            assert!((value - 0.333173).abs() < 1e-5);
        }
        let wide = Saturation { amount: 100.0 }.signal([0.6, 0.3, 0.1]);
        assert!(wide[0] > 1.0 && wide[2] < 0.0);
        assert_eq!(
            Vibrance {
                intensity: 0.0,
                ..Default::default()
            }
            .signal([4.0, -0.1, 0.5]),
            [4.0, -0.1, 0.5]
        );
        assert!(skin([0.8, 0.6, 0.5]) > 0.0);
        assert_eq!(skin([0.25; 3]), 0.0);
        for amount in [-101.0, 101.0, f32::NAN] {
            assert!(Saturation { amount }.validate().is_err());
        }
        for p in [
            Vibrance {
                amount: 0.09,
                intensity: 100.0,
            },
            Vibrance {
                amount: 2.01,
                intensity: 100.0,
            },
            Vibrance {
                amount: 0.7,
                intensity: 100.1,
            },
        ] {
            assert!(p.validate().is_err());
        }
        let animation:VibranceAnimation=serde_json::from_value(serde_json::json!({"amount":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.1},{"tick":1000000,"value":2.0}]}},"intensity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100},{"tick":1000000,"value":0}]}}})).unwrap();
        let sampled = sample_vibrance(Vibrance::default(), &animation, 500000).unwrap();
        assert!((sampled.amount - 1.05).abs() < 1e-6);
        assert_eq!(sampled.intensity, 50.0);
    }
    #[test]
    fn original_metal_saturation_and_vibrance_oracles() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            amounts: Vec<f32>,
            effective_input: [f32; 4],
            gpu: [f32; 4],
            tolerance: [f32; 4],
            #[serde(default = "full")]
            intensity: f32,
        }
        fn full() -> f32 {
            100.0
        }
        for (name, text, count, probes, vibrance) in [
            (
                "saturation",
                include_str!("../../tests/fixtures/concat-saturation-gpu.json"),
                512,
                5,
                false,
            ),
            (
                "vibrance",
                include_str!("../../tests/fixtures/concat-vibrance-gpu.json"),
                2048,
                3,
                true,
            ),
        ] {
            let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(
                fixture["reference"]["commit"],
                "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
            );
            assert_eq!(fixture["adapter"]["backend"], "Metal");
            assert_eq!(fixture["upstreamProbes"], probes);
            let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
            assert_eq!(cases.len(), count);
            let mut worst = 0.0f32;
            for (index, case) in cases.into_iter().enumerate() {
                let alpha = case.effective_input[3];
                let input = [
                    case.effective_input[0] * alpha,
                    case.effective_input[1] * alpha,
                    case.effective_input[2] * alpha,
                    alpha,
                ];
                let mut frame =
                    LinearColorFrame::from_working(1, 1, &input, &mut || Ok(())).unwrap();
                let effects = case
                    .amounts
                    .iter()
                    .map(|amount| {
                        if vibrance {
                            super::super::composition::VideoEffect::ConcatVibrance {
                                parameters: Vibrance {
                                    amount: *amount,
                                    intensity: case.intensity,
                                },
                                animation: None,
                            }
                        } else {
                            super::super::composition::VideoEffect::ConcatSaturation {
                                parameters: Saturation { amount: *amount },
                                animation: None,concat_keys:None,
                            }
                        }
                    })
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
                assert_eq!(got[3], case.gpu[3]);
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
                    assert!(error<=allowed,"{name} case{index} channel{channel} amounts{:?} intensity{} input{:?}: native{actual} GPU{} error{error} allowed{allowed}",case.amounts,case.intensity,case.effective_input,case.gpu[channel]);
                }
            }
            eprintln!("original Metal {name}:{count} cases, max tolerance fraction={worst}");
        }
    }
}
