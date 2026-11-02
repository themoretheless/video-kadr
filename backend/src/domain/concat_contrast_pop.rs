//! Independently authored display-space Contrast Pop, qualified against original Metal.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContrastPop {
    pub contrast: f32,
}
impl Default for ContrastPop {
    fn default() -> Self {
        Self { contrast: 1.25 }
    }
}
impl ContrastPop {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.contrast.is_finite() && (1.0..=2.0).contains(&self.contrast),
            "Concat Contrast Pop contrast out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite Contrast Pop input"
        );
        Ok(self.signal_finite(rgb))
    }
    fn signal_finite(&self, rgb: [f32; 3]) -> [f32; 3] {
        let amount = self.contrast - 1.0;
        rgb.map(|v| {
            let level = (display(v) - 0.5) * (1.0 + amount * 0.7) + 0.5;
            let c = level.clamp(0.0, 1.0);
            let toe = amount * 0.5 * (1.0 - c);
            let t = c * (1.0 - toe) + (c * c) * toe;
            let shoulder = amount * 0.5 * t;
            let curved = t * (1.0 - shoulder) + (1.0 - (1.0 - t) * (1.0 - t)) * shoulder;
            linear(curved + (level - c))
        })
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(frame, |rgb| self.signal_finite(rgb), checkpoint)
    }
}
