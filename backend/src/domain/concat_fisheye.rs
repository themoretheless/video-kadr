//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Fisheye v2, pinned78e4be101c2d3c41a7dc73c112c6b1f11b9ce170.
use super::{concat_chromatic_aberration::sample, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fisheye {
    pub strength: f32,
}
impl Default for Fisheye {
    fn default() -> Self {
        Self { strength: 50. }
    }
}
impl Fisheye {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.strength.is_finite() && (5. ..=100.).contains(&self.strength),
            "Concat fisheye strength out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.strength, frame, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        strength: f32,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(strength.is_finite(), "nonfinite fisheye runtime strength");
        let (w, h) = frame.dimensions();
        let bytes = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(104))
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| anyhow::anyhow!("Fisheye scratch overflow"))?;
        ensure!(
            bytes <= 1024 * 1024 * 1024,
            "Fisheye scratch exceeds1GiB budget"
        );
        let mut pixels = frame.concat_straight_snapshot(checkpoint)?;
        for row in pixels.chunks_mut(w) {
            checkpoint()?;
            for pixel in row {
                ensure!(
                    pixel.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                        && (0. ..=1.).contains(&pixel[3]),
                    "invalid fisheye source"
                );
                *pixel = pixel.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
        let s = strength / 100. * 0.6;
        let aspect = w as f32 / h as f32;
        let c = [aspect * 0.5, 0.5f32];
        let corner = (c[0] * c[0] + c[1] * c[1]).sqrt();
        frame.map_concat_straight_pixels(
            &mut |_, x, y| {
                let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                let p = [(uv[0] - 0.5) * aspect, uv[1] - 0.5];
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt() / corner;
                let pulled = (1. - s) + s * r * r;
                let at = [0.5 + p[0] * pulled / aspect, 0.5 + p[1] * pulled];
                if at.iter().any(|v| !v.is_finite()) {
                    return [f32::NAN; 4];
                }
                [
                    sample(&pixels, w, h, at, 0),
                    sample(&pixels, w, h, at, 1),
                    sample(&pixels, w, h, at, 2),
                    sample(&pixels, w, h, at, 3),
                ]
            },
            checkpoint,
        )
    }
}
pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many fisheye source keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (5. ..=100.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid fisheye source key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    parameters: Fisheye,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<f32> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    let value = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.strength as f64)? as f32;
    ensure!(value.is_finite(), "nonfinite fisheye runtime key value");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_runtime_and_authored_admission() {
        assert!(Fisheye { strength: 4. }.validate().is_err());
        assert!(Fisheye { strength: 101. }.validate().is_err());
        let input = [0.25, 0.5, 1., 0.5];
        let mut frame = LinearColorFrame::from_working(1, 1, &input, &mut || Ok(())).unwrap();
        Fisheye::default()
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
    #[ignore = "requires independently generated FISHEYE_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("FISHEYE_GPU_ORACLE").unwrap();
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
            let p: Fisheye = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_sampled_linear(p.strength, &mut frame, &mut || Ok(()))
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
        println!("Fisheye original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
