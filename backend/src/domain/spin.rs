//! Duration-driven Filmcraft Spin; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Spin {
    pub amount: f32,
    pub scale: f32,
    pub center: Option<[Option<f64>; 2]>,
    pub easing: u32,
}
impl Default for Spin {
    fn default() -> Self {
        Self {
            amount: 360.,
            scale: 100.,
            center: None,
            easing: 3,
        }
    }
}
pub(crate) fn compose(m: [f64; 6], o: [f64; 6]) -> [f64; 6] {
    [
        m[0] * o[0] + m[2] * o[1],
        m[1] * o[0] + m[3] * o[1],
        m[0] * o[2] + m[2] * o[3],
        m[1] * o[2] + m[3] * o[3],
        m[0] * o[4] + m[2] * o[5] + m[4],
        m[1] * o[4] + m[3] * o[5] + m[5],
    ]
}
impl Spin {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (-36000.0..=36000.).contains(&self.amount),
            "Spin amount out of range"
        );
        ensure!(
            self.scale.is_finite() && (1.0..=1000.).contains(&self.scale),
            "Spin scale out of range"
        );
        ensure!(self.easing < 4, "Spin easing out of range");
        ensure!(
            self.center
                .into_iter()
                .flatten()
                .flatten()
                .all(f64::is_finite),
            "nonfinite Spin center"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        progress: f64,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(progress.is_finite(), "nonfinite clip progress");
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Spin pixel scale"
        );
        checkpoint()?;
        let t = progress.clamp(0., 1.);
        let p = match self.easing {
            1 => t * t,
            2 => 1. - (1. - t) * (1. - t),
            3 => t * t * (3. - 2. * t),
            _ => t,
        };
        let angle = self.amount as f64 * p;
        let scale = (self.scale as f64 / 100.).max(1e-4);
        let (w, h) = frame.dimensions();
        let center = self.center.unwrap_or([None, None]);
        let x = center[0]
            .map(|v| v * px_scale as f64)
            .unwrap_or(w as f64 / 2.);
        let y = center[1]
            .map(|v| v * px_scale as f64)
            .unwrap_or(h as f64 / 2.);
        let (s, c) = angle.to_radians().sin_cos();
        let matrix = compose(
            compose(
                compose([1., 0., 0., 1., x, y], [c, s, -s, c, 0., 0.]),
                [scale, 0., 0., scale, 0., 0.],
            ),
            [1., 0., 0., 1., -x, -y],
        );
        if matrix == GroupAffine::IDENTITY.0 {
            return Ok(());
        }
        *frame = frame.transformed(w, h, GroupAffine(matrix), checkpoint)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn identity_quarter_rotation_and_cancellation() {
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 2,
                height: 2,
                rgba: vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let original = frame.clone();
        Spin::default()
            .apply_linear(&mut frame, 0., 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels(), original.pixels());
        Spin {
            amount: 180.,
            easing: 0,
            ..Default::default()
        }
        .apply_linear(&mut frame, 0.5, 1., &mut || Ok(()))
        .unwrap();
        assert_ne!(frame.pixels(), original.pixels());
        let expected = [
            original.pixels()[2],
            original.pixels()[0],
            original.pixels()[3],
            original.pixels()[1],
        ];
        for (a, b) in frame.pixels().iter().zip(expected) {
            for (c, d) in a.iter().zip(b) {
                assert!((*c - d).abs() < 1e-6);
            }
        }
        assert!(Spin::default()
            .apply_linear(&mut frame, 0.5, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpinChannel {
    Amount,
    Scale,
    CenterX,
    CenterY,
}
pub type SpinAnimation =
    std::collections::BTreeMap<SpinChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &SpinAnimation) -> Result<()> {
    for (channel, value) in a {
        value.validate()?;
        let (lo, hi) = match channel {
            SpinChannel::Amount => (-36000., 36000.),
            SpinChannel::Scale => (1., 1000.),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Spin animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Spin, a: &SpinAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Spin> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (channel, value) in a {
        let value = super::nested_composition::sample_value(value, time)?;
        match channel {
            SpinChannel::Amount => p.amount = value as f32,
            SpinChannel::Scale => p.scale = value as f32,
            SpinChannel::CenterX | SpinChannel::CenterY => {
                p.center.get_or_insert([None, None])[if *channel == SpinChannel::CenterX {
                    0
                } else {
                    1
                }] = Some(value);
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
    fn four_curves_midpoint_roundtrip_and_auto_axis() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"spin","parameters":{},"animation":{"amount":curve(0.,360.),"scale":curve(100.,200.),"centerX":curve(1.123456789012,1.123456789012),"centerY":curve(0.,2.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Spin {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Spin")
        };
        assert_eq!(parameters, Spin::default());
        let p = sample_animation(parameters, &a, 250000).unwrap();
        assert_eq!(p.amount, 180.);
        assert_eq!(p.scale, 150.);
        assert_eq!(p.center, Some([Some(1.123456789012), Some(1.)]));
        let a = SpinAnimation::from([(SpinChannel::CenterX, a[&SpinChannel::CenterX].clone())]);
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap().center,
            Some([Some(1.123456789012), None])
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Case {
        parameters: Spin,
        progress: f64,
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
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spin.json")).unwrap();
        assert_eq!(fixture.cases.len(), 2560);
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
                .apply_linear(&mut frame, c.progress, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original Spin {index}"
            );
        }
    }
}
