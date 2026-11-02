//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Pixelate's sixteen alpha-weighted bilinear taps in scene-linear light.
//! Pinned concat.pixelate v2 shader contract; native renderer registered; spatial precision qualification and UI pending.
use super::{concat_working::texture_half, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pixelate {
    pub size: u32,
}
impl Default for Pixelate {
    fn default() -> Self {
        Self { size: 16 }
    }
}
fn sample(pixels: &[[f32; 4]], w: usize, h: usize, at: [f32; 2]) -> [f32; 4] {
    let uv = [at[0] / w as f32, at[1] / h as f32];
    let p = [uv[0] * w as f32 - 0.5, uv[1] * h as f32 - 0.5];
    let base = [p[0].floor(), p[1].floor()];
    let f = [p[0] - base[0], p[1] - base[1]];
    let get = |x: usize, y: usize| {
        pixels[(base[1] as i64 + y as i64).clamp(0, h as i64 - 1) as usize * w
            + (base[0] as i64 + x as i64).clamp(0, w as i64 - 1) as usize]
    };
    // Original hardware fetch chooses the opaque/transparent fast path.
    let hardware = f.map(|v| (v * 256.).round() / 256.);
    let mut c = [0.; 4];
    for y in 0..2 {
        for x in 0..2 {
            let weight = (if x == 0 {
                1. - hardware[0]
            } else {
                hardware[0]
            }) * (if y == 0 {
                1. - hardware[1]
            } else {
                hardware[1]
            });
            let t = get(x, y);
            for k in 0..4 {
                c[k] += t[k] * weight;
            }
        }
    }
    if (c[3] - c[3].round()).abs() < 1e-4 {
        return [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
    }
    let mut mixed = [0.; 4];
    for y in 0..2 {
        for x in 0..2 {
            let weight =
                (if x == 0 { 1. - f[0] } else { f[0] }) * (if y == 0 { 1. - f[1] } else { f[1] });
            let t = get(x, y);
            for k in 0..3 {
                mixed[k] += (t[k] * t[3]) * weight;
            }
            mixed[3] += t[3] * weight;
        }
    }
    mixed
}
impl Pixelate {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (2..=64).contains(&self.size),
            "Concat pixel size out of range"
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
        let scratch = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(104))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| anyhow::anyhow!("Pixelate scratch overflow"))?;
        ensure!(
            scratch <= 1024 * 1024 * 1024,
            "Pixelate scratch exceeds 1 GiB budget"
        );
        let mut pixels = frame.concat_straight_snapshot(checkpoint)?;
        for pixel in &mut pixels {
            ensure!(
                pixel.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                    && (0. ..=1.).contains(&pixel[3]),
                "invalid Pixelate source"
            );
            *pixel = pixel.map(|v| half::f16::from_f32(v).to_f32());
        }
        let block = self.size as f32;
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let corner = [
                    (uv[0] * w as f32 / block).floor() * block,
                    (uv[1] * h as f32 / block).floor() * block,
                ];
                let mut sum = [0.; 4];
                for y in 0..4 {
                    for x in 0..4 {
                        let at = [
                            corner[0] + (x as f32 + 0.5) * block / 4.,
                            corner[1] + (y as f32 + 0.5) * block / 4.,
                        ];
                        let t = sample(&pixels, w, h, at);
                        for k in 0..4 {
                            sum[k] += t[k];
                        }
                    }
                }
                let mean = sum.map(|v| v / 16.);
                let alpha = mean[3];
                [
                    texture_half((mean[0] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((mean[1] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((mean[2] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half(alpha),
                ]
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transparent_color_does_not_bleed_and_block_is_frozen() {
        let mut frame = LinearColorFrame::from_working(
            2,
            2,
            &[
                1., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 1., 0., 0., 0., 0.,
            ],
            &mut || Ok(()),
        )
        .unwrap();
        frame
            .map_concat_straight_pixels(
                &mut |mut p, x, _| {
                    if x == 1 {
                        p = [0., 100., 0., 0.];
                    }
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
        Pixelate { size: 2 }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
        let mut got = Vec::new();
        frame
            .map_concat_straight_pixels(
                &mut |p, _, _| {
                    got.push(p);
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
        assert!(got.iter().all(|p| *p == [1., 0., 0., 0.5]));
        assert!(Pixelate { size: 1 }.validate().is_err());
        assert!(Pixelate { size: 65 }.validate().is_err());
    }
    #[test]
    #[ignore = "requires independently generated PIXELATE_GPU_ORACLE"]
    fn frame_alpha_and_hidden_rgb_match_original_metal() {
        use super::super::linear_color_frame::LinearColorFrame;
        let path = std::env::var("PIXELATE_GPU_ORACLE").expect("set PIXELATE_GPU_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut values = 0;
        let mut mismatches = [0usize; 4];
        for (index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let p: Pixelate = serde_json::from_value(case["settings"].clone()).unwrap();
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
                serde_json::json!({"kind":"concat_pixelate","parameters":p}),
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
                        "Pixelate mismatch case{index} channel{channel}: {} != {expected}",
                        actual[channel]
                    );
                }
                values += 1;
            }
        }
        eprintln!("Pixelate actual frame: {values} channels compared, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original bitwise frame gate remains strict"
        );
    }
    #[test]
    #[ignore = "requires independently generated PIXELATE_SPATIAL_ORACLE"]
    fn spatial_frames_match_original_metal() {
        let path = std::env::var("PIXELATE_SPATIAL_ORACLE").expect("set PIXELATE_SPATIAL_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut mismatches = [0usize; 4];
        let mut values = 0;
        for (case_index, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
            let side = case["side"].as_u64().unwrap() as usize;
            let input: Vec<f32> = case["inputWorking"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            assert_eq!(input.len(), side * side * 4);
            let mut opaque = input.clone();
            for p in opaque.chunks_exact_mut(4) {
                p[3] = 1.;
            }
            let mut frame =
                LinearColorFrame::from_working(side, side, &opaque, &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |_, x, y| {
                        let i = (y * side + x) * 4;
                        [input[i], input[i + 1], input[i + 2], input[i + 3]]
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            let effect: Pixelate = serde_json::from_value(case["parameters"].clone()).unwrap();
            effect.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let actual = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let expected = case["expectedWorking"].as_array().unwrap();
            assert_eq!(expected.len(), side * side * 4);
            for (index, p) in actual.iter().enumerate() {
                for channel in 0..4 {
                    let want = expected[index * 4 + channel].as_f64().unwrap() as f32;
                    if p[channel].to_bits() != want.to_bits() {
                        if mismatches[channel] < 8 {
                            eprintln!(
                                "case{case_index} pixel{index} channel{channel}: {} != {want}",
                                p[channel]
                            );
                        }
                        mismatches[channel] += 1;
                    }
                    values += 1;
                }
            }
        }
        eprintln!("Pixelate spatial: {values} channels, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original spatial bitwise gate remains strict"
        );
    }
    #[test]
    fn serialized_effect_averages_light_and_preserves_alpha() {
        use super::super::{composition::VideoEffect, flat_surface::apply_effect_stack_working};
        let effect: VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"concat_pixelate","parameters":{"size":2}}),
        )
        .unwrap();
        effect.validate().unwrap();
        let restored: VideoEffect =
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let mut frame = LinearColorFrame::from_working(
            2,
            2,
            &[
                1., 0., 0., 1., 0., 0., 1., 1., 1., 0., 0., 1., 0., 0., 1., 1.,
            ],
            &mut || Ok(()),
        )
        .unwrap();
        apply_effect_stack_working(&mut frame, &[restored], 0, &[], None, &mut || Ok(())).unwrap();
        let pixels = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
        assert_eq!(pixels, vec![[0.5, 0., 0.5, 1.]; 4]);
        for size in [1, 65] {
            let effect: VideoEffect = serde_json::from_value(
                serde_json::json!({"kind":"concat_pixelate","parameters":{"size":size}}),
            )
            .unwrap();
            assert!(effect.validate().is_err());
        }
    }
}
