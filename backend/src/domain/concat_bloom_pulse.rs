//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat BloomPulse v2: highlight-weighted Gaussian and timeline-driven HDR screen.
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
pub struct BloomPulse {
    pub amount: f32,
    pub speed: f32,
}
impl Default for BloomPulse {
    fn default() -> Self {
        Self {
            amount: 50.,
            speed: 2.,
        }
    }
}
fn highlight(c: [f32; 4]) -> f32 {
    let y = display(c[0]) * 0.2126 + display(c[1]) * 0.7152 + display(c[2]) * 0.0722;
    let t = ((y - 0.55) / (1. - 0.55)).clamp(0., 1.);
    (t * t * (3. - 2. * t)).powf(2.4)
}
fn screen(base: f32, over: f32) -> f32 {
    let b = base.min(1.);
    1. - (1. - b) * (1. - over) + (base - b)
}
fn layout(w: usize, h: usize) -> Result<(usize, usize, f32, f32)> {
    ensure!(w > 0 && h > 0, "empty BloomPulse image");
    let n = w
        .checked_mul(h)
        .ok_or_else(|| anyhow::anyhow!("BloomPulse pixel overflow"))?;
    let bytes = n
        .checked_mul(120)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| anyhow::anyhow!("BloomPulse scratch overflow"))?;
    ensure!(
        bytes <= 1024 * 1024 * 1024,
        "BloomPulse scratch exceeds1GiB budget"
    );
    let aw = w.div_ceil(4);
    let dh = h.div_ceil(4);
    let rows = 1. / (1. / h as f32);
    let sigma = 12. * rows / h as f32;
    ensure!(
        sigma.is_finite() && sigma > 0. && (3. * sigma).ceil() + 1. <= 64.,
        "invalid BloomPulse stage geometry"
    );
    let work = aw
        .checked_mul(h)
        .and_then(|n| n.checked_mul(37))
        .and_then(|a| {
            aw.checked_mul(dh)
                .and_then(|n| n.checked_mul((3. * sigma).ceil() as usize + 1))
                .and_then(|b| a.checked_add(b))
        })
        .and_then(|v| n.checked_mul(2).and_then(|n| v.checked_add(n)))
        .ok_or_else(|| anyhow::anyhow!("BloomPulse work overflow"))?;
    ensure!(
        work <= 1_000_000_000,
        "BloomPulse source-sample work exceeds budget"
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
            let bright = highlight(c);
            for k in 0..3 {
                c[k] = c[k] * alpha * bright;
            }
        }
        for k in 0..4 {
            sum[k] += c[k] * weight;
        }
        total += weight;
    }
    sum.map(|v| v / total)
}
impl BloomPulse {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (0. ..=100.).contains(&self.amount),
            "Concat BloomPulse amount out of range"
        );
        ensure!(
            self.speed.is_finite() && (0.5..=5.).contains(&self.speed),
            "Concat BloomPulse speed out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        time: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.amount, self.speed, time, frame, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        amount: f32,
        speed: f32,
        time: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(amount.is_finite(), "nonfinite BloomPulse runtime amount");
        ensure!(
            time.is_finite() && speed.is_finite() && (time * speed).is_finite(),
            "nonfinite BloomPulse runtime phase"
        );
        let strength = amount * 0.01 * (0.6 + 0.4 * (time * speed).sin());
        ensure!(strength.is_finite(), "nonfinite BloomPulse strength");
        let (w, h) = frame.dimensions();
        let (aw, dh, rows, sigma) = layout(w, h)?;
        let mut source = frame.concat_straight_snapshot(checkpoint)?;
        for row in source.chunks_mut(w) {
            checkpoint()?;
            for p in row {
                ensure!(
                    p.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&p[3]),
                    "invalid BloomPulse source"
                );
                *p = p.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let mut across = Vec::with_capacity(aw * h);
        for y in 0..h {
            checkpoint()?;
            for x in 0..aw {
                let uv = [(x as f32 + 0.5) / aw as f32, (y as f32 + 0.5) / h as f32];
                across.push(weigh(&source, w, h, uv, w as f32, 12., 0, true).map(texture_half));
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
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let c = read(&source, w, h, uv);
                let blurred = read(&down, aw, dh, uv);
                let rgb = std::array::from_fn::<_, 3, _>(|k| {
                    let base = display(c[k]);
                    let bloom = (display(blurred[k]) * strength).max(0.);
                    texture_half(linear(screen(base, bloom)).clamp(-65504., 65504.))
                });
                [rgb[0], rgb[1], rgb[2], c[3]]
            },
            checkpoint,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Amount,
    Speed,
}
impl Channel {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "amount" => Some(Self::Amount),
            "speed" => Some(Self::Speed),
            _ => None,
        }
    }
}
pub type OriginalKeys =
    std::collections::BTreeMap<Channel, Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys: &OriginalKeys) -> Result<()> {
    ensure!(
        keys.values().map(Vec::len).sum::<usize>() <= 4096,
        "too many Bloom Pulse source keys"
    );
    for (channel, run) in keys {
        let (low, high) = match channel {
            Channel::Amount => (0., 100.),
            Channel::Speed => (0.5, 5.),
        };
        ensure!(
            run.iter().all(|k| k.at.is_finite()
                && k.value.is_finite()
                && (low..=high).contains(&k.value)
                && k.ease.iter().all(|v| v.is_finite())),
            "invalid Bloom Pulse source key"
        );
    }
    Ok(())
}
pub fn sample_keys(mut p: BloomPulse, keys: &OriginalKeys, progress: f64) -> Result<BloomPulse> {
    p.validate()?;
    validate_keys(keys)?;
    for (channel, run) in keys {
        if run.is_empty() {
            continue;
        }
        let rest = match channel {
            Channel::Amount => p.amount,
            Channel::Speed => p.speed,
        };
        let value = super::concat_animation::PreparedTrack::new(run)?
            .value_at(progress, rest as f64)? as f32;
        ensure!(value.is_finite(), "nonfinite Bloom Pulse runtime key");
        match channel {
            Channel::Amount => p.amount = value,
            Channel::Speed => p.speed = value,
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_in_across_down_map_and_final_commit_is_atomic() {
        let input: Vec<f32> = (0..256).flat_map(|_| [0.25, 0.25, 0.25, 0.5]).collect();
        for limit in [40, 50, 60, 69] {
            let mut frame = LinearColorFrame::from_working(16, 16, &input, &mut || Ok(())).unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(BloomPulse::default()
                .apply_linear(0., &mut frame, &mut || {
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
        BloomPulse::default()
            .apply_linear(0., &mut control, &mut || Ok(()))
            .unwrap();
        assert_ne!(
            before,
            control.concat_straight_snapshot(&mut || Ok(())).unwrap()
        );
    }
    #[test]
    fn independent_original_tracks_and_descriptor_roundtrip() {
        let key =
            |at, value, ease| super::super::concat_effect_upgrade::ParamKey {derived:false,  at, value, ease };
        let keys = OriginalKeys::from([
            (
                Channel::Amount,
                vec![
                    key(0., 0., [0., 0., 1., 1.]),
                    key(1., 100., [0.42, -0.5, 0.58, 1.]),
                ],
            ),
            (
                Channel::Speed,
                vec![
                    key(0., 0.5, [0., 0., 1., 1.]),
                    key(1., 5., [0., 0., 1., 1.]),
                ],
            ),
        ]);
        let sampled = sample_keys(BloomPulse::default(), &keys, 0.5).unwrap();
        assert_eq!(
            sampled,
            BloomPulse {
                amount: 31.25,
                speed: 2.75
            }
        );
        let effect = super::super::composition::VideoEffect::ConcatBloomPulse {
            parameters: BloomPulse::default(),
            concat_keys: Some(keys),
        };
        effect.validate().unwrap();
        let wire = serde_json::to_value(&effect).unwrap();
        assert_eq!(wire["kind"], "concat_bloom_pulse");
        assert_eq!(
            serde_json::from_value::<super::super::composition::VideoEffect>(wire).unwrap(),
            effect
        );
    }
    #[test]
    fn highlight_knee_and_parameter_bounds() {
        assert_eq!(highlight([0.1, 0.1, 0.1, 1.]), 0.);
        assert_eq!(highlight([1., 1., 1., 0.]), 1.);
        assert_eq!(highlight([2., 2., 2., 1.]), 1.);
        assert!(BloomPulse::default().validate().is_ok());
        assert!(BloomPulse {
            amount: 0.,
            speed: 0.5
        }
        .validate()
        .is_ok());
        assert!(BloomPulse {
            amount: 100.,
            speed: 5.
        }
        .validate()
        .is_ok());
        assert!(BloomPulse {
            amount: 101.,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(BloomPulse {
            speed: 0.49,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(layout(usize::MAX, 2).is_err());
    }
    #[test]
    fn pulse_uses_clock_and_rejects_nonfinite_phase() {
        let input = [0.25, 0.25, 0.25, 0.5].repeat(16 * 16);
        let make = || LinearColorFrame::from_working(16, 16, &input, &mut || Ok(())).unwrap();
        let mut a = make();
        let mut b = make();
        BloomPulse::default()
            .apply_linear(0., &mut a, &mut || Ok(()))
            .unwrap();
        BloomPulse::default()
            .apply_linear(0.75, &mut b, &mut || Ok(()))
            .unwrap();
        let a = a.concat_straight_snapshot(&mut || Ok(())).unwrap();
        let b = b.concat_straight_snapshot(&mut || Ok(())).unwrap();
        assert!(b[0][0] > a[0][0]);
        assert_eq!(b[0][3], 0.5);
        assert!(BloomPulse::default()
            .apply_linear(f32::NAN, &mut make(), &mut || Ok(()))
            .is_err());
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated BLOOM_PULSE_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("BLOOM_PULSE_GPU_ORACLE").unwrap();
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
            let p: BloomPulse = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_sampled_linear(
                p.amount,
                p.speed,
                case["time"].as_f64().unwrap() as f32,
                &mut frame,
                &mut || Ok(()),
            )
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
        println!(
            "BloomPulse original spatial: {channels} channels, mismatches RGBA {mismatches:?}"
        );
        assert_eq!(mismatches, [0; 4]);
    }
}
