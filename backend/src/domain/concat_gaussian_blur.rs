//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Native Concat Gaussian Blur v2 multipass; strict Metal precision remains open.
use super::{concat_working::texture_half, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GaussianBlur {
    pub radius: f32,
}
impl Default for GaussianBlur {
    fn default() -> Self {
        Self { radius: 10. }
    }
}

struct Picture {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
}
impl Picture {
    fn get(&self, x: i64, y: i64) -> [f32; 4] {
        self.pixels[y.clamp(0, self.height as i64 - 1) as usize * self.width
            + x.clamp(0, self.width as i64 - 1) as usize]
    }
    fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        let p = [
            uv[0] * self.width as f32 - 0.5,
            uv[1] * self.height as f32 - 0.5,
        ];
        let base = p.map(f32::floor);
        // Hardware filtering fraction approximation, independently qualified below.
        let f = std::array::from_fn::<_, 2, _>(|i| ((p[i] - base[i]) * 256.).round() / 256.);
        let mut sum = [0.; 4];
        for y in 0..2 {
            for x in 0..2 {
                let weight = (if x == 0 { 1. - f[0] } else { f[0] })
                    * (if y == 0 { 1. - f[1] } else { f[1] });
                let t = self.get(base[0] as i64 + x, base[1] as i64 + y);
                for k in 0..4 {
                    sum[k] += t[k] * weight;
                }
            }
        }
        sum
    }
}
fn weight(d: f32, sigma: f32) -> f32 {
    (-d * d / (2. * sigma * sigma)).exp()
}
fn half(p: [f32; 4]) -> [f32; 4] {
    p.map(texture_half)
}
fn premultiply(p: [f32; 4]) -> [f32; 4] {
    [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]]
}
fn shrink(radius: f32) -> usize {
    let n = (radius as f64 / 3.).min(64.);
    if n < 2. {
        1
    } else {
        1usize << n.log2().floor() as usize
    }
}
fn scratch_bytes(count: usize) -> Result<usize> {
    let bytes = count
        .checked_mul(128)
        .and_then(|n| n.checked_add(256))
        .ok_or_else(|| anyhow::anyhow!("Gaussian Blur scratch overflow"))?;
    ensure!(
        bytes <= 1024 * 1024 * 1024,
        "Gaussian Blur scratch exceeds 1 GiB budget"
    );
    Ok(bytes)
}
// Pair-iteration allowance, not a wall-clock or RSS guarantee.
fn work_budget(width: usize, height: usize, radius: f32) -> Result<()> {
    ensure!(
        width > 0 && height > 0 && radius.is_finite(),
        "invalid Gaussian Blur runtime dimensions/radius"
    );
    let sigma = radius.max(0.5);
    let rows = 1. / (1. / height as f32);
    let pair_count = |s: f32| -> Result<u128> {
        let reach = (3. * s).ceil();
        ensure!(
            reach.is_finite() && reach as f64 <= (i32::MAX / 2) as f64,
            "Gaussian Blur radius exceeds defined shader pair arithmetic"
        );
        Ok(reach as u128 + 1)
    };
    let horizontal = pair_count(sigma)?;
    let vertical = pair_count(sigma * rows / height as f32)?;
    let n = shrink(radius);
    let aw = width.div_ceil(n);
    let dn = if radius < 6. { 64 } else { n };
    let across = (aw as u128) * (height as u128);
    let down = (width.div_ceil(dn) as u128) * (height.div_ceil(dn) as u128);
    let final_columns = if (1. / aw as f32) * (width as f32) < 1.5 {
        (width as u128) * (height as u128)
    } else {
        0
    };
    let iterations = across
        .checked_mul(horizontal)
        .and_then(|v| {
            down.checked_add(final_columns)
                .and_then(|n| n.checked_mul(vertical))
                .and_then(|n| v.checked_add(n))
        })
        .ok_or_else(|| anyhow::anyhow!("Gaussian Blur work overflow"))?;
    ensure!(
        iterations <= 1_000_000_000,
        "Gaussian Blur exceeds pair-iteration budget"
    );
    Ok(())
}

impl GaussianBlur {
    /// Held values and authored key values use the original slider bounds.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.radius.is_finite() && (1. ..=50.).contains(&self.radius),
            "Concat Gaussian Blur radius out of range"
        );
        Ok(())
    }
    fn pair(&self, source: &Picture, x: f32, y: f32, wa: f32, wb: f32) -> [f32; 4] {
        let w = wa + wb;
        let both = source.sample([(x + wb / w) / source.width as f32, y / source.height as f32]);
        if (both[3] - both[3].round()).abs() < 1e-4 {
            return premultiply(both).map(|v| v * w);
        }
        let a = premultiply(source.get(x.floor() as i64, y.floor() as i64));
        let b = premultiply(source.get(x.floor() as i64 + 1, y.floor() as i64));
        std::array::from_fn(|k| a[k] * wa + b[k] * wb)
    }
    fn across(
        &self,
        source: &Picture,
        uv: [f32; 2],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<[f32; 4]> {
        let sigma = self.radius.max(0.5);
        let centre = uv[0] * source.width as f32;
        let row = uv[1] * source.height as f32;
        let first = (centre - 3. * sigma - 0.5).floor() + 0.5;
        let mut sum = [0.; 4];
        let mut total = 0.;
        for i in 0..(3. * sigma).ceil() as usize + 1 {
            if i > 0 && i % 64 == 0 {
                checkpoint()?;
            }
            let a = first + (2 * i) as f32;
            let wa = weight(a - centre, sigma);
            let wb = weight(a + 1. - centre, sigma);
            let p = self.pair(source, a, row, wa, wb);
            for k in 0..4 {
                sum[k] += p[k];
            }
            total += wa + wb;
        }
        Ok(sum.map(|v| v / total))
    }
    fn column(
        &self,
        across: &Picture,
        height: usize,
        uv: [f32; 2],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<[f32; 4]> {
        // The shader recovers the row count from the reciprocal texture size.
        let rows = 1. / (1. / across.height as f32);
        let sigma = self.radius.max(0.5) * rows / height as f32;
        let centre = uv[1] * rows;
        let first = (centre - 3. * sigma - 0.5).floor() + 0.5;
        let mut sum = [0.; 4];
        let mut total = 0.;
        for i in 0..(3. * sigma).ceil() as usize + 1 {
            if i > 0 && i % 64 == 0 {
                checkpoint()?;
            }
            let a = first + (2 * i) as f32;
            let wa = weight(a - centre, sigma);
            let wb = weight(a + 1. - centre, sigma);
            let w = wa + wb;
            let p = across.sample([uv[0], (a + wb / w) / rows]);
            for k in 0..4 {
                sum[k] += p[k] * w;
            }
            total += w;
        }
        Ok(sum.map(|v| v / total))
    }
    fn spread(&self, down: &Picture, uv: [f32; 2]) -> [f32; 4] {
        let size = [
            1. / (1. / down.width as f32),
            1. / (1. / down.height as f32),
        ];
        let at = [uv[0] * size[0] - 0.5, uv[1] * size[1] - 0.5];
        let inside = [at[0].clamp(0., size[0] - 1.), at[1].clamp(0., size[1] - 1.)];
        let read = |offset: [f32; 2]| {
            down.sample([
                (inside[0] + offset[0] + 0.5) / size[0],
                (inside[1] + offset[1] + 0.5) / size[1],
            ])
        };
        let mut blurred = read([0., 0.]);
        let over = [at[0] - inside[0], at[1] - inside[1]];
        if over != [0.; 2] {
            let back = over.map(|v| if v == 0. { 0. } else { -v.signum() });
            let x = read([back[0], 0.]);
            let y = read([0., back[1]]);
            let xy = read(back);
            let d = over.map(f32::abs);
            for k in 0..4 {
                blurred[k] += (blurred[k] - x[k]) * d[0]
                    + (blurred[k] - y[k]) * d[1]
                    + (blurred[k] - x[k] - y[k] + xy[k]) * d[0] * d[1];
            }
        }
        blurred
    }
    /// Runs a sampled finite radius, including overshoot, under memory/work budgets.
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let (width, height) = frame.dimensions();
        let count = width
            .checked_mul(height)
            .ok_or_else(|| anyhow::anyhow!("Gaussian Blur dimensions overflow"))?;
        scratch_bytes(count)?;
        work_budget(width, height, self.radius)?;
        let mut source = Picture {
            width,
            height,
            pixels: frame.concat_straight_snapshot(checkpoint)?,
        };
        for row in source.pixels.chunks_mut(width) {
            checkpoint()?;
            for p in row {
                ensure!(
                    p.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&p[3]),
                    "invalid Gaussian Blur source"
                );
                *p = p.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let n = shrink(self.radius);
        let mut across = Picture {
            width: width.div_ceil(n),
            height,
            pixels: Vec::with_capacity(width.div_ceil(n) * height),
        };
        for y in 0..height {
            checkpoint()?;
            for x in 0..across.width {
                across.pixels.push(half(self.across(
                    &source,
                    [
                        (x as f32 + 0.5) / across.width as f32,
                        (y as f32 + 0.5) / height as f32,
                    ],
                    checkpoint,
                )?));
            }
        }
        drop(source);
        let dn = if self.radius < 6. { 64 } else { n };
        let mut down = Picture {
            width: width.div_ceil(dn),
            height: height.div_ceil(dn),
            pixels: Vec::with_capacity(width.div_ceil(dn) * height.div_ceil(dn)),
        };
        for y in 0..down.height {
            checkpoint()?;
            for x in 0..down.width {
                down.pixels.push(half(self.column(
                    &across,
                    height,
                    [
                        (x as f32 + 0.5) / down.width as f32,
                        (y as f32 + 0.5) / down.height as f32,
                    ],
                    checkpoint,
                )?));
            }
        }
        let mut output = Vec::with_capacity(count);
        for y in 0..height {
            checkpoint()?;
            for x in 0..width {
                let uv = [
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                ];
                let blurred = if (1. / across.width as f32) * (width as f32) < 1.5 {
                    self.column(&across, height, uv, checkpoint)?
                } else {
                    self.spread(&down, uv)
                };
                let alpha = blurred[3].clamp(0., 1.);
                output.push([
                    texture_half((blurred[0] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[1] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half((blurred[2] / alpha.max(1e-6)).clamp(-65504., 65504.)),
                    texture_half(alpha),
                ]);
            }
        }
        drop(across);
        drop(down);
        frame.map_concat_straight_pixels(&mut |_, x, y| output[y * width + x], checkpoint)
    }
}

pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many original Concat radius keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (1. ..=50.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid original Concat radius key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    mut parameters: GaussianBlur,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<GaussianBlur> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    parameters.radius = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.radius as f64)? as f32;
    ensure!(
        parameters.radius.is_finite(),
        "nonfinite Gaussian Blur runtime radius"
    );
    Ok(parameters)
}

pub fn validate_animation(value: &super::composition::AnimatableValue) -> Result<()> {
    value.validate()?;
    ensure!(
        super::composition::animatable_values_in(value, 1., 50., true),
        "Concat Gaussian Blur animation out of range"
    );
    Ok(())
}
pub fn sample_animation(
    mut parameters: GaussianBlur,
    value: &super::composition::AnimatableValue,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<GaussianBlur> {
    parameters.validate()?;
    validate_animation(value)?;
    parameters.radius = super::nested_composition::sample_value(value, time.into())? as f32;
    ensure!(
        parameters.radius.is_finite(),
        "nonfinite Gaussian Blur runtime radius"
    );
    Ok(parameters)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shrink_boundaries_ceil_sizes_and_budget() {
        for (radius, n) in [
            (1., 1),
            (5.999, 1),
            (6., 2),
            (12., 4),
            (24., 8),
            (48., 16),
            (50., 16),
        ] {
            assert_eq!(shrink(radius), n);
        }
        assert_eq!(13usize.div_ceil(shrink(6.)), 7);
        assert!(scratch_bytes(3840 * 2160).is_ok());
        assert!(scratch_bytes(usize::MAX).is_err());
        assert!(GaussianBlur { radius: f32::NAN }.validate().is_err());
        assert!(GaussianBlur { radius: 50.1 }.validate().is_err());
    }
    #[test]
    fn transparent_color_does_not_bleed() {
        for radius in [1., 5.9, 6., 10., 50.] {
            let mut frame = LinearColorFrame::from_working(
                2,
                1,
                &[1., 0., 0., 1., 0., 0., 0., 0.],
                &mut || Ok(()),
            )
            .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |p, x, _| if x == 1 { [0., 100., 0., 0.] } else { p },
                    &mut || Ok(()),
                )
                .unwrap();
            GaussianBlur { radius }
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            for p in frame.concat_straight_snapshot(&mut || Ok(())).unwrap() {
                assert_eq!(&p[..3], &[1., 0., 0.]);
                assert!(p[3] > 0. && p[3] < 1.);
            }
        }
    }
    #[test]
    #[ignore = "requires independently generated GAUSSIAN_BLUR_SPATIAL_ORACLE"]
    fn spatial_frames_match_original_metal() {
        let path = std::env::var("GAUSSIAN_BLUR_SPATIAL_ORACLE")
            .expect("set GAUSSIAN_BLUR_SPATIAL_ORACLE");
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
            let effect: GaussianBlur = serde_json::from_value(case["parameters"].clone()).unwrap();
            if oracle["overshootInput"].as_bool().unwrap_or(false) {
                effect.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            } else {
                let serialized: super::super::composition::VideoEffect = serde_json::from_value(
                    serde_json::json!({"kind":"concat_gaussian_blur","parameters":effect}),
                )
                .unwrap();
                serialized.validate().unwrap();
                super::super::flat_surface::apply_effect_stack_working(
                    &mut frame,
                    &[serialized],
                    0,
                    &[],
                    None,
                    &mut || Ok(()),
                )
                .unwrap();
            }
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
        eprintln!("GaussianBlur spatial: {values} channels, mismatches {mismatches:?}");
        assert_eq!(
            mismatches, [0; 4],
            "original spatial bitwise gate remains strict"
        );
    }
    #[test]
    fn animated_radius_uses_local_clock_and_rejects_invalid_authored_values() {
        use super::super::{
            composition::{AnimatableValue, VideoEffect},
            flat_surface::apply_effect_stack_working,
        };
        let animation:AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1},{"tick":250000,"value":10},{"tick":500000,"value":1}]}})).unwrap();
        for (at, radius) in [(0u64, 1.), (125000, 5.5), (250000, 10.), (500000, 1.)] {
            assert_eq!(
                sample_animation(GaussianBlur { radius: 30. }, &animation, at)
                    .unwrap()
                    .radius,
                radius
            );
        }
        assert!(validate_animation(&AnimatableValue::constant(0.)).is_err());
        assert!(validate_animation(&AnimatableValue::constant(50.1)).is_err());
        let effect:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_gaussian_blur","parameters":{"radius":1},"animation":animation})).unwrap();
        effect.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<VideoEffect>(serde_json::to_value(&effect).unwrap()).unwrap(),
            effect
        );
        let input: Vec<f32> = (0..13)
            .flat_map(|x| {
                let v = if x < 6 { 0. } else { 1. };
                [v, v, v, 1.]
            })
            .collect();
        let mut actual = LinearColorFrame::from_working(13, 1, &input, &mut || Ok(())).unwrap();
        let mut expected = actual.clone();
        GaussianBlur { radius: 10. }
            .apply_linear(&mut expected, &mut || Ok(()))
            .unwrap();
        apply_effect_stack_working(&mut actual, &[effect], 250000, &[], None, &mut || Ok(()))
            .unwrap();
        assert_eq!(
            actual.concat_straight_snapshot(&mut || Ok(())).unwrap(),
            expected.concat_straight_snapshot(&mut || Ok(())).unwrap()
        );
    }
    #[test]
    fn original_keys_require_duration_and_preserve_normalized_clock() {
        use super::super::{
            composition::VideoEffect,
            flat_surface::{apply_effect_stack_working, EffectTrackContext},
        };
        struct Duration;
        impl EffectTrackContext for Duration {
            fn clip_duration_us(&self) -> Option<u64> {
                Some(500000)
            }
            fn track(
                &mut self,
                _: usize,
                _: &mut dyn FnMut() -> Result<()>,
            ) -> Result<Option<LinearColorFrame>> {
                Ok(None)
            }
            fn layer_to_output(&self, x: f64, y: f64) -> (f64, f64) {
                (x, y)
            }
        }
        let effect:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_gaussian_blur","parameters":{"radius":1},"concatKeys":[{"at":0,"value":1},{"at":0.5,"value":10,"ease":"out"},{"at":1,"value":1}]})).unwrap();
        effect.validate().unwrap();
        let input: Vec<f32> = (0..13)
            .flat_map(|x| {
                let v = if x < 6 { 0. } else { 1. };
                [v, v, v, 1.]
            })
            .collect();
        let mut actual = LinearColorFrame::from_working(13, 1, &input, &mut || Ok(())).unwrap();
        let mut expected = actual.clone();
        assert!(apply_effect_stack_working(
            &mut actual,
            &[effect.clone()],
            250000,
            &[],
            None,
            &mut || Ok(())
        )
        .is_err());
        GaussianBlur { radius: 10. }
            .apply_linear(&mut expected, &mut || Ok(()))
            .unwrap();
        apply_effect_stack_working(
            &mut actual,
            &[effect.clone()],
            250000,
            &[],
            Some(&mut Duration),
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(
            actual.concat_straight_snapshot(&mut || Ok(())).unwrap(),
            expected.concat_straight_snapshot(&mut || Ok(())).unwrap()
        );
        let mut ambiguous = serde_json::to_value(effect).unwrap();
        ambiguous["animation"] = serde_json::json!({"mode":"constant","value":2});
        assert!(serde_json::from_value::<VideoEffect>(ambiguous)
            .unwrap()
            .validate()
            .is_err());
    }
    #[test]
    fn finite_overshoot_is_not_clamped_and_long_pairs_cancel_atomically() {
        use super::super::concat_effect_upgrade::ParamKey;
        let input: Vec<f32> = (0..64)
            .flat_map(|x| {
                let v = if x < 32 { 0. } else { 1. };
                [v, v, v, 1.]
            })
            .collect();
        let seed = LinearColorFrame::from_working(64, 1, &input, &mut || Ok(())).unwrap();
        for (y, expected, clamped) in [(-2., -11.375, 1.), (20., 137.125, 50.)] {
            let keys = vec![
                ParamKey {derived:false,
                    at: 0.,
                    value: 1.,
                    ease: [0., 0., 1., 1.],
                },
                ParamKey {derived:false,
                    at: 1.,
                    value: 10.,
                    ease: [0.42, y, 0.58, y],
                },
            ];
            let sampled = sample_concat_keys(GaussianBlur { radius: 10. }, &keys, 0.5).unwrap();
            assert_eq!(sampled.radius, expected);
            let native:super::super::composition::AnimatableValue=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":{"kind":"bezier","p1":[0.42,y],"p2":[0.58,y]},"keyframes":[{"tick":0,"value":1},{"tick":1000000,"value":10}]}})).unwrap();
            assert_eq!(
                sample_animation(GaussianBlur { radius: 10. }, &native, 500000u64)
                    .unwrap()
                    .radius,
                expected
            );

            let mut actual = seed.clone();
            sampled.apply_linear(&mut actual, &mut || Ok(())).unwrap();
            let mut boundary = seed.clone();
            GaussianBlur { radius: clamped }
                .apply_linear(&mut boundary, &mut || Ok(()))
                .unwrap();
            assert_ne!(
                actual.concat_straight_snapshot(&mut || Ok(())).unwrap(),
                boundary.concat_straight_snapshot(&mut || Ok(())).unwrap()
            );
            if expected < 0. {
                let mut minimum = seed.clone();
                GaussianBlur { radius: 0.5 }
                    .apply_linear(&mut minimum, &mut || Ok(()))
                    .unwrap();
                assert_eq!(
                    actual.concat_straight_snapshot(&mut || Ok(())).unwrap(),
                    minimum.concat_straight_snapshot(&mut || Ok(())).unwrap()
                );
            }
        }
        assert!(work_budget(3840, 2160, 5.999).is_ok());
        assert!(work_budget(3840, 2160, 1e6).is_err());
        assert!(work_budget(1, 1, 1e6).is_ok());
        let mut frame =
            LinearColorFrame::from_working(1, 1, &[1., 0., 0., 1.], &mut || Ok(())).unwrap();
        let mut calls = 0;
        assert!(GaussianBlur { radius: 1e6 }
            .apply_linear(&mut frame, &mut || {
                calls += 1;
                ensure!(calls < 20, "cancelled");
                Ok(())
            })
            .is_err());
        assert_eq!(calls, 20);
        assert_eq!(
            frame.concat_straight_snapshot(&mut || Ok(())).unwrap(),
            vec![[1., 0., 0., 1.]]
        );
    }
}
