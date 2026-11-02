//! FilmCraft Lumetri creative faded film/split tone/vibrance SDR bridge; MIT notice docs/licenses/filmcraft-MIT.txt.
use super::{composition::AnimatableValue, group_surface::GroupSurface};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeAdjust {
    pub enabled: bool,
    pub faded_film: f32,
    pub vibrance: f32,
    pub saturation: f32,
    pub shadow_red: f32,
    pub shadow_green: f32,
    pub shadow_blue: f32,
    pub shadow_alpha: f32,
    pub highlight_red: f32,
    pub highlight_green: f32,
    pub highlight_blue: f32,
    pub highlight_alpha: f32,
}
impl Default for CreativeAdjust {
    fn default() -> Self {
        Self {
            enabled: true,
            faded_film: 0.0,
            vibrance: 0.0,
            saturation: 100.0,
            shadow_red: 0.5,
            shadow_green: 0.5,
            shadow_blue: 0.5,
            shadow_alpha: 1.0,
            highlight_red: 0.5,
            highlight_green: 0.5,
            highlight_blue: 0.5,
            highlight_alpha: 1.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdjustChannel {
    FadedFilm,
    Vibrance,
    Saturation,
    ShadowRed,
    ShadowGreen,
    ShadowBlue,
    ShadowAlpha,
    HighlightRed,
    HighlightGreen,
    HighlightBlue,
    HighlightAlpha,
}
pub type AdjustAnimation = BTreeMap<AdjustChannel, AnimatableValue>;
impl AdjustChannel {
    fn bounds(self) -> (f64, f64) {
        match self {
            Self::FadedFilm => (0.0, 100.0),
            Self::Vibrance => (-100.0, 100.0),
            Self::Saturation => (0.0, 200.0),
            _ => (0.0, 1.0),
        }
    }
    fn set(self, p: &mut CreativeAdjust, v: f32) {
        match self {
            Self::FadedFilm => p.faded_film = v,
            Self::Vibrance => p.vibrance = v,
            Self::Saturation => p.saturation = v,
            Self::ShadowRed => p.shadow_red = v,
            Self::ShadowGreen => p.shadow_green = v,
            Self::ShadowBlue => p.shadow_blue = v,
            Self::ShadowAlpha => p.shadow_alpha = v,
            Self::HighlightRed => p.highlight_red = v,
            Self::HighlightGreen => p.highlight_green = v,
            Self::HighlightBlue => p.highlight_blue = v,
            Self::HighlightAlpha => p.highlight_alpha = v,
        }
    }
}
pub fn validate_animation(a: &AdjustAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = c.bounds();
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "invalid creative adjustment animation"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: CreativeAdjust,
    a: &AdjustAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<CreativeAdjust> {
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
impl CreativeAdjust {
    pub fn validate(&self) -> Result<()> {
        for (c, v) in [
            (AdjustChannel::FadedFilm, self.faded_film),
            (AdjustChannel::Vibrance, self.vibrance),
            (AdjustChannel::Saturation, self.saturation),
            (AdjustChannel::ShadowRed, self.shadow_red),
            (AdjustChannel::ShadowGreen, self.shadow_green),
            (AdjustChannel::ShadowBlue, self.shadow_blue),
            (AdjustChannel::ShadowAlpha, self.shadow_alpha),
            (AdjustChannel::HighlightRed, self.highlight_red),
            (AdjustChannel::HighlightGreen, self.highlight_green),
            (AdjustChannel::HighlightBlue, self.highlight_blue),
            (AdjustChannel::HighlightAlpha, self.highlight_alpha),
        ] {
            let (lo, hi) = c.bounds();
            ensure!(
                v.is_finite() && (lo..=hi).contains(&(v as f64)),
                "invalid creative adjustment parameter"
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
            &mut |v, _, _| Ok(self.signal_with_saturation(v, 1.0)),
            checkpoint,
        )
    }
    pub(crate) fn signal_with_saturation(&self, mut v: [f32; 3], base_saturation: f32) -> [f32; 3] {
        if !self.enabled {
            let l = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
            return v.map(|q| l + (q - l) * base_saturation);
        }
        let st = [self.shadow_red, self.shadow_green, self.shadow_blue];
        let ht = [
            self.highlight_red,
            self.highlight_green,
            self.highlight_blue,
        ];
        let faded = self.faded_film / 100.0;
        if faded > 0.0 {
            v = v.map(|q| q * (1.0 - 0.25 * faded) + 0.12 * faded);
        }
        let l = (0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]).clamp(0.0, 1.0);
        for c in 0..3 {
            v[c] += (st[c] - 0.5) * 0.3 * (1.0 - l) + (ht[c] - 0.5) * 0.3 * l;
        }
        let l = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
        let range = v[0].max(v[1]).max(v[2]) - v[0].min(v[1]).min(v[2]);
        let sat = base_saturation * (self.saturation / 100.0)
            * (1.0 + self.vibrance / 100.0 * (1.0 - range.clamp(0.0, 1.0)));
        v.map(|q| l + (q - l) * sat)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_actual_filmcraft_sdr_creative_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-creative-adjust.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 25);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: CreativeAdjust = serde_json::from_value(case["parameters"].clone()).unwrap();
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
    fn samples_all_nine_channels_and_cancels() {
        for c in [
            AdjustChannel::FadedFilm,
            AdjustChannel::Vibrance,
            AdjustChannel::Saturation,
            AdjustChannel::ShadowRed,
            AdjustChannel::ShadowGreen,
            AdjustChannel::ShadowBlue,
            AdjustChannel::HighlightRed,
            AdjustChannel::HighlightGreen,
            AdjustChannel::HighlightBlue,
        ] {
            let (lo, hi) = c.bounds();
            let v=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":lo},{"tick":1000,"value":hi}]}})).unwrap();
            let a = BTreeMap::from([(c, v)]);
            let mut expected = CreativeAdjust::default();
            c.set(&mut expected, ((lo + hi) / 2.0) as f32);
            assert_eq!(
                sample_animation(CreativeAdjust::default(), &a, 500000).unwrap(),
                expected
            );
        }
        let mut frame = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 128],
        };
        assert!(CreativeAdjust::default()
            .apply(&mut frame, &mut || anyhow::bail!("cancel"))
            .is_err());
        assert_eq!(frame.rgba, vec![1, 2, 3, 128]);
    }
    #[test]
    fn faded_black_and_alpha() {
        let mut f = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 128, 12, 34, 56, 0],
        };
        CreativeAdjust {
            faded_film: 100.0,
            ..Default::default()
        }
        .apply(&mut f, &mut || Ok(()))
        .unwrap();
        assert_eq!(f.rgba, vec![31, 31, 31, 128, 12, 34, 56, 0]);
    }
    #[test]
    fn neutral_identity() {
        let mut f = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![72, 131, 207, 128],
        };
        let original = f.rgba.clone();
        CreativeAdjust::default()
            .apply(&mut f, &mut || Ok(()))
            .unwrap();
        assert_eq!(f.rgba, original);
        assert!(CreativeAdjust {
            shadow_red: 1.01,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
