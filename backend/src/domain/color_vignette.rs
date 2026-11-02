//! FilmCraft Lumetri vignette SDR bridge; MIT notice in docs/licenses/filmcraft-MIT.txt.
use super::{composition::AnimatableValue, group_surface::GroupSurface};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorVignette {
    pub enabled: bool,
    pub amount: f32,
    pub midpoint: f32,
    pub roundness: f32,
    pub feather: f32,
}
impl Default for ColorVignette {
    fn default() -> Self {
        Self {
            enabled: true,
            amount: 0.0,
            midpoint: 50.0,
            roundness: 0.0,
            feather: 50.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VignetteChannel {
    Amount,
    Midpoint,
    Roundness,
    Feather,
}
pub type VignetteAnimation = BTreeMap<VignetteChannel, AnimatableValue>;
impl VignetteChannel {
    fn bounds(self) -> (f64, f64) {
        match self {
            Self::Amount => (-5.0, 5.0),
            Self::Roundness => (-100.0, 100.0),
            _ => (0.0, 100.0),
        }
    }
    fn set(self, p: &mut ColorVignette, v: f32) {
        match self {
            Self::Amount => p.amount = v,
            Self::Midpoint => p.midpoint = v,
            Self::Roundness => p.roundness = v,
            Self::Feather => p.feather = v,
        }
    }
}
pub fn validate_animation(a: &VignetteAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = c.bounds();
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "invalid vignette animation"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: ColorVignette,
    a: &VignetteAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ColorVignette> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    validate_animation(a)?;
    for (c, v) in a {
        c.set(
            &mut p,
            super::nested_composition::sample_value(v, t)? as f32,
        );
    }
    p.validate()?;
    Ok(p)
}
impl ColorVignette {
    pub fn validate(&self) -> Result<()> {
        for (c, v) in [
            (VignetteChannel::Amount, self.amount),
            (VignetteChannel::Midpoint, self.midpoint),
            (VignetteChannel::Roundness, self.roundness),
            (VignetteChannel::Feather, self.feather),
        ] {
            let (lo, hi) = c.bounds();
            ensure!(
                v.is_finite() && (lo..=hi).contains(&(v as f64)),
                "invalid vignette parameter"
            );
        }
        Ok(())
    }
    pub fn apply(
        &self,
        image: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        image.validate()?;
        checkpoint()?;
        if !self.enabled || self.amount.abs() <= 1e-4 {
            return Ok(());
        }
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(image, checkpoint)?;
        self.apply_linear(&mut frame, checkpoint)?;
        *image = frame.to_surface(checkpoint)?;
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        if !self.enabled || self.amount.abs() <= 1e-4 {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        frame.map_signal(&mut |v, x, y| Ok(self.signal(v, x, y, w, h)), checkpoint)
    }
    pub(crate) fn signal(&self, v: [f32; 3], x: usize, y: usize, w: usize, h: usize) -> [f32; 3] {
        if !self.enabled || self.amount.abs() <= 1e-4 {
            return v;
        }
        let (w, h) = (w as f32, h as f32);
        let aspect = w / h;
        let round = self.roundness / 100.0;
        let nx = (x as f32 / w - 0.5)
            * 2.0
            * if round < 0.0 {
                aspect.powf(-round)
            } else {
                1.0
            };
        let ny = (y as f32 / h - 0.5) * 2.0;
        let d = (nx * nx + ny * ny).sqrt() / std::f32::consts::SQRT_2;
        let edge = ((d - self.midpoint / 100.0 * 0.9) / ((self.feather / 100.0).max(0.01) * 0.9))
            .clamp(0.0, 1.0);
        let e2 = edge * edge * (3.0 - 2.0 * edge);
        let k = 1.0 + self.amount * 0.2 * e2;
        v.map(|q| {
            if self.amount < 0.0 {
                q * k.max(0.0)
            } else {
                q + (1.0 - q) * (k - 1.0)
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_pinned_upstream_sdr_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-color-vignette.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 45);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: ColorVignette = serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: f["width"].as_u64().unwrap() as usize,
                height: f["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            p.apply(&mut frame, &mut || Ok(())).unwrap();
            for (pixel, (actual, expected)) in frame
                .rgba
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(actual[3], expected[3]);
                if actual[3] == 0 {
                    assert_eq!(actual, &input[pixel * 4..pixel * 4 + 4]);
                    continue;
                }
                for c in 0..3 {
                    assert!(
                        actual[c].abs_diff(expected[c]) <= 1,
                        "case {index} pixel {pixel} channel {c}: {} != {}",
                        actual[c],
                        expected[c]
                    );
                }
            }
        }
    }
    #[test]
    fn animation_samples_all_channels_at_clip_local_time() {
        for channel in [
            VignetteChannel::Amount,
            VignetteChannel::Midpoint,
            VignetteChannel::Roundness,
            VignetteChannel::Feather,
        ] {
            let (lo, hi) = channel.bounds();
            let track=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":lo},{"tick":1000,"value":hi}]}})).unwrap();
            let animation = BTreeMap::from([(channel, track)]);
            let mut expected = ColorVignette::default();
            channel.set(&mut expected, ((lo + hi) / 2.0) as f32);
            assert_eq!(
                sample_animation(ColorVignette::default(), &animation, 500000).unwrap(),
                expected
            );
        }
    }
    #[test]
    fn corners_center_alpha_and_identity() {
        let original = [128, 64, 32, 128].repeat(16);
        let mut f = GroupSurface {
            width: 4,
            height: 4,
            rgba: original.clone(),
        };
        ColorVignette {
            amount: -5.0,
            ..Default::default()
        }
        .apply(&mut f, &mut || Ok(()))
        .unwrap();
        assert_eq!(&f.rgba[..4], &[0, 0, 0, 128]);
        assert_eq!(&f.rgba[40..44], &[128, 64, 32, 128]);
        assert!(f.rgba.chunks_exact(4).all(|p| p[3] == 128));
        ColorVignette::default()
            .apply(&mut f, &mut || Ok(()))
            .unwrap();
    }
    #[test]
    fn rejects_invalid_and_cancellation() {
        assert!(ColorVignette {
            amount: f32::NAN,
            ..Default::default()
        }
        .validate()
        .is_err());
        let mut f = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 0],
        };
        assert!(ColorVignette {
            amount: 5.0,
            ..Default::default()
        }
        .apply(&mut f, &mut || anyhow::bail!("cancel"))
        .is_err());
        assert_eq!(f.rgba, vec![1, 2, 3, 0]);
    }
}
