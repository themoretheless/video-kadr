//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Zoom Blur: all three original sixteen-scale passes, pinned
//! 78e4be101c2d3c41a7dc73c112c6b1f11b9ce170. Metal precision qualified separately.
use super::{
    concat_chromatic_aberration::sample, concat_working::texture_half,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ZoomBlur {
    pub amount: f32,
    pub x: f32,
    pub y: f32,
}
impl Default for ZoomBlur {
    fn default() -> Self {
        Self {
            amount: 25.,
            x: 50.,
            y: 50.,
        }
    }
}
pub(super) fn read(pixels: &[[f32; 4]], w: usize, h: usize, at: [f32; 2]) -> [f32; 4] {
    [
        sample(pixels, w, h, at, 0),
        sample(pixels, w, h, at, 1),
        sample(pixels, w, h, at, 2),
        sample(pixels, w, h, at, 3),
    ]
}
pub(super) fn premultiplied(pixels: &[[f32; 4]], w: usize, h: usize, uv: [f32; 2]) -> [f32; 4] {
    let c = read(pixels, w, h, uv);
    if (c[3] - c[3].round()).abs() < 1e-4 {
        return [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
    }
    let p = [uv[0] * w as f32 - 0.5, uv[1] * h as f32 - 0.5];
    let base = p.map(f32::floor);
    let f = [p[0] - base[0], p[1] - base[1]];
    let mut mixed = [0.; 4];
    for y in 0..2 {
        for x in 0..2 {
            let ix = (base[0] as i64)
                .saturating_add(x as i64)
                .clamp(0, w as i64 - 1) as usize;
            let iy = (base[1] as i64)
                .saturating_add(y as i64)
                .clamp(0, h as i64 - 1) as usize;
            let t = pixels[iy * w + ix];
            let weight =
                (if x == 0 { 1. - f[0] } else { f[0] }) * (if y == 0 { 1. - f[1] } else { f[1] });
            for k in 0..3 {
                mixed[k] += (t[k] * t[3]) * weight;
            }
            mixed[3] += t[3] * weight;
        }
    }
    mixed
}
fn intermediate_stage(
    pixels: &[[f32; 4]],
    w: usize,
    h: usize,
    span: f32,
    centre: [f32; 2],
    divisor: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<[f32; 4]>> {
    let step = span / divisor;
    let mut next = Vec::with_capacity(pixels.len());
    for y in 0..h {
        checkpoint()?;
        for x in 0..w {
            let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
            let mut sum = [0.; 4];
            for i in 0..16 {
                let scale = (-(step * i as f32)).exp();
                let at = [
                    centre[0] + (uv[0] - centre[0]) * scale,
                    centre[1] + (uv[1] - centre[1]) * scale,
                ];
                let p = if divisor == 4096. {
                    premultiplied(pixels, w, h, at)
                } else {
                    read(pixels, w, h, at)
                };
                for k in 0..4 {
                    sum[k] += p[k];
                }
            }
            next.push(sum.map(|v| texture_half(v / 16.)));
        }
    }
    Ok(next)
}
impl ZoomBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [self.amount, self.x, self.y]
                .iter()
                .all(|v| v.is_finite() && (0. ..=100.).contains(v)),
            "Concat Zoom Blur parameter out of range"
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
            [self.amount, self.x, self.y].iter().all(|v| v.is_finite()),
            "nonfinite Zoom Blur runtime parameters"
        );
        let (w, h) = frame.dimensions();
        ensure!(
            (self.x / 100. * w as f32).is_finite() && (self.y / 100. * h as f32).is_finite(),
            "Zoom Blur runtime centre exceeds coordinate range"
        );
        let bytes = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(104))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| anyhow::anyhow!("Zoom Blur scratch overflow"))?;
        ensure!(
            bytes <= 1024 * 1024 * 1024,
            "Zoom Blur scratch exceeds1GiB budget"
        );
        let mut pixels = frame.concat_straight_snapshot(checkpoint)?;
        for row in pixels.chunks_mut(w) {
            checkpoint()?;
            for pixel in row {
                ensure!(
                    pixel.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&pixel[3]),
                    "invalid zoom source"
                );
                *pixel = pixel.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let span = -(1. - (self.amount / 100.).clamp(0., 0.99)).ln();
        let centre = [self.x / 100., self.y / 100.];
        // Original pass order is significant: intermediate textures hold premultiplied
        // values. Never unpremultiply between fine and middle, and never collapse the
        // three passes into sixteen samples or a direct4096-tap implementation.
        for divisor in [4096., 256.] {
            pixels = intermediate_stage(&pixels, w, h, span, centre, divisor, checkpoint)?;
        }
        let step = span / 16.;
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let mut sum = [0.; 4];
                for i in 0..16 {
                    let scale = (-(step * i as f32)).exp();
                    let at = [
                        centre[0] + (uv[0] - centre[0]) * scale,
                        centre[1] + (uv[1] - centre[1]) * scale,
                    ];
                    let p = read(&pixels, w, h, at);
                    for k in 0..4 {
                        sum[k] += p[k];
                    }
                }
                let blurred = sum.map(|v| v / 16.);
                let alpha = blurred[3];
                [
                    texture_half((blurred[0] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[1] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[2] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half(alpha),
                ]
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Amount,
    X,
    Y,
}
impl Channel {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "amount" => Some(Self::Amount),
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            _ => None,
        }
    }
}
pub type OriginalKeys =
    std::collections::BTreeMap<Channel, Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys: &OriginalKeys) -> Result<()> {
    ensure!(
        keys.values().map(Vec::len).sum::<usize>() <= 4096,
        "too many Zoom Blur source keys"
    );
    ensure!(
        keys.values().flatten().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (0. ..=100.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid Zoom Blur source key"
    );
    Ok(())
}
pub fn sample_keys(mut p: ZoomBlur, keys: &OriginalKeys, progress: f64) -> Result<ZoomBlur> {
    p.validate()?;
    validate_keys(keys)?;
    for (channel, run) in keys {
        if run.is_empty() {
            continue;
        }
        let rest = match channel {
            Channel::Amount => p.amount,
            Channel::X => p.x,
            Channel::Y => p.y,
        };
        let value = super::concat_animation::PreparedTrack::new(run)?
            .value_at(progress, rest as f64)? as f32;
        ensure!(value.is_finite(), "nonfinite Zoom Blur runtime key");
        match channel {
            Channel::Amount => p.amount = value,
            Channel::X => p.x = value,
            Channel::Y => p.y = value,
        }
    }
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_channels_keep_custom_easing_and_unclipped_overshoot() {
        let key =
            |at, value, ease| super::super::concat_effect_upgrade::ParamKey {derived:false,  at, value, ease };
        let linear = [0., 0., 1., 1.];
        let keys = OriginalKeys::from([
            (
                Channel::Amount,
                vec![key(0., 0., linear), key(1., 100., [0.42, -0.5, 0.58, 1.])],
            ),
            (Channel::X, vec![key(0., 0., linear), key(1., 50., linear)]),
            (
                Channel::Y,
                vec![key(0., 100., linear), key(1., 50., linear)],
            ),
        ]);
        assert_eq!(
            sample_keys(ZoomBlur::default(), &keys, 0.5).unwrap(),
            ZoomBlur {
                amount: 31.25,
                x: 25.,
                y: 75.
            }
        );
        let overshoot = OriginalKeys::from([(
            Channel::Amount,
            vec![key(0., 0., linear), key(1., 100., [0.42, 3., 0.58, 3.])],
        )]);
        assert_eq!(
            sample_keys(ZoomBlur::default(), &overshoot, 0.5)
                .unwrap()
                .amount,
            237.5
        );
    }
    #[test]
    fn one_pixel_all_passes_preserve_visible_signal_and_remove_hidden_colour() {
        for alpha in [0., 0.25, 0.5, 1.] {
            let mut frame =
                LinearColorFrame::from_working(1, 1, &[0., 0., 0., alpha], &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(&mut |_, _, _| [0.5, 1., 2., alpha], &mut || Ok(()))
                .unwrap();
            ZoomBlur {
                amount: 100.,
                x: 0.,
                y: 100.,
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
        assert!(ZoomBlur {
            amount: -1.,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(ZoomBlur {
            x: 101.,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn cancelled_fine_middle_and_final_passes_leave_source_unchanged() {
        // before prepares hidden state. Apply then visits16snapshot rows,
        //16source-quantization rows,16fine,16middle,16final rows.
        for limit in [40, 56, 72] {
            let mut frame =
                LinearColorFrame::from_working(16, 16, &vec![0.5; 16 * 16 * 4], &mut || Ok(()))
                    .unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(ZoomBlur::default()
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
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated ZOOM_BLUR_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("ZOOM_BLUR_GPU_ORACLE").unwrap();
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
            let p: ZoomBlur = serde_json::from_value(case["parameters"].clone()).unwrap();
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
        println!("ZoomBlur original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}

#[cfg(test)]
mod stage_diagnostic {
    use super::*;
    #[test]
    #[ignore = "instrumented Metal diagnostic, not original final-output parity"]
    fn compare_instrumented_intermediate_stages() {
        let path = std::env::var("ZOOM_BLUR_STAGE_DIAGNOSTIC").unwrap();
        let diagnostic: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(diagnostic["instrumented"], true);
        assert_eq!(
            diagnostic["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(
            diagnostic["rawEntryReadback"], true,
            "stage comparison must bypass final mix observer"
        );
        let mut counts = [[0usize; 4]; 2];
        let mut isolated_middle = [0usize; 4];
        let mut uv_division = [0usize; 2];
        let mut records = Vec::new();
        for case in diagnostic["cases"].as_array().unwrap() {
            let side = case["side"].as_u64().unwrap() as usize;
            if matches!(case["stage"].as_str(), Some("uvx" | "uvy")) {
                let axis = usize::from(case["stage"] == "uvy");
                let packed = case["expectedWorking"].as_array().unwrap();
                for (index, pixel) in packed.chunks_exact(4).enumerate() {
                    let bits = pixel[0].as_f64().unwrap() as u32
                        | ((pixel[1].as_f64().unwrap() as u32) << 11)
                        | ((pixel[2].as_f64().unwrap() as u32) << 22);
                    let coordinate = if axis == 0 {
                        index % side
                    } else {
                        index / side
                    };
                    let cpu = (coordinate as f32 + 0.5) / side as f32;
                    uv_division[axis] += usize::from(cpu.to_bits() != bits);
                }
                records.push(case.clone());
                continue;
            }
            let input: Vec<f32> = serde_json::from_value(case["inputWorking"].clone()).unwrap();
            let expected: Vec<f32> =
                serde_json::from_value(case["expectedWorking"].clone()).unwrap();
            let p: ZoomBlur = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.validate().unwrap();
            let pixels: Vec<[f32; 4]> = input
                .chunks_exact(4)
                .map(|p| std::array::from_fn(|k| half::f16::from_f32(p[k]).to_f32()))
                .collect();
            let span = -(1. - (p.amount / 100.).clamp(0., 0.99)).ln();
            let centre = [p.x / 100., p.y / 100.];
            let fine = intermediate_stage(&pixels, side, side, span, centre, 4096., &mut || Ok(()))
                .unwrap();
            let (stage, cpu) = match case["stage"].as_str().unwrap() {
                "fine" => (0, fine),
                "middle" => (
                    1,
                    intermediate_stage(&fine, side, side, span, centre, 256., &mut || Ok(()))
                        .unwrap(),
                ),
                _ => panic!("invalid diagnostic stage"),
            };
            if stage == 1 {
                let original_fine = diagnostic["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|candidate| {
                        candidate["stage"] == "fine"
                            && candidate["side"] == case["side"]
                            && candidate["seed"] == case["seed"]
                            && candidate["parameters"] == case["parameters"]
                    })
                    .unwrap();
                let values: Vec<f32> =
                    serde_json::from_value(original_fine["expectedWorking"].clone()).unwrap();
                let fine: Vec<[f32; 4]> = values
                    .chunks_exact(4)
                    .map(|p| [p[0], p[1], p[2], p[3]])
                    .collect();
                let isolated =
                    intermediate_stage(&fine, side, side, span, centre, 256., &mut || Ok(()))
                        .unwrap();
                for (i, p) in isolated.iter().enumerate() {
                    for k in 0..4 {
                        isolated_middle[k] +=
                            usize::from(p[k].to_bits() != expected[i * 4 + k].to_bits());
                    }
                }
            }
            for (i, p) in cpu.iter().enumerate() {
                for k in 0..4 {
                    counts[stage][k] +=
                        usize::from(p[k].to_bits() != expected[i * 4 + k].to_bits());
                }
            }
            let mut record = case.clone();
            record["cpuWorking"] =
                serde_json::json!(cpu.iter().flatten().copied().collect::<Vec<f32>>());
            records.push(record);
        }
        println!(
            "Instrumented stage mismatches fine {:?}, middle {:?}",
            counts[0], counts[1]
        );
        println!("Isolated middle with exact Metal fine input {isolated_middle:?}; raster UV differences from CPU division {uv_division:?}");
        if let Ok(path) = std::env::var("ZOOM_BLUR_CPU_STAGE_OUTPUT") {
            std::fs::write(path,serde_json::to_vec(&serde_json::json!({"instrumented":true,"cases":records,"mismatchCounts":counts})).unwrap()).unwrap();
        }
    }
}
