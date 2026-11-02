//! Independently authored bounded Fritsch–Carlson display/luma curves for Concat.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Curves {
    pub luma: Vec<[f64; 2]>,
    pub red: Vec<[f64; 2]>,
    pub green: Vec<[f64; 2]>,
    pub blue: Vec<[f64; 2]>,
}
#[derive(Debug, Clone)]
pub struct PreparedCurve {
    points: Vec<[f32; 3]>,
}
impl PreparedCurve {
    pub fn new(points: &[[f64; 2]]) -> Result<Self> {
        ensure!(points.len() <= 8, "Concat curve exceeds eight points");
        ensure!(
            points
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid Concat curve point"
        );
        let mut sorted = points.to_vec();
        sorted.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut p: Vec<[f64; 2]> = Vec::new();
        for point in sorted {
            if let Some(last) = p.last_mut() {
                if (point[0] - last[0]).abs() < 1e-6 {
                    *last = point;
                    continue;
                }
            }
            p.push(point)
        }
        if p.len() < 2 {
            p = vec![[0., 0.], [1., 1.]]
        }
        let secants: Vec<f64> = p
            .windows(2)
            .map(|v| (v[1][1] - v[0][1]) / (v[1][0] - v[0][0]).max(1e-9))
            .collect();
        let mut slopes = vec![0.; p.len()];
        slopes[0] = secants[0];
        let last = slopes.len() - 1;
        slopes[last] = secants[last - 1];
        for i in 1..last {
            let (a, b) = (secants[i - 1], secants[i]);
            slopes[i] = if a * b > 0. { (a + b) * 0.5 } else { 0. };
        }
        for (i, &d) in secants.iter().enumerate() {
            if d == 0. {
                slopes[i] = 0.;
                slopes[i + 1] = 0.;
                continue;
            }
            let a = slopes[i] / d;
            let b = slopes[i + 1] / d;
            let radius = (a * a + b * b).sqrt();
            if radius > 3. {
                slopes[i] = 3. / radius * a * d;
                slopes[i + 1] = 3. / radius * b * d;
            }
        }
        Ok(Self {
            points: p
                .into_iter()
                .zip(slopes)
                .map(|([x, y], s)| [x as f32, y as f32, s as f32])
                .collect(),
        })
    }
    fn straight(&self) -> bool {
        self.points.len() == 2 && self.points[0][..2] == [0., 0.] && self.points[1][..2] == [1., 1.]
    }
    pub fn sample(&self, level: f32) -> f32 {
        let inside = level.clamp(0., 1.);
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];
        let y = if inside <= first[0] {
            first[1]
        } else if inside >= last[0] {
            last[1]
        } else {
            let i = self
                .points
                .windows(2)
                .position(|p| inside < p[1][0])
                .unwrap_or(self.points.len() - 2);
            let a = self.points[i];
            let b = self.points[i + 1];
            let h = (b[0] - a[0]).max(1e-6);
            let s = (inside - a[0]) / h;
            let s2 = s * s;
            let s3 = s2 * s;
            (2. * s3 - 3. * s2 + 1.) * a[1]
                + (s3 - 2. * s2 + s) * h * a[2]
                + (3. * s2 - 2. * s3) * b[1]
                + (s3 - s2) * h * b[2]
        };
        y + (level - inside)
    }
}
// Round the inverse transfer to f32 only after evaluating the power precisely.
// A one-half-step difference can be amplified by the following luma-ratio pass.
fn from_display(value: f32) -> f32 {
    value.signum() * (value.abs() as f64).powf(2.4) as f32
}
pub(crate) struct Prepared {
    curves: [PreparedCurve; 4],
    neutral: bool,
}
impl Prepared {
    fn signal(&self, input: [f32; 3]) -> [f32; 3] {
        if self.neutral {
            return input;
        }
        self.display(input.map(super::concat_chroma::display))
            .map(from_display)
    }
    pub(crate) fn display(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.neutral {
            return rgb;
        }
        let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
        let moved = self.curves[0].sample(y);
        let balanced = if y.abs() > 1e-4 {
            rgb.map(|v| v * (moved / y))
        } else {
            rgb.map(|v| v + (moved - y))
        };
        std::array::from_fn(|i| self.curves[i + 1].sample(balanced[i]))
    }
}
impl Curves {
    pub(crate) fn prepared(&self) -> Result<Prepared> {
        let curves = [
            PreparedCurve::new(&self.luma)?,
            PreparedCurve::new(&self.red)?,
            PreparedCurve::new(&self.green)?,
            PreparedCurve::new(&self.blue)?,
        ];
        let neutral = curves.iter().all(PreparedCurve::straight);
        Ok(Prepared { curves, neutral })
    }
    pub fn validate(&self) -> Result<()> {
        self.prepared().map(|_| ())
    }
    pub fn signal(&self, input: [f32; 3]) -> Result<[f32; 3]> {
        Ok(self.prepared()?.signal(input))
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let p = self.prepared()?;
        super::concat_working::apply_rgb(frame, |rgb| p.signal(rgb), checkpoint)
    }
}
