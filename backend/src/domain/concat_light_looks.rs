//! Independently authored Concat Golden Hour display balance and Daydream halation.
use super::{
    concat_chroma::{display, linear, skin},
    concat_working,
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
// Both admitted temperatures (and the daylight baseline) use the source <=6600K branch.
fn kelvin(k: f32) -> [f32; 3] {
    let t = k / 100.;
    [
        1.,
        ((99.4708 * t.ln() - 161.1196) / 255.).clamp(0., 1.),
        ((138.5177 * (t - 10.).ln() - 305.0448) / 255.).clamp(0., 1.),
    ]
}
struct PreparedGoldenHour {
    gain: [f32; 3],
    glow: f32,
}
impl PreparedGoldenHour {
    fn signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let out = std::array::from_fn(|i| rgb[i] * self.gain[i]);
        let low = 1. - smooth(0., 0.6, luma(out));
        let high = smooth(0.4, 1., luma(out));
        let shadow = [0.05, 0., 0.04];
        let light = [0.14, 0.05, 0.];
        let black = self.glow * 0.04;
        std::array::from_fn(|i| {
            linear(
                (out[i] + (shadow[i] * low + light[i] * high) * self.glow) * (1. - black) + black,
            )
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GoldenHour {
    pub glow: f32,
    pub warmth: f32,
}
impl Default for GoldenHour {
    fn default() -> Self {
        Self {
            glow: 50.,
            warmth: 4600.,
        }
    }
}
impl GoldenHour {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.glow.is_finite()
                && (0.0..=100.0).contains(&self.glow)
                && self.warmth.is_finite()
                && (3500.0..=6500.0).contains(&self.warmth),
            "Concat Golden Hour parameter out of range"
        );
        Ok(())
    }
    fn prepared(&self) -> Result<PreparedGoldenHour> {
        self.validate()?;
        let daylight = kelvin(6500.);
        let warm = kelvin(self.warmth);
        let tint = std::array::from_fn(|i| warm[i] / daylight[i]);
        let y = luma(tint).max(0.001);
        Ok(PreparedGoldenHour {
            gain: tint.map(|v| v / y),
            glow: self.glow / 100.,
        })
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        let p = self.prepared()?;
        ensure!(
            rgb.iter().all(|v| v.is_finite()),
            "nonfinite Golden Hour input"
        );
        Ok(p.signal(rgb))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let p = self.prepared()?;
        concat_working::apply_rgb(frame, |rgb| p.signal(rgb), checkpoint)
    }
}
fn premult(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    let rgb = rgb.map(display);
    [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Daydream {
    pub haze: f32,
    pub warmth: f32,
}
impl Default for Daydream {
    fn default() -> Self {
        Self {
            haze: 50.,
            warmth: 50.,
        }
    }
}
impl Daydream {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.haze, self.warmth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Daydream parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let h = self.haze / 100.;
        let w = self.warmth / 100.;
        let keep = 1. - skin(rgb) * 0.6;
        let black = h * 0.08;
        let white = 1. - h * 0.03;
        let out = rgb
            .map(|v| v * (white - black) + black)
            .map(|v| (v - 0.5) * (1. - h * 0.12) + 0.5);
        let low = 1. - smooth(0., 0.6, luma(out));
        let high = smooth(0.4, 1., luma(out));
        let shadow = [0., 0., 0.04];
        let light = [0.08, 0.02, -0.05];
        let out: [f32; 3] =
            std::array::from_fn(|i| out[i] + (shadow[i] * low + light[i] * high) * (w * keep));
        let mut sum = [0.; 3];
        for n in neighbors {
            let alpha = n[3].max(1e-6);
            let bright = smooth(0.55, 1., luma([n[0] / alpha, n[1] / alpha, n[2] / alpha]));
            for i in 0..3 {
                sum[i] += n[i] * bright;
            }
        }
        let tint = [1., 0.9, 0.75];
        std::array::from_fn(|i| {
            let glow = (sum[i] / 25. * tint[i] * (h * 0.5)).max(0.);
            let base = out[i].min(1.);
            linear(1. - (1. - base) * (1. - glow) + (out[i] - base))
        })
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Daydream input"
        );
        Ok(self.finite_signal(rgb, neighbors.map(|n| premult([n[0], n[1], n[2]], n[3]))))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_original_rgba_neighborhood_step::<25>(
            7,
            &mut |p, a| {
                premult(
                    concat_working::straight_half(p),
                    half::f16::from_f32(a).to_f32(),
                )
            },
            &mut |p, neighbors, _, _| {
                concat_working::store_half(
                    p,
                    self.finite_signal(concat_working::straight_half(p), neighbors),
                )
            },
            checkpoint,
        )
    }
}
