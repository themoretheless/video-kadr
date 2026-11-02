//! Active Filmcraft Compound Blur; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{gaussian_blur::GaussianBlur, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CompoundBlur {
    pub layer: u32,
    pub max: f32,
    pub stretch: bool,
    pub invert: bool,
}
impl Default for CompoundBlur {
    fn default() -> Self {
        Self {
            layer: 0,
            max: 20.,
            stretch: true,
            invert: false,
        }
    }
}
pub use super::gradient_wipe::GradientLayer as CompoundLayer;
impl CompoundBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.layer <= 8 && self.max.is_finite() && (0. ..=500.).contains(&self.max),
            "Compound Blur parameter out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        layer: Option<CompoundLayer<'_>>,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Compound Blur pixel scale"
        );
        checkpoint()?;
        let max = self.max * px_scale * 0.5;
        if max <= 0.3 {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        let own = frame.clone();
        let levels = [0.0f32, 0.125, 0.25, 0.5, 1.];
        let mut stack = Vec::with_capacity(5);
        for k in levels {
            let mut c = own.clone();
            if k > 0. {
                GaussianBlur {
                    blurriness: 0.,
                    dimensions: 0,
                    repeat_edge: true,
                }
                .apply_sigma(&mut c, max * k, checkpoint)?;
            }
            stack.push(c);
        }
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let (x, y) = (index % w, index / w);
                let s = if self.layer > 0 {
                    if let Some(layer) = &layer {
                        let (tw, th) = layer.frame.dimensions();
                        let (px, py) = if self.stretch {
                            (layer.layer_to_output)(x as f64 + 0.5, y as f64 + 0.5)
                        } else {
                            (
                                (x as f64 + 0.5) / w as f64 * tw as f64,
                                (y as f64 + 0.5) / h as f64 * th as f64,
                            )
                        };
                        super::mosaic::sample(layer.frame.pixels(), tw, th, px as f32, py as f32)
                    } else {
                        own.pixels()[index]
                    }
                } else {
                    own.pixels()[index]
                };
                let l = super::basic_grade::display(
                    (0.2126 * s[0] + 0.7152 * s[1] + 0.0722 * s[2]).clamp(0., 1.),
                );
                let a = if self.invert { 1. - l } else { l };
                let a = a.clamp(0., 1.);
                let i = levels.partition_point(|&v| v <= a).clamp(1, 4);
                let t = ((a - levels[i - 1]) / (levels[i] - levels[i - 1])).clamp(0., 1.);
                let p0 = stack[i - 1].pixels()[index];
                let p1 = stack[i].pixels()[index];
                index += 1;
                std::array::from_fn(|c| p0[c] + (p1[c] - p0[c]) * t)
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
    fn compound_blur_own_external_stretch_invert_fallback_and_cancel() {
        let make = |w, h, rgba| {
            LinearColorFrame::from_surface(
                &GroupSurface {
                    width: w,
                    height: h,
                    rgba,
                },
                &mut || Ok(()),
            )
            .unwrap()
        };
        let own = make(
            6,
            4,
            (0..24)
                .flat_map(|i| {
                    if i % 3 == 0 {
                        [255, 255, 255, 128]
                    } else {
                        [64, 128, 192, 255]
                    }
                })
                .collect(),
        );
        let layer = make(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]);
        let mapping = |x: f64, y: f64| (x + 1., y);
        let mut identity = own.clone();
        CompoundBlur {
            max: 0.,
            ..Default::default()
        }
        .apply_linear(&mut identity, None, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(identity.pixels(), own.pixels());
        let mut outputs = vec![];
        for stretch in [false, true] {
            for invert in [false, true] {
                let mut out = own.clone();
                CompoundBlur {
                    layer: 1,
                    max: 4.,
                    stretch,
                    invert,
                }
                .apply_linear(
                    &mut out,
                    Some(CompoundLayer {
                        frame: &layer,
                        layer_to_output: &mapping,
                    }),
                    1.,
                    &mut || Ok(()),
                )
                .unwrap();
                outputs.push(out);
            }
        }
        assert_ne!(outputs[0].pixels(), outputs[1].pixels());
        assert_ne!(outputs[0].pixels(), outputs[2].pixels());
        let mut a = own.clone();
        let mut b = own.clone();
        CompoundBlur::default()
            .apply_linear(&mut a, None, 1., &mut || Ok(()))
            .unwrap();
        CompoundBlur {
            layer: 1,
            ..Default::default()
        }
        .apply_linear(&mut b, None, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), b.pixels());
        assert!(CompoundBlur {
            max: 501.,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(CompoundBlur::default()
            .apply_linear(&mut identity, None, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_compound_blur_own_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: CompoundBlur,
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
            "../../tests/fixtures/filmcraft-compound-blur.json"
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
                .apply_linear(&mut frame, None, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original Compound Blur own {index}"
            );
        }
    }
}

pub fn validate_animation(a: &super::composition::AnimatableValue) -> Result<()> {
    a.validate()?;
    ensure!(
        super::composition::animatable_values_in(a, 0., 500., true),
        "Compound Blur animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut p: CompoundBlur,
    a: &super::composition::AnimatableValue,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<CompoundBlur> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    p.max = super::nested_composition::sample_value(a, t)? as f32;
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn compound_blur_defaults_curve_flags_and_roundtrip() {
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"compound_blur","parameters":{"layer":2,"stretch":false,"invert":true},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":8.5}]}}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::CompoundBlur {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Compound Blur")
        };
        assert_eq!(
            parameters,
            CompoundBlur {
                layer: 2,
                stretch: false,
                invert: true,
                ..Default::default()
            }
        );
        assert_eq!(sample_animation(parameters, &a, 250000).unwrap().max, 4.25);
        assert_eq!(sample_animation(parameters, &a, 750000).unwrap().max, 8.5);
        assert!(
            validate_animation(&super::super::composition::AnimatableValue::Constant {
                value: 501.
            })
            .is_err()
        );
    }
}

#[cfg(test)]
mod external_original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn compound_blur_original_external_and_unavailable_layer_bits() {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Case {
            parameters: CompoundBlur,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
            track_rgba: Vec<u8>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            external: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-compound-blur.json"
        ))
        .unwrap();
        assert_eq!(fixture.external.len(), 192);
        for (index, c) in fixture.external.into_iter().enumerate() {
            let mut own = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            let track = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: 5,
                    height: 3,
                    rgba: c.track_rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            let mapping = |x: f64, y: f64| (x + 1.25, y - 0.75);
            let layer = if c.parameters.layer == 1 {
                Some(CompoundLayer {
                    frame: &track,
                    layer_to_output: &mapping,
                })
            } else {
                None
            };
            c.parameters
                .apply_linear(&mut own, layer, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                own.pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original Compound Blur external {index}"
            );
        }
    }
}
