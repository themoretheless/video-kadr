//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat display-luma alpha key; pinned concat.luma-key/effect.wgsl contract.
//! Native renderer registered; strict RGB transfer precision remains an open gate.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LumaKey {
    pub threshold: f32,
    pub softness: f32,
    pub invert: f32,
}
impl Default for LumaKey {
    fn default() -> Self {
        Self {
            threshold: 10.,
            softness: 10.,
            invert: 0.,
        }
    }
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
impl LumaKey {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.threshold.is_finite() && (0. ..=100.).contains(&self.threshold),
            "Concat luma threshold out of range"
        );
        ensure!(
            self.softness.is_finite() && (0. ..=50.).contains(&self.softness),
            "Concat luma softness out of range"
        );
        ensure!(
            self.invert.is_finite() && (0. ..=1.).contains(&self.invert),
            "Concat luma invert out of range"
        );
        Ok(())
    }
    pub fn display_signal(&self, input: [f32; 4]) -> [f32; 4] {
        let threshold = self.threshold / 100.;
        let softness = (self.softness / 100.).max(0.001);
        let level = input[0] * 0.2126 + input[1] * 0.7152 + input[2] * 0.0722;
        let keep = if self.invert > 0.5 {
            1. - smooth(threshold, threshold + softness, level)
        } else {
            smooth(threshold - softness, threshold, level)
        };
        [input[0], input[1], input[2], input[3] * keep]
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        frame.map_concat_straight_pixels(
            &mut |input, _, _| {
                let mut sample = input.map(|v| half::f16::from_f32(v).to_f32());
                for value in &mut sample[..3] {
                    *value = super::concat_chroma::display(*value);
                }
                let mut output = self.display_signal(sample);
                for value in &mut output[..3] {
                    *value = super::concat_working::texture_half(
                        super::concat_chroma::linear(*value).clamp(-65504., 65504.),
                    );
                }
                output[3] = super::concat_working::texture_half(output[3]);
                output
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_softness_inversion_and_color_retention() {
        let p = LumaKey::default();
        p.validate().unwrap();
        assert_eq!(p.display_signal([0., 0., 0., 1.]), [0.; 4]);
        assert_eq!(p.display_signal([0.5, 0.5, 0.5, 0.3]), [0.5, 0.5, 0.5, 0.3]);
        let input = [0.05, 0.05, 0.05, 0.6];
        let output = p.display_signal(input);
        assert_eq!(&output[..3], &input[..3]);
        assert!((output[3] - 0.3).abs() < 1e-6);
        let inverse = LumaKey { invert: 1., ..p };
        assert_eq!(inverse.display_signal([0., 0., 0., 0.6])[3], 0.6);
        assert_eq!(inverse.display_signal([1.; 4])[3], 0.);
        assert_eq!(
            LumaKey {
                threshold: 0.,
                softness: 0.,
                ..p
            }
            .display_signal([0., 0., 0., 0.4])[3],
            0.4
        );
        assert!(LumaKey { softness: 0., ..p }
            .display_signal([0.1; 4])
            .iter()
            .all(|v| v.is_finite()));
    }
    #[test]
    fn validation_and_original_invert_boundary() {
        for p in [
            LumaKey {
                threshold: f32::NAN,
                ..Default::default()
            },
            LumaKey {
                softness: 51.,
                ..Default::default()
            },
            LumaKey {
                invert: 1.1,
                ..Default::default()
            },
        ] {
            assert!(p.validate().is_err());
        }
        let input = [0., 0., 0., 1.];
        assert_eq!(
            LumaKey {
                invert: 0.5,
                ..Default::default()
            }
            .display_signal(input)[3],
            0.
        );
        let input = [0., 0., 0., 1.];
        assert_eq!(
            LumaKey {
                invert: 0.5001,
                ..Default::default()
            }
            .display_signal(input)[3],
            1.
        );
    }
    #[test]
    #[ignore = "requires independently generated LUMA_KEY_GPU_ORACLE"]
    fn frame_alpha_and_hidden_rgb_match_original_metal() {
        use super::super::linear_color_frame::LinearColorFrame;
        let path = std::env::var("LUMA_KEY_GPU_ORACLE").expect("set LUMA_KEY_GPU_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut values = 0;
        let mut mismatches = [0usize; 4];
        for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let p: LumaKey = serde_json::from_value(case["settings"].clone()).unwrap();
            let input: [f32; 4] =
                std::array::from_fn(|i| case["input"][i].as_f64().unwrap() as f32);
            let mut frame = LinearColorFrame::from_working(
                1,
                1,
                &[input[0], input[1], input[2], 1.],
                &mut || Ok(()),
            )
            .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |mut p, _, _| {
                        p[3] = input[3];
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            let effect: super::super::composition::VideoEffect = serde_json::from_value(
                serde_json::json!({"kind":"concat_luma_key","parameters":p}),
            )
            .unwrap();
            effect.validate().unwrap();
            super::super::flat_surface::apply_effect_stack_working(
                &mut frame,
                &[effect],
                0,
                &[],
                None,
                &mut || Ok(()),
            )
            .unwrap();
            let mut actual = [0.; 4];
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        actual = p;
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            for channel in 0..4 {
                let expected = case["gpu"][channel].as_f64().unwrap() as f32;
                if actual[channel].to_bits() != expected.to_bits() {
                    mismatches[channel] += 1;
                    eprintln!(
                        "Luma Key mismatch case{index} channel{channel}: {} != {expected}",
                        actual[channel]
                    );
                }
                values += 1;
            }
        }
        eprintln!("Luma Key actual frame: {values} channels compared, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original bitwise frame gate remains strict"
        );
    }
    #[test]
    fn serialized_stack_changes_matte_without_losing_hidden_white() {
        use super::super::{
            composition::VideoEffect, flat_surface::apply_effect_stack_working,
            linear_color_frame::LinearColorFrame,
        };
        let effect: VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_luma_key","parameters":{"threshold":50,"softness":0,"invert":1}})).unwrap();
        effect.validate().unwrap();
        let restored: VideoEffect =
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let mut frame =
            LinearColorFrame::from_working(2, 1, &[0., 0., 0., 1., 1., 1., 1., 1.], &mut || Ok(()))
                .unwrap();
        apply_effect_stack_working(&mut frame, &[restored], 0, &[], None, &mut || Ok(())).unwrap();
        let actual = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
        assert_eq!(actual, vec![[0., 0., 0., 1.], [1., 1., 1., 0.]]);
    }
}
