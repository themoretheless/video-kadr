//! Active Filmcraft Focus Blur; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{gaussian_blur::GaussianBlur, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusBlur {
    pub shape: u32,
    pub center: Option<[Option<f64>; 2]>,
    pub angle: f32,
    pub size: f32,
    pub feather: f32,
    pub amount: f32,
    pub show: bool,
}
impl Default for FocusBlur {
    fn default() -> Self {
        Self {
            shape: 0,
            center: None,
            angle: 0.,
            size: 200.,
            feather: 200.,
            amount: 20.,
            show: false,
        }
    }
}
impl FocusBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.shape <= 1 && self.angle.is_finite(),
            "Focus Blur shape/angle out of range"
        );
        for (v, max) in [
            (self.size, 4000.),
            (self.feather, 4000.),
            (self.amount, 500.),
        ] {
            ensure!(
                v.is_finite() && (0. ..=max).contains(&v),
                "Focus Blur parameter out of range"
            );
        }
        if let Some(c) = self.center {
            ensure!(
                c.into_iter().flatten().all(f64::is_finite),
                "nonfinite Focus Blur center"
            );
        }
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
            "invalid Focus Blur pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let amount = self.amount * px_scale * 0.5;
        let c = self.center.unwrap_or([None, None]);
        let (cx, cy) = (
            c[0].map(|v| v * px_scale as f64).unwrap_or(w as f64 / 2.),
            c[1].map(|v| v * px_scale as f64).unwrap_or(h as f64 / 2.),
        );
        let (s, co) = (self.angle as f64).to_radians().sin_cos();
        let size = self.size as f64 * px_scale as f64 * 0.5;
        let feather = (self.feather as f64 * px_scale as f64).max(1.);
        let dist = |x: usize, y: usize| {
            let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            let d = if self.shape == 1 {
                (-dx * s + dy * co).abs()
            } else {
                dx.hypot(dy)
            };
            ((d - size) / feather).clamp(0., 1.) as f32
        };
        if amount > 0.3 {
            let levels = [0.0f32, 0.125, 0.25, 0.5, 1.];
            let mut stack = Vec::with_capacity(5);
            for k in levels {
                let mut copy = frame.clone();
                if k > 0. {
                    GaussianBlur {
                        blurriness: 0.,
                        dimensions: 0,
                        repeat_edge: true,
                    }
                    .apply_sigma(&mut copy, amount * k, checkpoint)?;
                }
                stack.push(copy);
            }
            let mut index = 0;
            frame.map_premultiplied(
                &mut |_| {
                    let a = dist(index % w, index / w).clamp(0., 1.);
                    let i = levels.partition_point(|&l| l <= a).clamp(1, 4);
                    let t = ((a - levels[i - 1]) / (levels[i] - levels[i - 1])).clamp(0., 1.);
                    let p0 = stack[i - 1].pixels()[index];
                    let p1 = stack[i].pixels()[index];
                    index += 1;
                    std::array::from_fn(|c| p0[c] + (p1[c] - p0[c]) * t)
                },
                checkpoint,
            )?;
        }
        if self.show {
            let mut index = 0;
            frame.map_premultiplied(
                &mut |p| {
                    let k = (1. - dist(index % w, index / w)) * 0.35;
                    index += 1;
                    let src = [k, 0., 0., k];
                    let inv = 1. - src[3];
                    std::array::from_fn(|c| src[c] + p[c] * inv)
                },
                checkpoint,
            )?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_blur_identity_area_overlay_auto_axis_and_cancel() {
        let frame = LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: (0..24)
                    .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                    .collect(),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut a = frame.clone();
        FocusBlur {
            amount: 0.,
            ..Default::default()
        }
        .apply_linear(&mut a, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), frame.pixels());
        let p = FocusBlur {
            amount: 4.,
            size: 0.,
            feather: 1.,
            shape: 1,
            angle: 37.,
            show: true,
            center: Some([Some(1.123456789012), None]),
        };
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        assert_ne!(a.pixels(), frame.pixels());
        assert!(p
            .apply_linear(&mut a, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
        assert!(FocusBlur { amount: 500.1, ..p }.validate().is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FocusBlurChannel {
    Amount,
    Size,
    Feather,
    Angle,
    CenterX,
    CenterY,
}
pub type FocusBlurAnimation =
    std::collections::BTreeMap<FocusBlurChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &FocusBlurAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            FocusBlurChannel::Amount => (0., 500.),
            FocusBlurChannel::Size | FocusBlurChannel::Feather => (0., 4000.),
            FocusBlurChannel::Angle => (-(f32::MAX as f64), f32::MAX as f64),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Focus Blur animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: FocusBlur, a: &FocusBlurAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<FocusBlur> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            FocusBlurChannel::Amount => p.amount = v as f32,
            FocusBlurChannel::Size => p.size = v as f32,
            FocusBlurChannel::Feather => p.feather = v as f32,
            FocusBlurChannel::Angle => p.angle = v as f32,
            FocusBlurChannel::CenterX | FocusBlurChannel::CenterY => {
                p.center.get_or_insert([None, None])[if *c == FocusBlurChannel::CenterX {
                    0
                } else {
                    1
                }] = Some(v)
            }
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn focus_blur_six_curves_precise_auto_axis_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let precise = 3.123456789012;
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"focus_blur","parameters":{},"animation":{"amount":curve(0.,4.),"size":curve(0.,6.),"feather":curve(0.,2.),"angle":curve(-37.,74.),"centerX":curve(precise,precise),"centerY":curve(0.,2.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::FocusBlur {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Focus Blur")
        };
        assert_eq!(parameters, FocusBlur::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            FocusBlur {
                amount: 2.,
                size: 3.,
                feather: 1.,
                angle: 18.5,
                center: Some([Some(precise), Some(1.)]),
                ..Default::default()
            }
        );
        let one = FocusBlurAnimation::from([(
            FocusBlurChannel::CenterX,
            a[&FocusBlurChannel::CenterX].clone(),
        )]);
        assert_eq!(
            sample_animation(parameters, &one, 250000).unwrap().center,
            Some([Some(precise), None])
        );
        assert!(validate_animation(&FocusBlurAnimation::from([(
            FocusBlurChannel::Amount,
            super::super::composition::AnimatableValue::Constant { value: 501. }
        )]))
        .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: FocusBlur,
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
            "../../tests/fixtures/filmcraft-focus-blur.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 128);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
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
                "Focus Blur original working bits case{index}"
            );
        }
    }
}
