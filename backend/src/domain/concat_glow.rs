//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Glow v2: paired Gaussian, quarter-resolution stages and HDR screen.
//! Pinned78e4be101c2d3c41a7dc73c112c6b1f11b9ce170.
use super::{
    concat_chroma::{display, linear},
    concat_working::texture_half,
    concat_zoom_blur::read,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Glow {
    pub amount: f32,
}
impl Default for Glow {
    fn default() -> Self {
        Self { amount: 45. }
    }
}
fn screen(base: f32, over: f32) -> f32 {
    let b = base.min(1.);
    1. - (1. - b) * (1. - over) + (base - b)
}
fn layout(w: usize, h: usize) -> Result<(usize, usize, f32, f32)> {
    ensure!(w > 0 && h > 0, "empty Glow image");
    let n = w
        .checked_mul(h)
        .ok_or_else(|| anyhow::anyhow!("Glow pixel overflow"))?;
    let bytes = n
        .checked_mul(120)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| anyhow::anyhow!("Glow scratch overflow"))?;
    ensure!(
        bytes <= 1024 * 1024 * 1024,
        "Glow scratch exceeds1GiB budget"
    );
    let aw = w.div_ceil(4);
    let dh = h.div_ceil(4);
    let rows = 1. / (1. / h as f32);
    let sigma = 16. * rows / h as f32;
    ensure!(
        sigma.is_finite() && sigma > 0. && (3. * sigma).ceil() + 1. <= 64.,
        "invalid Glow stage geometry"
    );
    let work = aw
        .checked_mul(h)
        .and_then(|n| n.checked_mul(49))
        .and_then(|a| {
            aw.checked_mul(dh)
                .and_then(|n| n.checked_mul((3. * sigma).ceil() as usize + 1))
                .and_then(|b| a.checked_add(b))
        })
        .and_then(|v| n.checked_mul(2).and_then(|n| v.checked_add(n)))
        .ok_or_else(|| anyhow::anyhow!("Glow work overflow"))?;
    ensure!(
        work <= 1_000_000_000,
        "Glow source-sample work exceeds budget"
    );
    Ok((aw, dh, rows, sigma))
}
fn weigh(
    pixels: &[[f32; 4]],
    w: usize,
    h: usize,
    uv: [f32; 2],
    extent: f32,
    sigma: f32,
    axis: usize,
    premultiply: bool,
) -> [f32; 4] {
    let centre = uv[axis] * extent;
    let first = (centre - 3. * sigma - 0.5).floor() + 0.5;
    let pairs = (3. * sigma).ceil() as usize + 1;
    let mut sum = [0.; 4];
    let mut total = 0.;
    for i in 0..pairs {
        let a = first + (2 * i) as f32;
        let d = a - centre;
        let wa = (-d * d / (2. * sigma * sigma)).exp();
        let d = a + 1. - centre;
        let wb = (-d * d / (2. * sigma * sigma)).exp();
        let weight = wa + wb;
        let mut at = uv;
        at[axis] = (a + wb / weight) / extent;
        let mut c = read(pixels, w, h, at);
        // Original layer_at premultiplies AFTER the straight bilinear sample.
        if premultiply {
            let alpha = c[3];
            for k in 0..3 {
                c[k] *= alpha;
            }
        }
        for k in 0..4 {
            sum[k] += c[k] * weight;
        }
        total += weight;
    }
    sum.map(|v| v / total)
}
impl Glow {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (10. ..=100.).contains(&self.amount),
            "Concat Glow amount out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.amount, frame, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        amount: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(amount.is_finite(), "nonfinite Glow runtime amount");
        let (w, h) = frame.dimensions();
        let (aw, dh, rows, sigma) = layout(w, h)?;
        let mut source = frame.concat_straight_snapshot(checkpoint)?;
        for row in source.chunks_mut(w) {
            checkpoint()?;
            for p in row {
                ensure!(
                    p.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&p[3]),
                    "invalid Glow source"
                );
                *p = p.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let mut across = Vec::with_capacity(aw * h);
        for y in 0..h {
            checkpoint()?;
            for x in 0..aw {
                let uv = [(x as f32 + 0.5) / aw as f32, (y as f32 + 0.5) / h as f32];
                across.push(weigh(&source, w, h, uv, w as f32, 16., 0, true).map(texture_half));
            }
        }
        let mut down = Vec::with_capacity(aw * dh);
        for y in 0..dh {
            checkpoint()?;
            for x in 0..aw {
                let uv = [(x as f32 + 0.5) / aw as f32, (y as f32 + 0.5) / dh as f32];
                down.push(weigh(&across, aw, h, uv, rows, sigma, 1, false).map(texture_half));
            }
        }
        drop(across);
        let t = amount / 100.;
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let c = read(&source, w, h, uv);
                let blurred = read(&down, aw, dh, uv);
                let rgb = std::array::from_fn::<_, 3, _>(|k| {
                    let base = display(c[k]);
                    let glowing = screen(base, display(blurred[k]));
                    texture_half(linear(base * (1. - t) + glowing * t).clamp(-65504., 65504.))
                });
                [rgb[0], rgb[1], rgb[2], c[3]]
            },
            checkpoint,
        )
    }
}
pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many Glow source keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (10. ..=100.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid Glow source key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    parameters: Glow,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<f32> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    let value = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.amount as f64)? as f32;
    ensure!(value.is_finite(), "nonfinite Glow runtime key");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdr_screen_and_fractional_original_keys() {
        assert_eq!(screen(2., 0.5), 2.);
        assert_eq!(screen(0.25, 0.5), 0.625);
        let keys = vec![
            super::super::concat_effect_upgrade::ParamKey {derived:false,
                at: 0.,
                value: 10.,
                ease: [0., 0., 1., 1.],
            },
            super::super::concat_effect_upgrade::ParamKey {derived:false,
                at: 1.,
                value: 100.,
                ease: [0.42, -0.5, 0.58, 1.],
            },
        ];
        assert_eq!(
            sample_concat_keys(Glow::default(), &keys, 0.5).unwrap(),
            38.125
        );
        assert!(Glow { amount: 9. }.validate().is_err());
        assert!(Glow { amount: 101. }.validate().is_err());
    }
    #[test]
    fn source_alpha_and_resource_admission() {
        for alpha in [0., 0.25, 0.5, 1.] {
            let mut frame =
                LinearColorFrame::from_working(1, 1, &[0., 0., 0., alpha], &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(&mut |_, _, _| [0.25, 0.5, 2., alpha], &mut || Ok(()))
                .unwrap();
            Glow::default()
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        assert_eq!(p[3], alpha);
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
        }
        assert!(layout(3840, 2160).is_ok());
        assert!(layout(usize::MAX, 2).is_err());
    }
    #[test]
    fn cancellation_in_across_down_map_and_final_commit_is_atomic() {
        let input: Vec<f32> = (0..256).flat_map(|_| [0.125, 0.125, 0.125, 0.5]).collect();
        for limit in [40, 50, 60, 69] {
            let mut frame = LinearColorFrame::from_working(16, 16, &input, &mut || Ok(())).unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(Glow::default()
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
        let mut control = LinearColorFrame::from_working(16, 16, &input, &mut || Ok(())).unwrap();
        let before = control.concat_straight_snapshot(&mut || Ok(())).unwrap();
        Glow::default()
            .apply_linear(&mut control, &mut || Ok(()))
            .unwrap();
        assert_ne!(
            before,
            control.concat_straight_snapshot(&mut || Ok(())).unwrap()
        );
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated GLOW_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("GLOW_GPU_ORACLE").unwrap();
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert!(
            !oracle["instrumented"].as_bool().unwrap_or(false),
            "diagnostic cannot prove original parity"
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
            let p: Glow = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_sampled_linear(p.amount, &mut frame, &mut || Ok(()))
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
        println!("Glow original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
