//! Independently authored Concat Film and Digicam original-neighborhood grades with timed grain.
use super::{
    concat_chroma::{display, linear},
    concat_neon_looks::{halo_display, premult},
    concat_super8::grain_noise,
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn film(rgb: [f32; 3], n: [[f32; 4]; 25], grain: f32, warmth: f32, noise: f32) -> [f32; 3] {
    let mut out = rgb.map(display).map(|v| {
        let b = v.clamp(0., 1.);
        let m = 0.6 * (1. - b);
        let t = b * (1. - m) + b * b * m;
        let m = 0.6 * t;
        (t * (1. - m) + (1. - (1. - t) * (1. - t)) * m + (v - b)) * (0.97 - 0.03) + 0.03
    });
    let middle = 1. - ((luma(out) - 0.5).abs() * 2.).min(1.);
    let tint = [0.06, 0., -0.06];
    out = std::array::from_fn(|i| out[i] + tint[i] * middle * warmth);
    halo_display(out, n, 0.7, [1., 0.6, 0.3], 0.15 + warmth * 0.1)
        .map(|v| linear(v + noise * (grain * 0.12)))
}
fn digicam(
    rgb: [f32; 3],
    neighbors: [[f32; 4]; 25],
    flash: f32,
    noise: f32,
    random: f32,
) -> [f32; 3] {
    let c = rgb.map(display);
    let bright = smooth(0.4, 1., luma(c)) * flash * 0.35;
    let out = c.map(|v| v + bright);
    let y = luma(out);
    let saturation = 1. + flash * 0.3;
    let mut out = out
        .map(|v| y * (1. - saturation) + v * saturation)
        .map(|v| (v - 0.5) * (1. + flash * 0.15) + 0.5);
    let high = smooth(0.4, 1., luma(out));
    let tint = [-0.02, 0.04, 0.06];
    out = std::array::from_fn(|i| out[i] + tint[i] * high * flash);
    let mut sum = [0.; 4];
    for n in neighbors {
        for i in 0..4 {
            sum[i] += n[i];
        }
    }
    let alpha = sum[3].max(1e-6);
    std::array::from_fn(|i| {
        linear(out[i] + (c[i] - sum[i] / alpha) * (0.8 + flash * 0.8) + random * (noise * 0.14))
    })
}
macro_rules! camera_look {
    ($name:ident,$first:ident,$second:ident,$d1:expr,$d2:expr,$step:expr,$grade:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub $first: f32,
            pub $second: f32,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    $first: $d1,
                    $second: $d2,
                }
            }
        }
        impl $name {
            pub fn validate(&self) -> Result<()> {
                ensure!(
                    [self.$first, self.$second]
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
                    "Concat camera-look parameter out of range"
                );
                Ok(())
            }
            pub fn signal(
                &self,
                rgb: [f32; 3],
                neighbors: [[f32; 4]; 25],
                x: usize,
                y: usize,
                seconds: f32,
            ) -> Result<[f32; 3]> {
                self.validate()?;
                ensure!(
                    rgb.iter()
                        .chain(neighbors.iter().flatten())
                        .all(|v| v.is_finite())
                        && seconds.is_finite()
                        && (seconds * 7.31).is_finite(),
                    "nonfinite Concat camera-look input"
                );
                Ok($grade(
                    rgb,
                    neighbors.map(|n| premult([n[0], n[1], n[2]], n[3])),
                    self.$first / 100.,
                    self.$second / 100.,
                    grain_noise(x, y, seconds),
                ))
            }
            pub fn apply_linear(
                &self,
                frame: &mut LinearColorFrame,
                seconds: f64,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<()> {
                self.validate()?;
                let seconds = seconds as f32;
                ensure!(
                    seconds.is_finite() && (seconds * 7.31).is_finite(),
                    "invalid Concat camera-look clock"
                );
                let (a, b) = (self.$first / 100., self.$second / 100.);
                frame.map_original_rgba_neighborhood_step::<25>(
                    $step,
                    &mut |p, alpha| {
                        premult(
                            concat_working::straight_half(p),
                            half::f16::from_f32(alpha).to_f32(),
                        )
                    },
                    &mut |p, n, x, y| {
                        concat_working::store_half(
                            p,
                            $grade(
                                concat_working::straight_half(p),
                                n,
                                a,
                                b,
                                grain_noise(x, y, seconds),
                            ),
                        )
                    },
                    checkpoint,
                )
            }
        }
    };
}
camera_look!(Film, grain, warmth, 40., 50., 4, film);
camera_look!(Digicam, flash, noise, 60., 40., 1, digicam);
