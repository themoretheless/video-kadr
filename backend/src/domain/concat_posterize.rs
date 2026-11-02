//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Posterize v2: equally spaced signed display bands, including HDR.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Posterize {
    pub levels: u32,
}
impl Default for Posterize {
    fn default() -> Self {
        Self { levels: 4 }
    }
}
impl Posterize {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (2..=8).contains(&self.levels),
            "Concat posterize levels out of range"
        );
        Ok(())
    }
    pub fn display_signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        let steps = self.levels.max(2) as f32 - 1.;
        rgb.map(|v| (v * steps).round_ties_even() / steps)
    }
    pub fn signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        self.display_signal(rgb.map(super::concat_chroma::display))
            .map(|v| {
                if v == 0. {
                    0.
                } else {
                    super::concat_chroma::linear(v)
                }
            })
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        super::concat_working::apply_rgb(frame, |rgb| self.signal(rgb), checkpoint)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bands_keep_negative_and_hdr_levels() {
        let p = Posterize { levels: 5 };
        assert_eq!(p.display_signal([-0.75, 0.74, 1.75]), [-0.75, 0.75, 1.75]);
        assert_eq!(p.display_signal([0.125, 0.375, 0.625]), [0., 0.5, 0.5]);
        assert!(Posterize { levels: 1 }.validate().is_err());
        assert!(Posterize { levels: 9 }.validate().is_err());
        assert_eq!(Posterize::default().levels, 4);
        // WGSL sign(0) returns zero, whereas Rust signum(-0) returns -1.
        assert_eq!(Posterize { levels: 2 }.signal([-0.02; 3])[0].to_bits(), 0);
    }
    #[test]
    #[ignore = "requires independently generated POSTERIZE_GPU_ORACLE"]
    fn frame_alpha_and_hidden_rgb_match_original_metal() {
        use super::super::linear_color_frame::LinearColorFrame;
        let path = std::env::var("POSTERIZE_GPU_ORACLE").expect("set POSTERIZE_GPU_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut values = 0;
        let mut mismatches = [0usize; 4];
        for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let p: Posterize = serde_json::from_value(case["settings"].clone()).unwrap();
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
                serde_json::json!({"kind":"concat_posterize","parameters":p}),
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
                        "Posterize mismatch case{index} channel{channel}: {} != {expected}",
                        actual[channel]
                    );
                }
                values += 1;
            }
        }
        eprintln!("Posterize actual frame: {values} channels compared, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original bitwise frame gate remains strict"
        );
    }
}
