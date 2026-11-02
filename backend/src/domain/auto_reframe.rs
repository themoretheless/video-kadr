//! Filmcraft Auto Reframe saliency analysis; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::Result;
/// Gradient-energy centroid in normalized layer coordinates. Uses premultiplied
/// working RGB, source display-luma conversion, centre bias and f64 accumulation.
pub fn saliency_centroid(
    image: &LinearColorFrame,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<(f32, f32)> {
    let (w, h) = image.dimensions();
    let mut gray = Vec::with_capacity(w * h);
    for row in image.pixels().chunks(w) {
        checkpoint()?;
        for p in row {
            let luma = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
            gray.push(super::basic_grade::display(luma.clamp(0., 1.)));
        }
    }
    let (mut sx, mut sy, mut sw) = (0f64, 0f64, 0f64);
    for y in 1..h.saturating_sub(1) {
        checkpoint()?;
        for x in 1..w.saturating_sub(1) {
            let gx = gray[y * w + x + 1] - gray[y * w + x - 1];
            let gy = gray[(y + 1) * w + x] - gray[(y - 1) * w + x];
            let (u, v) = (x as f32 / w as f32 - 0.5, y as f32 / h as f32 - 0.5);
            let bias = 1. - (u * u + v * v) * 0.8;
            let e = ((gx * gx + gy * gy) * bias.max(0.1)) as f64;
            let e = e * e;
            sx += e * x as f64;
            sy += e * y as f64;
            sw += e;
        }
    }
    Ok(if sw <= 1e-12 {
        (0.5, 0.5)
    } else {
        (
            ((sx / sw + 0.5) / w as f64) as f32,
            ((sy / sw + 0.5) / h as f64) as f32,
        )
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn auto_reframe_centroid_uniform_and_thin_fallback() {
        for (w, h) in [(1, 7), (7, 1), (6, 4)] {
            let image = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: w,
                    height: h,
                    rgba: [64, 128, 192, 128].repeat(w * h),
                },
                &mut || Ok(()),
            )
            .unwrap();
            assert_eq!(
                saliency_centroid(&image, &mut || Ok(())).unwrap(),
                (0.5, 0.5)
            );
        }
    }
    #[test]
    fn auto_reframe_centroid_tracks_offcentre_energy() {
        let mut surface = GroupSurface::transparent(12, 10).unwrap();
        surface.rgba[(3 * 12 + 2) * 4..(3 * 12 + 2) * 4 + 4].copy_from_slice(&[255; 4]);
        let image = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
        let (x, y) = saliency_centroid(&image, &mut || Ok(())).unwrap();
        assert!((0.15..0.3).contains(&x));
        assert!((0.3..0.4).contains(&y));
        assert!(saliency_centroid(&image, &mut || anyhow::bail!("cancelled")).is_err());
    }
}
use anyhow::ensure;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoReframe {
    pub preset: u32,
    pub offset: [f64; 2],
    pub zoom: f32,
    pub aspect: u32,
}
impl Default for AutoReframe {
    fn default() -> Self {
        Self {
            preset: 1,
            offset: [0.; 2],
            zoom: 100.,
            aspect: 0,
        }
    }
}
impl AutoReframe {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.preset <= 2 && self.aspect <= 4,
            "Auto Reframe choice out of range"
        );
        ensure!(
            self.zoom.is_finite() && (100. ..=400.).contains(&self.zoom),
            "Auto Reframe zoom out of range"
        );
        ensure!(
            self.offset.iter().all(|v| v.is_finite()),
            "nonfinite Auto Reframe offset"
        );
        Ok(())
    }
    /// `sequence` supplies sequence width/height and original source width. `subject`
    /// is the temporally averaged normalized centre, or None for the source standalone fallback.
    pub fn apply_linear(
        &self,
        image: &mut LinearColorFrame,
        scale: f32,
        subject: Option<(f64, f64)>,
        sequence: Option<(u32, u32, u32)>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            scale.is_finite() && scale > 0.,
            "invalid Auto Reframe pixel scale"
        );
        let (w, h) = image.dimensions();
        let (lw, lh) = (w as f64, h as f64);
        let target = if self.aspect == 0 {
            let Some((sw, sh, src_w)) = sequence else {
                return Ok(());
            };
            let k = lw / src_w.max(1) as f64;
            (sw as f64 * k, sh as f64 * k)
        } else {
            let r = [16. / 9., 9. / 16., 1., 4. / 5., 16. / 9.][self.aspect as usize];
            if lw / lh > r {
                (lh * r, lh)
            } else {
                (lw, lw / r)
            }
        };
        ensure!(target.0 > 0. && target.1 > 0., "empty Auto Reframe target");
        let z = (target.0 / lw).max(target.1 / lh).max(1.) * (self.zoom as f64 / 100.).max(1.);
        let (vw, vh) = (target.0.min(lw * z) / z, target.1.min(lh * z) / z);
        let subject = match subject {
            Some(v) => v,
            None => {
                let (u, v) = saliency_centroid(&image.downsample2(checkpoint)?, checkpoint)?;
                (u as f64, v as f64)
            }
        };
        ensure!(
            subject.0.is_finite() && subject.1.is_finite(),
            "nonfinite Auto Reframe subject"
        );
        let sx = (subject.0 * lw + self.offset[0] * scale as f64).clamp(vw / 2., lw - vw / 2.);
        let sy = (subject.1 * lh + self.offset[1] * scale as f64).clamp(vh / 2., lh - vh / 2.);
        if (z - 1.).abs() < 1e-9 && (sx - lw / 2.).abs() < 1e-6 && (sy - lh / 2.).abs() < 1e-6 {
            return Ok(());
        }
        *image = image.transformed(
            w,
            h,
            super::group_transform::GroupAffine([z, 0., 0., z, lw / 2. - z * sx, lh / 2. - z * sy]),
            checkpoint,
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn auto_reframe_original_standalone_aspect_zoom_offset_frames() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-reframe.json"
        ))
        .unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 240);
        for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let parameters: AutoReframe = serde_json::from_value(c["parameters"].clone()).unwrap();
            let surface = GroupSurface {
                width: c["width"].as_u64().unwrap() as usize,
                height: c["height"].as_u64().unwrap() as usize,
                rgba: serde_json::from_value(c["rgba"].clone()).unwrap(),
            };
            let mut image = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            parameters
                .apply_linear(
                    &mut image,
                    c["scale"].as_f64().unwrap() as f32,
                    None,
                    None,
                    &mut || Ok(()),
                )
                .unwrap();
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "original Auto Reframe case{index}"
            );
        }
    }
}
/// Analysis cache for one clip/source revision. The caller owns its lifetime and
/// must replace it when the clip/source changes, preventing cross-project reuse.
#[derive(Default)]
pub struct AutoReframeAnalysis {
    centroids: std::collections::HashMap<i64, (f32, f32)>,
}
impl AutoReframeAnalysis {
    pub fn temporal_subject(
        &mut self,
        preset: u32,
        seconds: f64,
        duration: f64,
        fps: f64,
        source_width: u32,
        source_frame: &mut dyn FnMut(f64, f32) -> Result<Option<LinearColorFrame>>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<(f64, f64)> {
        ensure!(
            preset <= 2
                && seconds.is_finite()
                && duration.is_finite()
                && duration >= 0.
                && fps.is_finite(),
            "invalid Auto Reframe analysis clock"
        );
        let fps = fps.max(1.);
        let (half, step) = [(1., 0.25), (0.5, 0.125), (0.15, 0.05)][preset as usize];
        let mut k = -half;
        let mut acc = (0f64, 0f64, 0f64);
        while k <= half + 1e-9 {
            checkpoint()?;
            let t = seconds + k;
            if t >= -1e-9 && t <= duration + 1e-9 {
                let id = (t * fps).round() as i64;
                let centre = if let Some(c) = self.centroids.get(&id) {
                    Some(*c)
                } else {
                    match source_frame(
                        id as f64 / fps - seconds,
                        (160. / source_width.max(1) as f32).min(1.),
                    )? {
                        Some(f) => {
                            let c = saliency_centroid(&f, checkpoint)?;
                            if self.centroids.len() > 100000 {
                                self.centroids.clear();
                            }
                            self.centroids.insert(id, c);
                            Some(c)
                        }
                        None => None,
                    }
                };
                if let Some((u, v)) = centre {
                    let weight = 1. - (k / (half + step)).abs();
                    acc = (
                        acc.0 + u as f64 * weight,
                        acc.1 + v as f64 * weight,
                        acc.2 + weight,
                    );
                }
            }
            k += step;
        }
        Ok(if acc.2 > 0. {
            (acc.0 / acc.2, acc.1 / acc.2)
        } else {
            (0.5, 0.5)
        })
    }
}
#[cfg(test)]
mod temporal_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn auto_reframe_original_temporal_requests_and_cache() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-reframe.json"
        ))
        .unwrap();
        assert_eq!(f["temporal"].as_array().unwrap().len(), 144);
        for (index, c) in f["temporal"].as_array().unwrap().iter().enumerate() {
            let mut analysis = AutoReframeAnalysis::default();
            let mut requests = vec![];
            let available = c["available"].as_bool().unwrap();
            let mut decode = |dt: f64, scale: f32| {
                requests
                    .push(serde_json::json!({"dtBits":dt.to_bits(),"scaleBits":scale.to_bits()}));
                Ok(if available {
                    Some(LinearColorFrame::from_surface(
                        &GroupSurface::transparent(6, 4).unwrap(),
                        &mut || Ok(()),
                    )?)
                } else {
                    None
                })
            };
            let centre = analysis
                .temporal_subject(
                    c["preset"].as_u64().unwrap() as u32,
                    c["seconds"].as_f64().unwrap(),
                    1.,
                    c["fps"].as_f64().unwrap(),
                    c["width"].as_u64().unwrap() as u32,
                    &mut decode,
                    &mut || Ok(()),
                )
                .unwrap();
            assert!((centre.0 - 0.5).abs() < 1e-14 && (centre.1 - 0.5).abs() < 1e-14);
            assert_eq!(
                serde_json::to_value(&requests).unwrap(),
                c["requests"],
                "original temporal requests case{index}"
            );
            if available {
                let mut forbidden = |_, _| -> Result<Option<LinearColorFrame>> {
                    anyhow::bail!("cached frame decoded twice")
                };
                analysis
                    .temporal_subject(
                        c["preset"].as_u64().unwrap() as u32,
                        c["seconds"].as_f64().unwrap(),
                        1.,
                        c["fps"].as_f64().unwrap(),
                        c["width"].as_u64().unwrap() as u32,
                        &mut forbidden,
                        &mut || Ok(()),
                    )
                    .unwrap();
            }
        }
    }
}
#[cfg(test)]
mod moving_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    fn picture(t: u64) -> LinearColorFrame {
        let rgba = (0..180usize)
            .flat_map(|i| {
                let x = (t / 100000 % 12) as usize + 2;
                if (i % 18).abs_diff(x) <= 1 && (i / 18).abs_diff(4) <= 1 {
                    [255, 192, 64, 128]
                } else {
                    [
                        (i * 7 % 32) as u8,
                        (i * 3 % 24) as u8,
                        (i * 11 % 16) as u8,
                        128,
                    ]
                }
            })
            .collect();
        LinearColorFrame::from_surface(
            &GroupSurface {
                width: 18,
                height: 10,
                rgba,
            },
            &mut || Ok(()),
        )
        .unwrap()
    }
    #[test]
    fn auto_reframe_original_moving_temporal_subject_and_output() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-reframe.json"
        ))
        .unwrap();
        assert_eq!(f["moving"].as_array().unwrap().len(), 60);
        for (index, c) in f["moving"].as_array().unwrap().iter().enumerate() {
            let p: AutoReframe = serde_json::from_value(c["parameters"].clone()).unwrap();
            let tick = c["tick"].as_u64().unwrap();
            let seconds = tick as f64 / 1000000.;
            let mut analysis = AutoReframeAnalysis::default();
            let mut source = |dt: f64, scale: f32| {
                assert_eq!(scale, 1.);
                let t = seconds + dt;
                Ok(if (0. ..1.).contains(&t) {
                    Some(picture((t * 1000000.).round() as u64))
                } else {
                    None
                })
            };
            let subject = analysis
                .temporal_subject(
                    p.preset,
                    seconds,
                    1.,
                    29.97,
                    18,
                    &mut source,
                    &mut || Ok(()),
                )
                .unwrap();
            let mut image = picture(tick);
            p.apply_linear(
                &mut image,
                1.,
                Some(subject),
                Some((10, 16, 18)),
                &mut || Ok(()),
            )
            .unwrap();
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "original moving Auto Reframe case{index}"
            );
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoReframeChannel {
    Zoom,
    OffsetX,
    OffsetY,
}
pub type AutoReframeAnimation =
    std::collections::BTreeMap<AutoReframeChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &AutoReframeAnimation) -> Result<()> {
    for (k, v) in a {
        v.validate()?;
        let (lo, hi) = match k {
            AutoReframeChannel::Zoom => (100., 400.),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Auto Reframe animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: AutoReframe,
    a: &AutoReframeAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<AutoReframe> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (k, v) in a {
        let value = super::nested_composition::sample_value(v, t)?;
        match k {
            AutoReframeChannel::Zoom => p.zoom = value as f32,
            AutoReframeChannel::OffsetX => p.offset[0] = value,
            AutoReframeChannel::OffsetY => p.offset[1] = value,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn auto_reframe_three_curves_precise_offsets_roundtrip() {
        let mut values = serde_json::Map::new();
        for (k, a, b) in [
            ("zoom", 100., 175.),
            ("offsetX", 0., 1.123456789012),
            ("offsetY", 0., -0.75),
        ] {
            values.insert(k.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}));
        }
        let animation: AutoReframeAnimation =
            serde_json::from_value(serde_json::Value::Object(values)).unwrap();
        let p = sample_animation(AutoReframe::default(), &animation, 250000).unwrap();
        assert_eq!(p.zoom, 137.5);
        assert_eq!(p.offset, [1.123456789012 * 0.5, -0.375]);
        let p = sample_animation(AutoReframe::default(), &animation, 900000).unwrap();
        assert_eq!(p.offset, [1.123456789012, -0.75]);
        let effect = super::super::composition::VideoEffect::AutoReframe {
            parameters: AutoReframe::default(),
            animation: Some(animation),
        };
        let saved = serde_json::to_value(effect).unwrap();
        let restored: super::super::composition::VideoEffect =
            serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        for value in [99., 400.01] {
            assert!(validate_animation(&AutoReframeAnimation::from([(
                AutoReframeChannel::Zoom,
                super::super::composition::AnimatableValue::Constant { value }
            )]))
            .is_err());
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AutoReframeEnvironment {
    pub subject: (f64, f64),
    pub sequence: (u32, u32, u32),
}

pub fn analysis_decimation(have_width: usize, source_width: u32, scale: f32) -> Result<usize> {
    ensure!(
        have_width > 0 && source_width > 0 && scale.is_finite() && scale > 0.,
        "invalid analysis size"
    );
    let target = source_width as f32 * scale.clamp(1. / 64., 1.);
    let mut n = 1usize;
    while n < 16 && have_width as f32 / (n as f32 * 2.) >= target.max(1.) {
        n *= 2;
    }
    Ok(n)
}
#[cfg(test)]
mod decimation_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn auto_reframe_original_full_resolution_provider_decimation() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-reframe.json"
        ))
        .unwrap();
        assert_eq!(f["decimation"].as_array().unwrap().len(), 16);
        for (index, c) in f["decimation"].as_array().unwrap().iter().enumerate() {
            let width = c["width"].as_u64().unwrap() as usize;
            let image = LinearColorFrame::from_surface(
                &GroupSurface {
                    width,
                    height: c["height"].as_u64().unwrap() as usize,
                    rgba: serde_json::from_value(c["rgba"].clone()).unwrap(),
                },
                &mut || Ok(()),
            )
            .unwrap();
            let n = analysis_decimation(width, width as u32, c["scale"].as_f64().unwrap() as f32)
                .unwrap();
            assert_eq!(n, c["n"].as_u64().unwrap() as usize);
            let output = image.box_decimated(n, &mut || Ok(())).unwrap();
            assert_eq!(
                output.dimensions(),
                (
                    c["outputWidth"].as_u64().unwrap() as usize,
                    c["outputHeight"].as_u64().unwrap() as usize
                )
            );
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                output
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "source provider box decimation{index}"
            );
        }
    }
}
