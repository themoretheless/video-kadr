//! Filmcraft active 3D Rotate CPU warp. MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Rotate3D {
    pub rot_x: f32,
    pub rot_y: f32,
    pub rot_z: f32,
    pub perspective: f32,
    pub z: f32,
    pub hide_back: bool,
}
impl Default for Rotate3D {
    fn default() -> Self {
        Self {
            rot_x: 0.,
            rot_y: 0.,
            rot_z: 0.,
            perspective: 50.,
            z: 0.,
            hide_back: false,
        }
    }
}
impl Rotate3D {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.rot_x, self.rot_y, self.rot_z]
                .into_iter()
                .all(f32::is_finite),
            "nonfinite 3D Rotate angle"
        );
        ensure!(
            self.perspective.is_finite() && (0.0..=100.).contains(&self.perspective),
            "3D Rotate perspective out of range"
        );
        ensure!(
            self.z.is_finite() && (-10000.0..=10000.).contains(&self.z),
            "3D Rotate Z out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid 3D Rotate pixel scale"
        );
        checkpoint()?;
        let (width, height) = frame.dimensions();
        let (ax, ay, az) = (
            (self.rot_x as f64).to_radians(),
            (self.rot_y as f64).to_radians(),
            (self.rot_z as f64).to_radians(),
        );
        let z = self.z as f64 * px_scale as f64;
        if ax.abs() < 1e-9 && ay.abs() < 1e-9 && az.abs() < 1e-9 && z.abs() < 1e-9 {
            return Ok(());
        }
        let persp = self.perspective.clamp(0.0, 100.0) as f64;
        let (w, h) = (width as f64, height as f64);
        let f = if persp <= 0.0 {
            1e7
        } else {
            w.max(h) * (0.6 + (100.0 - persp) / 100.0 * 4.0)
        };
        let hide = self.hide_back;
        let (sx, cxr) = ax.sin_cos();
        let (sy, cy) = ay.sin_cos();
        let (sz, cz) = az.sin_cos();
        // R = Rz · Ry · Rx (screen y down; positive Z is clockwise like Rotation)
        let rx = [[1.0, 0.0, 0.0], [0.0, cxr, -sx], [0.0, sx, cxr]];
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        let mul = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
            let mut o = [[0.0; 3]; 3];
            for i in 0..3 {
                for j in 0..3 {
                    o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
                }
            }
            o
        };
        let r = mul(rz, mul(ry, rx));
        let u = [r[0][0], r[1][0], r[2][0]];
        let v = [r[0][1], r[1][1], r[2][1]];
        let n = [r[0][2], r[1][2], r[2][2]];
        let back = n[2] < 0.0;
        if hide && back {
            frame.map_premultiplied(&mut |_| [0.; 4], checkpoint)?;
            return Ok(());
        }
        let c = [0.0, 0.0, f + z];
        let nc = n[0] * c[0] + n[1] * c[1] + n[2] * c[2];
        let pixels = frame.pixels().to_vec();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let (x, y) = ((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
                index += 1;
                let d = [x - w / 2.0, y - h / 2.0, f];
                let den = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
                if den.abs() < 1e-12 {
                    return [0.; 4];
                }
                let t = nc / den;
                if t <= 0.0 {
                    return [0.; 4];
                }
                let l = [d[0] * t - c[0], d[1] * t - c[1], d[2] * t - c[2]];
                let su = l[0] * u[0] + l[1] * u[1] + l[2] * u[2];
                let sv = l[0] * v[0] + l[1] * v[1] + l[2] * v[2];
                super::twirl::sample(
                    &pixels,
                    width,
                    height,
                    (su + w / 2.) as f32,
                    (sv + h / 2.) as f32,
                )
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn identity_back_face_camera_plane_and_cancellation() {
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 2,
                height: 2,
                rgba: vec![128; 16],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let original = frame.clone();
        Rotate3D::default()
            .apply_linear(&mut frame, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels(), original.pixels());
        Rotate3D {
            rot_x: 180.,
            hide_back: true,
            ..Default::default()
        }
        .apply_linear(&mut frame, 1., &mut || Ok(()))
        .unwrap();
        assert!(frame.pixels().iter().all(|p| *p == [0.; 4]));
        frame = original.clone();
        Rotate3D {
            z: -100.,
            ..Default::default()
        }
        .apply_linear(&mut frame, 1., &mut || Ok(()))
        .unwrap();
        assert!(frame.pixels().iter().all(|p| *p == [0.; 4]));
        assert!(Rotate3D::default()
            .apply_linear(&mut frame, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
        assert!(Rotate3D {
            rot_z: f32::INFINITY,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rotate3DChannel {
    RotX,
    RotY,
    RotZ,
    Perspective,
    Z,
}
pub type Rotate3DAnimation =
    std::collections::BTreeMap<Rotate3DChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &Rotate3DAnimation) -> Result<()> {
    for (channel, value) in a {
        value.validate()?;
        let (lo, hi) = match channel {
            Rotate3DChannel::Perspective => (0., 100.),
            Rotate3DChannel::Z => (-10000., 10000.),
            _ => (-f32::MAX as f64, f32::MAX as f64),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "3D Rotate animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Rotate3D, a: &Rotate3DAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Rotate3D> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (channel, value) in a {
        let value = super::nested_composition::sample_value(value, time)? as f32;
        match channel {
            Rotate3DChannel::RotX => p.rot_x = value,
            Rotate3DChannel::RotY => p.rot_y = value,
            Rotate3DChannel::RotZ => p.rot_z = value,
            Rotate3DChannel::Perspective => p.perspective = value,
            Rotate3DChannel::Z => p.z = value,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn five_curves_roundtrip_and_local_midpoints() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"rotate_3d","parameters":{"hideBack":true},"animation":{"rotX":curve(0.,180.),"rotY":curve(-45.,45.),"rotZ":curve(0.,90.),"perspective":curve(0.,100.),"z":curve(-100.,100.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Rotate3D {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("3D Rotate")
        };
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Rotate3D {
                rot_x: 90.,
                rot_y: 0.,
                rot_z: 45.,
                perspective: 50.,
                z: 0.,
                hide_back: true
            }
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Case {
        parameters: Rotate3D,
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
    #[test]
    fn original_working_frame_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-rotate-3d.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 384);
        for (index, case) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: case.width,
                    height: case.height,
                    rgba: case.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            case.parameters
                .apply_linear(&mut frame, case.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                case.bits,
                "original 3D Rotate {index}"
            );
        }
    }
}
