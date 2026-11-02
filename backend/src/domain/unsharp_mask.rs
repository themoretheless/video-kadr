//! Filmcraft Unsharp Mask; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{gaussian_blur::GaussianBlur, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct UnsharpMask {
    pub amount: f32,
    pub radius: f32,
    pub threshold: f32,
}
impl UnsharpMask {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0. ..=500.).contains(&self.amount),
            "Unsharp Mask amount out of range"
        );
        ensure!(
            self.radius.is_finite() && (0.1..=250.).contains(&self.radius),
            "Unsharp Mask radius out of range"
        );
        ensure!(
            self.threshold.is_finite() && (0. ..=255.).contains(&self.threshold),
            "Unsharp Mask threshold out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid UnsharpMask pixel scale"
        );
        checkpoint()?;
        let mut blurred = frame.clone();
        GaussianBlur {
            blurriness: 0.,
            dimensions: 0,
            repeat_edge: true,
        }
        .apply_sigma(&mut blurred, self.radius * px_scale, checkpoint)?;
        let amount = self.amount / 100.;
        let threshold = self.threshold / 255.;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let b = blurred.pixels()[index];
                index += 1;
                for k in 0..3 {
                    let d = p[k] - b[k];
                    if d.abs() >= threshold * p[3] {
                        let v = p[k] + d * amount;
                        p[k] = v.clamp(0., p[3].max(v).max(0.)).max(0.);
                    }
                }
                p
            },
            checkpoint,
        )
    }
}

impl Default for UnsharpMask {
    fn default() -> Self {
        Self {
            amount: 50.,
            radius: 1.,
            threshold: 0.,
        }
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_unsharp_mask_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: UnsharpMask,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-unsharp-mask.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 128);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original UnsharpMask {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnsharpMaskChannel {
    Amount,
    Radius,
    Threshold,
}
pub type UnsharpMaskAnimation =
    std::collections::BTreeMap<UnsharpMaskChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &UnsharpMaskAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (min, max) = match c {
            UnsharpMaskChannel::Amount => (0., 500.),
            UnsharpMaskChannel::Radius => (0.1, 250.),
            UnsharpMaskChannel::Threshold => (0., 255.),
        };
        ensure!(
            super::composition::animatable_values_in(v, min, max, true),
            "Unsharp Mask animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: UnsharpMask,
    a: &UnsharpMaskAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<UnsharpMask> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            UnsharpMaskChannel::Amount => p.amount = v,
            UnsharpMaskChannel::Radius => p.radius = v,
            UnsharpMaskChannel::Threshold => p.threshold = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn unsharp_mask_three_curves_defaults_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"unsharp_mask","parameters":{},"animation":{"amount":curve(0.,500.),"radius":curve(1.,4.),"threshold":curve(0.,255.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::UnsharpMask {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Unsharp Mask")
        };
        assert_eq!(parameters, UnsharpMask::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            UnsharpMask {
                amount: 250.,
                radius: 2.5,
                threshold: 127.5
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap(),
            UnsharpMask {
                amount: 500.,
                radius: 4.,
                threshold: 255.
            }
        );
        for (c, value) in [
            (UnsharpMaskChannel::Amount, -1.),
            (UnsharpMaskChannel::Amount, 500.1),
            (UnsharpMaskChannel::Radius, 0.09),
            (UnsharpMaskChannel::Radius, 250.1),
            (UnsharpMaskChannel::Threshold, -1.),
            (UnsharpMaskChannel::Threshold, 255.1),
        ] {
            assert!(validate_animation(&UnsharpMaskAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value }
            )]))
            .is_err());
        }
    }
}
