//! FilmCraft monotone RGB tone curves, adapted from crates/render/src/effects.rs.
//! Upstream 5231852443363f001c3f6b396dd9b1e6461ae2be, MIT license in docs/licenses/filmcraft-MIT.txt.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToneCurves {
    #[serde(default)]
    pub master: Vec<[f32; 2]>,
    #[serde(default)]
    pub red: Vec<[f32; 2]>,
    #[serde(default)]
    pub green: Vec<[f32; 2]>,
    #[serde(default)]
    pub blue: Vec<[f32; 2]>,
    #[serde(default)]
    pub hue_vs_sat: Vec<[f32; 2]>,
    #[serde(default)]
    pub hue_vs_hue: Vec<[f32; 2]>,
    #[serde(default)]
    pub hue_vs_luma: Vec<[f32; 2]>,
    #[serde(default)]
    pub luma_vs_sat: Vec<[f32; 2]>,
    #[serde(default)]
    pub sat_vs_sat: Vec<[f32; 2]>,
}
impl ToneCurves {
    pub(crate) fn has_signal_adjustment(&self) -> bool {
        [&self.master, &self.red, &self.green, &self.blue]
            .into_iter()
            .any(|points| {
                points
                    .iter()
                    .any(|point| (point[0] - point[1]).abs() >= 1e-4)
            })
            || [
                &self.hue_vs_sat,
                &self.hue_vs_hue,
                &self.hue_vs_luma,
                &self.luma_vs_sat,
                &self.sat_vs_sat,
            ]
            .into_iter()
            .any(|points| !points.is_empty())
    }
    pub fn validate(&self) -> Result<()> {
        for points in [
            &self.master,
            &self.red,
            &self.green,
            &self.blue,
            &self.hue_vs_sat,
            &self.hue_vs_hue,
            &self.hue_vs_luma,
            &self.luma_vs_sat,
            &self.sat_vs_sat,
        ] {
            ensure!(points.len() <= 256, "tone curve exceeds point budget");
            ensure!(
                points
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid tone curve point"
            );
        }
        Ok(())
    }
    /// SDR display-encoded RGBA8 bridge; master curve precedes channel curves.
    pub fn apply(
        &self,
        surface: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        surface.validate()?;
        checkpoint()?;
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(surface, checkpoint)?;
        self.apply_linear(&mut frame, checkpoint)?;
        *surface = frame.to_surface(checkpoint)?;
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let signal = self.prepare_signal()?;
        frame.map_signal(&mut |v, _, _| Ok(signal(v)), checkpoint)
    }
    pub(crate) fn prepare_signal(&self) -> Result<impl Fn([f32; 3]) -> [f32; 3]> {
        self.validate()?;
        let master = curve_lut(&self.master, 1024)?;
        let channels = [
            curve_lut(&self.red, 1024)?,
            curve_lut(&self.green, 1024)?,
            curve_lut(&self.blue, 1024)?,
        ];
        let secondary = [
            periodic_curve_lut(&self.hue_vs_sat, 1024)?,
            periodic_curve_lut(&self.hue_vs_hue, 1024)?,
            periodic_curve_lut(&self.hue_vs_luma, 1024)?,
            periodic_curve_lut(&self.luma_vs_sat, 1024)?,
            periodic_curve_lut(&self.sat_vs_sat, 1024)?,
        ];
        let active = [&self.master, &self.red, &self.green, &self.blue]
            .map(|p| p.iter().any(|q| (q[0] - q[1]).abs() >= 1e-4));
        let sample = |lut: &[f32], x: f32| {
            let p = x.clamp(0.0, 1.0) * (lut.len() - 1) as f32;
            let i = p.floor() as usize;
            let j = (i + 1).min(lut.len() - 1);
            lut[i] + (lut[j] - lut[i]) * (p - i as f32)
        };
        Ok(move |rgb: [f32; 3]| {
            let mut v = rgb;
            for c in 0..3 {
                if active[0] {
                    v[c] = sample(&master, v[c]);
                }
                if active[c + 1] {
                    v[c] = sample(&channels[c], v[c]);
                }
            }
            if secondary.iter().any(Option::is_some) {
                let unit = v.map(|q| q.clamp(0.0, 1.0));
                let mut h = rgb_to_hsl(unit[0], unit[1], unit[2]);
                let [h0, s0, l0] = h;
                if let Some(t) = &secondary[1] {
                    h[0] = (h[0] + (sample(t, h0) - 0.5)).rem_euclid(1.0);
                }
                let mut sm = 1.0;
                for (index, x) in [(0, h0), (3, l0), (4, s0)] {
                    if let Some(t) = &secondary[index] {
                        sm *= sample(t, x) * 2.0;
                    }
                }
                h[1] = (h[1] * sm).clamp(0.0, 1.0);
                if let Some(t) = &secondary[2] {
                    h[2] = (h[2] + (sample(t, h0) - 0.5) * 0.5).clamp(0.0, 1.0);
                }
                v = hsl_to_rgb(h[0], h[1], h[2]);
            }
            v
        })
    }
}
pub fn curve_lut(points: &[[f32; 2]], n: usize) -> Result<Vec<f32>> {
    ensure!((2..=4096).contains(&n), "invalid tone LUT size");
    ensure!(
        points.len() <= 256
            && points
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
        "invalid tone curve points"
    );
    Ok(curve_lut_unchecked(points, n))
}
/// FilmCraft repeats all five secondary curves around one period, including
/// luma/saturation inputs. Preserve that source contract rather than RGB identity.
pub fn periodic_curve_lut(points: &[[f32; 2]], n: usize) -> Result<Option<Vec<f32>>> {
    curve_lut(points, n)?;
    if points.is_empty() {
        return Ok(None);
    }
    let mut ext = Vec::with_capacity(points.len() * 3);
    for off in [-1.0f32, 0.0, 1.0] {
        for p in points {
            ext.push([(p[0] + off + 1.0) / 3.0, p[1]]);
        }
    }
    let lut = curve_lut_unchecked(&ext, n * 3);
    Ok(Some(lut[n..2 * n].to_vec()))
}

fn curve_lut_unchecked(points: &[[f32; 2]], n: usize) -> Vec<f32> {
    let mut pts: Vec<[f32; 2]> = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-6);
    if pts.len() < 2 {
        let y = pts.first().map_or(0.0, |p| p[1]);
        return if pts.is_empty() {
            (0..n).map(|i| i as f32 / (n - 1) as f32).collect()
        } else {
            vec![y; n]
        };
    }
    let m = pts.len();
    let d: Vec<f32> = (0..m - 1)
        .map(|i| (pts[i + 1][1] - pts[i][1]) / (pts[i + 1][0] - pts[i][0]).max(1e-6))
        .collect();
    let mut t = vec![0f32; m];
    t[0] = d[0];
    t[m - 1] = d[m - 2];
    for i in 1..m - 1 {
        t[i] = if d[i - 1] * d[i] <= 0.0 {
            0.0
        } else {
            (d[i - 1] + d[i]) / 2.0
        };
    }
    for i in 0..m - 1 {
        if d[i].abs() < 1e-9 {
            t[i] = 0.0;
            t[i + 1] = 0.0;
            continue;
        }
        let a = t[i] / d[i];
        let b = t[i + 1] / d[i];
        let h = a * a + b * b;
        if h > 9.0 {
            let k = 3.0 / h.sqrt();
            t[i] = k * a * d[i];
            t[i + 1] = k * b * d[i];
        }
    }
    (0..n)
        .map(|j| {
            let x = j as f32 / (n - 1) as f32;
            if x <= pts[0][0] {
                return pts[0][1];
            }
            if x >= pts[m - 1][0] {
                return pts[m - 1][1];
            }
            let i = pts.partition_point(|p| p[0] <= x) - 1;
            let hh = pts[i + 1][0] - pts[i][0];
            let u = (x - pts[i][0]) / hh;
            let (h00, h10, h01, h11) = (
                2.0 * u * u * u - 3.0 * u * u + 1.0,
                u * u * u - 2.0 * u * u + u,
                -2.0 * u * u * u + 3.0 * u * u,
                u * u * u - u * u,
            );
            h00 * pts[i][1] + h10 * hh * t[i] + h01 * pts[i + 1][1] + h11 * hh * t[i + 1]
        })
        .collect()
}

pub(crate) fn rgb_to_hsl(r: f32, g: f32, b: f32) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-7 {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_actual_filmcraft_sdr_curves_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-tone-curves.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 20);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: ToneCurves = serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: f["width"].as_u64().unwrap() as usize,
                height: f["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            p.apply(&mut frame, &mut || Ok(())).unwrap();
            for (pixel, (actual, expected)) in frame
                .rgba
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(actual[3], expected[3]);
                if actual[3] == 0 {
                    assert_eq!(actual, &input[pixel * 4..pixel * 4 + 4]);
                    continue;
                }
                for c in 0..3 {
                    assert!(
                        actual[c].abs_diff(expected[c]) <= 1,
                        "case {index} pixel {pixel} channel {c}: {} != {}",
                        actual[c],
                        expected[c]
                    );
                }
            }
        }
    }
    #[test]
    fn secondary_curves_preserve_neutral_and_apply_all_five_operations() {
        let source = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 73, 128, 128, 128, 0],
        };
        let neutral = vec![[0.0, 0.5], [0.5, 0.5]];
        let mut image = source.clone();
        ToneCurves {
            hue_vs_sat: neutral.clone(),
            hue_vs_hue: neutral.clone(),
            hue_vs_luma: neutral.clone(),
            luma_vs_sat: neutral.clone(),
            sat_vs_sat: neutral,
            ..Default::default()
        }
        .apply(&mut image, &mut || Ok(()))
        .unwrap();
        assert_eq!(image.rgba, source.rgba);
        for index in 0..5 {
            let mut curves = ToneCurves::default();
            let point = vec![[
                0.0,
                if index == 1 {
                    5.0 / 6.0
                } else if index == 2 {
                    1.0
                } else {
                    0.0
                },
            ]];
            match index {
                0 => curves.hue_vs_sat = point,
                1 => curves.hue_vs_hue = point,
                2 => curves.hue_vs_luma = point,
                3 => curves.luma_vs_sat = point,
                _ => curves.sat_vs_sat = point,
            }
            let mut image = source.clone();
            curves.apply(&mut image, &mut || Ok(())).unwrap();
            let expected = match index {
                1 => [0, 255, 0, 73],
                2 => [255, 128, 128, 73],
                _ => [128, 128, 128, 73],
            };
            assert_eq!(&image.rgba[..4], &expected, "secondary curve {index}");
            assert_eq!(image.rgba[7], 0);
        }
    }
    #[test]
    fn periodic_sampling_matches_filmcraft_and_is_bounded_and_cancellable() {
        let points = [[0.1, 0.2], [0.4, 0.8], [0.9, 0.4]];
        let lut = periodic_curve_lut(&points, 1024).unwrap().unwrap();
        let ext: Vec<_> = [-1.0, 0.0, 1.0]
            .into_iter()
            .flat_map(|off| points.map(|p| [(p[0] + off + 1.0) / 3.0, p[1]]))
            .collect();
        assert_eq!(lut, curve_lut(&ext, 3072).unwrap()[1024..2048]);
        assert!((lut[0] - lut[1023]).abs() < 0.003);
        assert!(periodic_curve_lut(&[[f32::NAN, 0.5]], 1024).is_err());
        assert!(periodic_curve_lut(&[], 4097).is_err());
        let curves = ToneCurves {
            sat_vs_sat: vec![[0.0, 2.0]],
            ..Default::default()
        };
        assert!(curves.validate().is_err());
        let mut image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        assert!(ToneCurves::default()
            .apply(&mut image, &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn identity_monotonic_plateaus_and_hostile_inputs() {
        let identity = curve_lut(&[], 256).unwrap();
        for (i, v) in identity.iter().enumerate() {
            assert!((*v - i as f32 / 255.0).abs() < 1e-6);
        }
        let lut = curve_lut(&[[1.0, 1.0], [0.0, 0.0], [0.2, 0.8], [0.8, 0.8]], 4096).unwrap();
        assert!(lut.windows(2).all(|w| w[1] >= w[0] - 1e-6));
        assert!(lut.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(curve_lut(&[], 0).is_err());
        assert!(curve_lut(&[[f32::NAN, 0.0]], 256).is_err());
        assert!(curve_lut(&vec![[0.0, 0.0]; 257], 256).is_err());
    }
    #[test]
    fn master_then_channel_and_alpha_preservation() {
        let curves = ToneCurves {
            master: vec![[0.0, 0.0], [1.0, 0.5]],
            red: vec![[0.0, 0.0], [1.0, 0.5]],
            ..Default::default()
        };
        let mut surface = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 128],
        };
        curves.apply(&mut surface, &mut || Ok(())).unwrap();
        assert_eq!(surface.rgba, [64, 128, 128, 128]);
    }
}
