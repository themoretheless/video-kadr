//! Filmcraft Crop; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Crop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub zoom: bool,
    pub feather: f32,
}
impl Crop {
    pub fn validate(&self) -> Result<()> {
        for v in [self.left, self.top, self.right, self.bottom] {
            ensure!(
                v.is_finite() && (0. ..=100.).contains(&v),
                "Crop percentage out of range"
            );
        }
        ensure!(
            self.feather.is_finite() && (0. ..=1000.).contains(&self.feather),
            "Crop feather out of range"
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
            "invalid Crop pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let (l, t, r, b) = (
            self.left / 100.,
            self.top / 100.,
            self.right / 100.,
            self.bottom / 100.,
        );
        if self.zoom && l + r < 0.99 && t + b < 0.99 {
            let sx = 1. / (1. - (l + r) as f64);
            let sy = 1. / (1. - (t + b) as f64);
            *frame = frame.transformed(
                w,
                h,
                GroupAffine([
                    sx,
                    0.,
                    0.,
                    sy,
                    sx * (-(l as f64) * w as f64),
                    sy * (-(t as f64) * h as f64),
                ]),
                checkpoint,
            )?;
            return Ok(());
        }
        let (x0, x1, y0, y1) = (
            l * w as f32,
            w as f32 * (1. - r),
            t * h as f32,
            h as f32 * (1. - b),
        );
        let fe = (self.feather * px_scale).max(0.);
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let px = (index % w) as f32 + 0.5;
                let py = (index / w) as f32 + 0.5;
                index += 1;
                let d = (px - x0).min(x1 - px).min(py - y0).min(y1 - py);
                let a = if fe > 0. {
                    (d / fe).clamp(0., 1.)
                } else {
                    (d + 0.5).clamp(0., 1.)
                };
                if a < 1. {
                    for v in &mut p {
                        *v *= a;
                    }
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
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_crop_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: Crop,
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
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-crop.json")).unwrap();
        assert_eq!(fixture.cases.len(), 192);
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
                "original Crop {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CropChannel {
    Left,
    Top,
    Right,
    Bottom,
    Feather,
}
pub type CropAnimation =
    std::collections::BTreeMap<CropChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &CropAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let max = if *c == CropChannel::Feather {
            1000.
        } else {
            100.
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., max, true),
            "Crop animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Crop, a: &CropAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Crop> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            CropChannel::Left => p.left = v,
            CropChannel::Top => p.top = v,
            CropChannel::Right => p.right = v,
            CropChannel::Bottom => p.bottom = v,
            CropChannel::Feather => p.feather = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn crop_defaults_five_curves_zoom_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"crop","parameters":{"zoom":true},"animation":{"left":curve(0.,20.),"top":curve(0.,40.),"right":curve(0.,60.),"bottom":curve(0.,80.),"feather":curve(0.,1000.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Crop {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Crop")
        };
        assert_eq!(
            parameters,
            Crop {
                zoom: true,
                ..Default::default()
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Crop {
                left: 10.,
                top: 20.,
                right: 30.,
                bottom: 40.,
                feather: 500.,
                zoom: true
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap(),
            Crop {
                left: 20.,
                top: 40.,
                right: 60.,
                bottom: 80.,
                feather: 1000.,
                zoom: true
            }
        );
        for c in [
            CropChannel::Left,
            CropChannel::Top,
            CropChannel::Right,
            CropChannel::Bottom,
            CropChannel::Feather,
        ] {
            for value in [
                -0.1,
                if c == CropChannel::Feather {
                    1000.1
                } else {
                    100.1
                },
            ] {
                assert!(validate_animation(&CropAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value }
                )]))
                .is_err());
            }
        }
    }
}
