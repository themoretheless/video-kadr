//! Filmcraft Transform; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TransformEffect {
    pub anchor: Option<[Option<f64>; 2]>,
    pub position: Option<[Option<f64>; 2]>,
    pub uniform_scale: bool,
    pub scale_height: f32,
    pub scale_width: f32,
    pub skew: f32,
    pub skew_axis: f32,
    pub rotation: f32,
    pub opacity: f32,
    pub shutter_angle: f32,
}
impl Default for TransformEffect {
    fn default() -> Self {
        Self {
            anchor: None,
            position: None,
            uniform_scale: true,
            scale_height: 100.,
            scale_width: 100.,
            skew: 0.,
            skew_axis: 0.,
            rotation: 0.,
            opacity: 100.,
            shutter_angle: 0.,
        }
    }
}
impl TransformEffect {
    pub fn validate(&self) -> Result<()> {
        for (v, lo, hi) in [
            (self.scale_height, 0., 10000.),
            (self.scale_width, 0., 10000.),
            (self.skew, -70., 70.),
            (self.opacity, 0., 100.),
            (self.shutter_angle, 0., 360.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "Transform parameter out of range"
            );
        }
        ensure!(
            self.skew_axis.is_finite() && self.rotation.is_finite(),
            "nonfinite Transform angle"
        );
        for p in [self.anchor, self.position].into_iter().flatten() {
            ensure!(
                p.into_iter().flatten().all(f64::is_finite),
                "nonfinite Transform point"
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
        ensure!(scale.is_finite() && scale > 0., "invalid Transform scale");
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let point = |p: Option<[Option<f64>; 2]>| {
            let p = p.unwrap_or([None, None]);
            [
                p[0].map(|v| v * scale as f64).unwrap_or(w as f64 / 2.),
                p[1].map(|v| v * scale as f64).unwrap_or(h as f64 / 2.),
            ]
        };
        let anchor = point(self.anchor);
        let pos = point(self.position);
        let rotate = |deg: f64| {
            let (s, c) = deg.to_radians().sin_cos();
            GroupAffine([c, s, -s, c, 0., 0.])
        };
        let sh = self.scale_height as f64 / 100.;
        let sw = if self.uniform_scale {
            sh
        } else {
            self.scale_width as f64 / 100.
        };
        let sk = rotate(self.skew_axis as f64)
            .compose(GroupAffine([
                1.,
                0.,
                (self.skew as f64).to_radians().tan(),
                1.,
                0.,
                0.,
            ]))?
            .compose(rotate(-(self.skew_axis as f64)))?;
        let m = GroupAffine([1., 0., 0., 1., pos[0], pos[1]])
            .compose(rotate(self.rotation as f64))?
            .compose(sk)?
            .compose(GroupAffine([sw, 0., 0., sh, 0., 0.]))?
            .compose(GroupAffine([1., 0., 0., 1., -anchor[0], -anchor[1]]))?;
        let [a, b, c, d, _, _] = m.0;
        if (a * d - b * c).abs() < 1e-12 {
            return frame.map_premultiplied(&mut |_| [0.; 4], checkpoint);
        }
        let mut out = frame.transformed(w, h, m, checkpoint)?;
        let opacity = self.opacity / 100.;
        out.map_premultiplied(&mut |p| p.map(|v| v * opacity), checkpoint)?;
        *frame = out;
        Ok(())
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_transform_effect_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: TransformEffect,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-transform-effect.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 96);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
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
                "original TransformEffect {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransformChannel {
    ScaleHeight,
    ScaleWidth,
    Skew,
    SkewAxis,
    Rotation,
    Opacity,
    ShutterAngle,
    AnchorX,
    AnchorY,
    PositionX,
    PositionY,
}
pub type TransformAnimation =
    std::collections::BTreeMap<TransformChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &TransformAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            TransformChannel::ScaleHeight | TransformChannel::ScaleWidth => (0., 10000.),
            TransformChannel::Skew => (-70., 70.),
            TransformChannel::Opacity => (0., 100.),
            TransformChannel::ShutterAngle => (0., 360.),
            TransformChannel::SkewAxis | TransformChannel::Rotation => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Transform animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: TransformEffect,
    a: &TransformAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<TransformEffect> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            TransformChannel::ScaleHeight => p.scale_height = v as f32,
            TransformChannel::ScaleWidth => p.scale_width = v as f32,
            TransformChannel::Skew => p.skew = v as f32,
            TransformChannel::SkewAxis => p.skew_axis = v as f32,
            TransformChannel::Rotation => p.rotation = v as f32,
            TransformChannel::Opacity => p.opacity = v as f32,
            TransformChannel::ShutterAngle => p.shutter_angle = v as f32,
            TransformChannel::AnchorX => p.anchor.get_or_insert([None, None])[0] = Some(v),
            TransformChannel::AnchorY => p.anchor.get_or_insert([None, None])[1] = Some(v),
            TransformChannel::PositionX => p.position.get_or_insert([None, None])[0] = Some(v),
            TransformChannel::PositionY => p.position.get_or_insert([None, None])[1] = Some(v),
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn transform_eleven_curves_auto_axes_precision_defaults_roundtrip_and_bounds() {
        let controls = [
            ("scaleHeight", 100., 50.),
            ("scaleWidth", 100., 125.),
            ("skew", 0., 12.5),
            ("skewAxis", 0., 37.25),
            ("rotation", 0., -37.25),
            ("opacity", 100., 62.5),
            ("shutterAngle", 0., 180.),
            ("anchorX", 1.123456789012, 1.123456789012),
            ("anchorY", 0., 2.),
            ("positionX", 2., 4.),
            ("positionY", -0.75, 0.75),
        ];
        let animation:serde_json::Map<String,serde_json::Value>=controls.iter().map(|(n,a,b)|((*n).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}))).collect();
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"transform","parameters":{"uniformScale":false},"animation":animation})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Transform {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Transform")
        };
        assert_eq!(
            parameters,
            TransformEffect {
                uniform_scale: false,
                ..Default::default()
            }
        );
        assert_eq!(a.len(), 11);
        for time in [0, 250000, 500000, 750000] {
            let p = sample_animation(parameters, &a, time).unwrap();
            assert!(!p.uniform_scale);
            let v = serde_json::to_value(p).unwrap();
            for (n, start, end) in controls {
                let expected = start + (end - start) * (time.min(500000) as f64 / 500000.);
                let actual = match n {
                    "anchorX" => v["anchor"][0].as_f64().unwrap(),
                    "anchorY" => v["anchor"][1].as_f64().unwrap(),
                    "positionX" => v["position"][0].as_f64().unwrap(),
                    "positionY" => v["position"][1].as_f64().unwrap(),
                    _ => v[n].as_f64().unwrap(),
                };
                assert_eq!(
                    actual,
                    if n.starts_with("anchor") || n.starts_with("position") {
                        expected
                    } else {
                        expected as f32 as f64
                    },
                    "{n}/{time}"
                );
            }
        }
        let x = TransformAnimation::from([(
            TransformChannel::AnchorX,
            a[&TransformChannel::AnchorX].clone(),
        )]);
        assert_eq!(
            sample_animation(parameters, &x, 250000).unwrap().anchor,
            Some([Some(1.123456789012), None])
        );
        assert_eq!(
            sample_animation(parameters, &x, 250000).unwrap().position,
            None
        );
        for (c, v) in [
            (TransformChannel::ScaleHeight, -0.01),
            (TransformChannel::ScaleWidth, 10000.01),
            (TransformChannel::Skew, 70.01),
            (TransformChannel::Opacity, 100.01),
            (TransformChannel::ShutterAngle, 360.01),
            (TransformChannel::Rotation, f64::MAX),
        ] {
            assert!(validate_animation(&TransformAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
