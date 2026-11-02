//! Independently authored Concat Ivory/Milk matte grades, in display space.
use super::{
    concat_chroma::{display, linear},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn saturation(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = luma(rgb);
    rgb.map(|v| y * (1. - factor) + v * factor)
}
fn highlights(rgb: [f32; 3]) -> f32 {
    let t = ((luma(rgb) - 0.4) / 0.6).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn grade(rgb: [f32; 3], m: f32, tone: f32, ivory: bool) -> [f32; 3] {
    let (gamma, black, white, sat) = if ivory {
        (1. + m * 0.18, m * 0.12, 1. - m * 0.04, 1. - m * 0.18)
    } else {
        (1. + m * 0.25, m * 0.14, 1. - m * 0.05, 1. - m * 0.25)
    };
    let exponent = 1. / gamma;
    let out = rgb
        .map(display)
        .map(|v| v.max(0.).powf(exponent))
        .map(|v| v * (white - black) + black);
    let out = saturation(out, sat);
    let high = highlights(out);
    let color = if ivory {
        [0.08, 0.03, -0.06]
    } else {
        [-0.03, 0., 0.06]
    };
    let out = std::array::from_fn(|i| out[i] + color[i] * high * tone);
    let out = if ivory {
        let mid = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
        let tint = [0.04, 0., -0.04];
        std::array::from_fn(|i| out[i] + tint[i] * mid * tone)
    } else {
        out
    };
    out.map(linear)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ivory {
    pub matte: f32,
    pub warmth: f32,
}
impl Default for Ivory {
    fn default() -> Self {
        Self {
            matte: 60.,
            warmth: 50.,
        }
    }
}
impl Ivory {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.matte, self.warmth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Ivory parameter out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Ivory input");
        Ok(grade(rgb, self.matte / 100., self.warmth / 100., true))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(
            frame,
            |rgb| grade(rgb, self.matte / 100., self.warmth / 100., true),
            checkpoint,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Milk {
    pub matte: f32,
    pub cool: f32,
}
impl Default for Milk {
    fn default() -> Self {
        Self {
            matte: 60.,
            cool: 40.,
        }
    }
}
impl Milk {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.matte, self.cool]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Milk parameter out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Milk input");
        Ok(grade(rgb, self.matte / 100., self.cool / 100., false))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(
            frame,
            |rgb| grade(rgb, self.matte / 100., self.cool / 100., false),
            checkpoint,
        )
    }
}
