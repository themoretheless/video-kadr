//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Lateral channel scaling from Concat chromatic-aberration v2, pinned
//! 78e4be101c2d3c41a7dc73c112c6b1f11b9ce170. GPU sampling precision unqualified.
use super::{concat_working::texture_half, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChromaticAberration {
    pub shift: f32,
}
impl Default for ChromaticAberration {
    fn default() -> Self {
        Self { shift: 4. }
    }
}
// Straight-alpha source sampling, as in the upstream effect's sample(), not
// sample_premultiplied(). Clamp each tap independently at the image border.
// Metal RGBA16F filtered samples in the independent signed spatial oracle
// round to nearest half, with exact ties away from zero. Keep this local:
// arithmetic render-target writes elsewhere still use texture_half (RTZ).
fn sampled_half(value: f32) -> f32 {
    let nearest = half::f16::from_f32(value);
    let rounded = nearest.to_f32();
    if rounded.abs() < value.abs() {
        let farther = half::f16::from_bits(nearest.to_bits() + 1).to_f32();
        if value.abs() - rounded.abs() == farther.abs() - value.abs() {
            return farther;
        }
    }
    rounded
}
pub(super) fn sample(pixels: &[[f32; 4]], w: usize, h: usize, uv: [f32; 2], channel: usize) -> f32 {
    let uv = uv.map(|v| v.clamp(0., 1.));
    let p = [uv[0] * w as f32 - 0.5, uv[1] * h as f32 - 0.5];
    let base = p.map(f32::floor);
    let f = [p[0] - base[0], p[1] - base[1]].map(|v| (v * 256.).round() / 256.);
    let mut result = 0.;
    for y in 0..2 {
        for x in 0..2 {
            let ix = (base[0] as i64 + x as i64).clamp(0, w as i64 - 1) as usize;
            let iy = (base[1] as i64 + y as i64).clamp(0, h as i64 - 1) as usize;
            result += pixels[iy * w + ix][channel]
                * (if x == 0 { 1. - f[0] } else { f[0] })
                * (if y == 0 { 1. - f[1] } else { f[1] });
        }
    }
    sampled_half(result)
}
impl ChromaticAberration {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.shift.is_finite() && (1. ..=24.).contains(&self.shift),
            "Concat chromatic shift out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.shift, frame, checkpoint)
    }
    /// Original curves may overshoot the authored range; never round or clip them.
    pub fn apply_sampled_linear(
        &self,
        shift: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(shift.is_finite(), "nonfinite chromatic runtime shift");
        let (w, h) = frame.dimensions();
        let scale = shift / (0.5 * w as f32);
        ensure!(
            scale.is_finite() && 1. + scale != 0.,
            "singular chromatic runtime scale"
        );
        let (w, h) = frame.dimensions();
        let bytes = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(104))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| anyhow::anyhow!("Chromatic scratch overflow"))?;
        ensure!(
            bytes <= 1024 * 1024 * 1024,
            "Chromatic scratch exceeds 1 GiB budget"
        );
        let mut pixels = frame.concat_straight_snapshot(checkpoint)?;
        for row in pixels.chunks_mut(w) {
            checkpoint()?;
            for pixel in row {
                ensure!(
                    pixel.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&pixel[3]),
                    "invalid chromatic source"
                );
                *pixel = pixel.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let red = uv.map(|v| 0.5 + (v - 0.5) / (1. + scale));
                let blue = uv.map(|v| 0.5 + (v - 0.5) / (1. - scale).max(0.01));
                [
                    texture_half(sample(&pixels, w, h, red, 0)),
                    texture_half(sample(&pixels, w, h, uv, 1)),
                    texture_half(sample(&pixels, w, h, blue, 2)),
                    texture_half(sample(&pixels, w, h, uv, 3)),
                ]
            },
            checkpoint,
        )
    }
}
pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many chromatic source keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (1. ..=24.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid chromatic source key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    parameters: ChromaticAberration,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<f32> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    let value = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.shift as f64)? as f32;
    ensure!(value.is_finite(), "nonfinite chromatic runtime key value");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_keys_keep_fractional_midpoints_and_overshoot() {
        let keys = |ease| {
            vec![
                super::super::concat_effect_upgrade::ParamKey {derived:false,
                    at: 0.,
                    value: 1.,
                    ease: [0., 0., 1., 1.],
                },
                super::super::concat_effect_upgrade::ParamKey {derived:false,
                    at: 1.,
                    value: 24.,
                    ease,
                },
            ]
        };
        let base = ChromaticAberration::default();
        assert_eq!(
            sample_concat_keys(base, &keys([0.42, -0.5, 0.58, 1.]), 0.5).unwrap(),
            8.1875
        );
        assert_eq!(
            sample_concat_keys(base, &keys([0., 0., 1., 1.]), 0.5).unwrap(),
            12.5
        );
        assert_eq!(
            sample_concat_keys(base, &keys([0.42, 3., 0.58, 3.]), 0.5).unwrap(),
            55.625
        );
        assert_eq!(
            sample_concat_keys(base, &keys([0.42, -1., 0.58, -1.]), 0.5).unwrap(),
            -13.375
        );
        let mut frame = LinearColorFrame::from_working(4, 1, &[1.; 16], &mut || Ok(())).unwrap();
        assert!(base
            .apply_sampled_linear(-2., &mut frame, &mut || Ok(()))
            .is_err());
        assert!(base
            .apply_sampled_linear(f32::INFINITY, &mut frame, &mut || Ok(()))
            .is_err());
    }
    #[test]
    fn filtered_half_ties_round_away_in_both_signs() {
        for sign in [-1., 1.] {
            assert_eq!(sampled_half(sign * 1.11181640625), sign * 1.1123046875);
            assert_eq!(sampled_half(sign * 2.0830078125), sign * 2.083984375);
            assert_eq!(sampled_half(sign * 1.0), sign);
        }
    }
    #[test]
    fn channels_use_distinct_radial_scales_and_clamped_borders() {
        let mut frame = LinearColorFrame::from_working(
            4,
            1,
            &[
                0., 0., 0., 1., 1., 1., 1., 1., 2., 2., 2., 1., 3., 3., 3., 1.,
            ],
            &mut || Ok(()),
        )
        .unwrap();
        ChromaticAberration { shift: 1. }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
        let mut got = vec![];
        frame
            .map_concat_straight_pixels(
                &mut |p, _, _| {
                    got.push(p);
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(got[0], [0.5, 0., 0., 1.]);
        assert_eq!(got[3], [2.5, 3., 3., 1.]);
        assert!(got[1][0] > got[1][1] && got[1][2] < got[1][1]);
        assert!(got[2][0] < got[2][1] && got[2][2] > got[2][1]);
    }
    #[test]
    fn one_pixel_and_parameter_admission() {
        assert!(ChromaticAberration { shift: 0. }.validate().is_err());
        assert!(ChromaticAberration { shift: 25. }.validate().is_err());
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[0.25, 0.5, 1., 0.5], &mut || Ok(())).unwrap();
        ChromaticAberration { shift: 24. }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
        frame
            .map_concat_straight_pixels(
                &mut |p, _, _| {
                    assert_eq!(p, [0.5, 1., 2., 0.5]);
                    p
                },
                &mut || Ok(()),
            )
            .unwrap();
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated CHROMATIC_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("CHROMATIC_GPU_ORACLE").unwrap();
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let mut mismatches = [0usize; 4];
        let mut channels = 0;
        for case in oracle["cases"].as_array().unwrap() {
            let side = case["side"].as_u64().unwrap() as usize;
            let input: Vec<f32> = serde_json::from_value(case["inputWorking"].clone()).unwrap();
            let expected: Vec<f32> =
                serde_json::from_value(case["expectedWorking"].clone()).unwrap();
            // from_working consumes premultiplied data; synthetic source is straight.
            let premult: Vec<f32> = input
                .chunks_exact(4)
                .flat_map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
                .collect();
            let mut frame =
                LinearColorFrame::from_working(side, side, &premult, &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |_, x, y| {
                        let i = (y * side + x) * 4;
                        [input[i], input[i + 1], input[i + 2], input[i + 3]]
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            let p: ChromaticAberration =
                serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_sampled_linear(p.shift, &mut frame, &mut || Ok(()))
                .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |p, x, y| {
                        for k in 0..4 {
                            channels += 1;
                            if p[k].to_bits() != expected[(y * side + x) * 4 + k].to_bits() {
                                mismatches[k] += 1;
                            }
                        }
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
        }
        println!("Chromatic original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
