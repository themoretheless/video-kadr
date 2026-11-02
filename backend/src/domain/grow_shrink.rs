//! Duration-driven Filmcraft Grow/Shrink CPU scale; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GrowShrink {
    pub from: f32,
    pub to: f32,
    pub center: Option<[Option<f64>; 2]>,
    pub easing: u32,
}
impl Default for GrowShrink {
    fn default() -> Self {
        Self {
            from: 100.,
            to: 120.,
            center: None,
            easing: 0,
        }
    }
}
impl GrowShrink {
    pub fn shrink_default() -> Self {
        Self {
            from: 120.,
            to: 100.,
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.from, self.to]
                .into_iter()
                .all(|v| v.is_finite() && (1.0..=1000.).contains(&v)),
            "Grow/Shrink scale out of range"
        );
        ensure!(self.easing < 4, "Grow/Shrink easing out of range");
        ensure!(
            self.center
                .into_iter()
                .flatten()
                .flatten()
                .all(f64::is_finite),
            "nonfinite Grow/Shrink center"
        );
        Ok(())
    }
    pub fn scale_at(&self, progress: f64) -> Result<f64> {
        self.validate()?;
        ensure!(progress.is_finite(), "nonfinite clip progress");
        let t = progress.clamp(0., 1.);
        let p = match self.easing {
            1 => t * t,
            2 => 1. - (1. - t) * (1. - t),
            3 => t * t * (3. - 2. * t),
            _ => t,
        };
        let (a, b) = (self.from as f64, self.to as f64);
        Ok(((a + (b - a) * p) / 100.).max(1e-4))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        progress: f64,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Grow/Shrink pixel scale"
        );
        checkpoint()?;
        let scale = self.scale_at(progress)?;
        if scale == 1. {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        let center = self.center.unwrap_or([None, None]);
        let x = center[0]
            .map(|v| v * px_scale as f64)
            .unwrap_or(w as f64 / 2.);
        let y = center[1]
            .map(|v| v * px_scale as f64)
            .unwrap_or(h as f64 / 2.);
        let matrix = GroupAffine([
            scale,
            0.,
            0.,
            scale,
            scale * (-x) + 0. * (-y) + x,
            0. * (-x) + scale * (-y) + y,
        ]);
        *frame = frame.transformed(w, h, matrix, checkpoint)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn duration_easing_endpoints_identity_and_cancellation() {
        for (easing, quarter) in [(0, 1.25), (1, 1.0625), (2, 1.4375), (3, 1.15625)] {
            let p = GrowShrink {
                from: 100.,
                to: 200.,
                easing,
                ..Default::default()
            };
            assert_eq!(p.scale_at(0.).unwrap(), 1.);
            assert_eq!(p.scale_at(1.).unwrap(), 2.);
            assert_eq!(p.scale_at(0.25).unwrap(), quarter);
        }
        let p = GrowShrink::shrink_default();
        assert_eq!(p.scale_at(0.).unwrap(), 1.2);
        assert_eq!(p.scale_at(1.).unwrap(), 1.);
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 2,
                height: 2,
                rgba: vec![128; 16],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let original = frame.clone();
        p.apply_linear(&mut frame, 1., 1., &mut || Ok(())).unwrap();
        assert_eq!(frame.pixels(), original.pixels());
        assert!(p
            .apply_linear(&mut frame, 0.5, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Shrink {
    pub from: f32,
    pub to: f32,
    pub center: Option<[Option<f64>; 2]>,
    pub easing: u32,
}
impl Default for Shrink {
    fn default() -> Self {
        Self {
            from: 120.,
            to: 100.,
            center: None,
            easing: 0,
        }
    }
}
impl From<Shrink> for GrowShrink {
    fn from(p: Shrink) -> Self {
        Self {
            from: p.from,
            to: p.to,
            center: p.center,
            easing: p.easing,
        }
    }
}
impl Shrink {
    pub fn validate(&self) -> Result<()> {
        GrowShrink::from(*self).validate()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScaleChannel {
    From,
    To,
    CenterX,
    CenterY,
}
pub type ScaleAnimation =
    std::collections::BTreeMap<ScaleChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &ScaleAnimation) -> Result<()> {
    for (channel, value) in a {
        value.validate()?;
        let (lo, hi) = match channel {
            ScaleChannel::From | ScaleChannel::To => (1., 1000.),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Grow/Shrink animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: GrowShrink, a: &ScaleAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<GrowShrink> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (channel, value) in a {
        let value = super::nested_composition::sample_value(value, time)?;
        match channel {
            ScaleChannel::From => p.from = value as f32,
            ScaleChannel::To => p.to = value as f32,
            ScaleChannel::CenterX | ScaleChannel::CenterY => {
                p.center.get_or_insert([None, None])[if *channel == ScaleChannel::CenterX {
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
    fn grow_shrink_defaults_four_curves_and_precise_auto_center() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        for kind in ["grow", "shrink"] {
            let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":{},"animation":{"from":curve(100.,200.),"to":curve(200.,400.),"centerX":curve(1.123456789012,1.123456789012),"centerY":curve(0.,2.)}})).unwrap();
            assert_eq!(
                e,
                serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
            );
            let (p, a) = match e {
                super::super::composition::VideoEffect::Grow {
                    parameters,
                    animation: Some(a),
                } => {
                    assert_eq!(parameters, GrowShrink::default());
                    (parameters, a)
                }
                super::super::composition::VideoEffect::Shrink {
                    parameters,
                    animation: Some(a),
                } => {
                    assert_eq!(parameters, Shrink::default());
                    (parameters.into(), a)
                }
                _ => panic!("Grow/Shrink"),
            };
            let sampled = sample_animation(p, &a, 250000).unwrap();
            assert_eq!(sampled.from, 150.);
            assert_eq!(sampled.to, 300.);
            assert_eq!(sampled.center, Some([Some(1.123456789012), Some(1.)]));
            let a =
                ScaleAnimation::from([(ScaleChannel::CenterX, a[&ScaleChannel::CenterX].clone())]);
            assert_eq!(
                sample_animation(p, &a, 250000).unwrap().center,
                Some([Some(1.123456789012), None])
            );
        }
        let p: Shrink = serde_json::from_value(serde_json::json!({"easing":3})).unwrap();
        assert_eq!(p.from, 120.);
        assert_eq!(p.to, 100.);
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Case {
        parameters: GrowShrink,
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
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-grow-shrink.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 640);
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
                "original Grow/Shrink {index}"
            );
        }
    }
}
