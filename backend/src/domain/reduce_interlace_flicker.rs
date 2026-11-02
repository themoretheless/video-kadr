//! Filmcraft Reduce Interlace Flicker: vertical Gaussian; MIT docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReduceInterlaceFlicker {
    pub softness: f32,
}
impl ReduceInterlaceFlicker {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.softness.is_finite() && (0.0..=10.0).contains(&self.softness),
            "Reduce Interlace Flicker softness out of range"
        );
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
            "invalid interlace pixel scale"
        );
        super::gaussian_blur::GaussianBlur {
            dimensions: 2,
            repeat_edge: true,
            blurriness: 0.0,
        }
        .apply_sigma(frame, self.softness * px_scale.max(0.25), checkpoint)
    }
}
pub fn validate_animation(animation: &super::composition::AnimatableValue) -> Result<()> {
    animation.validate()?;
    ensure!(
        super::composition::animatable_values_in(animation, 0.0, 10.0, true),
        "Reduce Interlace Flicker animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut parameters: ReduceInterlaceFlicker,
    animation: &super::composition::AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ReduceInterlaceFlicker> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.softness = super::nested_composition::sample_value(animation, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reduce_interlace_flicker_persistence_curve_axis_scale_and_cancel() {
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"reduce_interlace_flicker","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":4}]}}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::ReduceInterlaceFlicker {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Interlace")
        };
        let p = sample_animation(parameters, &a, 250000).unwrap();
        assert_eq!(p.softness, 2.);
        let frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: (0..24)
                    .flat_map(|i| {
                        [
                            if i % 6 < 3 { 255 } else { 0 },
                            if i / 6 < 2 { 255 } else { 0 },
                            0,
                            255,
                        ]
                    })
                    .collect(),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut x = frame.clone();
        let mut y = frame.clone();
        p.apply_linear(&mut x, 0.1, &mut || Ok(())).unwrap();
        p.apply_linear(&mut y, 0.25, &mut || Ok(())).unwrap();
        assert_eq!(x.pixels(), y.pixels());
        for (a, b) in x.pixels().iter().zip(frame.pixels()) {
            assert_eq!(a[0], b[0]);
            assert_eq!(a[3], b[3]);
        }
        assert!(p
            .apply_linear(&mut y, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
        assert!(ReduceInterlaceFlicker { softness: 10.1 }
            .validate()
            .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: ReduceInterlaceFlicker,
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
            "../../tests/fixtures/filmcraft-reduce-interlace-flicker.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 80);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
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
                "Reduce Interlace Flicker original working bits case{index}"
            );
        }
    }
}
