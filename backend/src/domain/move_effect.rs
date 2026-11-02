//! Filmcraft Move with five-sample linear-working motion blur; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveEffect {
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub easing: u32,
    pub motion_blur: f32,
}
impl Default for MoveEffect {
    fn default() -> Self {
        Self {
            from: [-100., 0.],
            to: [0., 0.],
            easing: 3,
            motion_blur: 0.,
        }
    }
}
impl MoveEffect {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.from.into_iter().chain(self.to).all(f64::is_finite),
            "nonfinite Move offset"
        );
        ensure!(self.easing < 4, "Move easing out of range");
        ensure!(
            self.motion_blur.is_finite() && (0.0..=100.).contains(&self.motion_blur),
            "Move motion blur out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        progress: f64,
        span: f64,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            progress.is_finite() && span.is_finite() && span >= 0.,
            "invalid Move clip clock"
        );
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Move pixel scale"
        );
        checkpoint()?;
        let p = progress.clamp(0., 1.);
        let blur = self.motion_blur as f64 / 100.;
        let n = if blur > 0. && span > 0. { 5 } else { 1 };
        let (w, h) = frame.dimensions();
        let (cx, cy) = (w as f64 / 2., h as f64 / 2.);
        let scale = px_scale as f64;
        let a = self.from.map(|v| v * scale);
        let b = self.to.map(|v| v * scale);
        let mut parts = Vec::with_capacity(n);
        for i in 0..n {
            checkpoint()?;
            let k = if n == 1 {
                0.
            } else {
                i as f64 / (n - 1) as f64 - 0.5
            };
            let t = (p + k * blur * span).clamp(0., 1.);
            let q = match self.easing {
                1 => t * t,
                2 => 1. - (1. - t) * (1. - t),
                3 => t * t * (3. - 2. * t),
                _ => t,
            };
            let x = a[0] + (b[0] - a[0]) * q;
            let y = a[1] + (b[1] - a[1]) * q;
            let m = super::spin::compose(
                super::spin::compose(
                    super::spin::compose(
                        [1., 0., 0., 1., cx + x, cy + y],
                        [1., 0., -0., 1., 0., 0.],
                    ),
                    [1., 0., 0., 1., 0., 0.],
                ),
                [1., 0., 0., 1., -cx, -cy],
            );
            parts.push(if m == GroupAffine::IDENTITY.0 {
                frame.clone()
            } else {
                frame.transformed(w, h, GroupAffine(m), checkpoint)?
            });
        }
        if n == 1 {
            *frame = parts.pop().unwrap();
            return Ok(());
        }
        let inv = 1. / n as f32;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let pixel = std::array::from_fn(|channel| {
                    parts
                        .iter()
                        .map(|p| p.pixels()[index][channel])
                        .sum::<f32>()
                        * inv
                });
                index += 1;
                pixel
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
    fn translation_blur_and_cancellation() {
        let input = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 4,
                height: 2,
                rgba: [
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ]
                .repeat(2),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut frame = input.clone();
        let p = MoveEffect {
            from: [0., 0.],
            to: [2., 0.],
            easing: 0,
            motion_blur: 0.,
        };
        p.apply_linear(&mut frame, 0., 0.1, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels(), input.pixels());
        p.apply_linear(&mut frame, 0.5, 0.1, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels()[0], [0.; 4]);
        assert_eq!(frame.pixels()[1], input.pixels()[0]);
        let sharp = frame;
        let mut blurred = input.clone();
        MoveEffect {
            motion_blur: 100.,
            ..p
        }
        .apply_linear(&mut blurred, 0.5, 0.5, 1., &mut || Ok(()))
        .unwrap();
        assert_ne!(blurred.pixels(), sharp.pixels());
        assert!(p
            .apply_linear(&mut blurred, 0.5, 0.1, 1., &mut || anyhow::bail!(
                "cancelled"
            ))
            .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveChannel {
    FromX,
    FromY,
    ToX,
    ToY,
    MotionBlur,
}
pub type MoveAnimation =
    std::collections::BTreeMap<MoveChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &MoveAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = if *c == MoveChannel::MotionBlur {
            (0., 100.)
        } else {
            (-f64::MAX, f64::MAX)
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Move animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: MoveEffect, a: &MoveAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<MoveEffect> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)?;
        match c {
            MoveChannel::FromX => p.from[0] = v,
            MoveChannel::FromY => p.from[1] = v,
            MoveChannel::ToX => p.to[0] = v,
            MoveChannel::ToY => p.to[1] = v,
            MoveChannel::MotionBlur => p.motion_blur = v as f32,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn five_curves_midpoint_precise_points_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"move","parameters":{},"animation":{"fromX":curve(1.123456789012,1.123456789012),"fromY":curve(0.,2.),"toX":curve(0.,4.),"toY":curve(0.,6.),"motionBlur":curve(0.,100.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Move {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Move")
        };
        assert_eq!(parameters, MoveEffect::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            MoveEffect {
                from: [1.123456789012, 1.],
                to: [2., 3.],
                motion_blur: 50.,
                easing: 3
            }
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Case {
        parameters: MoveEffect,
        progress: f64,
        span: f64,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        scale: f32,
        bits: Vec<u32>,
    }
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[test]
    fn original_working_frame_bits() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-move.json")).unwrap();
        assert_eq!(fixture.cases.len(), 972);
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
                .apply_linear(&mut frame, c.progress, c.span, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original Move {index}"
            );
        }
    }
}
