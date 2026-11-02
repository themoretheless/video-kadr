//! Independently authored timed dust and corner wash, evaluated in display space.
use super::{
    concat_chroma::{display, linear},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FallingDust {
    pub dust: f32,
    pub leak: f32,
}
impl Default for FallingDust {
    fn default() -> Self {
        Self {
            dust: 60.,
            leak: 50.,
        }
    }
}
pub(super) fn hash(x: f32, y: f32, seed: f32) -> f32 {
    let mut bits = x.to_bits()
        ^ y.to_bits().wrapping_mul(0x9e3779b9)
        ^ seed.to_bits().wrapping_mul(0x85ebca6b);
    bits = (bits ^ (bits >> 16)).wrapping_mul(0x7feb352d);
    bits = (bits ^ (bits >> 15)).wrapping_mul(0x846ca68b);
    bits ^= bits >> 16;
    (bits >> 8) as f32 / 16777216.
}
fn smooth(end: f32, value: f32) -> f32 {
    let t = (value / end).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Clone, Copy)]
struct Mote {
    x: f32,
    y: f32,
    radius: f32,
    brightness: f32,
}
fn motes(seconds: f32) -> [Mote; 28] {
    std::array::from_fn(|i| {
        let k = i as f32;
        let a = hash(k, 1., 3.);
        let b = hash(k, 2., 5.);
        let c = hash(k, 3., 7.);
        let y = a - seconds * (0.02 + b * 0.05);
        let x = c + (seconds * (0.3 + a) + k).sin() * 0.02;
        Mote {
            x: x - x.floor(),
            y: y - y.floor(),
            radius: 0.003 + b * 0.006,
            brightness: 0.4 + c * 0.6,
        }
    })
}
impl FallingDust {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.dust, self.leak]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "invalid Falling Dust controls"
        );
        Ok(())
    }
    fn grade(&self, rgb: [f32; 3], uv: [f32; 2], aspect: f32, motes: &[Mote; 28]) -> [f32; 3] {
        let base = rgb.map(|v| display(v) * (0.97 - 0.04) + 0.04);
        let luma = base[0] * 0.2126 + base[1] * 0.7152 + base[2] * 0.0722;
        let high = smooth(0.6, luma - 0.4);
        let leak = self.leak / 100.;
        let tint = [0.12, 0.03, -0.08];
        let toned: [f32; 3] = std::array::from_fn(|i| base[i] + tint[i] * high * leak);
        let distance = ((uv[0] - 1.05).powi(2) + (uv[1] + 0.1).powi(2)).sqrt();
        let wash = (1. - smooth(0.9, distance)) * leak * 0.55;
        let wash_tint = [1., 0.72, 0.4];
        let glow = motes
            .iter()
            .map(|m| {
                let dx = (uv[0] - m.x) * aspect;
                let dy = uv[1] - m.y;
                (1. - smooth(m.radius, (dx * dx + dy * dy).sqrt())) * m.brightness
            })
            .sum::<f32>()
            .min(1.);
        let dust_tint = [1., 0.95, 0.85];
        std::array::from_fn(|i| {
            linear(
                1. - (1. - toned[i]) * (1. - wash_tint[i] * wash)
                    + dust_tint[i] * glow * (self.dust / 100.) * 0.8,
            )
        })
    }
    pub fn signal(
        &self,
        rgb: [f32; 3],
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        seconds: f32,
    ) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            width > 0 && height > 0 && x < width && y < height,
            "invalid Falling Dust pixel"
        );
        ensure!(
            rgb.iter().all(|v| v.is_finite()) && seconds.is_finite() && (seconds * 1.3).is_finite(),
            "nonfinite Falling Dust input"
        );
        Ok(self.grade(
            rgb,
            [
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            ],
            width as f32 / height as f32,
            &motes(seconds),
        ))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let seconds = seconds as f32;
        ensure!(
            seconds.is_finite() && (seconds * 1.3).is_finite(),
            "invalid Falling Dust clock"
        );
        let (width, height) = frame.dimensions();
        let motes = motes(seconds);
        concat_working::apply_rgb_at(
            frame,
            |rgb, x, y| {
                self.grade(
                    rgb,
                    [
                        (x as f32 + 0.5) / width as f32,
                        (y as f32 + 0.5) / height as f32,
                    ],
                    width as f32 / height as f32,
                    &motes,
                )
            },
            checkpoint,
        )
    }
}
