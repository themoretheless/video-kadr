//! Filmcraft Camera Shake; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraShake {
    pub amount: f32,
    pub rotation: f32,
    pub zoom: f32,
    pub frequency: f32,
    pub complexity: f32,
    pub motion_blur: f32,
    pub seed: f32,
}
impl Default for CameraShake {
    fn default() -> Self {
        Self {
            amount: 20.,
            rotation: 1.,
            zoom: 5.,
            frequency: 4.,
            complexity: 3.,
            motion_blur: 0.,
            seed: 0.,
        }
    }
}
impl CameraShake {
    pub fn validate(&self) -> Result<()> {
        for (v, lo, hi) in [
            (self.amount, 0., 1000.),
            (self.rotation, 0., 45.),
            (self.zoom, 0., 100.),
            (self.frequency, 0.1, 60.),
            (self.complexity, 1., 6.),
            (self.motion_blur, 0., 100.),
            (self.seed, 0., 10000.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "CameraShake parameter out of range"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        fps: f64,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            seconds.is_finite(),
            "invalid CameraShake local time"
        );
        ensure!(
            fps.is_finite() && fps > 0.,
            "invalid CameraShake frame rate"
        );
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid CameraShake pixel scale"
        );
        checkpoint()?;
        let freq = self.frequency as f64;
        let seed = self.seed.max(0.) as u64;
        let amt = (self.amount * px_scale) as f64;
        let rot = self.rotation as f64;
        let zoom = self.zoom as f64 / 100.;
        let octaves = self.complexity.clamp(1., 6.) as usize;
        let blur = self.motion_blur as f64 / 100.;
        let shake = |t: f64, ch: u64| {
            let (mut s, mut a, mut f, mut norm) = (0., 1., 1., 0.);
            for o in 0..octaves {
                s += super::wiggle::noise1(t * freq * f, ch * 10 + o as u64, seed) as f64 * a;
                norm += a;
                a *= 0.5;
                f *= 2.;
            }
            s / norm * 1.6
        };
        let (w, h) = frame.dimensions();
        let (x, y) = (w as f64 / 2., h as f64 / 2.);
        let n = if blur > 0. { 5 } else { 1 };
        let mut parts = Vec::with_capacity(n);
        for i in 0..n {
            checkpoint()?;
            let k = if n == 1 {
                0.
            } else {
                i as f64 / (n - 1) as f64 - 0.5
            };
            let t = seconds + k * blur / fps;
            let ox = shake(t, 0) * amt;
            let oy = shake(t, 1) * amt;
            let scale = 1. + zoom * (0.5 + 0.5 * shake(t, 3));
            let (s, c) = (shake(t, 2) * rot).to_radians().sin_cos();
            let matrix = super::spin::compose(
                super::spin::compose(
                    super::spin::compose([1., 0., 0., 1., x + ox, y + oy], [c, s, -s, c, 0., 0.]),
                    [scale, 0., 0., scale, 0., 0.],
                ),
                [1., 0., 0., 1., -x, -y],
            );
            parts.push(if matrix == GroupAffine::IDENTITY.0 {
                frame.clone()
            } else {
                frame.transformed(w, h, GroupAffine(matrix), checkpoint)?
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraShakeChannel {
    Amount,
    Rotation,
    Zoom,
    Frequency,
    Complexity,
    MotionBlur,
    Seed,
}
pub type CameraShakeAnimation =
    std::collections::BTreeMap<CameraShakeChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &CameraShakeAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            CameraShakeChannel::Amount => (0., 1000.),
            CameraShakeChannel::Rotation => (0., 45.),
            CameraShakeChannel::Frequency => (0.1, 60.),
            CameraShakeChannel::Complexity => (1., 6.),
            CameraShakeChannel::Seed => (0., 10000.),
            _ => (0., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "CameraShake curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: CameraShake,
    a: &CameraShakeAnimation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<CameraShake> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            CameraShakeChannel::Amount => p.amount = v,
            CameraShakeChannel::Rotation => p.rotation = v,
            CameraShakeChannel::Zoom => p.zoom = v,
            CameraShakeChannel::Frequency => p.frequency = v,
            CameraShakeChannel::Complexity => p.complexity = v,
            CameraShakeChannel::MotionBlur => p.motion_blur = v,
            CameraShakeChannel::Seed => p.seed = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sharp_blurred_deterministic_rate_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let p = CameraShake {
            amount: 2.5,
            rotation: 3.75,
            zoom: 25.,
            motion_blur: 100.,
            seed: 42.75,
            ..Default::default()
        };
        let mut a = frame.clone();
        p.apply_linear(&mut a, 0., 10., 1., &mut || Ok(())).unwrap();
        let mut b = frame.clone();
        p.apply_linear(&mut b, 0., 10., 1., &mut || Ok(())).unwrap();
        assert_eq!(a.pixels(), b.pixels());
        b = frame.clone();
        p.apply_linear(&mut b, 0., 30., 1., &mut || Ok(())).unwrap();
        assert_ne!(a.pixels(), b.pixels());
        assert!(p
            .apply_linear(&mut b, 0., 30., 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn persisted_seven_curves_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"camera_shake","parameters":{},"animation":{"amount":curve(0.,40.),"rotation":curve(0.,4.),"zoom":curve(0.,10.),"frequency":curve(2.,6.),"complexity":curve(1.,5.),"motionBlur":curve(0.,100.),"seed":curve(0.,42.75)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::CameraShake {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("CameraShake")
        };
        assert_eq!(parameters, CameraShake::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            CameraShake {
                amount: 20.,
                rotation: 2.,
                zoom: 5.,
                frequency: 4.,
                complexity: 3.,
                motion_blur: 50.,
                seed: 21.375
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
            parameters: CameraShake,
            seconds: f64,
            fps: f64,
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
            "../../tests/fixtures/filmcraft-camera-shake.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 3456);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            c.parameters
                .apply_linear(&mut frame, c.seconds, c.fps, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "CameraShake original working bits case{index}"
            );
        }
    }
}
