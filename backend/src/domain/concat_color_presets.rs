//! Independently authored Concat display balance and skin-protected split tone.
use super::{
    concat_chroma::{display, linear, skin},
    concat_fixed_looks::kelvin,
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn gain(k: f32) -> [f32; 3] {
    let baseline = kelvin(6500.);
    let target = kelvin(k);
    let tint = std::array::from_fn(|i| target[i] / baseline[i]);
    let y = luma(tint).max(0.001);
    tint.map(|v| v / y)
}
fn balanced(c: [f32; 3], gain: [f32; 3]) -> [f32; 3] {
    let c = c.map(display);
    std::array::from_fn(|i| linear(c[i] * gain[i]))
}
macro_rules! temperature_preset {
    ($name:ident,$default:expr,$min:expr,$max:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub temperature: f32,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    temperature: $default,
                }
            }
        }
        impl $name {
            pub fn validate(&self) -> Result<()> {
                ensure!(
                    self.temperature.is_finite() && ($min..=$max).contains(&self.temperature),
                    "Concat temperature out of range"
                );
                Ok(())
            }
            pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
                self.validate()?;
                ensure!(
                    rgb.iter().all(|v| v.is_finite()),
                    "nonfinite Concat temperature input"
                );
                Ok(balanced(rgb, gain(self.temperature)))
            }
            pub fn apply_linear(
                &self,
                frame: &mut LinearColorFrame,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<()> {
                self.validate()?;
                let g = gain(self.temperature);
                concat_working::apply_rgb(frame, |rgb| balanced(rgb, g), checkpoint)
            }
        }
    };
}
temperature_preset!(Cool, 8500., 7000., 11000.);
temperature_preset!(Warm, 4600., 3000., 6000.);
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TealOrange {
    pub amount: f32,
    pub contrast: f32,
    pub skin: f32,
}
impl Default for TealOrange {
    fn default() -> Self {
        Self {
            amount: 70.,
            contrast: 25.,
            skin: 80.,
        }
    }
}
impl TealOrange {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.amount, self.contrast, self.skin]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Teal & Orange parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let c = rgb.map(display);
        let keep = 1. - skin(c) * self.skin / 100.;
        let amount = self.amount / 100. * keep;
        let y = luma(c);
        let low = (y / 0.6).clamp(0., 1.);
        let low = 1. - low * low * (3. - 2. * low);
        let high = ((y - 0.4) / (1. - 0.4)).clamp(0., 1.);
        let high = high * high * (3. - 2. * high);
        let shadows = [-0.12, 0.04, 0.16];
        let highlights = [0.16, 0.04, -0.12];
        let toe = self.contrast / 100. * 0.6;
        let shoulder = self.contrast / 100. * 0.5;
        std::array::from_fn(|i| {
            let v = c[i] + (shadows[i] * low + highlights[i] * high) * amount;
            let b = v.clamp(0., 1.);
            let mix = toe * (1. - b);
            let t = b * (1. - mix) + b * b * mix;
            let mix = shoulder * t;
            linear(t * (1. - mix) + (1. - (1. - t) * (1. - t)) * mix + (v - b))
        })
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite Teal & Orange input"
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
