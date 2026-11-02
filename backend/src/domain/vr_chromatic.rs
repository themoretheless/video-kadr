//! Filmcraft VR Chromatic Aberrations. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::vr_sphere::{cross, dir, dot, px_to_lonlat, sample_dir, Image};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrChromatic {
    pub frame_layout: u32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub falloff: f32,
    pub inverse: bool,
}
impl Default for VrChromatic {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            red: 0.,
            green: 0.,
            blue: 0.,
            center_x: 0.,
            center_y: 0.,
            falloff: 50.,
            inverse: false,
        }
    }
}
impl VrChromatic {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.frame_layout <= 2, "invalid VR frame layout");
        for (v, lo, hi) in [
            (self.red, -100., 100.),
            (self.green, -100., 100.),
            (self.blue, -100., 100.),
            (self.center_x, -180., 180.),
            (self.center_y, -90., 90.),
            (self.falloff, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "VR chromatic parameter out of range"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let ab = [self.red, self.green, self.blue];
        if ab.iter().all(|a| a.abs() < 1e-4) {
            return Ok(());
        }
        let center = dir(
            (self.center_x as f64).to_radians(),
            (self.center_y as f64).to_radians(),
        );
        let p = 0.5 + self.falloff as f64 / 50.;
        let (w, h) = frame.dimensions();
        let input = frame.pixels();
        let mut output = input.to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            checkpoint()?;
            let mut px = Vec::with_capacity(vw * vh);
            for y in 0..vh {
                px.extend_from_slice(&input[(y0 + y) * w + x0..(y0 + y) * w + x0 + vw]);
            }
            let src = Image { w: vw, h: vh, px };
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let (lon, lat) = px_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, vw, vh);
                    let d = dir(lon, lat);
                    let th = dot(d, center).clamp(-1., 1.).acos();
                    let mut f = (th / std::f64::consts::PI).powf(p);
                    if self.inverse {
                        f = 1. - f;
                    }
                    let axis = cross(center, d);
                    let al = dot(axis, axis).sqrt();
                    if al > 1e-9 {
                        let k = [axis[0] / al, axis[1] / al, axis[2] / al];
                        for c in 0..3 {
                            let ang = ab[c] as f64 / 100. * 0.15 * f;
                            let (s, co) = (-ang).sin_cos();
                            let kd = cross(k, d);
                            let kdot = dot(k, d);
                            let r =
                                [0, 1, 2].map(|i| d[i] * co + kd[i] * s + k[i] * kdot * (1. - co));
                            output[(y0 + y) * w + x0 + x][c] = sample_dir(&src, r)[c];
                        }
                    }
                }
            }
        }
        let mut i = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = output[i];
                i += 1;
                p
            },
            checkpoint,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrChromaticChannel {
    Red,
    Green,
    Blue,
    CenterX,
    CenterY,
    Falloff,
}
pub type VrChromaticAnimation =
    std::collections::BTreeMap<VrChromaticChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrChromaticAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrChromaticChannel::CenterX => (-180., 180.),
            VrChromaticChannel::CenterY => (-90., 90.),
            VrChromaticChannel::Falloff => (0., 100.),
            _ => (-100., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR chromatic animation out of range"
        );
    }
    Ok(())
}
pub fn sample(mut p: VrChromatic, a: &VrChromaticAnimation, local: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<VrChromatic> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrChromaticChannel::Red => p.red = v,
            VrChromaticChannel::Green => p.green = v,
            VrChromaticChannel::Blue => p.blue = v,
            VrChromaticChannel::CenterX => p.center_x = v,
            VrChromaticChannel::CenterY => p.center_y = v,
            VrChromaticChannel::Falloff => p.falloff = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: VrChromatic,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_chromatic_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-chromatic.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 324);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            let alpha: Vec<u32> = frame.pixels().iter().map(|p| p[3].to_bits()).collect();
            c.parameters
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "VR chromatic case{i}"
            );
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .map(|p| p[3].to_bits())
                    .collect::<Vec<_>>(),
                alpha
            );
        }
    }
    #[test]
    fn six_channels_preserve_choices_and_roundtrip() {
        let mut a = serde_json::Map::new();
        for (name, value) in [
            ("red", 25.),
            ("green", -37.5),
            ("blue", 12.25),
            ("centerX", 37.25),
            ("centerY", -47.75),
            ("falloff", 50.),
        ] {
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":value-1.},{"tick":1000,"value":value+1.}]}}));
        }
        let a: VrChromaticAnimation = serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        let p = sample(
            VrChromatic {
                frame_layout: 2,
                inverse: true,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        assert_eq!(
            [p.red, p.green, p.blue, p.center_x, p.center_y, p.falloff],
            [25., -37.5, 12.25, 37.25, -47.75, 50.]
        );
        assert_eq!(p.frame_layout, 2);
        assert!(p.inverse);
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_chromatic_aberrations","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrChromatic { center_y: 91., ..p }.validate().is_err());
    }
}
