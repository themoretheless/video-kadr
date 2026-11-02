//! Filmcraft standalone Sharpen; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{gaussian_blur::GaussianBlur, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Sharpen {
    pub amount: f32,
}
impl Sharpen {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0. ..=4000.).contains(&self.amount),
            "Sharpen amount out of range"
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
            "invalid Sharpen pixel scale"
        );
        checkpoint()?;
        let mut blurred = frame.clone();
        GaussianBlur {
            blurriness: 0.,
            dimensions: 0,
            repeat_edge: true,
        }
        .apply_sigma(&mut blurred, px_scale.max(0.35), checkpoint)?;
        let amount = self.amount / 100.;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let b = blurred.pixels()[index];
                index += 1;
                for k in 0..3 {
                    let d = p[k] - b[k];
                    if d.abs() >= 0. * p[3] {
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn sharpen_source_range_alpha_scale_floor_and_cancellation() {
        let source = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 6,
                height: 4,
                rgba: (0..24)
                    .flat_map(|i| {
                        if i % 3 == 0 {
                            [255, 255, 255, 128]
                        } else {
                            [64, 128, 192, 255]
                        }
                    })
                    .collect(),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut zero = source.clone();
        Sharpen::default()
            .apply_linear(&mut zero, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(zero.pixels(), source.pixels());
        let mut a = source.clone();
        let mut b = source.clone();
        Sharpen { amount: 4000. }
            .apply_linear(&mut a, 0.1, &mut || Ok(()))
            .unwrap();
        Sharpen { amount: 4000. }
            .apply_linear(&mut b, 0.35, &mut || Ok(()))
            .unwrap();
        assert_eq!(a.pixels(), b.pixels());
        for (p, q) in source.pixels().iter().zip(a.pixels()) {
            assert_eq!(p[3], q[3]);
        }
        // Original Gaussian boxes at sigma 0.35 are identity on this frame.
        assert_eq!(a.pixels(), source.pixels());
        let mut full = source.clone();
        Sharpen { amount: 4000. }
            .apply_linear(&mut full, 1., &mut || Ok(()))
            .unwrap();
        assert_ne!(full.pixels(), source.pixels());
        assert!(Sharpen { amount: 4001. }.validate().is_err());
        assert!(Sharpen::default()
            .apply_linear(&mut zero, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_sharpen_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: Sharpen,
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
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-sharpen.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 96);
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
                "original Sharpen {index}"
            );
        }
    }
}

pub fn validate_animation(animation: &super::composition::AnimatableValue) -> Result<()> {
    animation.validate()?;
    ensure!(
        super::composition::animatable_values_in(animation, 0.0, 4000.0, true),
        "Sharpen animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut parameters: Sharpen,
    animation: &super::composition::AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Sharpen> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.amount = super::nested_composition::sample_value(animation, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}
