//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Motion Blur v2, pinned78e4be101c2d3c41a7dc73c112c6b1f11b9ce170.
//! Complete coarse Gaussian and triangular reconstruction, including outer edges.
use super::{
    concat_working::texture_half,
    concat_zoom_blur::{premultiplied, read},
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MotionBlur {
    pub length: f32,
    pub angle: f32,
}
impl Default for MotionBlur {
    fn default() -> Self {
        Self {
            length: 18.,
            angle: 0.,
        }
    }
}
fn coarse(
    pixels: &[[f32; 4]],
    w: usize,
    h: usize,
    uv: [f32; 2],
    step: [f32; 2],
    weights: &[f32; 24],
    total: f32,
) -> [f32; 4] {
    let mut sum = [0.; 4];
    for i in 0..24 {
        let d = i as f32 - 11.5;
        let at = [uv[0] + step[0] * d, uv[1] + step[1] * d];
        let p = premultiplied(pixels, w, h, at);
        for k in 0..4 {
            sum[k] += p[k] * weights[i];
        }
    }
    sum.map(|v| v / total)
}
// Conservative allocation and actual shape-aware source-sample admission. The
// extra boundary column/row covers floating pixel-centre classification error.
fn resource_plan(w: usize, h: usize, along: [f32; 2], taps: i32) -> Result<usize> {
    let n = w
        .checked_mul(h)
        .ok_or_else(|| anyhow::anyhow!("Motion Blur pixel overflow"))?;
    let bytes = n
        .checked_mul(120)
        .and_then(|n| n.checked_add(512))
        .ok_or_else(|| anyhow::anyhow!("Motion Blur scratch overflow"))?;
    ensure!(
        bytes <= 1024 * 1024 * 1024,
        "Motion Blur scratch exceeds1GiB budget"
    );
    let mut work = n
        .checked_mul(24)
        .ok_or_else(|| anyhow::anyhow!("Motion Blur work overflow"))?;
    for i in -taps..=taps {
        let nx =
            (((along[0] * i as f32 * w as f32).abs().ceil() as usize).saturating_add(1)).min(w);
        let ny =
            (((along[1] * i as f32 * h as f32).abs().ceil() as usize).saturating_add(1)).min(h);
        let outside = nx * h + ny * w - nx * ny;
        work = work
            .checked_add(n)
            .and_then(|v| outside.checked_mul(24).and_then(|o| v.checked_add(o)))
            .ok_or_else(|| anyhow::anyhow!("Motion Blur work overflow"))?;
    }
    ensure!(
        work <= 1_000_000_000,
        "Motion Blur source-sample work exceeds budget"
    );
    Ok(work)
}
impl MotionBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.length.is_finite()
                && (2. ..=60.).contains(&self.length)
                && self.angle.is_finite()
                && (-90. ..=90.).contains(&self.angle),
            "Concat Motion Blur parameter out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(frame, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            self.length.is_finite() && self.angle.is_finite(),
            "nonfinite Motion Blur runtime parameters"
        );
        let (w, h) = frame.dimensions();
        let a = self.angle.to_radians();
        let along = [a.cos() * 0.5 / w as f32, -a.sin() * 0.5 / h as f32];
        let reach = 12. * self.length.max(0.5) / 24.;
        ensure!(
            reach.is_finite() && along.iter().all(|v| v.is_finite()),
            "Motion Blur runtime geometry overflow"
        );
        let taps = (reach.ceil() as i32).min(64);
        resource_plan(w, h, along, taps)?;
        let mut source = frame.concat_straight_snapshot(checkpoint)?;
        for row in source.chunks_mut(w) {
            checkpoint()?;
            for pixel in row {
                ensure!(
                    pixel.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&pixel[3]),
                    "invalid Motion Blur source"
                );
                *pixel = pixel.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let weights = std::array::from_fn::<_, 24, _>(|i| {
            let d = i as f32 - 11.5;
            (-d * d / (2. * 4. * 4.)).exp()
        });
        let mut total = 0.;
        for weight in weights {
            total += weight;
        }
        let gaussian_total = total;
        let step = [along[0] * reach, along[1] * reach];
        let mut image = Vec::with_capacity(source.len());
        for y in 0..h {
            checkpoint()?;
            for x in 0..w {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                image.push(
                    coarse(&source, w, h, uv, step, &weights, gaussian_total).map(texture_half),
                );
            }
        }
        let mut output = Vec::with_capacity(source.len());
        for y in 0..h {
            checkpoint()?;
            for x in 0..w {
                if x % 64 == 0 {
                    checkpoint()?;
                }
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let mut sum = [0.; 4];
                let mut total = 0.;
                for i in -taps..=taps {
                    let weight = (1. - (i as f32).abs() / reach).max(0.);
                    let at = [uv[0] + along[0] * i as f32, uv[1] + along[1] * i as f32];
                    let p = [at[0] * w as f32, at[1] * h as f32];
                    let c = if p[0] < 0.5
                        || p[1] < 0.5
                        || p[0] > w as f32 - 0.5
                        || p[1] > h as f32 - 0.5
                    {
                        coarse(&source, w, h, at, step, &weights, gaussian_total)
                    } else {
                        read(&image, w, h, at)
                    };
                    for k in 0..4 {
                        sum[k] += c[k] * weight;
                    }
                    total += weight;
                }
                let blurred = sum.map(|v| v / total);
                let alpha = blurred[3];
                output.push([
                    texture_half((blurred[0] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[1] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[2] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half(alpha),
                ]);
            }
        }
        drop(source);
        drop(image);
        frame.map_concat_straight_pixels(&mut |_, x, y| output[y * w + x], checkpoint)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Length,
    Angle,
}
impl Channel {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "length" => Some(Self::Length),
            "angle" => Some(Self::Angle),
            _ => None,
        }
    }
}
pub type OriginalKeys =
    std::collections::BTreeMap<Channel, Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys: &OriginalKeys) -> Result<()> {
    ensure!(
        keys.values().map(Vec::len).sum::<usize>() <= 4096,
        "too many Motion Blur source keys"
    );
    for (channel, run) in keys {
        let (low, high) = match channel {
            Channel::Length => (2., 60.),
            Channel::Angle => (-90., 90.),
        };
        ensure!(
            run.iter().all(|k| k.at.is_finite()
                && k.value.is_finite()
                && (low..=high).contains(&k.value)
                && k.ease.iter().all(|v| v.is_finite())),
            "invalid Motion Blur source key"
        );
    }
    Ok(())
}
pub fn sample_keys(mut p: MotionBlur, keys: &OriginalKeys, progress: f64) -> Result<MotionBlur> {
    p.validate()?;
    validate_keys(keys)?;
    for (channel, run) in keys {
        if run.is_empty() {
            continue;
        }
        let rest = match channel {
            Channel::Length => p.length,
            Channel::Angle => p.angle,
        };
        let value = super::concat_animation::PreparedTrack::new(run)?
            .value_at(progress, rest as f64)? as f32;
        ensure!(value.is_finite(), "nonfinite Motion Blur runtime key");
        match channel {
            Channel::Length => p.length = value,
            Channel::Angle => p.angle = value,
        }
    }
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_keys_keep_length_midpoint_and_unclipped_angle() {
        let k = |at, value, ease| super::super::concat_effect_upgrade::ParamKey {derived:false,  at, value, ease };
        let linear = [0., 0., 1., 1.];
        let keys = OriginalKeys::from([
            (
                Channel::Length,
                vec![k(0., 2., linear), k(1., 60., [0.42, -0.5, 0.58, 1.])],
            ),
            (
                Channel::Angle,
                vec![k(0., -90., linear), k(1., 90., linear)],
            ),
        ]);
        assert_eq!(
            sample_keys(MotionBlur::default(), &keys, 0.5).unwrap(),
            MotionBlur {
                length: 20.125,
                angle: 0.
            }
        );
        let keys = OriginalKeys::from([(
            Channel::Angle,
            vec![k(0., -90., linear), k(1., 90., [0.42, 3., 0.58, 3.])],
        )]);
        assert_eq!(
            sample_keys(MotionBlur::default(), &keys, 0.5)
                .unwrap()
                .angle,
            337.5
        );
        assert!(MotionBlur {
            length: 1.,
            angle: 0.
        }
        .validate()
        .is_err());
        assert!(MotionBlur {
            length: 18.,
            angle: 91.
        }
        .validate()
        .is_err());
    }
    #[test]
    fn opaque_constant_and_hidden_rgb() {
        for alpha in [0., 1.] {
            let mut frame =
                LinearColorFrame::from_working(1, 1, &[0., 0., 0., alpha], &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(&mut |_, _, _| [0.5, 1., 2., alpha], &mut || Ok(()))
                .unwrap();
            MotionBlur {
                length: 60.,
                angle: 45.,
            }
            .apply_linear(&mut frame, &mut || Ok(()))
            .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        assert_eq!(
                            p,
                            if alpha == 0. {
                                [0.; 4]
                            } else {
                                [0.5, 1., 2., alpha]
                            }
                        );
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
        }
    }
    #[test]
    fn cancellation_in_coarse_reconstruction_and_commit_is_atomic() {
        for limit in [40, 52, 90] {
            let mut frame =
                LinearColorFrame::from_working(16, 16, &vec![0.5; 16 * 16 * 4], &mut || Ok(()))
                    .unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(MotionBlur::default()
                .apply_linear(&mut frame, &mut || {
                    calls += 1;
                    ensure!(calls < limit, "cancelled");
                    Ok(())
                })
                .is_err());
            assert_eq!(calls, limit);
            assert_eq!(
                before,
                frame.concat_straight_snapshot(&mut || Ok(())).unwrap()
            );
        }
    }
    #[test]
    fn ordinary_4k_is_admitted_and_skinny_pathological_work_is_bounded() {
        for length in [2f32, 18., 60.] {
            for angle in [0f32, 45., 90.] {
                let a = angle.to_radians();
                let along = [a.cos() * 0.5 / 3840., -a.sin() * 0.5 / 2160.];
                assert!(resource_plan(3840, 2160, along, (length * 0.5).ceil() as i32).is_ok());
            }
        }
        assert!(resource_plan(8_000_000, 1, [0., -0.5], 30).is_err());
        assert!(resource_plan(usize::MAX, 2, [0., 0.], 1).is_err());
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated MOTION_BLUR_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("MOTION_BLUR_GPU_ORACLE").unwrap();
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert!(
            !oracle["instrumented"].as_bool().unwrap_or(false),
            "diagnostic is not original final-output parity"
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
            let before = mismatches;
            let p: MotionBlur = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_linear(&mut frame, &mut || Ok(())).unwrap();
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
            if before != mismatches {
                println!(
                    "case side{side}, parameters {:?}, mismatches {:?}",
                    p,
                    std::array::from_fn::<_, 4, _>(|i| mismatches[i] - before[i])
                );
            }
        }
        println!(
            "MotionBlur original spatial: {channels} channels, mismatches RGBA {mismatches:?}"
        );
        assert_eq!(mismatches, [0; 4]);
    }
}
