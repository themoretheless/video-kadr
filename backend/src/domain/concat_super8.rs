//! Independently authored Concat Super 8 display grade and deterministic timed grain.
use super::{
    concat_chroma::{display, linear},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Super8 {}
pub(crate) fn grain_noise(x: usize, y: usize, seconds: f32) -> f32 {
    let clock = seconds * 7.31;
    let seed = clock - clock.floor();
    let mut h = (x as f32).to_bits()
        ^ (y as f32).to_bits().wrapping_mul(0x9e3779b9)
        ^ seed.to_bits().wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16777216. - 0.5
}
impl Super8 {
    fn finite_signal(&self, rgb: [f32; 3], x: usize, y: usize, seconds: f32) -> [f32; 3] {
        let c = rgb.map(|v| display(v) * 0.92 + 0.08);
        let luma = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
        let t = ((luma - 0.4) / 0.6).clamp(0., 1.);
        let highlight = t * t * (3. - 2. * t);
        let noise = grain_noise(x, y, seconds) * 0.05;
        let tint = [0.08, 0., -0.06];
        std::array::from_fn(|i| linear(c[i] + tint[i] * highlight + noise))
    }
    pub fn signal(&self, rgb: [f32; 3], x: usize, y: usize, seconds: f32) -> Result<[f32; 3]> {
        ensure!(
            rgb.iter().all(|v| v.is_finite())
                && seconds.is_finite()
                && (seconds * 7.31).is_finite(),
            "nonfinite Super 8 input"
        );
        Ok(self.finite_signal(rgb, x, y, seconds))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let seconds = seconds as f32;
        ensure!(
            seconds.is_finite() && (seconds * 7.31).is_finite(),
            "invalid Super 8 clock"
        );
        concat_working::apply_rgb_at(
            frame,
            |rgb, x, y| self.finite_signal(rgb, x, y, seconds),
            checkpoint,
        )
    }
}
