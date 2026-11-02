//! Independently authored Concat display-space autumn highlight look.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Autumn {}
impl Autumn {
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Autumn input");
        Ok(self.signal_finite(rgb))
    }
    fn signal_finite(&self, rgb: [f32; 3]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        let t = ((y - 0.4) / 0.6).clamp(0., 1.);
        let highlight = t * t * (3. - 2. * t);
        let offsets = [0.1, 0.04, -0.08];
        let toned = std::array::from_fn::<_, 3, _>(|i| rgb[i] + offsets[i] * highlight);
        let y = toned[0] * 0.2126 + toned[1] * 0.7152 + toned[2] * 0.0722;
        toned.map(|v| linear(y * (1. - 1.1) + v * 1.1))
    }

    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        super::concat_working::apply_rgb(frame, |rgb| self.signal_finite(rgb), checkpoint)
    }
}
