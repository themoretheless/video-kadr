//! FilmCraft HSL Secondary SDR key/refine/correct CPU bridge.
//! Adapted from render/src/effects.rs at 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! MIT notice: docs/licenses/filmcraft-MIT.txt.
use super::{
    group_surface::GroupSurface,
    tone_curves::{hsl_to_rgb, rgb_to_hsl},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct HslSecondary {
    pub enabled: bool,
    pub show_mask: u32,
    pub hue: f32,
    pub hue_range: f32,
    pub saturation_min: f32,
    pub lightness_min: f32,
    pub lightness_max: f32,
    pub soften: f32,
    pub denoise: f32,
    pub blur: f32,
    pub temperature: f32,
    pub tint: f32,
    pub saturation: f32,
    pub hue_shift: f32,
}
impl Default for HslSecondary {
    fn default() -> Self {
        Self {
            enabled: true,
            show_mask: 0,
            hue: 0.0,
            hue_range: 30.0,
            saturation_min: 10.0,
            lightness_min: 5.0,
            lightness_max: 95.0,
            soften: 20.0,
            denoise: 0.0,
            blur: 0.0,
            temperature: 0.0,
            tint: 0.0,
            saturation: 100.0,
            hue_shift: 0.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HslChannel {
    Hue,
    HueRange,
    SaturationMin,
    LightnessMin,
    LightnessMax,
    Soften,
    Denoise,
    Blur,
    Temperature,
    Tint,
    Saturation,
    HueShift,
}
pub type HslAnimation = BTreeMap<HslChannel, super::composition::AnimatableValue>;
impl HslChannel {
    fn bounds(self) -> (f64, f64) {
        match self {
            Self::Hue => (0.0, 360.0),
            Self::HueRange => (1.0, 180.0),
            Self::SaturationMin => (0.0, 100.0),
            Self::LightnessMin => (0.0, 100.0),
            Self::LightnessMax => (0.0, 100.0),
            Self::Soften => (0.0, 100.0),
            Self::Denoise => (0.0, 100.0),
            Self::Blur => (0.0, 100.0),
            Self::Temperature => (-100.0, 100.0),
            Self::Tint => (-100.0, 100.0),
            Self::Saturation => (0.0, 200.0),
            Self::HueShift => (-180.0, 180.0),
        }
    }
    fn set(self, parameters: &mut HslSecondary, value: f32) {
        match self {
            Self::Hue => parameters.hue = value,
            Self::HueRange => parameters.hue_range = value,
            Self::SaturationMin => parameters.saturation_min = value,
            Self::LightnessMin => parameters.lightness_min = value,
            Self::LightnessMax => parameters.lightness_max = value,
            Self::Soften => parameters.soften = value,
            Self::Denoise => parameters.denoise = value,
            Self::Blur => parameters.blur = value,
            Self::Temperature => parameters.temperature = value,
            Self::Tint => parameters.tint = value,
            Self::Saturation => parameters.saturation = value,
            Self::HueShift => parameters.hue_shift = value,
        }
    }
}
pub fn validate_animation(animation: &HslAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = channel.bounds();
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "invalid HSL animation channel"
        );
    }
    Ok(())
}
pub fn sample_animation(
    parameters: HslSecondary,
    animation: &HslAnimation,
    local_us: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<HslSecondary> {
    let local_us: super::animation_sample_time::AnimationSampleTime = local_us.into();

    validate_animation(animation)?;
    let mut parameters = parameters;
    for (channel, value) in animation {
        channel.set(
            &mut parameters,
            super::nested_composition::sample_value(value, local_us)? as f32,
        );
    }
    parameters.validate()?;
    Ok(parameters)
}
impl HslSecondary {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.show_mask <= 3, "invalid HSL mask view");
        ensure!(
            self.hue.is_finite() && (0.0..=360.0).contains(&self.hue),
            "invalid HSL hue"
        );
        ensure!(
            self.hue_range.is_finite() && (1.0..=180.0).contains(&self.hue_range),
            "invalid HSL hue_range"
        );
        ensure!(
            self.saturation_min.is_finite() && (0.0..=100.0).contains(&self.saturation_min),
            "invalid HSL saturation_min"
        );
        ensure!(
            self.lightness_min.is_finite() && (0.0..=100.0).contains(&self.lightness_min),
            "invalid HSL lightness_min"
        );
        ensure!(
            self.lightness_max.is_finite() && (0.0..=100.0).contains(&self.lightness_max),
            "invalid HSL lightness_max"
        );
        ensure!(
            self.soften.is_finite() && (0.0..=100.0).contains(&self.soften),
            "invalid HSL soften"
        );
        ensure!(
            self.denoise.is_finite() && (0.0..=100.0).contains(&self.denoise),
            "invalid HSL denoise"
        );
        ensure!(
            self.blur.is_finite() && (0.0..=100.0).contains(&self.blur),
            "invalid HSL blur"
        );
        ensure!(
            self.temperature.is_finite() && (-100.0..=100.0).contains(&self.temperature),
            "invalid HSL temperature"
        );
        ensure!(
            self.tint.is_finite() && (-100.0..=100.0).contains(&self.tint),
            "invalid HSL tint"
        );
        ensure!(
            self.saturation.is_finite() && (0.0..=200.0).contains(&self.saturation),
            "invalid HSL saturation"
        );
        ensure!(
            self.hue_shift.is_finite() && (-180.0..=180.0).contains(&self.hue_shift),
            "invalid HSL hue_shift"
        );
        Ok(())
    }
    pub fn apply(
        &self,
        image: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_scaled(image, 1.0, checkpoint)
    }
    /// Working-image pixels per full-resolution source pixel, matching FilmCraft FxCtx.
    pub fn apply_scaled(
        &self,
        image: &mut GroupSurface,
        pixel_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            pixel_scale.is_finite() && pixel_scale > 0.0 && pixel_scale <= 1.0,
            "invalid HSL pixel scale"
        );
        self.validate()?;
        image.validate()?;
        checkpoint()?;
        if !self.enabled {
            return Ok(());
        }
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(image, checkpoint)?;
        self.apply_linear_scaled(&mut frame, pixel_scale, checkpoint)?;
        *image = frame.to_surface(checkpoint)?;
        Ok(())
    }
    pub fn apply_linear_scaled(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        pixel_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            pixel_scale.is_finite() && pixel_scale > 0.0 && pixel_scale <= 1.0,
            "invalid HSL pixel scale"
        );
        checkpoint()?;
        if !self.enabled {
            return Ok(());
        }
        let (w, h) = frame.dimensions();
        let hc = self.hue / 360.0;
        let hr = (self.hue_range / 360.0).max(1e-3);
        let smin = self.saturation_min / 100.0;
        let lmin = self.lightness_min / 100.0;
        let lmax = self.lightness_max / 100.0;
        let soft = (self.soften / 100.0 * 0.3).max(0.01);
        let mut mask = frame.signal_mask(
            &mut |u| Ok(hsl_key(u, hc, hr, smin, lmin, lmax, soft)),
            checkpoint,
        )?;
        // Match Filmcraft: normalize slider values before multiplying pixel values.
        let temperature = self.temperature / 100.0;
        let tint = self.tint / 100.0;
        let saturation = self.saturation / 100.0;
        let hue_shift = self.hue_shift / 360.0;
        let denoise = self.denoise / 100.0;
        let blur = self.blur / 100.0;
        if denoise > 0.0 {
            let r = ((1.0 + 2.0 * denoise) * pixel_scale).round().max(1.0) as usize;
            let median = median_filter(&mask, w, h, r, checkpoint)?;
            for (m, d) in mask.iter_mut().zip(median) {
                *m += (d - *m) * denoise;
            }
        }
        if blur > 0.0 {
            let sigma = blur * 20.0 * pixel_scale;
            if sigma >= 0.3 {
                blur_plane(&mut mask, w, h, sigma, checkpoint)?;
            }
        }
        frame.map_signal(
            &mut |v, x, y| {
                let u = v.map(|q| q.clamp(0.0, 1.0));
                let m = mask[y * w + x];
                let out = match self.show_mask {
                    1 => {
                        let g = rgb_to_hsl(u[0], u[1], u[2])[2];
                        [g + (u[0] - g) * m, g + (u[1] - g) * m, g + (u[2] - g) * m]
                    }
                    2 => u.map(|q| q * m),
                    3 => [m; 3],
                    _ => {
                        let mut hh = rgb_to_hsl(u[0], u[1], u[2]);
                        hh[0] = (hh[0] + hue_shift).rem_euclid(1.0);
                        hh[1] = (hh[1] * saturation).clamp(0.0, 1.0);
                        let mut c2 = hsl_to_rgb(hh[0], hh[1], hh[2]);
                        c2[0] *= 1.0 + 0.25 * temperature;
                        c2[2] *= 1.0 - 0.25 * temperature;
                        c2[1] *= 1.0 - 0.2 * tint;
                        [
                            u[0] + (c2[0] - u[0]) * m,
                            u[1] + (c2[1] - u[1]) * m,
                            u[2] + (c2[2] - u[2]) * m,
                        ]
                    }
                };
                Ok(out)
            },
            checkpoint,
        )
    }
}
fn hsl_key(v: [f32; 3], hc: f32, hr: f32, smin: f32, lmin: f32, lmax: f32, soft: f32) -> f32 {
    let h = rgb_to_hsl(v[0], v[1], v[2]);
    let dh = (h[0] - hc).abs().min(1.0 - (h[0] - hc).abs());
    let mh = 1.0 - ((dh - hr / 2.0) / soft).clamp(0.0, 1.0);
    let ms = ((h[1] - smin) / soft).clamp(0.0, 1.0);
    let ml = ((h[2] - lmin) / soft)
        .clamp(0.0, 1.0)
        .min(((lmax - h[2]) / soft).clamp(0.0, 1.0));
    mh * ms * ml
}
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let wideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = wideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let mideal = (12.0 * sigma * sigma
        - (n as i32 * wl * wl) as f32
        - 4.0 * n as f32 * wl as f32
        - 3.0 * n as f32)
        / (-4.0 * wl as f32 - 4.0);
    let m = mideal.round() as i32;
    (0..n as i32)
        .map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize)
        .collect()
}
fn median_filter(
    src: &[f32],
    w: usize,
    h: usize,
    r: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<f32>> {
    let mut out = vec![0.0; src.len()];
    let mut win = Vec::with_capacity((2 * r + 1) * (2 * r + 1));
    for (y, row) in out.chunks_mut(w).enumerate() {
        checkpoint()?;
        for (x, o) in row.iter_mut().enumerate() {
            win.clear();
            for yy in y.saturating_sub(r)..(y + r + 1).min(h) {
                let line = &src[yy * w..yy * w + w];
                win.extend_from_slice(&line[x.saturating_sub(r)..(x + r + 1).min(w)]);
            }
            let k = win.len() / 2;
            *o = *win.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1;
        }
    }
    Ok(out)
}
fn box_pass(
    p: &mut [f32],
    w: usize,
    r: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    if r == 0 {
        return Ok(());
    }
    for row in p.chunks_mut(w) {
        checkpoint()?;
        let src = row.to_vec();
        let n = (2 * r + 1) as f32;
        let at = |i: isize| src[i.clamp(0, w as isize - 1) as usize];
        let mut acc: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for x in 0..w {
            row[x] = acc / n;
            acc += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
        }
    }
    Ok(())
}
fn transpose(
    p: &[f32],
    w: usize,
    h: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Vec<f32>> {
    let mut t = vec![0.0; p.len()];
    for y in 0..h {
        checkpoint()?;
        for x in 0..w {
            t[x * h + y] = p[y * w + x];
        }
    }
    Ok(t)
}
pub(crate) fn blur_plane(
    p: &mut [f32],
    w: usize,
    h: usize,
    sigma: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    let radii = boxes_for_gauss(sigma, 3);
    for &r in &radii {
        box_pass(p, w, r, checkpoint)?;
    }
    let mut t = transpose(p, w, h, checkpoint)?;
    for &r in &radii {
        box_pass(&mut t, h, r, checkpoint)?;
    }
    p.copy_from_slice(&transpose(&t, h, w, checkpoint)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_pinned_filmcraft_renderer_sdr_oracle() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-hsl-secondary.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let input: Vec<u8> = serde_json::from_value(fixture["input"].clone()).unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let parameters: HslSecondary =
                serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: fixture["width"].as_u64().unwrap() as usize,
                height: fixture["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            parameters
                .apply_scaled(
                    &mut frame,
                    case["pixelScale"].as_f64().unwrap_or(1.0) as f32,
                    &mut || Ok(()),
                )
                .unwrap();
            for (pixel, (actual, reference)) in frame
                .rgba
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(actual[3], reference[3], "alpha case {index} pixel {pixel}");
                if actual[3] == 0 {
                    continue;
                } // Upstream serialization clears hidden RGB.
                for c in 0..3 {
                    assert!(
                        actual[c].abs_diff(reference[c]) <= 1,
                        "case {index} pixel {pixel} channel {c}: {} != {}",
                        actual[c],
                        reference[c]
                    );
                }
            }
        }
    }
    #[test]
    fn scaled_refinement_rejects_invalid_scale_before_mutation() {
        for scale in [0.0, -0.5, 1.01, f32::NAN, f32::INFINITY] {
            let mut frame = image();
            let before = frame.clone();
            assert!(HslSecondary::default()
                .apply_scaled(&mut frame, scale, &mut || Ok(()))
                .is_err());
            assert_eq!(frame.rgba, before.rgba);
        }
    }
    fn image() -> GroupSurface {
        GroupSurface {
            width: 4,
            height: 1,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 12, 34, 56, 0,
            ],
        }
    }
    #[test]
    fn selected_red_rotates_to_green_without_changing_other_colors_or_alpha() {
        let mut frame = image();
        HslSecondary {
            hue_shift: 120.0,
            ..Default::default()
        }
        .apply(&mut frame, &mut || Ok(()))
        .unwrap();
        assert_eq!(
            frame.rgba,
            vec![0, 255, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 12, 34, 56, 0]
        );
    }
    #[test]
    fn mask_views_and_disabled_identity() {
        for (view, expected) in [
            (
                1,
                vec![
                    255, 0, 0, 255, 128, 128, 128, 128, 128, 128, 128, 255, 12, 34, 56, 0,
                ],
            ),
            (
                2,
                vec![255, 0, 0, 255, 0, 0, 0, 128, 0, 0, 0, 255, 12, 34, 56, 0],
            ),
            (
                3,
                vec![
                    255, 255, 255, 255, 0, 0, 0, 128, 0, 0, 0, 255, 12, 34, 56, 0,
                ],
            ),
        ] {
            let mut frame = image();
            HslSecondary {
                show_mask: view,
                ..Default::default()
            }
            .apply(&mut frame, &mut || Ok(()))
            .unwrap();
            assert_eq!(frame.rgba, expected);
        }
        let mut frame = image();
        HslSecondary {
            enabled: false,
            hue_shift: 180.0,
            ..Default::default()
        }
        .apply(&mut frame, &mut || Ok(()))
        .unwrap();
        assert_eq!(frame.rgba, image().rgba);
    }
    #[test]
    fn refinement_removes_isolated_selection_and_preserves_constant_fields() {
        let mut frame = GroupSurface {
            width: 9,
            height: 9,
            rgba: [0, 255, 0, 255].repeat(81),
        };
        frame.rgba[40 * 4..41 * 4].copy_from_slice(&[255, 0, 0, 255]);
        HslSecondary {
            denoise: 100.0,
            show_mask: 3,
            ..Default::default()
        }
        .apply(&mut frame, &mut || Ok(()))
        .unwrap();
        assert!(frame.rgba.chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
        let mut frame = GroupSurface {
            width: 3,
            height: 2,
            rgba: [255, 0, 0, 128].repeat(6),
        };
        HslSecondary {
            denoise: 100.0,
            blur: 100.0,
            show_mask: 3,
            ..Default::default()
        }
        .apply(&mut frame, &mut || Ok(()))
        .unwrap();
        assert!(frame
            .rgba
            .chunks_exact(4)
            .all(|p| p == [255, 255, 255, 128]));
    }
    #[test]
    fn rejects_invalid_parameters_and_checks_cancellation_during_refinement() {
        for channel in [
            HslChannel::Hue,
            HslChannel::HueRange,
            HslChannel::SaturationMin,
            HslChannel::LightnessMin,
            HslChannel::LightnessMax,
            HslChannel::Soften,
            HslChannel::Denoise,
            HslChannel::Blur,
            HslChannel::Temperature,
            HslChannel::Tint,
            HslChannel::Saturation,
            HslChannel::HueShift,
        ] {
            let mut parameters = HslSecondary::default();
            channel.set(&mut parameters, f32::NAN);
            assert!(parameters.validate().is_err());
        }
        let mut calls = 0;
        let result = HslSecondary {
            denoise: 100.0,
            blur: 100.0,
            ..Default::default()
        }
        .apply(&mut image(), &mut || {
            calls += 1;
            anyhow::ensure!(calls < 7, "cancelled");
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
    #[test]
    fn all_twelve_animation_channels_sample_clip_local_time() {
        for channel in [
            HslChannel::Hue,
            HslChannel::HueRange,
            HslChannel::SaturationMin,
            HslChannel::LightnessMin,
            HslChannel::LightnessMax,
            HslChannel::Soften,
            HslChannel::Denoise,
            HslChannel::Blur,
            HslChannel::Temperature,
            HslChannel::Tint,
            HslChannel::Saturation,
            HslChannel::HueShift,
        ] {
            let (lo, hi) = channel.bounds();
            let track = serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":lo},{"tick":1000,"value":hi}]}})).unwrap();
            let animation = BTreeMap::from([(channel, track)]);
            let mut expected = HslSecondary::default();
            channel.set(&mut expected, ((lo + hi) / 2.0) as f32);
            assert_eq!(
                sample_animation(HslSecondary::default(), &animation, 500000).unwrap(),
                expected
            );
        }
        assert!(serde_json::from_value::<HslAnimation>(
            serde_json::json!({"unknown":{"mode":"constant","value":0}})
        )
        .is_err());
    }
}
