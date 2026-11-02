//! Independently authored Concat Cloudless Sky/Lagoon multi-band grades.
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
fn hue_weight(rgb: [f32; 3], centre: f32, width: f32) -> f32 {
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
        let f = h / 6.;
        (f - f.floor()) * 360.
    };
    let f = (hue - centre) / 360. + 0.5;
    let distance = ((f - f.floor()) * 360. - 180.).abs();
    (1. - smooth(width * 0.5, width, distance)) * smooth(0., 0.25, d)
}
fn rotate(rgb: [f32; 3], degrees: f32) -> [f32; 3] {
    let a = degrees.to_radians();
    let y = rgb[0] * 0.299 + rgb[1] * 0.587 + rgb[2] * 0.114;
    let i = rgb[0] * 0.596 - rgb[1] * 0.274 - rgb[2] * 0.322;
    let q = rgb[0] * 0.211 - rgb[1] * 0.523 + rgb[2] * 0.312;
    let i2 = i * a.cos() - q * a.sin();
    let q2 = i * a.sin() + q * a.cos();
    [
        y + 0.956 * i2 + 0.621 * q2,
        y - 0.272 * i2 - 0.647 * q2,
        y - 1.106 * i2 + 1.703 * q2,
    ]
}
pub(crate) fn hsl(rgb: [f32; 3], centre: f32, width: f32, turn: f32, sat: f32, lum: f32) -> [f32; 3] {
    let weight = hue_weight(rgb, centre, width);
    let treated = saturate(rotate(rgb, turn), sat).map(|v| v * lum);
    std::array::from_fn(|i| rgb[i] * (1. - weight) + treated[i] * weight)
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CloudlessSky {
    pub sky: f32,
    pub greens: f32,
}
impl Default for CloudlessSky {
    fn default() -> Self {
        Self {
            sky: 60.,
            greens: 40.,
        }
    }
}
impl CloudlessSky {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.sky, self.greens]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Cloudless Sky parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let s = self.sky / 100.;
        let g = self.greens / 100.;
        let out = hsl(
            rgb.map(display),
            210.,
            70.,
            -6. * s,
            1. + s * 0.55,
            1. - s * 0.08,
        );
        hsl(out, 105., 60., 12. * g, 1. - g * 0.2, 1.).map(linear)
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite Cloudless Sky input"
        );
        Ok(self.finite_signal(rgb))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        concat_working::apply_rgb(frame, |rgb| self.finite_signal(rgb), checkpoint)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lagoon {
    pub water: f32,
    pub brightness: f32,
}
impl Default for Lagoon {
    fn default() -> Self {
        Self {
            water: 60.,
            brightness: 40.,
        }
    }
}
impl Lagoon {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.water, self.brightness]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Lagoon parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let w = self.water / 100.;
        let exponent = 1. / (1. + self.brightness / 100. * 0.2);
        let out = rgb.map(display).map(|v| v.max(0.).powf(exponent));
        let out = hsl(out, 225., 60., -25. * w, 1. + w * 0.4, 1. + w * 0.12);
        let out = hsl(out, 185., 50., 0., 1. + w * 0.5, 1. + w * 0.08);
        saturate(out, 1. + w * 0.06).map(linear)
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Lagoon input");
        Ok(self.finite_signal(rgb))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        concat_working::apply_rgb(frame, |rgb| self.finite_signal(rgb), checkpoint)
    }
}
