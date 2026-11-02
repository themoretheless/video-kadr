//! FilmCraft Lumetri Basic Correction SDR bridge; MIT notice docs/licenses/filmcraft-MIT.txt.
use super::{composition::AnimatableValue, group_surface::GroupSurface};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BasicGrade {
    pub enabled: bool,
    pub temperature: f32,
    pub tint: f32,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
}
impl Default for BasicGrade {
    fn default() -> Self {
        Self {
            enabled: true,
            temperature: 0.0,
            tint: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            saturation: 100.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BasicChannel {
    Temperature,
    Tint,
    Exposure,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Saturation,
}
pub type BasicAnimation = BTreeMap<BasicChannel, AnimatableValue>;
impl BasicChannel {
    fn bounds(self) -> (f64, f64) {
        match self {
            Self::Temperature => (-100.0, 100.0),
            Self::Tint => (-100.0, 100.0),
            Self::Exposure => (-5.0, 5.0),
            Self::Contrast => (-100.0, 100.0),
            Self::Highlights => (-100.0, 100.0),
            Self::Shadows => (-100.0, 100.0),
            Self::Whites => (-100.0, 100.0),
            Self::Blacks => (-100.0, 100.0),
            Self::Saturation => (0.0, 200.0),
        }
    }
    fn set(self, p: &mut BasicGrade, v: f32) {
        match self {
            Self::Temperature => p.temperature = v,
            Self::Tint => p.tint = v,
            Self::Exposure => p.exposure = v,
            Self::Contrast => p.contrast = v,
            Self::Highlights => p.highlights = v,
            Self::Shadows => p.shadows = v,
            Self::Whites => p.whites = v,
            Self::Blacks => p.blacks = v,
            Self::Saturation => p.saturation = v,
        }
    }
}
pub fn validate_animation(a: &BasicAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = c.bounds();
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "invalid basic grade animation"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: BasicGrade, a: &BasicAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<BasicGrade> {
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
pub(crate) fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
pub(crate) fn display(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
impl BasicGrade {
    pub fn validate(&self) -> Result<()> {
        for (c, v) in [
            (BasicChannel::Temperature, self.temperature),
            (BasicChannel::Tint, self.tint),
            (BasicChannel::Exposure, self.exposure),
            (BasicChannel::Contrast, self.contrast),
            (BasicChannel::Highlights, self.highlights),
            (BasicChannel::Shadows, self.shadows),
            (BasicChannel::Whites, self.whites),
            (BasicChannel::Blacks, self.blacks),
            (BasicChannel::Saturation, self.saturation),
        ] {
            let (lo, hi) = c.bounds();
            ensure!(
                v.is_finite() && (lo..=hi).contains(&(v as f64)),
                "invalid basic grade parameter"
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
        if !self.enabled {
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
        if !self.enabled {
            return Ok(());
        }
        frame.map_signal(
            &mut |v, _, _| {
                let v = self.signal_without_saturation(v);
                let l = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
                Ok(v.map(|q| l + (q - l) * self.saturation / 100.0))
            },
            checkpoint,
        )
    }
    /// Basic pass before saturation; combined Lumetri applies saturation after split tone.
    pub(crate) fn signal_without_saturation(&self, v: [f32; 3]) -> [f32; 3] {
        if !self.enabled {
            return v;
        }
        self.signal_from_linear(v.map(linear))
    }
    pub(crate) fn signal_from_linear(&self, mut v: [f32; 3]) -> [f32; 3] {
        if !self.enabled {
            return v.map(display);
        }
        let exposure = 2f32.powf(self.exposure);
        let gains = [
            1.0 + 0.35 * (self.temperature / 100.0),
            1.0 - 0.3 * (self.tint / 100.0),
            1.0 - 0.35 * (self.temperature / 100.0),
        ];
        let b0 = -self.blacks / 100.0 * 0.15;
        let w0 = 1.0 - self.whites / 100.0 * 0.15;
        for c in 0..3 {
            v[c] = display((v[c] * gains[c] * exposure).max(0.0));
        }
        v = v.map(|q| (q - b0) / (w0 - b0).max(1e-3));
        let l = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
        let ws = (1.0 - l).clamp(0.0, 1.0).powi(3);
        let wh = l.clamp(0.0, 1.0).powi(3);
        let nl =
            (l + self.shadows / 100.0 * 0.35 * ws + self.highlights / 100.0 * 0.35 * wh).max(0.0);
        if l > 1e-5 {
            let k = nl / l;
            v = v.map(|q| q * k);
        }
        let contrast = self.contrast / 100.0;
        if contrast.abs() > 1e-4 {
            let k = 1.0 + contrast;
            v = v.map(|q| {
                let q = q.clamp(0.0, 1.0);
                let s = q * q * (3.0 - 2.0 * q);
                if k >= 1.0 {
                    q + (s - q) * (k - 1.0)
                } else {
                    0.5 + (q - 0.5) * k
                }
            });
        }
        v
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_actual_filmcraft_sdr_basic_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-basic-grade.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 38);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: BasicGrade = serde_json::from_value(case["parameters"].clone()).unwrap();
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
    fn animation_samples_all_nine_channels() {
        for c in [
            BasicChannel::Temperature,
            BasicChannel::Tint,
            BasicChannel::Exposure,
            BasicChannel::Contrast,
            BasicChannel::Highlights,
            BasicChannel::Shadows,
            BasicChannel::Whites,
            BasicChannel::Blacks,
            BasicChannel::Saturation,
        ] {
            let (lo, hi) = c.bounds();
            let v=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":lo},{"tick":1000,"value":hi}]}})).unwrap();
            let a = BTreeMap::from([(c, v)]);
            let mut expected = BasicGrade::default();
            c.set(&mut expected, ((lo + hi) / 2.0) as f32);
            assert_eq!(
                sample_animation(BasicGrade::default(), &a, 500000).unwrap(),
                expected
            );
        }
    }
    #[test]
    fn identity_alpha_and_disabled() {
        let mut f = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![72, 131, 207, 128, 12, 34, 56, 0],
        };
        let original = f.rgba.clone();
        BasicGrade::default().apply(&mut f, &mut || Ok(())).unwrap();
        assert_eq!(f.rgba, original);
        BasicGrade {
            enabled: false,
            exposure: 5.0,
            ..Default::default()
        }
        .apply(&mut f, &mut || Ok(()))
        .unwrap();
        assert_eq!(f.rgba, original);
    }
    #[test]
    fn contrast_zero_saturation_and_validation() {
        let mut f = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 128],
        };
        BasicGrade {
            saturation: 0.0,
            ..Default::default()
        }
        .apply(&mut f, &mut || Ok(()))
        .unwrap();
        assert_eq!(f.rgba, vec![54, 54, 54, 128]);
        assert!(BasicGrade {
            exposure: 5.1,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
