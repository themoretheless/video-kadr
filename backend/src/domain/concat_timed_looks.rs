//! Independently authored Concat Mono and Midnight display grades with absolute-time grain.
use super::{
    concat_chroma::{display, linear},
    concat_super8::grain_noise,
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mono {
    pub colour: f32,
    pub contrast: f32,
    pub grain: f32,
}
impl Default for Mono {
    fn default() -> Self {
        Self {
            colour: 40.,
            contrast: 40.,
            grain: 0.,
        }
    }
}
impl Mono {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.colour.is_finite()
                && (-100.0..=100.0).contains(&self.colour)
                && [self.contrast, self.grain]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Mono out of range"
        );
        Ok(())
    }
    fn finite(&self, rgb: [f32; 3], x: usize, y: usize, time: f32) -> [f32; 3] {
        let c = rgb.map(display);
        let f = self.colour / 100.;
        let r = 0.3 + 0.35 * f;
        let b = 0.11 - 0.05 * f;
        let g = 1. - r - b;
        let total = (r + g + b).max(0.001);
        let v = c[0] * (r / total) + c[1] * (g / total) + c[2] * (b / total);
        let base = v.clamp(0., 1.);
        let amount = self.contrast / 100. * 0.8;
        let out = base * (1. - amount) + base * base * (3. - 2. * base) * amount + (v - base);
        let out = linear(out + grain_noise(x, y, time) * (self.grain / 100. * 0.1));
        [out; 3]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Midnight {
    pub depth: f32,
    pub grain: f32,
}
impl Default for Midnight {
    fn default() -> Self {
        Self {
            depth: 60.,
            grain: 25.,
        }
    }
}
impl Midnight {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.depth, self.grain]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Midnight out of range"
        );
        Ok(())
    }
    fn finite(&self, rgb: [f32; 3], x: usize, y: usize, time: f32) -> [f32; 3] {
        let d = self.depth / 100.;
        let c = rgb.map(display).map(|v| {
            let b = v.clamp(0., 1.);
            let m = d * (1. - b);
            b * (1. - m) + b * b * m + (v - b)
        });
        let y0 = luma(c);
        let saturation = 1. - d * 0.35;
        let c = c.map(|v| y0 * (1. - saturation) + v * saturation);
        let shadow = 1. - smooth(0., 0.6, luma(c));
        let tint = [-0.05, 0., 0.12];
        let mut out = std::array::from_fn(|i| c[i] + tint[i] * shadow * d);
        let middle = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
        out[2] += 0.08 * middle * d;
        let n = grain_noise(x, y, time) * (self.grain / 100. * 0.1);
        out.map(|v| linear(v + n))
    }
}
macro_rules! timed_api {
    ($name:ident) => {
        impl $name {
            pub fn signal(
                &self,
                rgb: [f32; 3],
                x: usize,
                y: usize,
                seconds: f32,
            ) -> Result<[f32; 3]> {
                self.validate()?;
                ensure!(
                    rgb.iter().all(|v| v.is_finite())
                        && seconds.is_finite()
                        && (seconds * 7.31).is_finite(),
                    "nonfinite Concat timed grade input"
                );
                Ok(self.finite(rgb, x, y, seconds))
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
                    seconds.is_finite() && (seconds * 7.31).is_finite(),
                    "invalid Concat grain clock"
                );
                concat_working::apply_rgb_at(
                    frame,
                    |rgb, x, y| self.finite(rgb, x, y, seconds),
                    checkpoint,
                )
            }
        }
    };
}
timed_api!(Mono);
timed_api!(Midnight);
