//! CPU port of WeftCut's color-difference chroma shader.
//! Source: apps/desktop/src/renderer/render/effects/filters/chromaKeySources.ts
//! at d1db8020164c51ee49df9cf0143a5cbf250951a4; MIT notice: docs/licenses/weftcut-MIT.txt.
//! The public surface is straight RGBA8; shader math and tap interpolation use
//! premultiplied RGB, then convert back exactly once at the output boundary.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaParameters {
    pub key: [f64; 3],
    pub balance: f64,
    pub clip_black: f64,
    pub clip_white: f64,
    pub despill: f64,
    pub feather: f64,
    pub shrink: f64,
    pub view_matte: bool,
}
/// Optional clip-local overrides; omitted channels retain the static baseline.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaAnimation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_r: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_g: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_b: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_black: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_white: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub despill: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feather: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shrink: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_matte: Option<super::composition::AnimatableValue>,
}
impl ChromaAnimation {
    pub fn validate(&self) -> Result<()> {
        if let Some(value) = &self.key_r {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation key_r"
            );
        }
        if let Some(value) = &self.key_g {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation key_g"
            );
        }
        if let Some(value) = &self.key_b {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation key_b"
            );
        }
        if let Some(value) = &self.balance {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation balance"
            );
        }
        if let Some(value) = &self.clip_black {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation clip_black"
            );
        }
        if let Some(value) = &self.clip_white {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation clip_white"
            );
        }
        if let Some(value) = &self.despill {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation despill"
            );
        }
        if let Some(value) = &self.feather {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 10.0, true),
                "invalid chroma animation feather"
            );
        }
        if let Some(value) = &self.shrink {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -5.0, 5.0, true),
                "invalid chroma animation shrink"
            );
        }
        if let Some(value) = &self.view_matte {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 1.0, true),
                "invalid chroma animation view_matte"
            );
        }
        Ok(())
    }
    pub fn sample(
        &self,
        mut parameters: ChromaParameters,
        local_us: impl Into<super::animation_sample_time::AnimationSampleTime>,
    ) -> Result<ChromaParameters> {
        let local_us: super::animation_sample_time::AnimationSampleTime = local_us.into();

        self.validate()?;
        if let Some(value) = &self.key_r {
            parameters.key[0] = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.key_g {
            parameters.key[1] = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.key_b {
            parameters.key[2] = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.balance {
            parameters.balance = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.clip_black {
            parameters.clip_black = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.clip_white {
            parameters.clip_white = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.despill {
            parameters.despill = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.feather {
            parameters.feather = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.shrink {
            parameters.shrink = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.view_matte {
            parameters.view_matte = super::nested_composition::sample_value(value, local_us)? > 0.5;
        }
        parameters.validate()?;
        Ok(parameters)
    }
}
impl Default for ChromaParameters {
    fn default() -> Self {
        Self {
            key: [0.0, 1.0, 0.0],
            balance: 0.5,
            clip_black: 0.0,
            clip_white: 1.0,
            despill: 1.0,
            feather: 0.0,
            shrink: 0.0,
            view_matte: false,
        }
    }
}
impl ChromaParameters {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.key
                .iter()
                .chain([
                    &self.balance,
                    &self.clip_black,
                    &self.clip_white,
                    &self.despill
                ])
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid chroma color or levels"
        );
        ensure!(
            self.feather.is_finite() && (0.0..=10.0).contains(&self.feather),
            "invalid chroma feather"
        );
        ensure!(
            self.shrink.is_finite() && (-5.0..=5.0).contains(&self.shrink),
            "invalid chroma shrink"
        );
        Ok(())
    }
}

fn difference(rgb: [f64; 3], dominant: usize, balance: f64) -> f64 {
    let others = [rgb[(dominant + 1) % 3], rgb[(dominant + 2) % 3]];
    rgb[dominant]
        - (others[0].max(others[1]) * (1.0 - balance) + others[0].min(others[1]) * balance)
}

/// Bilinear, edge-clamped texture sampling matches shader texel coordinates.
fn sample(surface: &GroupSurface, x: f64, y: f64) -> [f64; 4] {
    let x = x.clamp(0.0, (surface.width - 1) as f64);
    let y = y.clamp(0.0, (surface.height - 1) as f64);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(surface.width - 1);
    let y1 = (y0 + 1).min(surface.height - 1);
    let mut out = [0.0; 4];
    for (column, row, weight) in [
        (x0, y0, (1.0 - x.fract()) * (1.0 - y.fract())),
        (x1, y0, x.fract() * (1.0 - y.fract())),
        (x0, y1, (1.0 - x.fract()) * y.fract()),
        (x1, y1, x.fract() * y.fract()),
    ] {
        let pixel = &surface.rgba[(row * surface.width + column) * 4..][..4];
        let alpha = f64::from(pixel[3]) / 255.0;
        for c in 0..3 {
            out[c] += f64::from(pixel[c]) / 255.0 * alpha * weight;
        }
        out[3] += alpha * weight;
    }
    out
}

pub fn render(
    source: &GroupSurface,
    params: ChromaParameters,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<GroupSurface> {
    source.validate()?;
    params.validate()?;
    checkpoint()?;
    let mut output = GroupSurface::transparent(source.width, source.height)?;
    let key = params.key;
    let dominant = if key[1] >= key[0] && key[1] >= key[2] {
        1
    } else if key[0] >= key[2] {
        0
    } else {
        2
    };
    let key_difference = difference(key, dominant, params.balance).max(0.0001);
    let alpha_at = |x, y| {
        let rgb = sample(source, x, y);
        let raw =
            1.0 - difference([rgb[0], rgb[1], rgb[2]], dominant, params.balance) / key_difference;
        ((raw - params.clip_black) / (params.clip_white - params.clip_black).max(0.0001))
            .clamp(0.0, 1.0)
    };
    let choked = |x, y| {
        let center = alpha_at(x, y);
        let radius = params.shrink.abs();
        if radius < 0.01 {
            return center;
        }
        let taps = [
            center,
            alpha_at(x + radius, y),
            alpha_at(x - radius, y),
            alpha_at(x, y + radius),
            alpha_at(x, y - radius),
        ];
        if params.shrink < 0.0 {
            taps.into_iter().fold(1.0, f64::min)
        } else {
            taps.into_iter().fold(0.0, f64::max)
        }
    };
    for (y, row) in output.rgba.chunks_mut(source.width * 4).enumerate() {
        checkpoint()?;
        for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
            let x = x as f64;
            let y = y as f64;
            let mut matte = choked(x, y);
            if params.feather >= 0.01 {
                let r = params.feather;
                matte = (matte
                    + choked(x + r, y)
                    + choked(x - r, y)
                    + choked(x, y + r)
                    + choked(x, y - r))
                    * 0.2;
            }
            if params.view_matte {
                let value = (matte * 255.0).round() as u8;
                pixel.copy_from_slice(&[value, value, value, 255]);
                continue;
            }
            let src = sample(source, x, y);
            let mut fg = [0.0; 3];
            for c in 0..3 {
                fg[c] = (src[c] - key[c] * (1.0 - matte) * src[3]).max(0.0);
            }
            let others = [fg[(dominant + 1) % 3], fg[(dominant + 2) % 3]];
            let spill = (fg[dominant] - (others[0] + others[1]) * 0.5).max(0.0) * params.despill;
            fg[dominant] -= spill;
            let luma = spill * [0.2126, 0.7152, 0.0722][dominant] * 0.5;
            let alpha = matte * src[3];
            for c in 0..3 {
                pixel[c] = if alpha > 0.0 {
                    ((fg[c] + luma) / alpha * 255.0).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                };
            }
            pixel[3] = (alpha * 255.0).round() as u8;
        }
    }
    checkpoint()?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screen_foreground_and_matte() {
        let source = GroupSurface {
            width: 3,
            height: 1,
            rgba: vec![0, 255, 0, 255, 255, 0, 0, 255, 128, 128, 128, 128],
        };
        let result = render(&source, ChromaParameters::default(), &mut || Ok(())).unwrap();
        assert_eq!(&result.rgba[..8], &[0, 0, 0, 0, 255, 0, 0, 255]);
        assert_eq!(&result.rgba[8..], &[128, 128, 128, 128]);
        let matte = render(
            &source,
            ChromaParameters {
                view_matte: true,
                ..Default::default()
            },
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(
            matte.rgba,
            [0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255]
        );
    }
    #[test]
    fn nested_shrink_feather_and_fractional_taps() {
        let source = GroupSurface {
            width: 3,
            height: 1,
            rgba: vec![0, 255, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255],
        };
        let params = ChromaParameters {
            view_matte: true,
            shrink: -1.0,
            ..Default::default()
        };
        assert!(render(&source, params, &mut || Ok(()))
            .unwrap()
            .rgba
            .chunks_exact(4)
            .all(|p| p == [0, 0, 0, 255]));
        let params = ChromaParameters {
            view_matte: true,
            shrink: 1.0,
            ..Default::default()
        };
        assert!(render(&source, params, &mut || Ok(()))
            .unwrap()
            .rgba
            .chunks_exact(4)
            .all(|p| p == [255, 255, 255, 255]));
        let params = ChromaParameters {
            view_matte: true,
            feather: 1.0,
            ..Default::default()
        };
        let result = render(&source, params, &mut || Ok(())).unwrap();
        assert_eq!(
            result
                .rgba
                .chunks_exact(4)
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [51, 153, 51]
        );
        let params = ChromaParameters {
            view_matte: true,
            feather: 0.5,
            ..Default::default()
        };
        assert_eq!(
            render(&source, params, &mut || Ok(())).unwrap().rgba[4],
            230
        );
    }
    #[test]
    fn animated_channels_sample_and_validate_on_clip_clock() {
        let animation: ChromaAnimation = serde_json::from_value(serde_json::json!({
            "balance":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000000,"value":1.0}]}},
            "viewMatte":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000000,"value":1.0}]}}
        })).unwrap();
        let baseline = ChromaParameters::default();
        let midpoint = animation.sample(baseline, 500000).unwrap();
        assert_eq!(midpoint.balance, 0.5);
        assert!(!midpoint.view_matte);
        assert!(animation.sample(baseline, 750000).unwrap().view_matte);
        assert_eq!(midpoint.key, baseline.key);
        assert_eq!(animation.sample(baseline, 2000000).unwrap().balance, 1.0);
        let invalid = ChromaAnimation {
            shrink: Some(super::super::composition::AnimatableValue::constant(6.0)),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
        let invalid = ChromaAnimation {
            balance: Some(super::super::composition::AnimatableValue::constant(
                f64::NAN,
            )),
            ..Default::default()
        };
        assert!(invalid.sample(baseline, 0).is_err());
    }
    #[test]
    fn invalid_parameters_and_cancellation_do_not_modify_source() {
        let source = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![0, 255, 0, 255],
        };
        assert!(render(
            &source,
            ChromaParameters {
                feather: f64::NAN,
                ..Default::default()
            },
            &mut || Ok(())
        )
        .is_err());
        assert!(
            render(&source, ChromaParameters::default(), &mut || anyhow::bail!(
                "cancelled"
            ))
            .is_err()
        );
        assert_eq!(source.rgba, [0, 255, 0, 255]);
    }
}
