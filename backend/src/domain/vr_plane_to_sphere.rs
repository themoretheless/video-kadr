//! Filmcraft VR Plane to Sphere. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    light_leaks::smoothstep,
    vr_sphere::{dir, mvec, px_to_lonlat, rotation, transpose, Image},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrPlaneToSphere {
    pub frame_layout: u32,
    pub scale: f32,
    pub pan: f32,
    pub tilt: f32,
    pub roll: f32,
    pub feather: f32,
}
impl Default for VrPlaneToSphere {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            scale: 60.,
            pan: 0.,
            tilt: 0.,
            roll: 0.,
            feather: 0.,
        }
    }
}
fn sample_clamped(src: &Image, x: f32, y: f32) -> [f32; 4] {
    let fx = (x - 0.5).clamp(0., src.w as f32 - 1.);
    let fy = (y - 0.5).clamp(0., src.h as f32 - 1.);
    let x0 = fx.floor() as usize;
    let y0 = fy.floor() as usize;
    let x1 = (x0 + 1).min(src.w - 1);
    let y1 = (y0 + 1).min(src.h - 1);
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let (a, b, c, d) = (
        src.px[y0 * src.w + x0],
        src.px[y0 * src.w + x1],
        src.px[y1 * src.w + x0],
        src.px[y1 * src.w + x1],
    );
    [0, 1, 2, 3].map(|k| {
        let top = a[k] + (b[k] - a[k]) * tx;
        let bot = c[k] + (d[k] - c[k]) * tx;
        top + (bot - top) * ty
    })
}
impl VrPlaneToSphere {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_layout <= 2 && self.roll.is_finite(),
            "invalid VR plane layout/roll"
        );
        for (v, lo, hi) in [
            (self.scale, 1., 180.),
            (self.pan, -180., 180.),
            (self.tilt, -90., 90.),
            (self.feather, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "invalid VR plane parameter"
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
        let fov = (self.scale as f64)
            .to_radians()
            .clamp(1e-3, std::f64::consts::PI * 0.999);
        let r = transpose(rotation(
            self.tilt as f64,
            self.pan as f64,
            self.roll as f64,
        ));
        let feather = self.feather / 100.;
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
            let tx = (fov / 2.).tan();
            let ty = tx * vh as f64 / vw as f64;
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let (lon, lat) = px_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, vw, vh);
                    let d = mvec(&r, dir(lon, lat));
                    let mut o = [0.; 4];
                    if d[2] > 1e-6 {
                        let (u, v) = (d[0] / d[2] / tx, -d[1] / d[2] / ty);
                        if u.abs() <= 1. && v.abs() <= 1. {
                            o = sample_clamped(
                                &src,
                                ((u + 1.) / 2. * vw as f64) as f32,
                                ((v + 1.) / 2. * vh as f64) as f32,
                            );
                            if feather > 0. {
                                let edge = (1. - u.abs().max(v.abs())) as f32;
                                let k = smoothstep(0., feather, edge);
                                o = o.map(|c| c * k);
                            }
                        }
                    }
                    output[(y0 + y) * w + x0 + x] = o;
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
        parameters: VrPlaneToSphere,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_plane_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-plane-to-sphere.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 162);
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
                "VR plane case{i}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrPlaneChannel {
    Scale,
    Pan,
    Tilt,
    Roll,
    Feather,
}
pub type VrPlaneAnimation =
    std::collections::BTreeMap<VrPlaneChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrPlaneAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrPlaneChannel::Scale => (1., 180.),
            VrPlaneChannel::Pan => (-180., 180.),
            VrPlaneChannel::Tilt => (-90., 90.),
            VrPlaneChannel::Feather => (0., 100.),
            VrPlaneChannel::Roll => (-(f32::MAX as f64), f32::MAX as f64),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR plane animation out of range"
        );
    }
    Ok(())
}
pub fn sample(mut p: VrPlaneToSphere, a: &VrPlaneAnimation, local: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<VrPlaneToSphere> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrPlaneChannel::Scale => p.scale = v,
            VrPlaneChannel::Pan => p.pan = v,
            VrPlaneChannel::Tilt => p.tilt = v,
            VrPlaneChannel::Roll => p.roll = v,
            VrPlaneChannel::Feather => p.feather = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn five_channels_roundtrip_and_bounds() {
        let mut a = serde_json::Map::new();
        for (name, value) in [
            ("scale", 120.25),
            ("pan", 37.5),
            ("tilt", -47.75),
            ("roll", 12.25),
            ("feather", 37.5),
        ] {
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-1.},{"tick":750,"value":value+1.}]}}));
        }
        let a: VrPlaneAnimation = serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        let p = sample(
            VrPlaneToSphere {
                frame_layout: 2,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        assert_eq!(
            [p.scale, p.pan, p.tilt, p.roll, p.feather],
            [120.25, 37.5, -47.75, 12.25, 37.5]
        );
        assert_eq!(p.frame_layout, 2);
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_plane_to_sphere","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrPlaneToSphere { scale: 0., ..p }.validate().is_err());
        assert!(VrPlaneToSphere {
            roll: f32::INFINITY,
            ..p
        }
        .validate()
        .is_err());
        let invalid: VrPlaneAnimation =
            serde_json::from_value(serde_json::json!({"roll":{"mode":"constant","value":1e100}}))
                .unwrap();
        assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn cancellation_preserves_input() {
        let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: vec![128; 96],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let before = frame.pixels().to_vec();
        let mut n = 0;
        assert!(VrPlaneToSphere::default()
            .apply_linear(&mut frame, &mut || {
                n += 1;
                anyhow::ensure!(n < 4, "cancelled");
                Ok(())
            })
            .is_err());
        assert_eq!(frame.pixels(), before);
    }
}
