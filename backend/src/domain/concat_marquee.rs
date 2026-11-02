//! Independently authored Concat Marquee protected light enrichment and original-source halo.
use super::{
    concat_chroma::{display, skin},
    concat_neon_looks::{halo, premult},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Marquee {
    pub lights: f32,
    pub depth: f32,
}
impl Default for Marquee {
    fn default() -> Self {
        Self {
            lights: 60.,
            depth: 40.,
        }
    }
}
fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn grade(rgb: [f32; 3], neighbors: [[f32; 4]; 25], lights: f32, depth: f32) -> [f32; 3] {
    let c = rgb.map(display);
    let mut out = c.map(|v| {
        let b = v.clamp(0., 1.);
        let m = depth * 0.8 * (1. - b);
        b * (1. - m) + b * b * m + (v - b)
    });
    let shadows = 1. - smooth(0., 0.6, luma(out));
    out[2] += 0.1 * shadows * depth;
    let chroma = c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
    let lit = smooth(0.4, 1., luma(c)) * smooth(0.15, 0.5, chroma) * (1. - skin(c)) * lights;
    let y = luma(out);
    out = out.map(|v| v * (1. - lit) + (y * (1. - 1.6) + v * 1.6) * lit);
    halo(out, neighbors, 0.7, [1.; 3], lights * 0.5)
}
impl Marquee {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.lights, self.depth]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0).contains(v)),
            "Concat Marquee out of range"
        );
        Ok(())
    }
    pub fn signal(&self, rgb: [f32; 3], neighbors: [[f32; 4]; 25]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.iter()
                .chain(neighbors.iter().flatten())
                .all(|v| v.is_finite()),
            "nonfinite Marquee input"
        );
        Ok(grade(
            rgb,
            neighbors.map(|n| premult([n[0], n[1], n[2]], n[3])),
            self.lights / 100.,
            self.depth / 100.,
        ))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let (l, d) = (self.lights / 100., self.depth / 100.);
        frame.map_original_rgba_neighborhood_step::<25>(
            4,
            &mut |p, alpha| {
                premult(
                    concat_working::straight_half(p),
                    half::f16::from_f32(alpha).to_f32(),
                )
            },
            &mut |p, n, _, _| {
                concat_working::store_half(p, grade(concat_working::straight_half(p), n, l, d))
            },
            checkpoint,
        )
    }
}
