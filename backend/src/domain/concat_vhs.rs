//! Independently authored VHS channel bleed, original-image smear, scanlines and timed grain.
use super::{
    concat_chroma::{display, linear},
    concat_super8::grain_noise,
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vhs {
    pub bleed: f32,
    pub lines: f32,
}
impl Default for Vhs {
    fn default() -> Self {
        Self {
            bleed: 60.,
            lines: 50.,
        }
    }
}
// Straight linear RGB for source sampling, premultiplied display RGB for the box,
// and actual alpha (which differs from the hidden-colour proxy's alpha).
fn tap(rgb: [f32; 3], alpha: f32) -> [f32; 7] {
    [
        rgb[0],
        rgb[1],
        rgb[2],
        display(rgb[0]) * alpha,
        display(rgb[1]) * alpha,
        display(rgb[2]) * alpha,
        alpha,
    ]
}
impl Vhs {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.bleed, self.lines]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "invalid VHS controls"
        );
        Ok(())
    }
    fn grade(&self, taps: &[[f32; 7]; 49], x: usize, y: usize, seconds: f32) -> [f32; 3] {
        let bleed = self.bleed / 100.;
        let lines = self.lines / 100.;
        let sample = |offset: f32, channel: usize| {
            let at = 3. + offset;
            let first = at.floor() as usize;
            // Pinned original Metal sampler: measured 8-bit interpolation weights.
            // Separate 11%/17% source cases distinguish this from 9-bit weights.
            let fraction = ((at - at.floor()) * 256.).round() / 256.;
            display(
                taps[21 + first][channel] * (1. - fraction)
                    + taps[21 + (first + 1).min(6)][channel] * fraction,
            )
        };
        let shifted = [
            sample(-bleed * 3., 0),
            display(taps[24][1]),
            sample(bleed * 3., 2),
        ];
        let smear: [f32; 3] =
            std::array::from_fn(|i| (display(taps[17][i]) + display(taps[10][i])) * 0.5);
        let mut sum = [0f32; 4];
        for row in 1..=5 {
            for col in 1..=5 {
                let t = taps[row * 7 + col];
                for i in 0..3 {
                    sum[i] += t[i + 3];
                }
                sum[3] += t[6];
            }
        }
        let alpha = sum[3].max(1e-6);
        let softened: [f32; 3] = std::array::from_fn(|i| {
            let v = shifted[i] * (1. - bleed * 0.25) + smear[i] * (bleed * 0.25);
            v * (1. - bleed * 0.3) + (sum[i] / alpha) * (bleed * 0.3)
        });
        let luma = softened[0] * 0.2126 + softened[1] * 0.7152 + softened[2] * 0.0722;
        let row = (y as f32 + 0.5) / 3.;
        let stripe = if row - row.floor() >= 0.5 { 1. } else { 0. };
        let gain = 1. - lines * 0.25 * stripe;
        let noise = grain_noise(x, y, seconds) * (lines * 0.08);
        softened.map(|v| linear((luma * (1. - 1.3) + v * 1.3) * gain + noise))
    }
    pub fn signal(
        &self,
        neighbors: [[f32; 4]; 49],
        x: usize,
        y: usize,
        seconds: f32,
    ) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            neighbors.iter().flatten().all(|v| v.is_finite())
                && seconds.is_finite()
                && (seconds * 7.31).is_finite(),
            "nonfinite VHS input"
        );
        Ok(self.grade(
            &neighbors.map(|p| tap([p[0], p[1], p[2]], p[3])),
            x,
            y,
            seconds,
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
            seconds.is_finite() && (seconds * 7.31).is_finite(),
            "invalid VHS clock"
        );
        frame.map_original_sample_neighborhood::<49, [f32; 7]>(
            1,
            &mut |p, alpha| {
                tap(
                    concat_working::straight_half(p),
                    half::f16::from_f32(alpha).to_f32(),
                )
            },
            &mut |p, n, x, y| concat_working::store_half(p, self.grade(&n, x, y, seconds)),
            checkpoint,
        )
    }
}
