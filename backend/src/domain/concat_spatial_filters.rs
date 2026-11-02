//! Independently authored scene-linear Concat Sharpen/Vignette package behavior.
use super::{
    concat_chroma::{display, linear},
    concat_working,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sharpen {
    pub amount: f32,
}
impl Default for Sharpen {
    fn default() -> Self {
        Self { amount: 1. }
    }
}
struct PreparedSharpen {
    amount: f32,
    weights: [f32; 25],
    total: f32,
}
impl PreparedSharpen {
    fn display_signal(&self, neighbors: [[f32; 3]; 25]) -> [f32; 3] {
        let mut blur = [0.; 3];
        for (n, w) in neighbors.iter().zip(self.weights) {
            for i in 0..3 {
                blur[i] += n[i] * w;
            }
        }
        std::array::from_fn(|i| {
            linear(neighbors[12][i] + (neighbors[12][i] - blur[i] / self.total) * self.amount)
        })
    }
}
impl Sharpen {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0.2..=3.).contains(&self.amount),
            "Concat Sharpen amount out of range"
        );
        Ok(())
    }
    fn prepared(&self) -> Result<PreparedSharpen> {
        self.validate()?;
        let weights = std::array::from_fn(|n| {
            let x = n as i32 % 5 - 2;
            let y = n as i32 / 5 - 2;
            (-(x * x + y * y) as f32 / 2.).exp()
        });
        let total = weights.iter().sum();
        Ok(PreparedSharpen {
            amount: self.amount,
            weights,
            total,
        })
    }
    pub fn signal(&self, original: [[f32; 3]; 25]) -> Result<[f32; 3]> {
        ensure!(
            original.iter().flatten().all(|v| v.is_finite()),
            "nonfinite sharpening input"
        );
        Ok(self
            .prepared()?
            .display_signal(original.map(|p| p.map(display))))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let prepared = self.prepared()?;
        frame.map_original_rgb_neighborhood(
            &mut |p| concat_working::straight_half(p).map(display),
            &mut |p, neighbors: [[f32; 3]; 25], _, _| {
                if p[3] <= 0. {
                    p
                } else {
                    concat_working::store_half(p, prepared.display_signal(neighbors))
                }
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vignette {
    pub strength: f32,
}
impl Default for Vignette {
    fn default() -> Self {
        Self { strength: 50. }
    }
}
impl Vignette {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.strength.is_finite() && (10. ..=100.).contains(&self.strength),
            "Concat Vignette strength out of range"
        );
        Ok(())
    }
    fn attenuation(&self, uv: [f32; 2]) -> f32 {
        let s = self.strength / 100.;
        let r = ((uv[0] - 0.5) * 2.).hypot((uv[1] - 0.5) * 2.);
        let t = ((r - (1.4 - s * 0.9)) / ((1.6 - s * 0.3) - (1.4 - s * 0.9))).clamp(0., 1.);
        let fall = t * t * (3. - 2. * t);
        (1. - fall * (0.4 + 0.6 * s)).powf(2.4)
    }
    pub fn signal(&self, rgb: [f32; 3], uv: [f32; 2]) -> Result<[f32; 3]> {
        self.validate()?;
        ensure!(
            rgb.into_iter().chain(uv).all(f32::is_finite),
            "nonfinite vignette input"
        );
        Ok(rgb.map(|v| v * self.attenuation(uv)))
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
                let gain =
                    self.attenuation([(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32]);
                rgb.map(|v| v * gain)
            },
            checkpoint,
        )
    }
}
