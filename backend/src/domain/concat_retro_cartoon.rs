//! Independently authored Concat display-band quantization and Sobel ink.
use super::{
    concat_chroma::{display, linear},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RetroCartoon {
    pub levels: f32,
    pub ink: f32,
}
impl Default for RetroCartoon {
    fn default() -> Self {
        Self {
            levels: 5.,
            ink: 60.,
        }
    }
}
impl RetroCartoon {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.levels.is_finite()
                && (2.0..=8.0).contains(&self.levels)
                && self.ink.is_finite()
                && (0.0..=100.0).contains(&self.ink),
            "Concat Retro Cartoon out of range"
        );
        Ok(())
    }
    fn display_signal(&self, neighbors: [[f32; 3]; 9]) -> [f32; 3] {
        let y = neighbors.map(|c| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722);
        let gx = (y[2] + 2. * y[5] + y[8]) - (y[0] + 2. * y[3] + y[6]);
        let gy = (y[6] + 2. * y[7] + y[8]) - (y[0] + 2. * y[1] + y[2]);
        let edge = (gx * gx + gy * gy).sqrt().clamp(0., 1.);
        let attenuation = 1. - edge * self.ink / 100.;
        let levels = self.levels.round_ties_even().max(2.);
        neighbors[4].map(|v| {
            linear(
                (if v > 1. {
                    v
                } else {
                    (v * levels).floor().min(levels - 1.) / (levels - 1.)
                }) * attenuation,
            )
        })
    }
    pub fn signal(&self, original: [[f32; 3]; 9]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            original.iter().flatten().all(|v| v.is_finite()),
            "nonfinite Retro Cartoon input"
        );
        Ok(self.display_signal(original.map(|p| p.map(display))))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_original_rgb_neighborhood(
            &mut |p| concat_working::straight_half(p).map(display),
            &mut |p, n: [[f32; 3]; 9], _, _| concat_working::store_half(p, self.display_signal(n)),
            checkpoint,
        )
    }
}
