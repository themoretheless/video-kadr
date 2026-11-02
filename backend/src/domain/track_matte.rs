//! Track Matte Key working-frame kernel, ported from Filmcraft (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackMatte {
    pub matte: u32,
    pub composite: u32,
    pub reverse: bool,
}
/// Selected output-sized track, mapped from the current working layer.
pub struct MatteLayer<'a> {
    pub frame: &'a LinearColorFrame,
    pub layer_to_output: &'a dyn Fn(f64, f64) -> (f64, f64),
}
impl TrackMatte {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.matte as usize <= super::composition::MAX_VISIBLE_LAYERS && self.composite <= 1,
            "invalid Track Matte choice"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        layer: Option<MatteLayer<'_>>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        if self.matte == 0 {
            return Ok(());
        }
        let Some(layer) = layer else {
            return Ok(());
        };
        let (w, _) = frame.dimensions();
        let mut index = 0;
        let mut output = frame.clone();
        output.map_premultiplied(
            &mut |pixel| {
                let (x, y) = (index % w, index / w);
                index += 1;
                let (qx, qy) = (layer.layer_to_output)(x as f64 + 0.5, y as f64 + 0.5);
                let sample =
                    super::gradient_wipe::transparent_sample(layer.frame, qx as f32, qy as f32);
                let mut value = if self.composite == 1 {
                    super::basic_grade::display(
                        (0.2126 * sample[0] + 0.7152 * sample[1] + 0.0722 * sample[2])
                            .clamp(0., 1.),
                    )
                } else {
                    sample[3]
                };
                if self.reverse {
                    value = 1. - value;
                }
                pixel.map(|channel| channel * value.clamp(0., 1.))
            },
            checkpoint,
        )?;
        checkpoint()?;
        *frame = output;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_working_frame_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-track-matte.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 720);
        for (index, case) in cases.iter().enumerate() {
            let make = |width, height, rgba: &serde_json::Value| {
                LinearColorFrame::from_surface(
                    &GroupSurface {
                        width,
                        height,
                        rgba: serde_json::from_value(rgba.clone()).unwrap(),
                    },
                    &mut || Ok(()),
                )
                .unwrap()
            };
            let mut frame = make(
                case["width"].as_u64().unwrap() as usize,
                case["height"].as_u64().unwrap() as usize,
                &case["rgba"],
            );
            let track = make(6, 4, &case["trackRgba"]);
            let m: [f64; 6] = serde_json::from_value(case["matrix"].clone()).unwrap();
            let transform =
                |x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            let parameters: TrackMatte =
                serde_json::from_value(case["parameters"].clone()).unwrap();
            let layer = (case["environment"] == 1).then_some(MatteLayer {
                frame: &track,
                layer_to_output: &transform,
            });
            parameters
                .apply_linear(&mut frame, layer, &mut || Ok(()))
                .unwrap();
            let actual: Vec<u32> = frame
                .pixels()
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect();
            let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(actual, expected, "case {index}");
        }
    }
    #[test]
    fn cancellation_preserves_input_at_every_checkpoint() {
        let source = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 6,
                height: 4,
                rgba: vec![128; 96],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let identity = |x, y| (x, y);
        let parameters = TrackMatte {
            matte: 1,
            composite: 1,
            reverse: true,
        };
        let mut count = 0;
        parameters
            .apply_linear(
                &mut source.clone(),
                Some(MatteLayer {
                    frame: &source,
                    layer_to_output: &identity,
                }),
                &mut || {
                    count += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert!(count > 4);
        for stop in 1..=count {
            let mut frame = source.clone();
            let mut seen = 0;
            assert!(parameters
                .apply_linear(
                    &mut frame,
                    Some(MatteLayer {
                        frame: &source,
                        layer_to_output: &identity
                    }),
                    &mut || {
                        seen += 1;
                        anyhow::ensure!(seen != stop, "cancelled");
                        Ok(())
                    }
                )
                .is_err());
            assert_eq!(frame.pixels(), source.pixels(), "checkpoint {stop}");
        }
    }
}

#[cfg(test)]
mod layer_namespace_tests {
    use super::*;
    #[test]
    fn admits_every_native_visual_lane_and_refuses_outside_namespace() {
        for matte in 0..=super::super::composition::MAX_VISIBLE_LAYERS as u32 {
            assert!(TrackMatte { matte, ..Default::default() }.validate().is_ok());
        }
        assert!(TrackMatte { matte: super::super::composition::MAX_VISIBLE_LAYERS as u32 + 1, ..Default::default() }.validate().is_err());
    }
}
