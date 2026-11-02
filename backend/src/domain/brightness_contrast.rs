//! Filmcraft Brightness & Contrast; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BrightnessContrast {
    pub brightness: f32,
    pub contrast: f32,
}
impl BrightnessContrast {
    pub fn validate(&self) -> Result<()> {
        for v in [self.brightness, self.contrast] {
            ensure!(
                v.is_finite() && (-100. ..=100.).contains(&v),
                "Brightness & Contrast parameter out of range"
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
        let br = self.brightness / 100. * 0.4;
        let co = 1. + self.contrast / 100.;
        frame.map_signal(
            &mut |c, _, _| Ok(c.map(|v| (v - 0.5) * co + 0.5 + br)),
            checkpoint,
        )
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    use crate::domain::linear_color_frame::LinearColorFrame;
    #[test]
    fn original_brightness_contrast_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: BrightnessContrast,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-brightness-contrast.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 100);
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
                "original BrightnessContrast {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrightnessContrastChannel {
    Brightness,
    Contrast,
}
pub type BrightnessContrastAnimation =
    std::collections::BTreeMap<BrightnessContrastChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &BrightnessContrastAnimation) -> Result<()> {
    for v in a.values() {
        v.validate()?;
        ensure!(
            super::composition::animatable_values_in(v, -100., 100., true),
            "Brightness & Contrast animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: BrightnessContrast,
    a: &BrightnessContrastAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<BrightnessContrast> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            BrightnessContrastChannel::Brightness => p.brightness = v,
            BrightnessContrastChannel::Contrast => p.contrast = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn brightness_contrast_defaults_two_curves_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"brightness_contrast","parameters":{},"animation":{"brightness":curve(-100.,100.),"contrast":curve(100.,-100.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::BrightnessContrast {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Brightness & Contrast")
        };
        assert_eq!(parameters, BrightnessContrast::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            parameters
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap(),
            BrightnessContrast {
                brightness: 100.,
                contrast: -100.
            }
        );
        for c in [
            BrightnessContrastChannel::Brightness,
            BrightnessContrastChannel::Contrast,
        ] {
            for value in [-100.1, 100.1] {
                assert!(validate_animation(&BrightnessContrastAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value }
                )]))
                .is_err());
            }
        }
    }
}
