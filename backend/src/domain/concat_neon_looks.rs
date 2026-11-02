//! Independently authored Concat Neon and Pink Fuzz alpha-aware display halation.
use super::{
    concat_chroma::{display, linear},
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
fn saturation(c: [f32; 3], amount: f32) -> [f32; 3] {
    let y = luma(c);
    c.map(|v| y * (1. - amount) + v * amount)
}
fn split(c: [f32; 3], shadow: [f32; 3], highlight: [f32; 3], amount: f32) -> [f32; 3] {
    let y = luma(c);
    let low = 1. - smooth(0., 0.6, y);
    let high = smooth(0.4, 1., y);
    std::array::from_fn(|i| c[i] + (shadow[i] * low + highlight[i] * high) * amount)
}
pub(crate) fn premult(c: [f32; 3], a: f32) -> [f32; 4] {
    let c = c.map(display);
    [c[0] * a, c[1] * a, c[2] * a, a]
}
pub(crate) fn halo_display(
    out: [f32; 3],
    neighbors: [[f32; 4]; 25],
    threshold: f32,
    tint: [f32; 3],
    amount: f32,
) -> [f32; 3] {
    let mut sum = [0.; 3];
    for n in neighbors {
        let a = n[3].max(1e-6);
        let bright = smooth(threshold, 1., luma([n[0] / a, n[1] / a, n[2] / a]));
        for i in 0..3 {
            sum[i] += n[i] * bright;
        }
    }
    std::array::from_fn(|i| {
        let glow = (sum[i] / 25. * tint[i] * amount).max(0.);
        let base = out[i].min(1.);
        1. - (1. - base) * (1. - glow) + (out[i] - base)
    })
}
pub(crate) fn halo(
    out: [f32; 3],
    neighbors: [[f32; 4]; 25],
    threshold: f32,
    tint: [f32; 3],
    amount: f32,
) -> [f32; 3] {
    halo_display(out, neighbors, threshold, tint, amount).map(linear)
}
fn neon(c: [f32; 3], neighbors: [[f32; 4]; 25], g: f32, p: f32) -> [f32; 3] {
    let out = split(c.map(display), [0.18, -0.1, 0.18], [-0.1, 0.1, 0.15], g)
        .map(|v| (v - 0.5) * (1. + p * 0.5) + 0.5);
    halo(
        saturation(out, 1. + p * 0.6),
        neighbors,
        0.75,
        [1.; 3],
        g * 0.4,
    )
}
fn pink(c: [f32; 3], neighbors: [[f32; 4]; 25], h: f32, b: f32) -> [f32; 3] {
    let black = h * 0.1;
    let out = c
        .map(display)
        .map(|v| v * (1. - black) + black)
        .map(|v| (v - 0.5) * (1. - h * 0.2) + 0.5);
    let out = saturation(out, 1. - h * 0.15);
    halo(
        split(out, [0.; 3], [0.1, -0.04, 0.08], b),
        neighbors,
        0.5,
        [1., 0.85, 0.95],
        h * 0.55,
    )
}
macro_rules! halo_look {
    ($name:ident,$first:ident,$second:ident,$step:expr,$grade:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name {
            pub $first: f32,
            pub $second: f32,
        }
        impl Default for $name {
            fn default() -> Self {
                Self {
                    $first: 60.,
                    $second: 50.,
                }
            }
        }
        impl $name {
            pub fn validate(&self) -> Result<()> {
                ensure!(
                    [self.$first, self.$second]
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
                    "Concat halation parameter out of range"
                );
                Ok(())
            }
            pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
                self.validate()?;
                ensure!(
                    rgb.iter()
                        .chain(neighbors.iter().flatten())
                        .all(|v| v.is_finite()),
                    "nonfinite Concat halation input"
                );
                Ok($grade(
                    rgb,
                    neighbors.map(|n| premult([n[0], n[1], n[2]], n[3])),
                    self.$first / 100.,
                    self.$second / 100.,
                ))
            }
            pub fn apply_linear(
                &self,
                frame: &mut LinearColorFrame,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<()> {
                self.validate()?;
                let a = self.$first / 100.;
                let b = self.$second / 100.;
                frame.map_original_rgba_neighborhood_step::<25>(
                    $step,
                    &mut |p, alpha| {
                        premult(
                            concat_working::straight_half(p),
                            half::f16::from_f32(alpha).to_f32(),
                        )
                    },
                    &mut |p, n, _, _| {
                        concat_working::store_half(
                            p,
                            $grade(concat_working::straight_half(p), n, a, b),
                        )
                    },
                    checkpoint,
                )
            }
        }
    };
}
halo_look!(Neon, glow, punch, 5, neon);
halo_look!(PinkFuzz, haze, blush, 6, pink);
