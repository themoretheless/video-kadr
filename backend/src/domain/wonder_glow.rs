//! Filmcraft Wonder Glow; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WonderGlow {
    pub threshold: f32,
    pub radius: f32,
    pub intensity: f32,
    pub saturation: f32,
    pub use_color: bool,
    pub color: [f32; 4],
    pub mode: u8,
}
impl Default for WonderGlow {
    fn default() -> Self {
        Self {
            threshold: 60.,
            radius: 25.,
            intensity: 100.,
            saturation: 100.,
            use_color: false,
            color: [1., 0.8, 0.5, 1.],
            mode: 0,
        }
    }
}
impl WonderGlow {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [
            (self.threshold, 100.),
            (self.radius, 1000.),
            (self.intensity, 1000.),
            (self.saturation, 400.),
        ] {
            ensure!(
                v.is_finite() && (0.0..=max).contains(&v),
                "Wonder Glow parameter out of range"
            );
        }
        ensure!(
            self.mode <= 1
                && self
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "Wonder Glow color/mode out of range"
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
        ensure!(scale.is_finite() && scale > 0., "invalid Wonder Glow scale");
        checkpoint()?;
        let mut hi = frame.clone();
        let threshold = self.threshold / 100.;
        hi.map_premultiplied(
            &mut |p| {
                let a = p[3];
                if a <= 1e-6 {
                    return [0.; 4];
                }
                let rgb = std::array::from_fn::<f32, 3, _>(|k| {
                    super::basic_grade::display((p[k] / a).max(0.))
                });
                let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let t =
                    ((l - threshold) / ((threshold + 0.15) - threshold).max(1e-6)).clamp(0., 1.);
                let k = t * t * (3. - 2. * t);
                p.map(|v| v * k)
            },
            checkpoint,
        )?;
        let sat = self.saturation / 100.;
        if (sat - 1.).abs() >= 1e-4 {
            hi.map_premultiplied(
                &mut |mut p| {
                    let l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                    for v in &mut p[..3] {
                        *v = (l + (*v - l) * sat).max(0.);
                    }
                    p
                },
                checkpoint,
            )?;
        }
        if self.use_color {
            let color =
                std::array::from_fn::<f32, 3, _>(|k| super::basic_grade::linear(self.color[k]));
            hi.map_premultiplied(
                &mut |mut p| {
                    let l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                    for k in 0..3 {
                        p[k] += (l * color[k] * 1.5 - p[k]) * 1.;
                    }
                    p
                },
                checkpoint,
            )?;
        }
        let r = self.radius * scale * 0.5;
        ensure!(r.is_finite(), "Wonder Glow radius overflow");
        let mut acc = vec![[0.; 4]; frame.pixels().len()];
        for (k, weight) in [(0.25f32, 0.45f32), (1., 0.35), (3., 0.2)] {
            checkpoint()?;
            let mut layer = hi.clone();
            super::gaussian_blur::GaussianBlur::default().apply_sigma(
                &mut layer,
                r * k,
                checkpoint,
            )?;
            for (a, b) in acc.iter_mut().zip(layer.pixels()) {
                for c in 0..4 {
                    a[c] += b[c] * weight;
                }
            }
        }
        let gain = self.intensity / 100. * 1.5;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let light = acc[index];
                index += 1;
                let add = [light[0] * gain, light[1] * gain, light[2] * gain];
                if self.mode == 1 {
                    let a = add.map(|v| v.min(1.));
                    for k in 0..3 {
                        p[k] = p[k] + a[k] - p[k].min(1.) * a[k];
                    }
                    p[3] = p[3] + (1. - p[3]) * a[0].max(a[1]).max(a[2]);
                } else {
                    let m = add[0].max(add[1]).max(add[2]).clamp(0., 1.);
                    for k in 0..3 {
                        p[k] += add[k];
                    }
                    p[3] = p[3] + (1. - p[3]) * m;
                }
                p
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn wonder_glow_original_working_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: WonderGlow,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-wonder-glow.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 128);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Wonder Glow original {i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WonderGlowChannel {
    Threshold,
    Saturation,
    Radius,
    Intensity,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type WonderGlowAnimation =
    std::collections::BTreeMap<WonderGlowChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &WonderGlowAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let hi = match c {
            WonderGlowChannel::Threshold => 100.,
            WonderGlowChannel::Saturation => 400.,
            WonderGlowChannel::Radius => 1000.,
            WonderGlowChannel::Intensity => 1000.,
            _ => 1.,
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., hi, true),
            "Wonder Glow animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: WonderGlow, a: &WonderGlowAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<WonderGlow> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            WonderGlowChannel::Threshold => p.threshold = v,
            WonderGlowChannel::Saturation => p.saturation = v,
            WonderGlowChannel::Radius => p.radius = v,
            WonderGlowChannel::Intensity => p.intensity = v,
            WonderGlowChannel::ColorRed => p.color[0] = v,
            WonderGlowChannel::ColorGreen => p.color[1] = v,
            WonderGlowChannel::ColorBlue => p.color[2] = v,
            WonderGlowChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn wonder_glow_eight_curves_static_choices_bounds_midpoint_hold_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let a:WonderGlowAnimation=serde_json::from_value(serde_json::json!({"threshold":curve(60.,20.),"radius":curve(25.,75.),"intensity":curve(100.,350.),"saturation":curve(100.,400.),"colorRed":curve(1.,0.),"colorGreen":curve(0.8,0.2),"colorBlue":curve(0.5,0.5),"colorAlpha":curve(1.,0.)})).unwrap();
        for mode in [0, 1] {
            for use_color in [false, true] {
                let p = WonderGlow {
                    mode,
                    use_color,
                    ..WonderGlow::default()
                };
                let mid = sample_animation(p, &a, 250000).unwrap();
                assert_eq!(
                    (
                        mid.threshold,
                        mid.radius,
                        mid.intensity,
                        mid.saturation,
                        mid.mode,
                        mid.use_color
                    ),
                    (40., 50., 225., 250., mode, use_color)
                );
                assert_eq!(mid.color, [0.5; 4]);
                let end = sample_animation(p, &a, 750000).unwrap();
                assert_eq!(
                    (
                        end.threshold,
                        end.radius,
                        end.intensity,
                        end.saturation,
                        end.mode,
                        end.use_color
                    ),
                    (20., 75., 350., 400., mode, use_color)
                );
                assert_eq!(end.color, [0., 0.2, 0.5, 0.]);
                let e: super::super::composition::VideoEffect = serde_json::from_value(
                    serde_json::json!({"kind":"wonder_glow","parameters":p,"animation":a}),
                )
                .unwrap();
                assert_eq!(
                    e,
                    serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
                );
            }
        }
        for (c, hi) in [
            (WonderGlowChannel::Threshold, 100.),
            (WonderGlowChannel::Radius, 1000.),
            (WonderGlowChannel::Intensity, 1000.),
            (WonderGlowChannel::Saturation, 400.),
            (WonderGlowChannel::ColorRed, 1.),
            (WonderGlowChannel::ColorGreen, 1.),
            (WonderGlowChannel::ColorBlue, 1.),
            (WonderGlowChannel::ColorAlpha, 1.),
        ] {
            for v in [0., hi] {
                assert!(validate_animation(&WonderGlowAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value: v }
                )]))
                .is_ok());
            }
            for v in [-0.01, hi + 0.01] {
                assert!(validate_animation(&WonderGlowAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value: v }
                )]))
                .is_err());
            }
        }
    }
}
