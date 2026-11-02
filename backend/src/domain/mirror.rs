//! Filmcraft Mirror; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Mirror {
    pub center: Option<[Option<f64>; 2]>,
    pub angle: f32,
}
impl Mirror {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.angle.is_finite(), "nonfinite Mirror angle");
        if let Some(c) = self.center {
            ensure!(
                c.into_iter().flatten().all(f64::is_finite),
                "nonfinite Mirror center"
            );
        }
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
            "invalid Mirror pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let c = self.center.unwrap_or([None, None]);
        let cx = c[0].map(|v| v * px_scale as f64).unwrap_or(w as f64 / 2.) as f32;
        let cy = c[1].map(|v| v * px_scale as f64).unwrap_or(h as f64 / 2.) as f32;
        ensure!(
            cx.is_finite() && cy.is_finite(),
            "Mirror center outside working range"
        );
        let angle = (self.angle as f64).to_radians();
        let (nx, ny) = (angle.cos() as f32 as f64, angle.sin() as f32 as f64);
        let (cx, cy) = (cx as f64, cy as f64);
        let source = frame.clone();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |p| {
                let (px, py) = ((index % w) as f64 + 0.5, (index / w) as f64 + 0.5);
                index += 1;
                let d = (px - cx) * nx + (py - cy) * ny;
                if d > 0. {
                    super::gradient_wipe::transparent_sample(
                        &source,
                        (px - 2. * d * nx) as f32,
                        (py - 2. * d * ny) as f32,
                    )
                } else {
                    p
                }
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
    fn original_mirror_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: Mirror,
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
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-mirror.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 288);
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
                "original Mirror {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MirrorChannel {
    Angle,
    CenterX,
    CenterY,
}
pub type MirrorAnimation =
    std::collections::BTreeMap<MirrorChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &MirrorAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let bound = if *c == MirrorChannel::Angle {
            f32::MAX as f64
        } else {
            f64::MAX
        };
        ensure!(
            super::composition::animatable_values_in(v, -bound, bound, true),
            "Mirror animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Mirror, a: &MirrorAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Mirror> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            MirrorChannel::Angle => p.angle = v as f32,
            MirrorChannel::CenterX | MirrorChannel::CenterY => {
                let center = p.center.get_or_insert([None, None]);
                center[if *c == MirrorChannel::CenterX { 0 } else { 1 }] = Some(v)
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
    fn mirror_three_curves_precise_center_auto_axis_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let precise = 3.123456789012;
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"mirror","parameters":{},"animation":{"angle":curve(-37.,74.),"centerX":curve(precise,precise),"centerY":curve(0.,2.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Mirror {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Mirror")
        };
        assert_eq!(parameters, Mirror::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Mirror {
                angle: 18.5,
                center: Some([Some(precise), Some(1.)])
            }
        );
        assert_eq!(sample_animation(parameters, &a, 750000).unwrap().angle, 74.);
        let x =
            MirrorAnimation::from([(MirrorChannel::CenterX, a[&MirrorChannel::CenterX].clone())]);
        assert_eq!(
            sample_animation(parameters, &x, 250000).unwrap().center,
            Some([Some(precise), None])
        );
        assert!(validate_animation(&MirrorAnimation::from([(
            MirrorChannel::Angle,
            super::super::composition::AnimatableValue::Constant { value: f64::MAX }
        )]))
        .is_err());
    }
}
