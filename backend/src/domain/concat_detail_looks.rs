//! Independently authored Concat Fresh/Vivid, including alpha-aware display blur.
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
fn vibrance(rgb: [f32; 3], amount: f32) -> [f32; 3] {
    let hi = rgb[0].max(rgb[1]).max(rgb[2]);
    let lo = rgb[0].min(rgb[1]).min(rgb[2]);
    saturate(rgb, 1. + amount * (1. - (hi - lo)).max(0.))
}
fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fresh {
    pub pop: f32,
    pub brightness: f32,
}
impl Default for Fresh {
    fn default() -> Self {
        Self {
            pop: 50.,
            brightness: 40.,
        }
    }
}
impl Fresh {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.pop, self.brightness]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Fresh parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let p = self.pop / 100.;
        let exponent = 1. / (1. + self.brightness / 100. * 0.25);
        let out = vibrance(rgb.map(display).map(|v| v.max(0.).powf(exponent)), p * 0.9);
        let high = smooth(0.4, 1., luma(out));
        let color = [0., 0.05, 0.03];
        std::array::from_fn(|i| linear(out[i] + color[i] * high * p))
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(rgb.iter().all(|v| v.is_finite()), "nonfinite Fresh input");
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
pub struct Vivid {
    pub colour: f32,
    pub punch: f32,
}
impl Default for Vivid {
    fn default() -> Self {
        Self {
            colour: 60.,
            punch: 50.,
        }
    }
}
impl Vivid {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.colour, self.punch]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Vivid parameter out of range"
        );
        Ok(())
    }
    fn finite_signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> [f32; 3] {
        let rgb = rgb.map(display);
        let punch = self.punch / 100.;
        let keep = 1. - skin(rgb) * 0.6;
        let amount = punch * 0.7;
        let out = rgb.map(|v| {
            let c = v.clamp(0., 1.);
            c * (1. - amount) + c * c * (3. - 2. * c) * amount + (v - c)
        });
        let boosted = vibrance(out, self.colour / 100. * 1.4);
        let out: [f32; 3] = std::array::from_fn(|i| out[i] * (1. - keep) + boosted[i] * keep);
        let mut sum = [0.; 4];
        for n in neighbors {
            for i in 0..4 {
                sum[i] += n[i];
            }
        }
        let average = sum.map(|v| v / 25.);
        let alpha = average[3].max(1e-6);
        std::array::from_fn(|i| linear(out[i] + (rgb[i] - average[i] / alpha) * punch * 0.7))
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Vivid input"
        );
        Ok(self.finite_signal(
            rgb,
            neighbors.map(|n| {
                let rgb = [n[0], n[1], n[2]].map(display);
                [rgb[0] * n[3], rgb[1] * n[3], rgb[2] * n[3], n[3]]
            }),
        ))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_original_rgba_neighborhood::<25>(
            &mut |p, actual_alpha| {
                let rgb = concat_working::straight_half(p).map(display);
                let a = half::f16::from_f32(actual_alpha).to_f32();
                [rgb[0] * a, rgb[1] * a, rgb[2] * a, a]
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
