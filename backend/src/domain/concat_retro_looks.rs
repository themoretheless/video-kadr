//! Independently authored Concat Polaroid fractional blur and Vintage grade.
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
fn radius(uv: [f32; 2]) -> f32 {
    let x = uv[0] - 0.5;
    let y = uv[1] - 0.5;
    (x * x + y * y).sqrt() * 1.4142
}
#[derive(Clone, Copy)]
pub(crate) struct Tap {
    linear: [f32; 3],
    display_premult: [f32; 3],
    alpha: f32,
}
impl Tap {
    pub(crate) fn new(rgb: [f32; 3], alpha: f32) -> Self {
        Self {
            linear: rgb,
            display_premult: rgb.map(display).map(|v| v * alpha),
            alpha,
        }
    }
}
// Source sampler: opaque/near-integer alpha filters linear light before gamma;
// semi-transparent samples filter display-premultiplied texels instead.
pub(crate) fn filtered(taps: &[Tap; 49], n: usize) -> [f32; 4] {
    let x = (n % 5) as f32 * 1.5;
    let y = (n / 5) as f32 * 1.5;
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let mut light = [0.; 3];
    let mut premult = [0.; 3];
    let mut alpha = 0.;
    for (yy, wy) in [(y0, 1. - fy), ((y0 + 1).min(6), fy)] {
        for (xx, wx) in [(x0, 1. - fx), ((x0 + 1).min(6), fx)] {
            let t = taps[yy * 7 + xx];
            let w = wx * wy;
            alpha += t.alpha * w;
            for i in 0..3 {
                light[i] += t.linear[i] * w;
                premult[i] += t.display_premult[i] * w;
            }
        }
    }
    if (alpha - alpha.round()).abs() < 1e-4 {
        premult = light.map(display).map(|v| v * alpha);
    }
    [premult[0], premult[1], premult[2], alpha]
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Polaroid {
    pub fade: f32,
    pub warmth: f32,
}
impl Default for Polaroid {
    fn default() -> Self {
        Self {
            fade: 60.,
            warmth: 50.,
        }
    }
}
impl Polaroid {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.fade, self.warmth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Polaroid parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], taps: &[Tap; 49], uv: [f32; 2]) -> [f32; 3] {
        let f = self.fade / 100.;
        let w = self.warmth / 100.;
        let black = f * 0.1;
        let white = 1. - f * 0.05;
        let out = rgb
            .map(display)
            .map(|v| v * (white - black) + black)
            .map(|v| (v - 0.5) * (1. - f * 0.1) + 0.5);
        let out = saturate(out, 1. - f * 0.15);
        let low = 1. - smooth(0., 0.6, luma(out));
        let high = smooth(0.4, 1., luma(out));
        let shadow = [-0.06, 0.05, 0.08];
        let highlight = [0.1, 0.05, -0.06];
        let out: [f32; 3] =
            std::array::from_fn(|i| out[i] + (shadow[i] * f * low + highlight[i] * w * high));
        let mut sum = [0.; 4];
        for n in 0..25 {
            let p = filtered(taps, n);
            for i in 0..4 {
                sum[i] += p[i];
            }
        }
        let avg = sum.map(|v| v / 25.);
        let alpha = avg[3].max(1e-6);
        let r = radius(uv);
        let blend = smooth(0.5, 1., r) * f * 0.6;
        let gain = 1. - smooth(0.35, 1.1, r) * (f * 0.3);
        std::array::from_fn(|i| linear((out[i] * (1. - blend) + (avg[i] / alpha) * blend) * gain))
    }
    pub fn signal(
        &self,
        rgb: [f32; 3],
        neighbors: [[f32; 4]; 49],
        uv: [f32; 2],
    ) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .chain(uv.iter())
                .all(|v| v.is_finite()),
            "nonfinite Polaroid input"
        );
        let taps = neighbors.map(|p| Tap::new([p[0], p[1], p[2]], p[3]));
        Ok(self.finite_signal(rgb, &taps, uv))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let (w, h) = frame.dimensions();
        frame.map_original_sample_neighborhood::<49, Tap>(
            1,
            &mut |p, a| {
                Tap::new(
                    concat_working::straight_half(p),
                    half::f16::from_f32(a).to_f32(),
                )
            },
            &mut |p, taps, x, y| {
                concat_working::store_half(
                    p,
                    self.finite_signal(
                        concat_working::straight_half(p),
                        &taps,
                        [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32],
                    ),
                )
            },
            checkpoint,
        )
    }
}
fn green_weight(rgb: [f32; 3]) -> f32 {
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
    let f = (hue - 120.) / 360. + 0.5;
    let distance = ((f - f.floor()) * 360. - 180.).abs();
    (1. - smooth(35., 70., distance)) * smooth(0., 0.25, d)
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
pub struct Vintage {
    pub age: f32,
    pub tint: f32,
}
impl Default for Vintage {
    fn default() -> Self {
        Self {
            age: 60.,
            tint: 50.,
        }
    }
}
impl Vintage {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.age, self.tint]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Vintage parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> [f32; 3] {
        let a = self.age / 100.;
        let t = self.tint / 100.;
        let black = a * 0.12;
        let white = 1. - a * 0.06;
        let out = saturate(
            rgb.map(display).map(|v| v * (white - black) + black),
            1. - a * 0.35,
        )
        .map(|v| (v - 0.5) * (1. - a * 0.1) + 0.5);
        let weight = green_weight(out);
        let treated = saturate(rotate(out, -35. * t), 1. - t * 0.1);
        let out: [f32; 3] = std::array::from_fn(|i| out[i] * (1. - weight) + treated[i] * weight);
        let mid = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
        let color = [0.05, 0.03, -0.08];
        let gain = 1. - smooth(0.35, 1.1, radius(uv)) * (a * 0.4);
        std::array::from_fn(|i| linear((out[i] + color[i] * mid * t) * gain))
    }
    pub fn signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter().chain(uv.iter()).all(|v| v.is_finite()),
            "nonfinite Vintage input"
        );
        Ok(self.finite_signal(rgb, uv))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let (w, h) = frame.dimensions();
        concat_working::apply_rgb_at(
            frame,
            |rgb, x, y| {
                self.finite_signal(
                    rgb,
                    [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32],
                )
            },
            checkpoint,
        )
    }
}
