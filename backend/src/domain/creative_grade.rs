//! FilmCraft procedural looks and tonal wheels, SDR display-encoded bridge.
//! Adapted from crates/render/src/effects.rs at 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! MIT notice: docs/licenses/filmcraft-MIT.txt.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorWheel {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    /// Authored percent, same -100..100 scale as FilmCraft.
    #[serde(default)]
    pub lightness: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeGrade {
    #[serde(default)]
    pub look: u32,
    #[serde(default = "default_intensity")]
    pub intensity: f32,
    #[serde(default)]
    pub shadows: ColorWheel,
    #[serde(default)]
    pub midtones: ColorWheel,
    #[serde(default)]
    pub highlights: ColorWheel,
}
/// Clip-local overrides; omitted channels keep the static authored baseline.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreativeAnimation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intensity: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadows_x: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadows_y: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadows_lightness: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midtones_x: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midtones_y: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midtones_lightness: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights_x: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights_y: Option<super::composition::AnimatableValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights_lightness: Option<super::composition::AnimatableValue>,
}
impl CreativeAnimation {
    pub fn validate(&self) -> Result<()> {
        if let Some(value) = &self.intensity {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, 0.0, 200.0, true),
                "invalid creative animation intensity"
            );
        }
        if let Some(value) = &self.shadows_x {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation shadows_x"
            );
        }
        if let Some(value) = &self.shadows_y {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation shadows_y"
            );
        }
        if let Some(value) = &self.shadows_lightness {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -100.0, 100.0, true),
                "invalid creative animation shadows_lightness"
            );
        }
        if let Some(value) = &self.midtones_x {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation midtones_x"
            );
        }
        if let Some(value) = &self.midtones_y {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation midtones_y"
            );
        }
        if let Some(value) = &self.midtones_lightness {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -100.0, 100.0, true),
                "invalid creative animation midtones_lightness"
            );
        }
        if let Some(value) = &self.highlights_x {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation highlights_x"
            );
        }
        if let Some(value) = &self.highlights_y {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -1.0, 1.0, true),
                "invalid creative animation highlights_y"
            );
        }
        if let Some(value) = &self.highlights_lightness {
            value.validate()?;
            ensure!(
                super::composition::animatable_values_in(value, -100.0, 100.0, true),
                "invalid creative animation highlights_lightness"
            );
        }
        Ok(())
    }
    pub fn sample(
        &self,
        mut parameters: CreativeGrade,
        local_us: impl Into<super::animation_sample_time::AnimationSampleTime>,
    ) -> Result<CreativeGrade> {
        let local_us: super::animation_sample_time::AnimationSampleTime = local_us.into();

        self.validate()?;
        if let Some(value) = &self.intensity {
            parameters.intensity = super::nested_composition::sample_value(value, local_us)? as f32;
        }
        if let Some(value) = &self.shadows_x {
            parameters.shadows.x = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.shadows_y {
            parameters.shadows.y = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.shadows_lightness {
            parameters.shadows.lightness =
                super::nested_composition::sample_value(value, local_us)? as f32;
        }
        if let Some(value) = &self.midtones_x {
            parameters.midtones.x = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.midtones_y {
            parameters.midtones.y = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.midtones_lightness {
            parameters.midtones.lightness =
                super::nested_composition::sample_value(value, local_us)? as f32;
        }
        if let Some(value) = &self.highlights_x {
            parameters.highlights.x = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.highlights_y {
            parameters.highlights.y = super::nested_composition::sample_value(value, local_us)?;
        }
        if let Some(value) = &self.highlights_lightness {
            parameters.highlights.lightness =
                super::nested_composition::sample_value(value, local_us)? as f32;
        }
        parameters.validate()?;
        Ok(parameters)
    }
}
fn default_intensity() -> f32 {
    100.0
}
impl Default for CreativeGrade {
    fn default() -> Self {
        Self {
            look: 0,
            intensity: 100.0,
            shadows: Default::default(),
            midtones: Default::default(),
            highlights: Default::default(),
        }
    }
}
fn wheel_rgb(w: ColorWheel) -> [f32; 3] {
    let (x, y) = (w.x, w.y);
    let len = (x * x + y * y).sqrt().min(1.0) as f32;
    if len < 1e-5 {
        return [0.0; 3];
    }
    let a = y.atan2(x) as f32;
    let tau = std::f32::consts::TAU;
    [
        a.cos() * len,
        (a - tau / 3.0).cos() * len,
        (a + tau / 3.0).cos() * len,
    ]
}
impl CreativeGrade {
    pub(crate) fn has_signal_adjustment(&self) -> bool {
        let [shadows, midtones, highlights] =
            [self.shadows, self.midtones, self.highlights].map(wheel_rgb);
        self.look > 0
            || shadows
                .iter()
                .chain(&midtones)
                .chain(&highlights)
                .any(|value| value.abs() > 1e-5)
            || (self.shadows.lightness / 100.).abs()
                + (self.midtones.lightness / 100.).abs()
                + (self.highlights.lightness / 100.).abs()
                > 1e-5
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.look <= 8 && self.intensity.is_finite() && (0.0..=200.0).contains(&self.intensity),
            "invalid creative look"
        );
        for w in [self.shadows, self.midtones, self.highlights] {
            ensure!(
                w.x.is_finite()
                    && w.y.is_finite()
                    && (-1.0..=1.0).contains(&w.x)
                    && (-1.0..=1.0).contains(&w.y)
                    && w.lightness.is_finite()
                    && (-100.0..=100.0).contains(&w.lightness),
                "invalid color wheel"
            );
        }
        Ok(())
    }
    pub fn apply(
        &self,
        surface: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        surface.validate()?;
        checkpoint()?;
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(surface, checkpoint)?;
        self.apply_linear(&mut frame, checkpoint)?;
        *surface = frame.to_surface(checkpoint)?;
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let signal = self.prepare_signal()?;
        frame.map_signal(&mut |v, _, _| Ok(signal(v)), checkpoint)
    }
    pub(crate) fn prepare_signal(&self) -> Result<impl Fn([f32; 3]) -> [f32; 3]> {
        self.validate()?;
        Ok(self.prepare_signal_unchecked())
    }
    // Trusted numerical derivative probes may temporarily exceed authored slider bounds.
    pub(crate) fn prepare_signal_unchecked(&self) -> impl Fn([f32; 3]) -> [f32; 3] {
        let [ws, wm, wh] = [self.shadows, self.midtones, self.highlights].map(wheel_rgb);
        let [ls, lm, lh] = [
            self.shadows.lightness,
            self.midtones.lightness,
            self.highlights.lightness,
        ]
        .map(|v| v / 100.0);
        let wheels = ws.iter().chain(&wm).chain(&wh).any(|v| v.abs() > 1e-5)
            || ls.abs() + lm.abs() + lh.abs() > 1e-5;
        let parameters = *self;
        // Filmcraft computes the percent coefficient before the pixel difference.
        let look_intensity = self.intensity / 100.0;
        move |mut v: [f32; 3]| {
            if parameters.look > 0 {
                let lk = apply_look(parameters.look, v);
                for k in 0..3 {
                    v[k] += (lk[k] - v[k]) * look_intensity;
                }
            }
            if wheels {
                let l = (0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]).clamp(0.0, 1.0);
                let wsh = (1.0 - l).powi(2);
                let whi = l * l;
                let wmid = (1.0 - wsh - whi).max(0.0);
                for k in 0..3 {
                    v[k] += (ws[k] * 0.3 + ls * 0.3) * wsh;
                    v[k] += (wm[k] * 0.3 + lm * 0.3) * wmid;
                    v[k] *= 1.0 + (wh[k] * 0.5 + lh * 0.5) * whi;
                }
            }
            v
        }
    }
}
pub fn apply_look(look: u32, c: [f32; 3]) -> [f32; 3] {
    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let mix = |a: [f32; 3], b: [f32; 3], k: f32| {
        [
            a[0] + (b[0] - a[0]) * k,
            a[1] + (b[1] - a[1]) * k,
            a[2] + (b[2] - a[2]) * k,
        ]
    };
    let s_curve = |v: f32, k: f32| {
        let x = v.clamp(0.0, 1.0);
        x + (x * x * (3.0 - 2.0 * x) - x) * k
    };
    match look {
        1 => {
            // teal shadows, orange highlights
            let shadow = [0.0, 0.08, 0.1];
            let high = [0.1, 0.04, -0.06];
            let c = [
                c[0] + shadow[0] * (1.0 - l) + high[0] * l,
                c[1] + shadow[1] * (1.0 - l) + high[1] * l,
                c[2] + shadow[2] * (1.0 - l) + high[2] * l,
            ];
            c.map(|v| s_curve(v, 0.35))
        }
        2 => {
            let c = [c[0] * 1.06 + 0.02, c[1] * 1.0 + 0.01, c[2] * 0.9];
            mix(c, [l, l, l], 0.15).map(|v| v * 0.94 + 0.04)
        }
        3 => [c[0] * 0.9, c[1] * 0.98, c[2] * 1.1 + 0.02].map(|v| s_curve(v, 0.2)),
        4 => mix(c, [l, l, l], 0.55).map(|v| s_curve(v, 0.6)),
        5 => mix(c, [l, l, l], 0.25).map(|v| 0.08 + v * 0.84),
        6 => [l, l, l].map(|v| s_curve(v, 0.3)),
        7 => [c[0] * 1.1 + 0.03, c[1] * 1.02 + 0.01, c[2] * 0.82].map(|v| s_curve(v, 0.15)),
        8 => [c[0] * 0.75, c[1] * 0.85, c[2] * 1.15].map(|v| v * 0.8),
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixel(parameters: CreativeGrade, input: [u8; 4]) -> Vec<u8> {
        let mut image = GroupSurface {
            width: 1,
            height: 1,
            rgba: input.to_vec(),
        };
        parameters.apply(&mut image, &mut || Ok(())).unwrap();
        image.rgba
    }
    #[test]
    fn matches_actual_filmcraft_sdr_looks_wheels_renderer() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-looks-wheels.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 28);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: CreativeGrade = serde_json::from_value(case["parameters"].clone()).unwrap();
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
    fn neutral_all_looks_intensity_and_alpha() {
        for rgb in [[0, 0, 0, 0], [255, 255, 255, 128], [22, 137, 244, 255]] {
            assert_eq!(pixel(Default::default(), rgb), rgb);
        }
        for look in 1..=8 {
            assert_eq!(
                pixel(
                    CreativeGrade {
                        look,
                        intensity: 0.0,
                        ..Default::default()
                    },
                    [22, 137, 244, 73]
                ),
                [22, 137, 244, 73]
            );
            let graded = pixel(
                CreativeGrade {
                    look,
                    ..Default::default()
                },
                [22, 137, 244, 73],
            );
            assert_eq!(graded[3], 73);
            assert_ne!(&graded[..3], &[22, 137, 244]);
        }
        assert_eq!(
            pixel(
                CreativeGrade {
                    look: 6,
                    ..Default::default()
                },
                [0, 0, 255, 73]
            ),
            [14, 14, 14, 73]
        );
        assert_eq!(
            pixel(
                CreativeGrade {
                    look: 8,
                    ..Default::default()
                },
                [255, 255, 255, 73]
            ),
            [153, 173, 235, 73]
        );
    }
    #[test]
    fn wheels_tonal_weights_radius_saturation_and_look_before_wheels() {
        let shadows = CreativeGrade {
            shadows: ColorWheel {
                x: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(pixel(shadows, [0, 0, 255, 73]), [66, 0, 222, 73]);
        assert_eq!(pixel(shadows, [255, 255, 255, 73]), [255, 255, 255, 73]);
        let midtones = CreativeGrade {
            midtones: ColorWheel {
                lightness: 100.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(pixel(midtones, [0, 0, 0, 73]), [0, 0, 0, 73]);
        assert_eq!(pixel(midtones, [255, 255, 255, 73]), [255, 255, 255, 73]);
        assert_eq!(pixel(midtones, [128, 128, 128, 73]), [166, 166, 166, 73]);
        let highlights = CreativeGrade {
            highlights: ColorWheel {
                lightness: -100.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(pixel(highlights, [255, 255, 255, 73]), [128, 128, 128, 73]);
        assert_eq!(pixel(highlights, [0, 0, 0, 73]), [0, 0, 0, 73]);
        let capped = wheel_rgb(ColorWheel {
            x: 1.0,
            y: 1.0,
            lightness: 0.0,
        });
        let unit = wheel_rgb(ColorWheel {
            x: f64::from(0.5f32.sqrt()),
            y: f64::from(0.5f32.sqrt()),
            lightness: 0.0,
        });
        for i in 0..3 {
            assert!((capped[i] - unit[i]).abs() < 1e-6);
        }
        let grade = CreativeGrade { look: 6, ..shadows };
        let result = pixel(grade, [0, 0, 255, 73]);
        assert_ne!(
            result,
            pixel(
                CreativeGrade {
                    look: 6,
                    ..Default::default()
                },
                [66, 0, 222, 73]
            )
        );
    }
    #[test]
    fn hostile_parameters_strict_wire_and_cancellation() {
        assert!(CreativeGrade {
            look: 9,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(CreativeGrade {
            intensity: f32::NAN,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(CreativeGrade {
            shadows: ColorWheel {
                x: 2.0,
                ..Default::default()
            },
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(serde_json::from_str::<CreativeGrade>(r#"{"shadows":{"unknown":1}}"#).is_err());
        let mut image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 255, 73],
        };
        assert!(CreativeGrade::default()
            .apply(&mut image, &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn all_animation_channels_use_local_clock_and_preserve_omitted_baselines() {
        let mut object = serde_json::Map::new();
        for name in [
            "intensity",
            "shadowsX",
            "shadowsY",
            "shadowsLightness",
            "midtonesX",
            "midtonesY",
            "midtonesLightness",
            "highlightsX",
            "highlightsY",
            "highlightsLightness",
        ] {
            let end = if name == "intensity" {
                200.0
            } else if name.ends_with("Lightness") {
                100.0
            } else {
                1.0
            };
            object.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000,"value":end}]}}));
        }
        let animation: CreativeAnimation =
            serde_json::from_value(serde_json::Value::Object(object)).unwrap();
        let value = animation
            .sample(
                CreativeGrade {
                    look: 6,
                    ..Default::default()
                },
                500000,
            )
            .unwrap();
        assert_eq!(value.look, 6);
        assert_eq!(value.intensity, 100.0);
        for wheel in [value.shadows, value.midtones, value.highlights] {
            assert_eq!(
                wheel,
                ColorWheel {
                    x: 0.5,
                    y: 0.5,
                    lightness: 50.0
                }
            );
        }
        assert_eq!(
            animation
                .sample(Default::default(), 1000000)
                .unwrap()
                .intensity,
            200.0
        );
        let baseline = CreativeGrade {
            intensity: 127.0,
            shadows: ColorWheel {
                x: 0.7,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            CreativeAnimation::default()
                .sample(baseline, 500000)
                .unwrap(),
            baseline
        );
        let invalid: CreativeAnimation =
            serde_json::from_value(serde_json::json!({"shadowsX":{"mode":"constant","value":2.0}}))
                .unwrap();
        assert!(invalid.validate().is_err());
        assert!(
            serde_json::from_value::<CreativeAnimation>(serde_json::json!({"unknown":{}})).is_err()
        );
    }
}
