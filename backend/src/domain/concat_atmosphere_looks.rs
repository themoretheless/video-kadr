//! Independently authored Concat Soft Light halation and Moody grading.
use super::{
    concat_chroma::{display, linear},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn saturate(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = luma(rgb);
    rgb.map(|v| y * (1. - factor) + v * factor)
}
fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn premult(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    let rgb = rgb.map(display);
    [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoftLight {
    pub lift: f32,
    pub warmth: f32,
}
impl Default for SoftLight {
    fn default() -> Self {
        Self {
            lift: 50.,
            warmth: 40.,
        }
    }
}
impl SoftLight {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.lift, self.warmth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Soft Light parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> [f32; 3] {
        let lift = self.lift / 100.;
        let warmth = self.warmth / 100.;
        let black = lift * 0.1;
        let white = 1. - lift * 0.04;
        let out = rgb
            .map(display)
            .map(|v| v * (white - black) + black)
            .map(|v| (v - 0.5) * (1. - lift * 0.18) + 0.5);
        let out = saturate(out, 1. - lift * 0.12);
        let high = smooth(0.4, 1., luma(out));
        let color = [0.07, 0.02, -0.06];
        let out: [f32; 3] = std::array::from_fn(|i| out[i] + color[i] * high * warmth);
        let mut sum = [0.; 3];
        for n in neighbors {
            let alpha = n[3].max(1e-6);
            let bright = smooth(0.6, 1., luma([n[0] / alpha, n[1] / alpha, n[2] / alpha]));
            for i in 0..3 {
                sum[i] += n[i] * bright;
            }
        }
        let tint = [1., 0.97, 0.9];
        std::array::from_fn(|i| {
            let glow = (sum[i] / 25. * tint[i] * (lift * 0.35)).max(0.);
            let base = out[i].min(1.);
            linear(1. - (1. - base) * (1. - glow) + (out[i] - base))
        })
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Soft Light input"
        );
        Ok(self.finite_signal(rgb, neighbors.map(|n| premult([n[0], n[1], n[2]], n[3]))))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_original_rgba_neighborhood_step::<25>(
            5,
            &mut |p, a| {
                premult(
                    concat_working::straight_half(p),
                    half::f16::from_f32(a).to_f32(),
                )
            },
            &mut |p, neighbors, _, _| {
                concat_working::store_half(
                    p,
                    self.finite_signal(concat_working::straight_half(p), neighbors),
                )
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Moody {
    pub depth: f32,
    pub warmth: f32,
}
impl Default for Moody {
    fn default() -> Self {
        Self {
            depth: 60.,
            warmth: 50.,
        }
    }
}
impl Moody {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.depth, self.warmth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Moody parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
        let d = self.depth / 100.;
        let w = self.warmth / 100.;
        let out = rgb.map(display).map(|v| {
            let c = v.clamp(0., 1.);
            let toe = d * 0.9 * (1. - c);
            (c * (1. - toe) + c * c * toe + (v - c)) * (1. - d * 0.06)
        });
        let out = saturate(out, 1. - d * 0.4);
        let mid = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
        let tint = [0.1, 0.03, -0.08];
        let out: [f32; 3] = std::array::from_fn(|i| out[i] + tint[i] * mid * w);
        let low = 1. - smooth(0., 0.6, luma(out));
        let out = [out[0], out[1], out[2] + 0.06 * low * d];
        let x = uv[0] - 0.5;
        let y = uv[1] - 0.5;
        let radius = (x * x + y * y).sqrt() * 1.4142;
        let gain = 1. - smooth(0.35, 1.1, radius) * (d * 0.7);
        out.map(|v| linear(v * gain))
    }
    pub fn signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().chain(uv.iter()).all(|v| v.is_finite()),
            "nonfinite Moody input"
        );
        Ok(self.finite_signal(rgb, uv))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let (w, h) = frame.dimensions();
        concat_working::apply_rgb_at(
            frame,
            |rgb, x, y| {
                self.finite_signal(
                    rgb,
                    [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32],
                )
            },
            checkpoint,
        )
    }
}
