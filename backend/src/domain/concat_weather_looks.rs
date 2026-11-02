//! Independently authored Concat Rain halation and Umber hue-band grade.
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
fn tint(rgb: [f32; 3], color: [f32; 3], amount: f32) -> [f32; 3] {
    let mid = 1. - ((luma(rgb) - 0.5).abs() * 2.).min(1.);
    std::array::from_fn(|i| rgb[i] + color[i] * mid * amount)
}
fn premult(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    let rgb = rgb.map(display);
    [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rain {
    pub haze: f32,
    pub chill: f32,
}
impl Default for Rain {
    fn default() -> Self {
        Self {
            haze: 60.,
            chill: 50.,
        }
    }
}
impl Rain {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.haze, self.chill]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Rain parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> [f32; 3] {
        let h = self.haze / 100.;
        let ch = self.chill / 100.;
        let black = h * 0.08;
        let white = 1. - h * 0.02;
        let out = saturate(
            rgb.map(display).map(|v| v * (white - black) + black),
            1. - h * 0.4,
        )
        .map(|v| (v - 0.5) * (1. - h * 0.08) + 0.5);
        let low = 1. - smooth(0., 0.6, luma(out));
        let out = tint(
            [out[0], out[1], out[2] + 0.1 * low * ch],
            [0., 0., 0.06],
            ch,
        );
        let mut sum = [0.; 3];
        for n in neighbors {
            let alpha = n[3].max(1e-6);
            let bright = smooth(0.65, 1., luma([n[0] / alpha, n[1] / alpha, n[2] / alpha]));
            for i in 0..3 {
                sum[i] += n[i] * bright;
            }
        }
        let tint = [0.85, 0.92, 1.];
        std::array::from_fn(|i| {
            let glow = (sum[i] / 25. * tint[i] * (h * 0.45)).max(0.);
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
            "nonfinite Rain input"
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
            6,
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
fn green_weight(rgb: [f32; 3]) -> f32 {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let lo = rgb[0].min(rgb[1]).min(rgb[2]);
    let delta = hi - lo;
    let hue = if delta < 0.0001 {
        0.
    } else {
        let h = if hi == rgb[0] {
            (rgb[1] - rgb[2]) / delta
        } else if hi == rgb[1] {
            2. + (rgb[2] - rgb[0]) / delta
        } else {
            4. + (rgb[0] - rgb[1]) / delta
        };
        let f = h / 6.;
        (f - f.floor()) * 360.
    };
    let f = (hue - 110.) / 360. + 0.5;
    let distance = ((f - f.floor()) * 360. - 180.).abs();
    (1. - smooth(30., 60., distance)) * smooth(0., 0.25, delta)
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Umber {
    pub warmth: f32,
    pub soften: f32,
}
impl Default for Umber {
    fn default() -> Self {
        Self {
            warmth: 60.,
            soften: 40.,
        }
    }
}
impl Umber {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.warmth, self.soften]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Umber parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let w = self.warmth / 100.;
        let s = self.soften / 100.;
        let out = tint(rgb.map(display), [0.1, 0.03, -0.1], w);
        let low = 1. - smooth(0., 0.6, luma(out));
        let out = [out[0] + 0.04 * low * w, out[1], out[2] - 0.04 * low * w];
        let weight = green_weight(out);
        let treated = saturate(rotate(out, -20. * w), 1. - w * 0.3).map(|v| v * (1. - w * 0.08));
        let out: [f32; 3] = std::array::from_fn(|i| out[i] * (1. - weight) + treated[i] * weight);
        let black = s * 0.05;
        out.map(|v| (v - 0.5) * (1. - s * 0.12) + 0.5)
            .map(|v| linear(v * (1. - black) + black))
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Umber input");
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
