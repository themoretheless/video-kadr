//! Independently authored Concat Bronze/Cool Blue display-space grades.
use super::{
    concat_chroma::{display, linear, skin},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn tint(rgb: [f32; 3], color: [f32; 3], amount: f32) -> [f32; 3] {
    let mid = 1. - ((luma(rgb) - 0.5).abs() * 2.).min(1.);
    std::array::from_fn(|i| rgb[i] + color[i] * mid * amount)
}
fn curve(rgb: [f32; 3], amount: f32) -> [f32; 3] {
    rgb.map(|v| {
        let c = v.clamp(0., 1.);
        c * (1. - amount) + c * c * (3. - 2. * c) * amount + (v - c)
    })
}
fn grade(rgb: [f32; 3], amount: f32, contrast: f32, bronze: bool) -> [f32; 3] {
    let rgb = rgb.map(display);
    let out = if bronze {
        let factor = 1. - amount * 0.9;
        let y = luma(rgb);
        let out = rgb.map(|v| y * (1. - factor) + v * factor);
        let out = tint(out, [0.12, 0.04, -0.12], amount);
        let high = smooth(0.4, 1., luma(out));
        let color = [0.04, 0.01, -0.04];
        std::array::from_fn(|i| out[i] + color[i] * high * amount)
    } else {
        let chill = amount * (1. - skin(rgb) * 0.7);
        let low = 1. - smooth(0., 0.6, luma(rgb));
        let color = [-0.08, 0., 0.25];
        let out = std::array::from_fn(|i| rgb[i] + color[i] * low * chill);
        tint(out, [0., 0., 0.1], chill)
    };
    curve(out, contrast * if bronze { 0.6 } else { 0.7 }).map(linear)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bronze {
    pub tone: f32,
    pub contrast: f32,
}
impl Default for Bronze {
    fn default() -> Self {
        Self {
            tone: 80.,
            contrast: 30.,
        }
    }
}
impl Bronze {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.tone, self.contrast]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Bronze parameter out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Bronze input");
        Ok(grade(rgb, self.tone / 100., self.contrast / 100., true))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(
            frame,
            |rgb| grade(rgb, self.tone / 100., self.contrast / 100., true),
            checkpoint,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CoolBlue {
    pub chill: f32,
    pub contrast: f32,
}
impl Default for CoolBlue {
    fn default() -> Self {
        Self {
            chill: 60.,
            contrast: 30.,
        }
    }
}
impl CoolBlue {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.chill, self.contrast]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat CoolBlue parameter out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite CoolBlue input"
        );
        Ok(grade(rgb, self.chill / 100., self.contrast / 100., false))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(
            frame,
            |rgb| grade(rgb, self.chill / 100., self.contrast / 100., false),
            checkpoint,
        )
    }
}
