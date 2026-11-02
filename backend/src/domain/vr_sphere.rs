//! Filmcraft VR Projection and Rotate Sphere geometry.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrSphere {
    pub frame_layout: u32,
    pub tilt: f32,
    pub pan: f32,
    pub roll: f32,
    pub fov: f32,
    pub stretch: bool,
}
impl Default for VrSphere {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            tilt: 0.,
            pan: 0.,
            roll: 0.,
            fov: 360.,
            stretch: true,
        }
    }
}
pub(crate) struct Image {
    pub(crate) w: usize,
    pub(crate) h: usize,
    pub(crate) px: Vec<[f32; 4]>,
}
impl Image {
    fn get(&self, x: usize, y: usize) -> [f32; 4] {
        self.px[y * self.w + x]
    }
}
type V3 = [f64; 3];

#[inline]
pub(crate) fn dir(lon: f64, lat: f64) -> V3 {
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}
#[inline]
fn lonlat(d: V3) -> (f64, f64) {
    (d[0].atan2(d[2]), d[1].clamp(-1.0, 1.0).asin())
}
#[inline]
pub(crate) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

type M3 = [[f64; 3]; 3];
fn mmul(a: M3, b: M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}
pub(crate) fn mvec(m: &M3, v: V3) -> V3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}
pub(crate) fn transpose(m: M3) -> M3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}
/// Rotation from tilt (about X), pan (about Y) and roll (about Z), degrees: R = Ry · Rx · Rz.
pub(crate) fn rotation(tilt: f64, pan: f64, roll: f64) -> M3 {
    let (sx, cx) = tilt.to_radians().sin_cos();
    let (sy, cy) = pan.to_radians().sin_cos();
    let (sz, cz) = roll.to_radians().sin_cos();
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mmul(ry, mmul(rx, rz))
}

#[inline]
pub(crate) fn px_to_lonlat(x: f64, y: f64, w: usize, h: usize) -> (f64, f64) {
    ((x / w as f64 - 0.5) * TAU, (0.5 - y / h as f64) * PI)
}

/// Bilinear sample with horizontal wrap (vertical clamp), pixel centres at +0.5.
pub(crate) fn sample_wrap(img: &Image, x: f64, y: f64) -> [f32; 4] {
    let (w, h) = (img.w as isize, img.h as isize);
    let fx = x - 0.5;
    let fy = (y - 0.5).clamp(0.0, (h - 1) as f64);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
    let (x0, y0) = (x0 as isize, y0 as isize);
    let g = |xx: isize, yy: isize| img.get(xx.rem_euclid(w) as usize, yy.clamp(0, h - 1) as usize);
    let (a, b, c, d) = (g(x0, y0), g(x0 + 1, y0), g(x0, y0 + 1), g(x0 + 1, y0 + 1));
    let mut o = [0.0; 4];
    for k in 0..4 {
        let top = a[k] + (b[k] - a[k]) * tx;
        let bot = c[k] + (d[k] - c[k]) * tx;
        o[k] = top + (bot - top) * ty;
    }
    o
}
pub(crate) fn sample_dir(img: &Image, d: V3) -> [f32; 4] {
    let (lon, lat) = lonlat(d);
    sample_wrap(
        img,
        (lon / TAU + 0.5) * img.w as f64,
        (0.5 - lat / PI) * img.h as f64,
    )
}

impl VrSphere {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.frame_layout <= 2, "invalid VR frame layout");
        ensure!(
            [self.tilt, self.pan, self.roll]
                .into_iter()
                .all(f32::is_finite),
            "invalid VR rotation"
        );
        ensure!(
            self.fov.is_finite() && (10. ..=360.).contains(&self.fov),
            "invalid VR field of view"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        image: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let r = rotation(self.tilt as f64, self.pan as f64, self.roll as f64);
        let fov = self.fov as f64;
        let ident =
            (0..3).all(|i| (0..3).all(|j| (r[i][j] - if i == j { 1. } else { 0. }).abs() < 1e-12));
        if ident && (fov - 360.).abs() < 1e-9 {
            return Ok(());
        }
        let rt = transpose(r);
        let (w, h) = image.dimensions();
        let input = image.pixels();
        let mut output = input.to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            let mut px = Vec::with_capacity(vw * vh);
            for y in 0..vh {
                px.extend_from_slice(&input[(y0 + y) * w + x0..(y0 + y) * w + x0 + vw]);
            }
            let source = Image { w: vw, h: vh, px };
            let (kx, ky) = (
                fov / 360.,
                if self.stretch {
                    (fov / 180.).min(1.)
                } else {
                    1.
                },
            );
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let (lon, lat) = px_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, vw, vh);
                    let d = mvec(&rt, dir(lon * kx, lat * ky));
                    output[(y0 + y) * w + x0 + x] = sample_dir(&source, d);
                }
            }
        }
        let mut i = 0;
        image.map_premultiplied(
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
        effect: String,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: serde_json::Value,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_sphere_working_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-sphere.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 324);
        for (i, c) in fixture.cases.into_iter().enumerate() {
            let mut parameters = c.parameters;
            let layout = parameters
                .as_object_mut()
                .unwrap()
                .remove("frame_layout")
                .unwrap();
            parameters["frameLayout"] = layout;
            let p: VrSphere = serde_json::from_value(parameters).unwrap();
            let mut image = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            p.apply_linear(&mut image, &mut || Ok(())).unwrap();
            assert_eq!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "{} case{}",
                c.effect,
                i
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrRotateSphere {
    pub frame_layout: u32,
    pub tilt: f32,
    pub pan: f32,
    pub roll: f32,
}
impl VrRotateSphere {
    pub fn projection(self) -> VrSphere {
        VrSphere {
            frame_layout: self.frame_layout,
            tilt: self.tilt,
            pan: self.pan,
            roll: self.roll,
            fov: 360.,
            stretch: false,
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.projection().validate()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrSphereChannel {
    Tilt,
    Pan,
    Roll,
    Fov,
}
pub type VrSphereAnimation =
    std::collections::BTreeMap<VrSphereChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &VrSphereAnimation, rotate: bool) -> Result<()> {
    for (channel, value) in animation {
        ensure!(
            !rotate || *channel != VrSphereChannel::Fov,
            "Rotate Sphere has no FOV channel"
        );
        value.validate()?;
        let (lo, hi) = if *channel == VrSphereChannel::Fov {
            (10., 360.)
        } else {
            (-(f32::MAX as f64), f32::MAX as f64)
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "VR animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: VrSphere,
    animation: &VrSphereAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
    rotate: bool,
) -> Result<VrSphere> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(animation, rotate)?;
    for (channel, value) in animation {
        let v = super::nested_composition::sample_value(value, local)? as f32;
        match channel {
            VrSphereChannel::Tilt => p.tilt = v,
            VrSphereChannel::Pan => p.pan = v,
            VrSphereChannel::Roll => p.roll = v,
            VrSphereChannel::Fov => p.fov = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn channels_sample_roundtrip_and_reject_projection_only_data() {
        let mut curves = serde_json::Map::new();
        for (name, a, b) in [
            ("tilt", -30., 60.),
            ("pan", -180., 180.),
            ("roll", -720., 360.),
            ("fov", 10., 360.),
        ] {
            curves.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":1000,"value":b}]}}));
        }
        let animation: VrSphereAnimation =
            serde_json::from_value(serde_json::Value::Object(curves)).unwrap();
        let restored: VrSphereAnimation =
            serde_json::from_str(&serde_json::to_string(&animation).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&animation).unwrap()
        );
        let p = sample_animation(
            VrSphere {
                frame_layout: 2,
                stretch: false,
                ..Default::default()
            },
            &animation,
            500000,
            false,
        )
        .unwrap();
        assert_eq!((p.tilt, p.pan, p.roll, p.fov), (15., 0., -180., 185.));
        assert_eq!(p.frame_layout, 2);
        assert!(!p.stretch);
        assert!(validate_animation(&animation, true).is_err());
        let mut angles = animation.clone();
        angles.remove(&VrSphereChannel::Fov);
        assert!(sample_animation(
            VrRotateSphere::default().projection(),
            &angles,
            500000,
            true
        )
        .is_ok());
        assert!(serde_json::from_value::<VrRotateSphere>(serde_json::json!({"fov":90})).is_err());
        let bad: VrSphereAnimation =
            serde_json::from_value(serde_json::json!({"fov":{"mode":"constant","value":9.}}))
                .unwrap();
        assert!(validate_animation(&bad, false).is_err());
    }
}
