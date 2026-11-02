//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat Film Grain v2: integer-hashed timeline noise in ACEScct-like log,
//! pinned78e4be101c2d3c41a7dc73c112c6b1f11b9ce170; signed toe and HDR retained.
use super::{
    concat_falling_dust::hash, concat_working::texture_half, linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilmGrain {
    pub amount: f32,
}
impl Default for FilmGrain {
    fn default() -> Self {
        Self { amount: 12. }
    }
}
fn to_log(v: f32) -> f32 {
    if v <= 0.0078125 {
        v * 10.5402377416545 + 0.0729055341958355
    } else {
        (v.max(0.0078125).log2() + 9.72) / 17.52
    }
}
fn from_log(v: f32) -> f32 {
    if v <= 0.155251141552511 {
        (v - 0.0729055341958355) / 10.5402377416545
    } else {
        (v * 17.52 - 9.72).exp2().min(65504.)
    }
}
fn phase(seconds: f64) -> Result<f32> {
    let clock = seconds as f32 * 7.31;
    ensure!(clock.is_finite(), "invalid Film Grain timeline clock");
    Ok(clock - clock.floor())
}
impl FilmGrain {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.amount.is_finite() && (2. ..=40.).contains(&self.amount),
            "Concat Film Grain amount out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_sampled_linear(self.amount, frame, seconds, checkpoint)
    }
    pub fn apply_sampled_linear(
        &self,
        amount: f32,
        frame: &mut LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(amount.is_finite(), "nonfinite Film Grain runtime amount");
        let seed = phase(seconds)?;
        let (w, h) = frame.dimensions();
        frame.map_concat_straight_pixels(
            &mut |input, x, y| {
                if !input.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                    || !(0. ..=1.).contains(&input[3])
                {
                    return [f32::NAN; 4];
                }
                let p = input.map(|v| half::f16::from_f32(v).to_f32());
                let at = [
                    (((x as f32 + 0.5) / w as f32) * w as f32).floor(),
                    (((y as f32 + 0.5) / h as f32) * h as f32).floor(),
                ];
                let n = hash(at[0], at[1], seed) - 0.5;
                let delta = n * amount * 0.008;
                let rgb = [p[0], p[1], p[2]]
                    .map(|v| texture_half(from_log(to_log(v) + delta).clamp(-65504., 65504.)));
                [rgb[0], rgb[1], rgb[2], p[3]]
            },
            checkpoint,
        )
    }
}
pub fn validate_concat_keys(keys: &[super::concat_effect_upgrade::ParamKey]) -> Result<()> {
    ensure!(keys.len() <= 4096, "too many Film Grain source keys");
    ensure!(
        keys.iter().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && (2. ..=40.).contains(&k.value)
            && k.ease.iter().all(|v| v.is_finite())),
        "invalid Film Grain source key"
    );
    Ok(())
}
pub fn sample_concat_keys(
    parameters: FilmGrain,
    keys: &[super::concat_effect_upgrade::ParamKey],
    progress: f64,
) -> Result<f32> {
    parameters.validate()?;
    validate_concat_keys(keys)?;
    let value = super::concat_animation::PreparedTrack::new(keys)?
        .value_at(progress, parameters.amount as f64)? as f32;
    ensure!(value.is_finite(), "nonfinite Film Grain runtime key");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_timeline_phase_and_signed_log_domain() {
        assert_eq!(hash(0., 0., 0.), 0.);
        assert_eq!(phase(0.).unwrap(), 0.);
        assert!(phase(-0.125).unwrap() > 0.);
        assert_ne!(
            hash(3., 7., phase(1.25).unwrap()),
            hash(3., 7., phase(0.25).unwrap())
        );
        assert!(from_log(to_log(-0.25)) < 0.);
        assert!(from_log(to_log(2.)) > 1.);
        assert!(phase(f64::NAN).is_err());
        assert!(phase(f64::MAX).is_err());
    }
    #[test]
    fn source_alpha_and_atomic_final_commit() {
        for alpha in [0., 0.25, 0.5, 1.] {
            let mut frame =
                LinearColorFrame::from_working(1, 1, &[0., 0., 0., alpha], &mut || Ok(())).unwrap();
            frame
                .map_concat_straight_pixels(&mut |_, _, _| [0.5, 1., 2., alpha], &mut || Ok(()))
                .unwrap();
            FilmGrain::default()
                .apply_linear(&mut frame, 1.25, &mut || Ok(()))
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
        for limit in [8, 17] {
            let mut frame =
                LinearColorFrame::from_working(16, 16, &vec![0.5; 16 * 16 * 4], &mut || Ok(()))
                    .unwrap();
            let before = frame.concat_straight_snapshot(&mut || Ok(())).unwrap();
            let mut calls = 0;
            assert!(FilmGrain::default()
                .apply_linear(&mut frame, 0., &mut || {
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
    fn fractional_original_key_midpoint() {
        let keys = vec![
            super::super::concat_effect_upgrade::ParamKey {derived:false,
                at: 0.,
                value: 2.,
                ease: [0., 0., 1., 1.],
            },
            super::super::concat_effect_upgrade::ParamKey {derived:false,
                at: 1.,
                value: 40.,
                ease: [0.42, -0.5, 0.58, 1.],
            },
        ];
        assert_eq!(
            sample_concat_keys(FilmGrain::default(), &keys, 0.5).unwrap(),
            13.875
        );
        assert!(FilmGrain { amount: 1. }.validate().is_err());
        assert!(FilmGrain { amount: 41. }.validate().is_err());
    }
}

#[cfg(test)]
mod original_spatial_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated FILM_GRAIN_GPU_ORACLE"]
    fn original_metal_spatial_bits() {
        let path = std::env::var("FILM_GRAIN_GPU_ORACLE").unwrap();
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
            let p: FilmGrain = serde_json::from_value(case["parameters"].clone()).unwrap();
            p.apply_linear(
                &mut frame,
                case["seconds"].as_f64().unwrap(),
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
        println!("FilmGrain original spatial: {channels} channels, mismatches RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
