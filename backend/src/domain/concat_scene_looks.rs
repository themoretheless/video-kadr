//! Independently authored Concat Noir vignette and Dreamy fractional softening.
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Noir {}
impl Noir {
    fn finite_signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
        let c = rgb.map(display);
        let weights = [0.3f32, 0.3, 0.4];
        let total = weights.iter().sum::<f32>().max(0.001);
        let w = weights.map(|v| v / total);
        let g = c[0] * w[0] + c[1] * w[1] + c[2] * w[2];
        let mid = 1. - ((luma([g; 3]) - 0.5).abs() * 2.).min(1.);
        let r = ((uv[0] - 0.5).powi(2) + (uv[1] - 0.5).powi(2)).sqrt() * 1.4142;
        let t = ((r - 0.35) / (1.1 - 0.35)).clamp(0., 1.);
        let gain = 1. - t * t * (3. - 2. * t) * 0.35;
        [-0.02, 0., 0.05].map(|offset| linear(((g + offset * mid) * gain - 0.5) * 1.3 + 0.5))
    }
    pub fn signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> Result<[f32; 3]> {
        ensure!(
            rgb.iter().chain(uv.iter()).all(|v| v.is_finite()),
            "nonfinite Noir input"
        );
        Ok(self.finite_signal(rgb, uv))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dreamy {}
impl Dreamy {
    fn finite_signal(&self, rgb: [f32; 3], taps: &[Tap; 49]) -> [f32; 3] {
        let mut sum = [0.; 4];
        for n in 0..25 {
            let p = filtered(taps, n);
            for i in 0..4 {
                sum[i] += p[i];
            }
        }
        let alpha = sum[3].max(1e-6);
        let c = rgb.map(display);
        let f = std::array::from_fn::<_, 3, _>(|i| {
            (c[i] * (1. - 0.4) + (sum[i] / alpha) * 0.4) * (1. - 0.08) + 0.08
        });
        let y = luma(f);
        f.map(|v| linear(y * (1. - 0.9) + v * 0.9))
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 49]) -> Result<[f32; 3]> {
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Dreamy input"
        );
        let taps = neighbors.map(|p| Tap::new([p[0], p[1], p[2]], p[3]));
        Ok(self.finite_signal(rgb, &taps))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
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
