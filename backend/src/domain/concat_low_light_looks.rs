//! Independently authored Concat Night shadow softening and Spooky vignette grade.
use super::{
    concat_chroma::{display, linear},
    concat_retro_looks::{filtered, Tap},
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
pub struct Night {
    pub lift: f32,
    pub clarity: f32,
}
impl Default for Night {
    fn default() -> Self {
        Self {
            lift: 60.,
            clarity: 40.,
        }
    }
}
impl Night {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.lift, self.clarity]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Night out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], taps: &[Tap; 49]) -> [f32; 3] {
        let c = rgb.map(display);
        let mut sum = [0.; 4];
        for n in 0..25 {
            let p = filtered(taps, n);
            for i in 0..4 {
                sum[i] += p[i];
            }
        }
        let amount = (1. - smooth(0., 0.6, luma(c))) * self.clarity / 100.;
        let alpha = sum[3].max(1e-6);
        let lift = self.lift / 100.;
        let level = lift * 0.05;
        let exponent = 1. / (1. + lift * 0.45).max(0.01);
        std::array::from_fn(|i| {
            let base = c[i] * (1. - amount) + (sum[i] / alpha) * amount;
            linear((base * (1. - level) + level).max(0.).powf(exponent))
        })
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 49]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Night input"
        );
        Ok(self.finite_signal(rgb, &neighbors.map(|p| Tap::new([p[0], p[1], p[2]], p[3]))))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_original_sample_neighborhood::<49, Tap>(
            1,
            &mut |p, a| {
                Tap::new(
                    concat_working::straight_half(p),
                    half::f16::from_f32(a).to_f32(),
                )
            },
            &mut |p, taps, _, _| {
                concat_working::store_half(
                    p,
                    self.finite_signal(concat_working::straight_half(p), &taps),
                )
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Spooky {
    pub dread: f32,
}
impl Default for Spooky {
    fn default() -> Self {
        Self { dread: 60. }
    }
}
impl Spooky {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.dread.is_finite() && (0.0..=100.0).contains(&self.dread),
            "Concat Spooky out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
        let c = rgb.map(display);
        let d = self.dread / 100.;
        let y = luma(c);
        let out = c.map(|v| {
            let v = y * (1. - (1. - d * 0.6)) + v * (1. - d * 0.6);
            let b = v.clamp(0., 1.);
            let amount = d * (1. - b);
            let t = b * (1. - amount) + b * b * amount;
            ((t + (v - b)) - 0.5) * (1. + d * 0.2) + 0.5
        });
        let mid = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
        let tint = [-0.08, 0.1, 0.06];
        let r = ((uv[0] - 0.5).powi(2) + (uv[1] - 0.5).powi(2)).sqrt() * 1.4142;
        let gain = 1. - smooth(0.35, 1.1, r) * (d * 0.85);
        std::array::from_fn(|i| linear((out[i] + tint[i] * mid * d) * gain))
    }
    pub fn signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().chain(uv.iter()).all(|v| v.is_finite()),
            "nonfinite Spooky input"
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
