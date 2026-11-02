//! Filmcraft Glint; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Glint {
    pub threshold: f32,
    pub rays: u8,
    pub length: f32,
    pub rotation: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub colorize: f32,
}
impl Default for Glint {
    fn default() -> Self {
        Self {
            threshold: 75.,
            rays: 1,
            length: 60.,
            rotation: 45.,
            intensity: 100.,
            color: [1.; 4],
            colorize: 0.,
        }
    }
}
impl Glint {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [
            (self.threshold, 100.),
            (self.length, 2000.),
            (self.intensity, 1000.),
            (self.colorize, 100.),
        ] {
            ensure!(
                v.is_finite() && (0.0..=max).contains(&v),
                "Glint parameter out of range"
            );
        }
        ensure!(
            self.rotation.is_finite()
                && self.rays <= 3
                && self
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "Glint rotation/rays/color out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(scale.is_finite() && scale > 0., "invalid Glint scale");
        checkpoint()?;
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
                let t =
                    ((l - threshold) / ((threshold + 0.08) - threshold).max(1e-6)).clamp(0., 1.);
                p.map(|v| v * (t * t * (3. - 2. * t)))
            },
            checkpoint,
        )?;
        let rays = [2usize, 4, 6, 8][self.rays as usize];
        let len = self.length * scale;
        let rot = self.rotation.to_radians();
        let (w, _) = frame.dimensions();
        let mut acc = hi.clone();
        acc.map_premultiplied(&mut |_| [0.; 4], checkpoint)?;
        for ray in 0..rays {
            let angle = rot + std::f32::consts::TAU * ray as f32 / rays as f32;
            let mut streak = hi.clone();
            let len = len * 0.35;
            let a = (-1. / len.max(1.)).exp();
            let mut n = 1f32;
            while n < len * 3. {
                let prev = streak.clone();
                let k = a.powf(n);
                let (dx, dy) = (angle.cos() * n, angle.sin() * n);
                let mut index = 0;
                streak.map_premultiplied(
                    &mut |mut p| {
                        let (x, y) = (index % w, index / w);
                        index += 1;
                        let q = super::gradient_wipe::transparent_sample(
                            &prev,
                            x as f32 + 0.5 - dx,
                            y as f32 + 0.5 - dy,
                        );
                        for c in 0..4 {
                            p[c] += q[c] * k;
                        }
                        p
                    },
                    checkpoint,
                )?;
                n *= 2.;
            }
            let norm = 1. - a;
            streak.map_premultiplied(&mut |p| p.map(|v| v * norm), checkpoint)?;
            let mut index = 0;
            acc.map_premultiplied(
                &mut |p| {
                    let q = streak.pixels()[index];
                    index += 1;
                    std::array::from_fn(|c| p[c] + q[c])
                },
                checkpoint,
            )?;
        }
        let color = std::array::from_fn::<f32, 3, _>(|k| super::basic_grade::linear(self.color[k]));
        let k = self.colorize / 100.;
        if k > 0. {
            acc.map_premultiplied(
                &mut |mut p| {
                    let l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                    for c in 0..3 {
                        p[c] += (l * color[c] * 1.5 - p[c]) * k;
                    }
                    p
                },
                checkpoint,
            )?;
        }
        let intensity = self.intensity / 100. * 3.;
        let gain = color.map(|c| intensity * c.max(0.2));
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let q = acc.pixels()[index];
                index += 1;
                let add = std::array::from_fn::<f32, 3, _>(|c| q[c] * gain[c]);
                for c in 0..3 {
                    p[c] += add[c];
                }
                let m = add[0].max(add[1]).max(add[2]).clamp(0., 1.);
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
    fn glint_original_working_bits() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-glint.json"))
                .unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 256);
        for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: Glint = serde_json::from_value(c["parameters"].clone()).unwrap();
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
                "original Glint case{i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlintChannel {
    Threshold,
    Colorize,
    Rotation,
    Length,
    Intensity,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type GlintAnimation =
    std::collections::BTreeMap<GlintChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &GlintAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        if *c == GlintChannel::Rotation {
            ensure!(
                super::composition::animatable_values_in(
                    v,
                    -(f32::MAX as f64),
                    f32::MAX as f64,
                    true
                ),
                "Glint rotation animation out of range"
            );
            continue;
        }
        let hi = match c {
            GlintChannel::Threshold => 100.,
            GlintChannel::Colorize => 100.,
            GlintChannel::Length => 2000.,
            GlintChannel::Intensity => 1000.,
            _ => 1.,
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., hi, true),
            "Glint animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Glint, a: &GlintAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Glint> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            GlintChannel::Threshold => p.threshold = v,
            GlintChannel::Rotation => p.rotation = v,
            GlintChannel::Colorize => p.colorize = v,
            GlintChannel::Length => p.length = v,
            GlintChannel::Intensity => p.intensity = v,
            GlintChannel::ColorRed => p.color[0] = v,
            GlintChannel::ColorGreen => p.color[1] = v,
            GlintChannel::ColorBlue => p.color[2] = v,
            GlintChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn glint_nine_curves_midpoint_hold_roundtrip_and_bounds() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let a:GlintAnimation=serde_json::from_value(serde_json::json!({"threshold":curve(75.,5.),"length":curve(60.,120.),"rotation":curve(45.,-135.),"intensity":curve(100.,350.),"colorize":curve(0.,100.),"colorRed":curve(1.,0.),"colorGreen":curve(1.,0.25),"colorBlue":curve(1.,0.5),"colorAlpha":curve(1.,0.)})).unwrap();
        assert_eq!(a.len(), 9);
        for rays in 0..4 {
            let p = Glint {
                rays,
                ..Glint::default()
            };
            let mid = sample_animation(p, &a, 250000).unwrap();
            assert_eq!(
                mid,
                Glint {
                    threshold: 40.,
                    length: 90.,
                    rotation: -45.,
                    intensity: 225.,
                    colorize: 50.,
                    color: [0.5, 0.625, 0.75, 0.5],
                    rays
                }
            );
            let end = sample_animation(p, &a, 900000).unwrap();
            assert_eq!(
                end,
                Glint {
                    threshold: 5.,
                    length: 120.,
                    rotation: -135.,
                    intensity: 350.,
                    colorize: 100.,
                    color: [0., 0.25, 0.5, 0.],
                    rays
                }
            );
            let effect = super::super::composition::VideoEffect::Glint {
                parameters: p,
                animation: Some(a.clone()),
            };
            p.validate().unwrap();
            validate_animation(&a).unwrap();
            let saved = serde_json::to_value(&effect).unwrap();
            assert_eq!(saved["kind"], "glint");
            let restored: super::super::composition::VideoEffect =
                serde_json::from_value(saved.clone()).unwrap();
            assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        }
        for (c, hi) in [
            (GlintChannel::Threshold, 100.),
            (GlintChannel::Length, 2000.),
            (GlintChannel::Intensity, 1000.),
            (GlintChannel::Colorize, 100.),
            (GlintChannel::ColorRed, 1.),
            (GlintChannel::ColorGreen, 1.),
            (GlintChannel::ColorBlue, 1.),
            (GlintChannel::ColorAlpha, 1.),
        ] {
            for v in [0., hi] {
                assert!(validate_animation(&serde_json::from_value(serde_json::json!({serde_json::to_value(c).unwrap().as_str().unwrap():curve(v,v)})).unwrap()).is_ok());
            }
            for v in [-0.01, hi + 0.01] {
                let map = GlintAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value: v },
                )]);
                assert!(validate_animation(&map).is_err());
            }
        }
        for v in [-(f32::MAX as f64), f32::MAX as f64, -720., 720.] {
            assert!(validate_animation(&GlintAnimation::from([(
                GlintChannel::Rotation,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_ok());
        }
        for v in [f64::MAX, -f64::MAX] {
            assert!(validate_animation(&GlintAnimation::from([(
                GlintChannel::Rotation,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
