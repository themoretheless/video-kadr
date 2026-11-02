//! Filmcraft Edge Feather; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgeFeather {
    pub amount: f32,
}
impl EdgeFeather {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0. ..=100.).contains(&self.amount),
            "Edge Feather amount out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let (w, h) = frame.dimensions();
        let fe = self.amount / 100. * w.min(h) as f32 * 0.5;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let px = (index % w) as f32 + 0.5;
                let py = (index / w) as f32 + 0.5;
                index += 1;
                let d = px.min(w as f32 - px).min(py).min(h as f32 - py);
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
    fn original_edge_feather_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: EdgeFeather,
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
            "../../tests/fixtures/filmcraft-edge-feather.json"
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
                "original EdgeFeather {index}"
            );
        }
    }
}

pub fn validate_animation(animation: &super::composition::AnimatableValue) -> Result<()> {
    animation.validate()?;
    ensure!(
        super::composition::animatable_values_in(animation, 0.0, 100.0, true),
        "EdgeFeather animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut parameters: EdgeFeather,
    animation: &super::composition::AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<EdgeFeather> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    parameters.validate()?;
    validate_animation(animation)?;
    parameters.amount = super::nested_composition::sample_value(animation, time)? as f32;
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn edge_feather_defaults_scalar_curve_roundtrip_and_bounds() {
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"edge_feather","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::EdgeFeather {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Edge Feather")
        };
        assert_eq!(parameters, EdgeFeather::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap().amount,
            50.
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap().amount,
            100.
        );
        for value in [-0.1, 100.1] {
            assert!(
                validate_animation(&super::super::composition::AnimatableValue::Constant { value })
                    .is_err()
            );
            assert!(EdgeFeather {
                amount: value as f32
            }
            .validate()
            .is_err());
        }
    }
}
