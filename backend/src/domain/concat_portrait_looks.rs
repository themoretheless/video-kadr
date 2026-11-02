//! Independently authored Concat Fair/Peach portrait grades.
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
fn saturate(rgb: [f32; 3], factor: f32) -> [f32; 3] {
    let y = luma(rgb);
    rgb.map(|v| y * (1. - factor) + v * factor)
}
fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn band_weight(rgb: [f32; 3]) -> f32 {
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
    let f = (hue - 20.) / 360. + 0.5;
    let distance = ((f - f.floor()) * 360. - 180.).abs();
    (1. - smooth(22.5, 45., distance)) * smooth(0., 0.25, delta)
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
fn hsl(rgb: [f32; 3], turn: f32, sat: f32, lum: f32) -> [f32; 3] {
    let weight = band_weight(rgb);
    let treated = saturate(rotate(rgb, turn), sat).map(|v| v * lum);
    std::array::from_fn(|i| rgb[i] * (1. - weight) + treated[i] * weight)
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fair {
    pub brighten: f32,
    pub even: f32,
}
impl Default for Fair {
    fn default() -> Self {
        Self {
            brighten: 50.,
            even: 50.,
        }
    }
}
impl Fair {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.brighten, self.even]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Fair parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let b = self.brighten / 100.;
        let e = self.even / 100.;
        let out = hsl(rgb.map(display), 4. * e, 1. - e * 0.15, 1. + b * 0.18);
        let exponent = 1. / (1. + b * 0.08);
        out.map(|v| linear(v.max(0.).powf(exponent)))
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Fair input");
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
fn premult(rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    let rgb = rgb.map(display);
    [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Peach {
    pub glow: f32,
    pub softness: f32,
}
impl Default for Peach {
    fn default() -> Self {
        Self {
            glow: 50.,
            softness: 40.,
        }
    }
}
impl Peach {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.glow, self.softness]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Peach parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let g = self.glow / 100.;
        let weight = skin(rgb) * self.softness / 100. * 0.7;
        let mut sum = [0.; 4];
        for n in neighbors {
            for i in 0..4 {
                sum[i] += n[i];
            }
        }
        let avg = sum.map(|v| v / 25.);
        let a = avg[3].max(1e-6);
        let out = std::array::from_fn(|i| rgb[i] * (1. - weight) + (avg[i] / a) * weight);
        let out = hsl(out, 6. * g, 1. + g * 0.1, 1. + g * 0.1);
        let black = g * 0.06;
        let out = out
            .map(|v| v * (1. - black) + black)
            .map(|v| (v - 0.5) * (1. - g * 0.1) + 0.5);
        let high = smooth(0.4, 1., luma(out));
        let color = [0.05, 0.02, 0.];
        std::array::from_fn(|i| linear(out[i] + color[i] * high * g))
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Peach input"
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
            2,
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
