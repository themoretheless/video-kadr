//! Independently authored Concat display-space silver-retention look.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BleachBypass {}
impl BleachBypass {
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite Bleach Bypass input"
        );
        Ok(self.signal_finite(rgb))
    }
    fn signal_finite(&self, rgb: [f32; 3]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        rgb.map(|v| {
            let level = luma * (1.0 - 0.35) + v * 0.35;
            let c = level.clamp(0.0, 1.0);
            let toe = 0.15 * (1.0 - c);
            let t = c * (1.0 - toe) + c * c * toe;
            let shoulder = 0.25 * t;
            let curved =
                t * (1.0 - shoulder) + (1.0 - (1.0 - t) * (1.0 - t)) * shoulder + (level - c);
            linear((curved - 0.5) * 1.25 + 0.5)
        })
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        super::concat_working::apply_rgb(frame, |rgb| self.signal_finite(rgb), checkpoint)
    }
}
