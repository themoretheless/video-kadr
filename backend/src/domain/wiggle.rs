//! Filmcraft Wiggle; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Wiggle {
    pub frequency: f32,
    pub amount: f32,
    pub rotation: f32,
    pub scale: f32,
    pub dimensions: u32,
    pub seed: f32,
}
impl Default for Wiggle {
    fn default() -> Self {
        Self {
            frequency: 2.,
            amount: 20.,
            rotation: 0.,
            scale: 0.,
            dimensions: 0,
            seed: 0.,
        }
    }
}
pub(crate) fn noise1(t: f64, channel: u64, seed: u64) -> f32 {
    super::roughen_edges::noise3(t as f32, channel as f32 * 7.31 + 0.5, 0.37, seed)
}
impl Wiggle {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [
            (self.frequency, 60.),
            (self.amount, 2000.),
            (self.rotation, 180.),
            (self.scale, 100.),
            (self.seed, 10000.),
        ] {
            ensure!(
                v.is_finite() && (0. ..=max).contains(&v),
                "Wiggle parameter out of range"
            );
        }
        ensure!(self.dimensions < 3, "Wiggle dimensions out of range");
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            seconds.is_finite(),
            "invalid Wiggle local time"
        );
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Wiggle pixel scale"
        );
        checkpoint()?;
        let t = seconds * self.frequency as f64;
        let seed = self.seed.max(0.) as u64;
        let amt = (self.amount * px_scale) as f64;
        let ox = if self.dimensions == 2 {
            0.
        } else {
            noise1(t, 0, seed) as f64 * amt * 1.6
        };
        let oy = if self.dimensions == 1 {
            0.
        } else {
            noise1(t, 1, seed) as f64 * amt * 1.6
        };
        let rot = noise1(t, 2, seed) as f64 * self.rotation as f64 * 1.6;
        let scale = (1. + noise1(t, 3, seed) as f64 * self.scale as f64 / 100. * 1.6).max(1e-3);
        let (w, h) = frame.dimensions();
        let (x, y) = (w as f64 / 2., h as f64 / 2.);
        let (s, c) = rot.to_radians().sin_cos();
        let matrix = super::spin::compose(
            super::spin::compose(
                super::spin::compose([1., 0., 0., 1., x + ox, y + oy], [c, s, -s, c, 0., 0.]),
                [scale, 0., 0., scale, 0., 0.],
            ),
            [1., 0., 0., 1., -x, -y],
        );
        if matrix != GroupAffine::IDENTITY.0 {
            *frame = frame.transformed(w, h, GroupAffine(matrix), checkpoint)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WiggleChannel {
    Frequency,
    Amount,
    Rotation,
    Scale,
    Seed,
}
pub type WiggleAnimation =
    std::collections::BTreeMap<WiggleChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &WiggleAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let hi = match c {
            WiggleChannel::Frequency => 60.,
            WiggleChannel::Amount => 2000.,
            WiggleChannel::Rotation => 180.,
            WiggleChannel::Scale => 100.,
            WiggleChannel::Seed => 10000.,
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., hi, true),
            "Wiggle curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Wiggle, a: &WiggleAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Wiggle> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            WiggleChannel::Frequency => p.frequency = v,
            WiggleChannel::Amount => p.amount = v,
            WiggleChannel::Rotation => p.rotation = v,
            WiggleChannel::Scale => p.scale = v,
            WiggleChannel::Seed => p.seed = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_time_axes_rotation_scale_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut same = frame.clone();
        Wiggle {
            amount: 0.,
            ..Default::default()
        }
        .apply_linear(&mut same, 0.25, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(same.pixels(), frame.pixels());
        let p = Wiggle {
            amount: 2.,
            rotation: 30.,
            scale: 50.,
            seed: 42.,
            ..Default::default()
        };
        let mut a = frame.clone();
        p.apply_linear(&mut a, 0.25, 1., &mut || Ok(())).unwrap();
        let mut b = frame.clone();
        p.apply_linear(&mut b, 0.25, 1., &mut || Ok(())).unwrap();
        assert_eq!(a.pixels(), b.pixels());
        b = frame.clone();
        p.apply_linear(&mut b, 0.5, 1., &mut || Ok(())).unwrap();
        assert_ne!(a.pixels(), b.pixels());
        assert!(p
            .apply_linear(&mut b, 0.5, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn persisted_five_local_curves_defaults_roundtrip() {
        let curve = |b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"wiggle","parameters":{},"animation":{"frequency":curve(4.),"amount":curve(40.),"rotation":curve(90.),"scale":curve(100.),"seed":curve(42.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Wiggle {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Wiggle")
        };
        assert_eq!(parameters, Wiggle::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Wiggle {
                frequency: 2.,
                amount: 20.,
                rotation: 45.,
                scale: 50.,
                seed: 21.,
                dimensions: 0
            }
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: Wiggle,
            seconds: f64,
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
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wiggle.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 1536);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            c.parameters
                .apply_linear(&mut frame, c.seconds, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Wiggle original working bits case{index}"
            );
        }
    }
}
