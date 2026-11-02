//! Filmcraft Volumetric Rays; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VolumetricRays {
    pub center: Option<[Option<f64>; 2]>,
    pub threshold: f32,
    pub length: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub only: bool,
}
impl Default for VolumetricRays {
    fn default() -> Self {
        Self {
            center: None,
            threshold: 60.,
            length: 50.,
            intensity: 100.,
            color: [1., 0.95, 0.8, 1.],
            only: false,
        }
    }
}
impl VolumetricRays {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [
            (self.threshold, 100.),
            (self.length, 100.),
            (self.intensity, 1000.),
        ] {
            ensure!(
                v.is_finite() && (0.0..=max).contains(&v),
                "Volumetric Rays parameter out of range"
            );
        }
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "Volumetric Rays color out of range"
        );
        if let Some(c) = self.center {
            ensure!(
                c.into_iter().flatten().all(f64::is_finite),
                "nonfinite ray center"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(scale.is_finite() && scale > 0., "invalid rays scale");
        let (fw, fh) = frame.dimensions();
        let c = self.center.unwrap_or([None, None]);
        let center = [
            c[0].map(|v| v * scale as f64).unwrap_or(fw as f64 * 0.5),
            c[1].map(|v| v * scale as f64).unwrap_or(fh as f64 * 0.25),
        ];
        let mut hi = frame.clone();
        let threshold = self.threshold / 100.;
        hi.map_premultiplied(
            &mut |p| {
                if p[3] <= 1e-6 {
                    return [0.; 4];
                }
                let rgb = std::array::from_fn::<f32, 3, _>(|k| {
                    super::basic_grade::display((p[k] / p[3]).max(0.))
                });
                let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let t = ((l - threshold) / ((threshold + 0.1) - threshold).max(1e-6)).clamp(0., 1.);
                p.map(|v| v * (t * t * (3. - 2. * t)))
            },
            checkpoint,
        )?;
        let mut down = 1f64;
        while hi.dimensions().0 > 960 {
            hi = hi.downsample2(checkpoint)?;
            down *= 2.;
        }
        let (cx, cy) = ((center[0] / down) as f32, (center[1] / down) as f32);
        ensure!(
            cx.is_finite() && cy.is_finite(),
            "ray center outside working range"
        );
        let src = hi.clone();
        let (w, h) = hi.dimensions();
        let len = self.length / 100.;
        let mut index = 0;
        hi.map_premultiplied(
            &mut |_| {
                let (x, y) = (index % w, index / w);
                index += 1;
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut acc = [0f32; 4];
                let mut wsum = 0.;
                for i in 0..48 {
                    let t = i as f32 / 48. * len;
                    let s = super::gradient_wipe::transparent_sample(
                        &src,
                        px + (cx - px) * t,
                        py + (cy - py) * t,
                    );
                    let weight = 1. - i as f32 / 48. * 0.6;
                    for k in 0..4 {
                        acc[k] += s[k] * weight;
                    }
                    wsum += weight;
                }
                acc.map(|v| v / wsum)
            },
            checkpoint,
        )?;
        let rays = if down > 1. {
            hi.transformed(
                fw,
                fh,
                super::group_transform::GroupAffine([
                    fw as f64 / w as f64,
                    0.,
                    0.,
                    fh as f64 / h as f64,
                    0.,
                    0.,
                ]),
                checkpoint,
            )?
        } else {
            hi
        };
        let gain = std::array::from_fn::<f32, 3, _>(|k| {
            super::basic_grade::linear(self.color[k]) * (self.intensity / 100.) * 2.5
        });
        let mut index = 0;
        frame.map_premultiplied(
            &mut |p| {
                let mut p = if self.only { [0.; 4] } else { p };
                let q = rays.pixels()[index];
                index += 1;
                let add = std::array::from_fn::<f32, 3, _>(|k| q[k] * gain[k]);
                let m = add[0].max(add[1]).max(add[2]).clamp(0., 1.);
                for k in 0..3 {
                    p[k] += add[k];
                }
                p[3] = p[3] + (1. - p[3]) * m;
                p
            },
            checkpoint,
        )
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn volumetric_rays_original_working_bits() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-volumetric-rays.json"
        ))
        .unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 896);
        for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: VolumetricRays = serde_json::from_value(c["parameters"].clone()).unwrap();
            let rgba = serde_json::from_value(c["rgba"].clone()).unwrap();
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c["width"].as_u64().unwrap() as usize,
                    height: c["height"].as_u64().unwrap() as usize,
                    rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            p.apply_linear(&mut frame, c["scale"].as_f64().unwrap() as f32, &mut || {
                Ok(())
            })
            .unwrap();
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "original VolumetricRays case{i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumetricRaysChannel {
    CenterX,
    CenterY,
    Threshold,
    Length,
    Intensity,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type VolumetricRaysAnimation =
    std::collections::BTreeMap<VolumetricRaysChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VolumetricRaysAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VolumetricRaysChannel::CenterX | VolumetricRaysChannel::CenterY => {
                (-f64::MAX, f64::MAX)
            }
            VolumetricRaysChannel::Threshold | VolumetricRaysChannel::Length => (0., 100.),
            VolumetricRaysChannel::Intensity => (0., 1000.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Volumetric Rays animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: VolumetricRays,
    a: &VolumetricRaysAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VolumetricRays> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            VolumetricRaysChannel::CenterX | VolumetricRaysChannel::CenterY => {
                p.center.get_or_insert([None, None])[if *c == VolumetricRaysChannel::CenterX {
                    0
                } else {
                    1
                }] = Some(v)
            }
            VolumetricRaysChannel::Threshold => p.threshold = v as f32,
            VolumetricRaysChannel::Length => p.length = v as f32,
            VolumetricRaysChannel::Intensity => p.intensity = v as f32,
            VolumetricRaysChannel::ColorRed => p.color[0] = v as f32,
            VolumetricRaysChannel::ColorGreen => p.color[1] = v as f32,
            VolumetricRaysChannel::ColorBlue => p.color[2] = v as f32,
            VolumetricRaysChannel::ColorAlpha => p.color[3] = v as f32,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn volumetric_rays_nine_channels_preserve_precision_midpoint_hold_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let a:VolumetricRaysAnimation=serde_json::from_value(serde_json::json!({"centerX":curve(1.123456789012,3.123456789012),"centerY":curve(-0.75,1.25),"threshold":curve(60.,20.),"length":curve(50.,100.),"intensity":curve(100.,350.),"colorRed":curve(1.,0.),"colorGreen":curve(0.95,0.25),"colorBlue":curve(0.8,0.2),"colorAlpha":curve(1.,0.)})).unwrap();
        assert_eq!(a.len(), 9);
        for only in [false, true] {
            let p = VolumetricRays {
                only,
                ..Default::default()
            };
            let mid = sample_animation(p, &a, 250000).unwrap();
            assert!((mid.center.unwrap()[0].unwrap()-2.123456789012).abs()<1e-14);
            assert_eq!(mid.center.unwrap()[1],Some(0.25));
            assert_eq!((mid.threshold, mid.length, mid.intensity), (40., 75., 225.));
            assert_eq!(mid.color, [0.5, 0.6, 0.5, 0.5]);
            assert_eq!(mid.only, only);
            let end = sample_animation(p, &a, 900000).unwrap();
            assert_eq!(end.center, Some([Some(3.123456789012), Some(1.25)]));
            assert_eq!(
                (end.threshold, end.length, end.intensity),
                (20., 100., 350.)
            );
            let effect = super::super::composition::VideoEffect::VolumetricRays {
                parameters: p,
                animation: Some(a.clone()),
            };
            let saved = serde_json::to_value(effect).unwrap();
            let restored: super::super::composition::VideoEffect =
                serde_json::from_value(saved.clone()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        }
        let x = VolumetricRaysAnimation::from([(
            VolumetricRaysChannel::CenterX,
            super::super::composition::AnimatableValue::Constant {
                value: 1.123456789012,
            },
        )]);
        assert_eq!(
            sample_animation(VolumetricRays::default(), &x, 0)
                .unwrap()
                .center,
            Some([Some(1.123456789012), None])
        );
        for (c, hi) in [
            (VolumetricRaysChannel::Threshold, 100.),
            (VolumetricRaysChannel::Length, 100.),
            (VolumetricRaysChannel::Intensity, 1000.),
            (VolumetricRaysChannel::ColorRed, 1.),
            (VolumetricRaysChannel::ColorGreen, 1.),
            (VolumetricRaysChannel::ColorBlue, 1.),
            (VolumetricRaysChannel::ColorAlpha, 1.),
        ] {
            for v in [0., hi] {
                assert!(validate_animation(&VolumetricRaysAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value: v }
                )]))
                .is_ok());
            }
            for v in [-0.01, hi + 0.01] {
                assert!(validate_animation(&VolumetricRaysAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value: v }
                )]))
                .is_err());
            }
        }
    }
}
