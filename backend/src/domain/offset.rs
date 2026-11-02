//! Filmcraft Offset; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Offset {
    pub shift: Option<[Option<f64>; 2]>,
    pub blend: f32,
}
impl Offset {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.blend.is_finite() && (0. ..=100.).contains(&self.blend),
            "Offset blend out of range"
        );
        if let Some(s) = self.shift {
            ensure!(
                s.into_iter().flatten().all(f64::is_finite),
                "nonfinite Offset shift"
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
        ensure!(
            scale.is_finite() && scale > 0.,
            "invalid Offset pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let s = self.shift.unwrap_or([None, None]);
        let sx = s[0].map(|v| v * scale as f64).unwrap_or(w as f64 / 2.);
        let sy = s[1].map(|v| v * scale as f64).unwrap_or(h as f64 / 2.);
        ensure!(
            sx.is_finite() && sy.is_finite(),
            "Offset shift outside working range"
        );
        let dx = (sx - w as f64 / 2.).rem_euclid(w.max(1) as f64) as f32 as f64;
        let dy = (sy - h as f64 / 2.).rem_euclid(h.max(1) as f64) as f32 as f64;
        let blend = self.blend / 100.;
        let source = frame.clone();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |o| {
                let x = index % w;
                let y = index / w;
                index += 1;
                let u = (x as f64 + 0.5 - dx).rem_euclid(w as f64) as f32;
                let v = (y as f64 + 0.5 - dy).rem_euclid(h as f64) as f32;
                let p = super::mosaic::sample(source.pixels(), w, h, u, v);
                std::array::from_fn(|k| p[k] + (o[k] - p[k]) * blend)
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
    fn original_offset_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: Offset,
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
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-offset.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 160);
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
                "original Offset {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OffsetChannel {
    Blend,
    ShiftX,
    ShiftY,
}
pub type OffsetAnimation =
    std::collections::BTreeMap<OffsetChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &OffsetAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let bound = if *c == OffsetChannel::Blend {
            100.
        } else {
            f64::MAX
        };
        ensure!(
            super::composition::animatable_values_in(
                v,
                if *c == OffsetChannel::Blend {
                    0.
                } else {
                    -bound
                },
                bound,
                true
            ),
            "Offset animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Offset, a: &OffsetAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Offset> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            OffsetChannel::Blend => p.blend = v as f32,
            OffsetChannel::ShiftX | OffsetChannel::ShiftY => {
                let shift = p.shift.get_or_insert([None, None]);
                shift[if *c == OffsetChannel::ShiftX { 0 } else { 1 }] = Some(v)
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
    fn offset_three_curves_auto_axis_precision_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"offset","parameters":{},"animation":{"blend":curve(0.,100.),"shiftX":curve(3.123456789012,3.123456789012),"shiftY":curve(0.,2.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Offset {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Offset")
        };
        assert_eq!(parameters, Offset::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Offset {
                blend: 50.,
                shift: Some([Some(3.123456789012), Some(1.)])
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap().blend,
            100.
        );
        let x = OffsetAnimation::from([(OffsetChannel::ShiftX, a[&OffsetChannel::ShiftX].clone())]);
        assert_eq!(
            sample_animation(parameters, &x, 250000).unwrap().shift,
            Some([Some(3.123456789012), None])
        );
        for value in [-0.01, 100.01] {
            assert!(validate_animation(&OffsetAnimation::from([(
                OffsetChannel::Blend,
                super::super::composition::AnimatableValue::Constant { value }
            )]))
            .is_err());
        }
    }
}
