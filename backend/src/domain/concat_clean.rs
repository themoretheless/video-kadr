//! Independently authored Concat Clean display-space grading and hue-band behavior.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clean {
    pub amount: f32,
}
impl Default for Clean {
    fn default() -> Self {
        Self { amount: 50. }
    }
}
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn saturation(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = luma(rgb);
    rgb.map(|v| y * (1.0 - factor) + v * factor)
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn red_weight(rgb: [f32; 3]) -> f32 {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let lo = rgb[0].min(rgb[1]).min(rgb[2]);
    let d = hi - lo;
    let hue = if d < 0.0001 {
        0.
    } else {
        let h = if hi == rgb[0] {
            (rgb[1] - rgb[2]) / d
        } else if hi == rgb[1] {
            2. + (rgb[2] - rgb[0]) / d
        } else {
            4. + (rgb[0] - rgb[1]) / d
        };
        let h = h / 6.;
        (h - h.floor()) * 360.
    };
    let f = (hue - 10.) / 360. + 0.5;
    let distance = ((f - f.floor()) * 360. - 180.).abs();
    (1. - smooth(20., 40., distance)) * smooth(0., 0.25, d)
}
impl Clean {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0.0..=100.0).contains(&self.amount),
            "Concat Clean amount out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Clean input");
        Ok(self.signal_finite(rgb))
    }
    fn signal_finite(&self, rgb: [f32; 3]) -> [f32; 3] {
        let amount = self.amount / 100.;
        let curve = amount * 0.3;
        let curved = rgb.map(display).map(|v| {
            let c = v.clamp(0., 1.);
            c * (1. - curve) + c * c * (3. - 2. * c) * curve + (v - c)
        });
        let rgb = saturation(curved, 1. + amount * 0.1);
        let w = red_weight(rgb);
        // The source's zero-angle YIQ transform is not an exact identity matrix.
        let y = rgb[0] * 0.299 + rgb[1] * 0.587 + rgb[2] * 0.114;
        let i = rgb[0] * 0.596 - rgb[1] * 0.274 - rgb[2] * 0.322;
        let q = rgb[0] * 0.211 - rgb[1] * 0.523 + rgb[2] * 0.312;
        let rotated = [
            y + 0.956 * i + 0.621 * q,
            y - 0.272 * i - 0.647 * q,
            y - 1.106 * i + 1.703 * q,
        ];
        let treated = saturation(rotated, 1. - amount * 0.12);
        std::array::from_fn(|i| linear(rgb[i] * (1. - w) + treated[i] * w))
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
