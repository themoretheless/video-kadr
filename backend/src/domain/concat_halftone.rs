//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Halftone v2: display-level dot area and HDR paper, pinned
//! 78e4be101c2d3c41a7dc73c112c6b1f11b9ce170.
use super::{
    concat_working::texture_half, concat_zoom_blur::read, linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Halftone {
    pub size: f32,
}
impl Default for Halftone {
    fn default() -> Self {
        Self { size: 6. }
    }
}
fn dot_level(seen: f32, at: [f32; 2], middle: [f32; 2], cell: f32) -> f32 {
    let level = seen.clamp(0., 1.);
    let radius = (1. - level).sqrt() * cell * 0.7071;
    let dx = at[0] - middle[0];
    let dy = at[1] - middle[1];
    let distance = (dx * dx + dy * dy).sqrt();
    let low = radius - 0.75;
    let high = radius + 0.75;
    let t = ((distance - low) / (high - low)).clamp(0., 1.);
    let ink = 1. - t * t * (3. - 2. * t);
    (1. - ink) * seen.max(1.)
}
impl Halftone {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.size.is_finite() && (4. ..=40.).contains(&self.size),
            "Concat Halftone size out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.size, frame, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        size: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(size.is_finite(), "nonfinite Halftone runtime size");
        let (w, h) = frame.dimensions();
        let bytes = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(104))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| anyhow::anyhow!("Halftone scratch overflow"))?;
        ensure!(
            bytes <= 1024 * 1024 * 1024,
            "Halftone scratch exceeds1GiB budget"
        );
        let mut pixels = frame.concat_straight_snapshot(checkpoint)?;
        for row in pixels.chunks_mut(w) {
            checkpoint()?;
            for p in row {
                ensure!(
                    p.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&p[3]),
                    "invalid Halftone source"
                );
                *p = p.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let cell = size.max(2.);
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let at = [uv[0] * w as f32, uv[1] * h as f32];
                let middle = at.map(|v| ((v / cell).floor() + 0.5) * cell);
                let sampled = read(&pixels, w, h, [middle[0] / w as f32, middle[1] / h as f32]);
                let rgb = [sampled[0], sampled[1], sampled[2]].map(super::concat_chroma::display);
                let seen = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
                let level = dot_level(seen, at, middle, cell);
                let light =
                    texture_half(super::concat_chroma::linear(level).clamp(-65504., 65504.));
                [light, light, light, read(&pixels, w, h, uv)[3]]
            },
            checkpoint,
        )
    }
}
pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many Halftone source keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (4. ..=40.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid Halftone source key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    parameters: Halftone,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<f32> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    let value = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.size as f64)? as f32;
    ensure!(value.is_finite(), "nonfinite Halftone runtime key value");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_dot_geometry_and_hdr_paper() {
        assert_eq!(dot_level(0.5, [3., 3.], [3., 3.], 6.), 0.);
        assert_eq!(dot_level(1., [100., 100.], [3., 3.], 6.), 1.);
        assert_eq!(dot_level(2., [100., 100.], [3., 3.], 6.), 2.);
        assert_eq!(dot_level(1., [3., 3.], [3., 3.], 6.), 0.5);
        assert!(Halftone { size: 3. }.validate().is_err());
        assert!(Halftone { size: 41. }.validate().is_err());
    }
    #[test]
    fn source_alpha_retained_and_cancellation_atomic() {
        for alpha in [0., 0.25, 0.5, 1.] {
            let mut frame =
                LinearColorFrame::from_working(1, 1, &[0., 0., 0., alpha], &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(&mut |_, _, _| [0.5, 1., 2., alpha], &mut || Ok(()))
                .unwrap();
            Halftone::default()
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            frame
                .map_concat_straight_pixels(
                    &mut |p, _, _| {
                        assert_eq!(p[3], alpha);
                        assert_eq!(p[0], p[1]);
                        assert_eq!(p[1], p[2]);
                        p
                    },
                    &mut || Ok(()),
                )
                .unwrap();
        }
        for limit in [8, 24, 40] {
            let mut frame =
                LinearColorFrame::from_working(16, 16, &vec![0.5; 16 * 16 * 4], &mut || Ok(()))
                    .unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(Halftone::default()
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
    fn original_keys_keep_fractional_midpoint_and_overshoot() {
        let k = |at, value, ease| super::super::concat_effect_upgrade::ParamKey {derived:false,  at, value, ease };
        let keys = vec![
            k(0., 4., [0., 0., 1., 1.]),
            k(1., 40., [0.42, -0.5, 0.58, 1.]),
        ];
        assert_eq!(
            sample_concat_keys(Halftone::default(), &keys, 0.5).unwrap(),
            15.25
        );
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated HALFTONE_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("HALFTONE_GPU_ORACLE").unwrap();
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert!(!oracle["instrumented"].as_bool().unwrap_or(false),"diagnostic cannot prove original parity");
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
            let p: Halftone = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_sampled_linear(p.size, &mut frame, &mut || Ok(()))
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
        println!("Halftone original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
