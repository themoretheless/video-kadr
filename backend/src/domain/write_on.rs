//! Filmcraft Write-on history raster; MIT: docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteOn {
    pub brush: [Option<f64>; 2],
    pub color: [f32; 4],
    pub size: f32,
    pub hardness: f32,
    pub opacity: f32,
    pub stroke_length: f32,
    pub spacing: f32,
    pub style: u32,
}
impl Default for WriteOn {
    fn default() -> Self {
        Self {
            brush: [None, None],
            color: [1.; 4],
            size: 8.,
            hardness: 75.,
            opacity: 100.,
            stroke_length: 0.,
            spacing: 0.01,
            style: 0,
        }
    }
}
impl WriteOn {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.brush.iter().flatten().all(|v| v.is_finite())
                && self
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "invalid Write-on point/color"
        );
        for (value, lo, hi) in [
            (self.size, 0., 500.),
            (self.hardness, 0., 100.),
            (self.opacity, 0., 100.),
            (self.stroke_length, 0., 3600.),
            (self.spacing, 0.001, 1.),
        ] {
            ensure!(
                value.is_finite() && (lo..=hi).contains(&value),
                "invalid Write-on scalar"
            );
        }
        ensure!(self.style <= 2, "invalid Write-on style");
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        tick: filmcraft_time::Tick,
        seconds: f64,
        scale: f32,
        history: &mut dyn FnMut(filmcraft_time::Tick) -> Result<[Option<f64>; 2]>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            seconds.is_finite() && scale.is_finite() && scale > 0.,
            "invalid Write-on clock/scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let r = (self.size * scale / 2.).max(0.5);
        let hard = self.hardness / 100.;
        let op = self.opacity / 100.;
        let spacing = (self.spacing as f64).max(0.001);
        let elapsed = seconds.max(0.);
        let n = ((elapsed / spacing).floor() as usize).min(20000);
        let first = if self.stroke_length > 0. {
            ((elapsed - self.stroke_length as f64) / spacing)
                .ceil()
                .max(0.) as usize
        } else {
            0
        };
        let mut coverage = vec![0f32; w * h];
        // Rasterize each stamp in original order; avoid retaining up to forty
        // million interpolated points before touching the fixed-size matte.
        let mut stamp = |(sx, sy): (f32, f32)| -> Result<()> {
            checkpoint()?;
            let (x0, x1) = (
                (sx - r - 1.).floor().max(0.) as usize,
                ((sx + r + 1.).ceil().max(0.) as usize).min(w),
            );
            let (y0, y1) = (
                (sy - r - 1.).floor().max(0.) as usize,
                ((sy + r + 1.).ceil().max(0.) as usize).min(h),
            );
            for y in y0..y1 {
                checkpoint()?;
                for x in x0..x1 {
                    let d = ((x as f32 + 0.5 - sx).powi(2) + (y as f32 + 0.5 - sy).powi(2)).sqrt();
                    let t = ((d - (r * hard - 0.5)) / ((r + 0.5) - (r * hard - 0.5)).max(1e-6))
                        .clamp(0., 1.);
                    let a = 1. - t * t * (3. - 2. * t);
                    let i = y * w + x;
                    if a > coverage[i] {
                        coverage[i] = a;
                    }
                }
            }
            Ok(())
        };
        let mut previous: Option<(f32, f32)> = None;
        for i in first..=n {
            let mt = tick - filmcraft_time::Tick::from_seconds_f64(elapsed - i as f64 * spacing);
            let v = history(mt)?;
            ensure!(
                v.iter().flatten().all(|v| v.is_finite()),
                "invalid Write-on history point"
            );
            let p = (
                v[0].map_or(w as f64 / 2., |v| v * scale as f64) as f32,
                v[1].map_or(h as f64 / 2., |v| v * scale as f64) as f32,
            );
            if let Some(q) = previous {
                let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                let steps = (d / (r * 0.5)).ceil() as usize;
                for s in 1..steps.min(2000) {
                    let t = s as f32 / steps as f32;
                    stamp((q.0 + (p.0 - q.0) * t, q.1 + (p.1 - q.1) * t))?;
                }
            }
            stamp(p)?;
            previous = Some(p);
        }
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let col = [
            linear(self.color[0]),
            linear(self.color[1]),
            linear(self.color[2]),
        ];
        let mut output = frame.clone();
        let mut index = 0;
        output.map_premultiplied(
            &mut |mut p| {
                let k = coverage[index] * op;
                index += 1;
                match self.style {
                    1 => [col[0] * k, col[1] * k, col[2] * k, k],
                    2 => p.map(|v| v * k),
                    _ => {
                        let src = [col[0] * k, col[1] * k, col[2] * k, k];
                        for c in 0..4 {
                            p[c] = src[c] + p[c] * (1. - src[3]);
                        }
                        p
                    }
                }
            },
            checkpoint,
        )?;
        *frame = output;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: WriteOn,
        keys: Vec<(i64, [f64; 2])>,
        hold: bool,
        seconds: f64,
        pixel_scale: f32,
        bits: Vec<u32>,
    }
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[test]
    fn original_write_on_history_working_bits() {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-write-on.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 288);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            let keys: Vec<_> = c
                .keys
                .iter()
                .map(|(t, p)| (filmcraft_time::Tick::from_units(*t, 1000000), *p))
                .collect();
            let mut history = |tick: filmcraft_time::Tick| -> Result<[Option<f64>; 2]> {
                if keys.is_empty() {
                    return Ok(c.parameters.brush);
                }
                let value = if tick <= keys[0].0 {
                    keys[0].1
                } else if tick >= keys.last().unwrap().0 {
                    keys.last().unwrap().1
                } else {
                    let i = keys.partition_point(|k| k.0 <= tick) - 1;
                    let (a, b) = (keys[i], keys[i + 1]);
                    let t = if c.hold {
                        0.
                    } else {
                        (tick - a.0).0 as f64 / (b.0 - a.0).0 as f64
                    };
                    [
                        a.1[0] + (b.1[0] - a.1[0]) * t,
                        a.1[1] + (b.1[1] - a.1[1]) * t,
                    ]
                };
                Ok(value.map(Some))
            };
            c.parameters
                .apply_linear(
                    &mut frame,
                    filmcraft_time::Tick::from_seconds_f64(c.seconds),
                    c.seconds,
                    c.pixel_scale,
                    &mut history,
                    &mut || Ok(()),
                )
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Write-on case {index}"
            );
        }
    }
}
