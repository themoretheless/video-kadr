//! Filmcraft Lighting Effects CPU renderer; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Light {
    pub kind: u8,
    pub color: [f32; 4],
    pub center: Option<[Option<f64>; 2]>,
    pub major: f32,
    pub minor: f32,
    pub angle: f32,
    pub intensity: f32,
    pub focus: f32,
}
impl Default for Light {
    fn default() -> Self {
        Self {
            kind: 0,
            color: [1.; 4],
            center: None,
            major: 20.,
            minor: 20.,
            angle: 0.,
            intensity: 20.,
            focus: 50.,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LightingEffects {
    pub lights: [Light; 5],
    pub ambient_color: [f32; 4],
    pub ambient: f32,
    pub gloss: f32,
    pub material: f32,
    pub exposure: f32,
    pub bump_height: f32,
    pub white_high: bool,
}
impl Default for LightingEffects {
    fn default() -> Self {
        let mut lights = [Light::default(); 5];
        lights[0].kind = 3;
        Self {
            lights,
            ambient_color: [1.; 4],
            ambient: 20.,
            gloss: 0.,
            material: 0.,
            exposure: 0.,
            bump_height: 0.,
            white_high: true,
        }
    }
}
impl LightingEffects {
    pub fn validate(&self) -> Result<()> {
        let color = |c: [f32; 4]| c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v));
        ensure!(
            color(self.ambient_color),
            "Lighting ambient color out of range"
        );
        ensure!(
            [
                self.ambient,
                self.gloss,
                self.material,
                self.exposure,
                self.bump_height
            ]
            .iter()
            .all(|v| v.is_finite() && (-100. ..=100.).contains(v)),
            "Lighting surface parameter out of range"
        );
        for l in self.lights {
            ensure!(
                l.kind <= 3
                    && color(l.color)
                    && l.angle.is_finite()
                    && l.center
                        .map_or(true, |p| p.iter().flatten().all(|v| v.is_finite())),
                "Lighting light parameter out of range"
            );
            ensure!(
                [l.major, l.minor, l.focus]
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=100.).contains(v))
                    && l.intensity.is_finite()
                    && (-100. ..=100.).contains(&l.intensity),
                "Lighting light radius/intensity out of range"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Lighting pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let src = frame.pixels().to_vec();
        let diag = (w as f64).hypot(h as f64);
        let lights: Vec<_> = self
            .lights
            .iter()
            .filter(|l| l.kind != 0)
            .map(|l| {
                let p = l.center.unwrap_or([None, None]);
                (
                    *l,
                    l.color.map(super::basic_grade::linear),
                    [
                        p[0].map(|x| x * px_scale as f64).unwrap_or(w as f64 / 2.),
                        p[1].map(|y| y * px_scale as f64).unwrap_or(h as f64 / 2.),
                    ],
                    (l.major as f64 / 100. * diag).max(1.),
                    (l.minor as f64 / 100. * diag).max(1.),
                    (l.angle as f64).to_radians().sin_cos(),
                )
            })
            .collect();
        let amb = self
            .ambient_color
            .map(|v| super::basic_grade::linear(v) * self.ambient / 100.);
        let gloss = self.gloss / 100.;
        let material = (self.material / 100. + 1.) / 2.;
        let exposure = 2f32.powf(self.exposure / 100. * 2.);
        let bump = self.bump_height / 100. * 4.;
        let lum = |x: isize, y: isize| {
            let p =
                src[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize];
            let c = if p[3] > 1e-6 {
                [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
            } else {
                [0.; 3]
            };
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            if self.white_high {
                l
            } else {
                1. - l
            }
        };
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let (x, y) = (index % w, index / w);
                index += 1;
                if p[3] <= 1e-6 {
                    return p;
                }
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let (gx, gy) = if bump != 0. {
                    (
                        (lum(x as isize + 1, y as isize) - lum(x as isize - 1, y as isize)) * bump,
                        (lum(x as isize, y as isize + 1) - lum(x as isize, y as isize - 1)) * bump,
                    )
                } else {
                    (0., 0.)
                };
                let mut light = [amb[0], amb[1], amb[2]];
                let mut spec = [0f32; 3];
                for (l, color, center, major, minor, (s, c)) in &lights {
                    let (dx, dy) = (px - center[0], py - center[1]);
                    let (u, v) = (dx * c + dy * s, -dx * s + dy * c);
                    let fall = match l.kind {
                        1 => 1.,
                        2 => {
                            let d = ((dx * dx + dy * dy).sqrt() / major) as f32;
                            (1. - d).max(0.).powi(2)
                        }
                        _ => {
                            let d = ((u / major).powi(2) + (v / minor).powi(2)).sqrt() as f32;
                            let t = ((d - l.focus / 100. * 0.95)
                                / (1. - l.focus / 100. * 0.95).max(1e-6))
                            .clamp(0., 1.);
                            1. - t * t * (3. - 2. * t)
                        }
                    };
                    let (lx, ly) = if l.kind == 1 {
                        (*c as f32, *s as f32)
                    } else {
                        (-dx as f32, -dy as f32)
                    };
                    let ln = (lx * lx + ly * ly).sqrt().max(1e-6);
                    let shade = (1. - (gx * lx + gy * ly) / ln).max(0.);
                    for k in 0..3 {
                        light[k] += color[k] * (l.intensity / 20.) * fall * shade;
                        spec[k] +=
                            color[k] * gloss.max(0.) * fall.powi(8) * (l.intensity / 20.).max(0.);
                    }
                }
                let c = [p[0] / p[3], p[1] / p[3], p[2] / p[3]];
                for k in 0..3 {
                    let s = spec[k] * (1. - material) + spec[k] * c[k] * material;
                    let v = ((c[k] * light[k] + s) * exposure).max(0.);
                    p[k] = v * p[3];
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
    fn lighting_effects_original_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            parameters: LightingEffects,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-lighting-effects.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 64);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
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
                "original Lighting Effects {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LightingChannel {
    #[serde(rename = "l1Major")]
    L1Major,
    #[serde(rename = "l1Minor")]
    L1Minor,
    #[serde(rename = "l1Angle")]
    L1Angle,
    #[serde(rename = "l1Intensity")]
    L1Intensity,
    #[serde(rename = "l1Focus")]
    L1Focus,
    #[serde(rename = "l1ColorRed")]
    L1ColorRed,
    #[serde(rename = "l1ColorGreen")]
    L1ColorGreen,
    #[serde(rename = "l1ColorBlue")]
    L1ColorBlue,
    #[serde(rename = "l1ColorAlpha")]
    L1ColorAlpha,
    #[serde(rename = "l1CenterX")]
    L1CenterX,
    #[serde(rename = "l1CenterY")]
    L1CenterY,
    #[serde(rename = "l2Major")]
    L2Major,
    #[serde(rename = "l2Minor")]
    L2Minor,
    #[serde(rename = "l2Angle")]
    L2Angle,
    #[serde(rename = "l2Intensity")]
    L2Intensity,
    #[serde(rename = "l2Focus")]
    L2Focus,
    #[serde(rename = "l2ColorRed")]
    L2ColorRed,
    #[serde(rename = "l2ColorGreen")]
    L2ColorGreen,
    #[serde(rename = "l2ColorBlue")]
    L2ColorBlue,
    #[serde(rename = "l2ColorAlpha")]
    L2ColorAlpha,
    #[serde(rename = "l2CenterX")]
    L2CenterX,
    #[serde(rename = "l2CenterY")]
    L2CenterY,
    #[serde(rename = "l3Major")]
    L3Major,
    #[serde(rename = "l3Minor")]
    L3Minor,
    #[serde(rename = "l3Angle")]
    L3Angle,
    #[serde(rename = "l3Intensity")]
    L3Intensity,
    #[serde(rename = "l3Focus")]
    L3Focus,
    #[serde(rename = "l3ColorRed")]
    L3ColorRed,
    #[serde(rename = "l3ColorGreen")]
    L3ColorGreen,
    #[serde(rename = "l3ColorBlue")]
    L3ColorBlue,
    #[serde(rename = "l3ColorAlpha")]
    L3ColorAlpha,
    #[serde(rename = "l3CenterX")]
    L3CenterX,
    #[serde(rename = "l3CenterY")]
    L3CenterY,
    #[serde(rename = "l4Major")]
    L4Major,
    #[serde(rename = "l4Minor")]
    L4Minor,
    #[serde(rename = "l4Angle")]
    L4Angle,
    #[serde(rename = "l4Intensity")]
    L4Intensity,
    #[serde(rename = "l4Focus")]
    L4Focus,
    #[serde(rename = "l4ColorRed")]
    L4ColorRed,
    #[serde(rename = "l4ColorGreen")]
    L4ColorGreen,
    #[serde(rename = "l4ColorBlue")]
    L4ColorBlue,
    #[serde(rename = "l4ColorAlpha")]
    L4ColorAlpha,
    #[serde(rename = "l4CenterX")]
    L4CenterX,
    #[serde(rename = "l4CenterY")]
    L4CenterY,
    #[serde(rename = "l5Major")]
    L5Major,
    #[serde(rename = "l5Minor")]
    L5Minor,
    #[serde(rename = "l5Angle")]
    L5Angle,
    #[serde(rename = "l5Intensity")]
    L5Intensity,
    #[serde(rename = "l5Focus")]
    L5Focus,
    #[serde(rename = "l5ColorRed")]
    L5ColorRed,
    #[serde(rename = "l5ColorGreen")]
    L5ColorGreen,
    #[serde(rename = "l5ColorBlue")]
    L5ColorBlue,
    #[serde(rename = "l5ColorAlpha")]
    L5ColorAlpha,
    #[serde(rename = "l5CenterX")]
    L5CenterX,
    #[serde(rename = "l5CenterY")]
    L5CenterY,
    #[serde(rename = "ambientColorRed")]
    AmbientColorRed,
    #[serde(rename = "ambientColorGreen")]
    AmbientColorGreen,
    #[serde(rename = "ambientColorBlue")]
    AmbientColorBlue,
    #[serde(rename = "ambientColorAlpha")]
    AmbientColorAlpha,
    #[serde(rename = "ambient")]
    Ambient,
    #[serde(rename = "gloss")]
    Gloss,
    #[serde(rename = "material")]
    Material,
    #[serde(rename = "exposure")]
    Exposure,
    #[serde(rename = "bumpHeight")]
    BumpHeight,
}
pub type LightingAnimation =
    std::collections::BTreeMap<LightingChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &LightingAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi, cast) = match c {
            LightingChannel::L1Major => (0.0, 100.0, true),
            LightingChannel::L1Minor => (0.0, 100.0, true),
            LightingChannel::L1Angle => (f32::MIN as f64, f32::MAX as f64, true),
            LightingChannel::L1Intensity => (-100.0, 100.0, true),
            LightingChannel::L1Focus => (0.0, 100.0, true),
            LightingChannel::L1ColorRed => (0.0, 1.0, true),
            LightingChannel::L1ColorGreen => (0.0, 1.0, true),
            LightingChannel::L1ColorBlue => (0.0, 1.0, true),
            LightingChannel::L1ColorAlpha => (0.0, 1.0, true),
            LightingChannel::L1CenterX => (f64::MIN, f64::MAX, false),
            LightingChannel::L1CenterY => (f64::MIN, f64::MAX, false),
            LightingChannel::L2Major => (0.0, 100.0, true),
            LightingChannel::L2Minor => (0.0, 100.0, true),
            LightingChannel::L2Angle => (f32::MIN as f64, f32::MAX as f64, true),
            LightingChannel::L2Intensity => (-100.0, 100.0, true),
            LightingChannel::L2Focus => (0.0, 100.0, true),
            LightingChannel::L2ColorRed => (0.0, 1.0, true),
            LightingChannel::L2ColorGreen => (0.0, 1.0, true),
            LightingChannel::L2ColorBlue => (0.0, 1.0, true),
            LightingChannel::L2ColorAlpha => (0.0, 1.0, true),
            LightingChannel::L2CenterX => (f64::MIN, f64::MAX, false),
            LightingChannel::L2CenterY => (f64::MIN, f64::MAX, false),
            LightingChannel::L3Major => (0.0, 100.0, true),
            LightingChannel::L3Minor => (0.0, 100.0, true),
            LightingChannel::L3Angle => (f32::MIN as f64, f32::MAX as f64, true),
            LightingChannel::L3Intensity => (-100.0, 100.0, true),
            LightingChannel::L3Focus => (0.0, 100.0, true),
            LightingChannel::L3ColorRed => (0.0, 1.0, true),
            LightingChannel::L3ColorGreen => (0.0, 1.0, true),
            LightingChannel::L3ColorBlue => (0.0, 1.0, true),
            LightingChannel::L3ColorAlpha => (0.0, 1.0, true),
            LightingChannel::L3CenterX => (f64::MIN, f64::MAX, false),
            LightingChannel::L3CenterY => (f64::MIN, f64::MAX, false),
            LightingChannel::L4Major => (0.0, 100.0, true),
            LightingChannel::L4Minor => (0.0, 100.0, true),
            LightingChannel::L4Angle => (f32::MIN as f64, f32::MAX as f64, true),
            LightingChannel::L4Intensity => (-100.0, 100.0, true),
            LightingChannel::L4Focus => (0.0, 100.0, true),
            LightingChannel::L4ColorRed => (0.0, 1.0, true),
            LightingChannel::L4ColorGreen => (0.0, 1.0, true),
            LightingChannel::L4ColorBlue => (0.0, 1.0, true),
            LightingChannel::L4ColorAlpha => (0.0, 1.0, true),
            LightingChannel::L4CenterX => (f64::MIN, f64::MAX, false),
            LightingChannel::L4CenterY => (f64::MIN, f64::MAX, false),
            LightingChannel::L5Major => (0.0, 100.0, true),
            LightingChannel::L5Minor => (0.0, 100.0, true),
            LightingChannel::L5Angle => (f32::MIN as f64, f32::MAX as f64, true),
            LightingChannel::L5Intensity => (-100.0, 100.0, true),
            LightingChannel::L5Focus => (0.0, 100.0, true),
            LightingChannel::L5ColorRed => (0.0, 1.0, true),
            LightingChannel::L5ColorGreen => (0.0, 1.0, true),
            LightingChannel::L5ColorBlue => (0.0, 1.0, true),
            LightingChannel::L5ColorAlpha => (0.0, 1.0, true),
            LightingChannel::L5CenterX => (f64::MIN, f64::MAX, false),
            LightingChannel::L5CenterY => (f64::MIN, f64::MAX, false),
            LightingChannel::AmbientColorRed => (0.0, 1.0, true),
            LightingChannel::AmbientColorGreen => (0.0, 1.0, true),
            LightingChannel::AmbientColorBlue => (0.0, 1.0, true),
            LightingChannel::AmbientColorAlpha => (0.0, 1.0, true),
            LightingChannel::Ambient => (-100.0, 100.0, true),
            LightingChannel::Gloss => (-100.0, 100.0, true),
            LightingChannel::Material => (-100.0, 100.0, true),
            LightingChannel::Exposure => (-100.0, 100.0, true),
            LightingChannel::BumpHeight => (-100.0, 100.0, true),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, cast),
            "Lighting animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: LightingEffects,
    a: &LightingAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<LightingEffects> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, curve) in a {
        let v = super::nested_composition::sample_value(curve, t)?;
        match c {
            LightingChannel::L1Major => {
                p.lights[0].major = v as f32;
            }
            LightingChannel::L1Minor => {
                p.lights[0].minor = v as f32;
            }
            LightingChannel::L1Angle => {
                p.lights[0].angle = v as f32;
            }
            LightingChannel::L1Intensity => {
                p.lights[0].intensity = v as f32;
            }
            LightingChannel::L1Focus => {
                p.lights[0].focus = v as f32;
            }
            LightingChannel::L1ColorRed => {
                p.lights[0].color[0] = v as f32;
            }
            LightingChannel::L1ColorGreen => {
                p.lights[0].color[1] = v as f32;
            }
            LightingChannel::L1ColorBlue => {
                p.lights[0].color[2] = v as f32;
            }
            LightingChannel::L1ColorAlpha => {
                p.lights[0].color[3] = v as f32;
            }
            LightingChannel::L1CenterX => {
                p.lights[0].center.get_or_insert([None, None])[0] = Some(v);
            }
            LightingChannel::L1CenterY => {
                p.lights[0].center.get_or_insert([None, None])[1] = Some(v);
            }
            LightingChannel::L2Major => {
                p.lights[1].major = v as f32;
            }
            LightingChannel::L2Minor => {
                p.lights[1].minor = v as f32;
            }
            LightingChannel::L2Angle => {
                p.lights[1].angle = v as f32;
            }
            LightingChannel::L2Intensity => {
                p.lights[1].intensity = v as f32;
            }
            LightingChannel::L2Focus => {
                p.lights[1].focus = v as f32;
            }
            LightingChannel::L2ColorRed => {
                p.lights[1].color[0] = v as f32;
            }
            LightingChannel::L2ColorGreen => {
                p.lights[1].color[1] = v as f32;
            }
            LightingChannel::L2ColorBlue => {
                p.lights[1].color[2] = v as f32;
            }
            LightingChannel::L2ColorAlpha => {
                p.lights[1].color[3] = v as f32;
            }
            LightingChannel::L2CenterX => {
                p.lights[1].center.get_or_insert([None, None])[0] = Some(v);
            }
            LightingChannel::L2CenterY => {
                p.lights[1].center.get_or_insert([None, None])[1] = Some(v);
            }
            LightingChannel::L3Major => {
                p.lights[2].major = v as f32;
            }
            LightingChannel::L3Minor => {
                p.lights[2].minor = v as f32;
            }
            LightingChannel::L3Angle => {
                p.lights[2].angle = v as f32;
            }
            LightingChannel::L3Intensity => {
                p.lights[2].intensity = v as f32;
            }
            LightingChannel::L3Focus => {
                p.lights[2].focus = v as f32;
            }
            LightingChannel::L3ColorRed => {
                p.lights[2].color[0] = v as f32;
            }
            LightingChannel::L3ColorGreen => {
                p.lights[2].color[1] = v as f32;
            }
            LightingChannel::L3ColorBlue => {
                p.lights[2].color[2] = v as f32;
            }
            LightingChannel::L3ColorAlpha => {
                p.lights[2].color[3] = v as f32;
            }
            LightingChannel::L3CenterX => {
                p.lights[2].center.get_or_insert([None, None])[0] = Some(v);
            }
            LightingChannel::L3CenterY => {
                p.lights[2].center.get_or_insert([None, None])[1] = Some(v);
            }
            LightingChannel::L4Major => {
                p.lights[3].major = v as f32;
            }
            LightingChannel::L4Minor => {
                p.lights[3].minor = v as f32;
            }
            LightingChannel::L4Angle => {
                p.lights[3].angle = v as f32;
            }
            LightingChannel::L4Intensity => {
                p.lights[3].intensity = v as f32;
            }
            LightingChannel::L4Focus => {
                p.lights[3].focus = v as f32;
            }
            LightingChannel::L4ColorRed => {
                p.lights[3].color[0] = v as f32;
            }
            LightingChannel::L4ColorGreen => {
                p.lights[3].color[1] = v as f32;
            }
            LightingChannel::L4ColorBlue => {
                p.lights[3].color[2] = v as f32;
            }
            LightingChannel::L4ColorAlpha => {
                p.lights[3].color[3] = v as f32;
            }
            LightingChannel::L4CenterX => {
                p.lights[3].center.get_or_insert([None, None])[0] = Some(v);
            }
            LightingChannel::L4CenterY => {
                p.lights[3].center.get_or_insert([None, None])[1] = Some(v);
            }
            LightingChannel::L5Major => {
                p.lights[4].major = v as f32;
            }
            LightingChannel::L5Minor => {
                p.lights[4].minor = v as f32;
            }
            LightingChannel::L5Angle => {
                p.lights[4].angle = v as f32;
            }
            LightingChannel::L5Intensity => {
                p.lights[4].intensity = v as f32;
            }
            LightingChannel::L5Focus => {
                p.lights[4].focus = v as f32;
            }
            LightingChannel::L5ColorRed => {
                p.lights[4].color[0] = v as f32;
            }
            LightingChannel::L5ColorGreen => {
                p.lights[4].color[1] = v as f32;
            }
            LightingChannel::L5ColorBlue => {
                p.lights[4].color[2] = v as f32;
            }
            LightingChannel::L5ColorAlpha => {
                p.lights[4].color[3] = v as f32;
            }
            LightingChannel::L5CenterX => {
                p.lights[4].center.get_or_insert([None, None])[0] = Some(v);
            }
            LightingChannel::L5CenterY => {
                p.lights[4].center.get_or_insert([None, None])[1] = Some(v);
            }
            LightingChannel::AmbientColorRed => {
                p.ambient_color[0] = v as f32;
            }
            LightingChannel::AmbientColorGreen => {
                p.ambient_color[1] = v as f32;
            }
            LightingChannel::AmbientColorBlue => {
                p.ambient_color[2] = v as f32;
            }
            LightingChannel::AmbientColorAlpha => {
                p.ambient_color[3] = v as f32;
            }
            LightingChannel::Ambient => {
                p.ambient = v as f32;
            }
            LightingChannel::Gloss => {
                p.gloss = v as f32;
            }
            LightingChannel::Material => {
                p.material = v as f32;
            }
            LightingChannel::Exposure => {
                p.exposure = v as f32;
            }
            LightingChannel::BumpHeight => {
                p.bump_height = v as f32;
            }
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn lighting_sixty_four_curves_roundtrip_midpoint_hold_and_bounds() {
        let names = serde_json::json!([
            "l1Major",
            "l1Minor",
            "l1Angle",
            "l1Intensity",
            "l1Focus",
            "l1ColorRed",
            "l1ColorGreen",
            "l1ColorBlue",
            "l1ColorAlpha",
            "l1CenterX",
            "l1CenterY",
            "l2Major",
            "l2Minor",
            "l2Angle",
            "l2Intensity",
            "l2Focus",
            "l2ColorRed",
            "l2ColorGreen",
            "l2ColorBlue",
            "l2ColorAlpha",
            "l2CenterX",
            "l2CenterY",
            "l3Major",
            "l3Minor",
            "l3Angle",
            "l3Intensity",
            "l3Focus",
            "l3ColorRed",
            "l3ColorGreen",
            "l3ColorBlue",
            "l3ColorAlpha",
            "l3CenterX",
            "l3CenterY",
            "l4Major",
            "l4Minor",
            "l4Angle",
            "l4Intensity",
            "l4Focus",
            "l4ColorRed",
            "l4ColorGreen",
            "l4ColorBlue",
            "l4ColorAlpha",
            "l4CenterX",
            "l4CenterY",
            "l5Major",
            "l5Minor",
            "l5Angle",
            "l5Intensity",
            "l5Focus",
            "l5ColorRed",
            "l5ColorGreen",
            "l5ColorBlue",
            "l5ColorAlpha",
            "l5CenterX",
            "l5CenterY",
            "ambientColorRed",
            "ambientColorGreen",
            "ambientColorBlue",
            "ambientColorAlpha",
            "ambient",
            "gloss",
            "material",
            "exposure",
            "bumpHeight"
        ]);
        let mut curves = serde_json::Map::new();
        for name in names.as_array().unwrap() {
            let name = name.as_str().unwrap();
            let point = name.contains("Center");
            let (a, b) = if point {
                (1.123456789012, 3.123456789012)
            } else {
                (0., 1.)
            };
            curves.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}));
        }
        let a: LightingAnimation =
            serde_json::from_value(serde_json::Value::Object(curves)).unwrap();
        assert_eq!(a.len(), 64);
        validate_animation(&a).unwrap();
        let restored: LightingAnimation =
            serde_json::from_value(serde_json::to_value(&a).unwrap()).unwrap();
        assert_eq!(restored, a);
        for tick in [250000, 750000] {
            let value = serde_json::to_value(
                sample_animation(LightingEffects::default(), &a, tick).unwrap(),
            )
            .unwrap();
            assert_eq!(
                value.pointer("/lights/0/major").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1Major"
            );
            assert_eq!(
                value.pointer("/lights/0/minor").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1Minor"
            );
            assert_eq!(
                value.pointer("/lights/0/angle").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1Angle"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/intensity")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1Intensity"
            );
            assert_eq!(
                value.pointer("/lights/0/focus").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1Focus"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/color/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1ColorRed"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/color/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1ColorGreen"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/color/2")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1ColorBlue"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/color/3")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l1ColorAlpha"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/center/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l1CenterX"
            );
            assert_eq!(
                value
                    .pointer("/lights/0/center/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l1CenterY"
            );
            assert_eq!(
                value.pointer("/lights/1/major").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2Major"
            );
            assert_eq!(
                value.pointer("/lights/1/minor").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2Minor"
            );
            assert_eq!(
                value.pointer("/lights/1/angle").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2Angle"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/intensity")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2Intensity"
            );
            assert_eq!(
                value.pointer("/lights/1/focus").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2Focus"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/color/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2ColorRed"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/color/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2ColorGreen"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/color/2")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2ColorBlue"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/color/3")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l2ColorAlpha"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/center/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l2CenterX"
            );
            assert_eq!(
                value
                    .pointer("/lights/1/center/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l2CenterY"
            );
            assert_eq!(
                value.pointer("/lights/2/major").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3Major"
            );
            assert_eq!(
                value.pointer("/lights/2/minor").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3Minor"
            );
            assert_eq!(
                value.pointer("/lights/2/angle").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3Angle"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/intensity")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3Intensity"
            );
            assert_eq!(
                value.pointer("/lights/2/focus").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3Focus"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/color/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3ColorRed"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/color/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3ColorGreen"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/color/2")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3ColorBlue"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/color/3")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l3ColorAlpha"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/center/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l3CenterX"
            );
            assert_eq!(
                value
                    .pointer("/lights/2/center/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l3CenterY"
            );
            assert_eq!(
                value.pointer("/lights/3/major").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4Major"
            );
            assert_eq!(
                value.pointer("/lights/3/minor").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4Minor"
            );
            assert_eq!(
                value.pointer("/lights/3/angle").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4Angle"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/intensity")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4Intensity"
            );
            assert_eq!(
                value.pointer("/lights/3/focus").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4Focus"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/color/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4ColorRed"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/color/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4ColorGreen"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/color/2")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4ColorBlue"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/color/3")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l4ColorAlpha"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/center/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l4CenterX"
            );
            assert_eq!(
                value
                    .pointer("/lights/3/center/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l4CenterY"
            );
            assert_eq!(
                value.pointer("/lights/4/major").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5Major"
            );
            assert_eq!(
                value.pointer("/lights/4/minor").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5Minor"
            );
            assert_eq!(
                value.pointer("/lights/4/angle").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5Angle"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/intensity")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5Intensity"
            );
            assert_eq!(
                value.pointer("/lights/4/focus").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5Focus"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/color/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5ColorRed"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/color/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5ColorGreen"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/color/2")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5ColorBlue"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/color/3")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "l5ColorAlpha"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/center/0")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l5CenterX"
            );
            assert_eq!(
                value
                    .pointer("/lights/4/center/1")
                    .unwrap()
                    .as_f64()
                    .unwrap(),
                if tick == 250000 {
                    1.123456789012 + (3.123456789012 - 1.123456789012) * 0.5
                } else {
                    3.123456789012
                },
                "l5CenterY"
            );
            assert_eq!(
                value.pointer("/ambientColor/0").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "ambientColorRed"
            );
            assert_eq!(
                value.pointer("/ambientColor/1").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "ambientColorGreen"
            );
            assert_eq!(
                value.pointer("/ambientColor/2").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "ambientColorBlue"
            );
            assert_eq!(
                value.pointer("/ambientColor/3").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "ambientColorAlpha"
            );
            assert_eq!(
                value.pointer("/ambient").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "ambient"
            );
            assert_eq!(
                value.pointer("/gloss").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "gloss"
            );
            assert_eq!(
                value.pointer("/material").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "material"
            );
            assert_eq!(
                value.pointer("/exposure").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "exposure"
            );
            assert_eq!(
                value.pointer("/bumpHeight").unwrap().as_f64().unwrap(),
                if tick == 250000 { 0.5 } else { 1.0 },
                "bumpHeight"
            );
        }
        let invalid: LightingAnimation = serde_json::from_value(
            serde_json::json!({"l1ColorRed":{"mode":"constant","value":2.0}}),
        )
        .unwrap();
        assert!(validate_animation(&invalid).is_err());
        let effect: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"lighting_effects","parameters":{},"animation":a}),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<super::super::composition::VideoEffect>(
                serde_json::to_value(&effect).unwrap()
            )
            .unwrap(),
            effect
        );
    }
}
