//! Unified FilmCraft SDR Lumetri section ordering; MIT notice docs/licenses/filmcraft-MIT.txt.
use super::{
    basic_grade::BasicGrade, color_lut::ColorLut, color_sharpen::ColorSharpen,
    color_vignette::ColorVignette, creative_adjust::CreativeAdjust, creative_grade::CreativeGrade,
    hsl_secondary::HslSecondary, linear_color_frame::LinearColorFrame, tone_curves::ToneCurves,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LumetriAnimation {
    pub basic: super::basic_grade::BasicAnimation,
    pub creative: super::creative_adjust::AdjustAnimation,
    pub vignette: super::color_vignette::VignetteAnimation,
    pub grade: super::creative_grade::CreativeAnimation,
    pub secondary: super::hsl_secondary::HslAnimation,
    pub sharpen: Option<super::composition::AnimatableValue>,
}
impl LumetriAnimation {
    pub fn validate(&self) -> Result<()> {
        super::basic_grade::validate_animation(&self.basic)?;
        super::creative_adjust::validate_animation(&self.creative)?;
        super::color_vignette::validate_animation(&self.vignette)?;
        self.grade.validate()?;
        super::hsl_secondary::validate_animation(&self.secondary)?;
        if let Some(a) = &self.sharpen {
            super::color_sharpen::validate_animation(a)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Lumetri {
    pub enabled: bool,
    pub basic: BasicGrade,
    pub creative: CreativeAdjust,
    pub vignette: ColorVignette,
    pub grade: CreativeGrade,
    pub curves: ToneCurves,
    pub secondary: HslSecondary,
    pub sharpen: ColorSharpen,
    pub curves_on: bool,
    pub wheels_on: bool,
    pub input_lut: Option<ColorLut>,
    pub look_lut: Option<ColorLut>,
    pub animation: Option<LumetriAnimation>,
    pub input_lut_builtin: Option<String>,
    pub look_lut_builtin: Option<String>,
    pub input_lut_library: Option<String>,
    pub look_lut_library: Option<String>,
}
impl Default for Lumetri {
    fn default() -> Self {
        Self {
            enabled: true,
            basic: Default::default(),
            creative: Default::default(),
            vignette: Default::default(),
            grade: Default::default(),
            curves: Default::default(),
            secondary: HslSecondary {
                enabled: false,
                ..Default::default()
            },
            sharpen: Default::default(),
            curves_on: true,
            wheels_on: true,
            input_lut: None,
            look_lut: None,
            animation: None,
            input_lut_builtin: None,
            look_lut_builtin: None,
            input_lut_library: None,
            look_lut_library: None,
        }
    }
}
impl Lumetri {
    pub fn validate(&self) -> Result<()> {
        if let Some(a) = &self.animation {
            a.validate()?;
        }
        for (inline, builtin, library) in [
            (
                &self.input_lut,
                &self.input_lut_builtin,
                &self.input_lut_library,
            ),
            (
                &self.look_lut,
                &self.look_lut_builtin,
                &self.look_lut_library,
            ),
        ] {
            ensure!(
                usize::from(inline.is_some())
                    + usize::from(builtin.is_some())
                    + usize::from(library.is_some())
                    <= 1,
                "choose one LUT per Lumetri role"
            );
            if let Some(id) = library {
                ensure!(
                    !id.trim().is_empty() && id.len() <= 128,
                    "invalid project LUT reference"
                );
            }
            if let Some(id) = builtin {
                ensure!(
                    super::builtin_luts::catalog()?.iter().any(|b| &b.id == id),
                    "unknown builtin LUT"
                );
            }
        }
        self.basic.validate()?;
        self.creative.validate()?;
        self.vignette.validate()?;
        self.grade.validate()?;
        self.curves.validate()?;
        self.secondary.validate()?;
        self.sharpen.validate()?;
        if let Some(lut) = &self.input_lut {
            lut.validate()?;
            ensure!(
                lut.intensity == 100.0,
                "Lumetri input LUT requires full intensity"
            );
        }
        if let Some(lut) = &self.look_lut {
            lut.validate()?;
            ensure!(
                lut.intensity == 100.0,
                "Lumetri look LUT uses grade intensity"
            );
        }
        Ok(())
    }
    pub fn validate_library(&self, library: &[super::project_luts::ProjectLut]) -> Result<()> {
        self.validate()?;
        for id in [&self.input_lut_library, &self.look_lut_library]
            .into_iter()
            .flatten()
        {
            super::project_luts::resolve_effect_reference(library, &format!("lib:{id}"))?;
        }
        Ok(())
    }
    pub fn apply_linear_with_library(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        library: &[super::project_luts::ProjectLut],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate_library(library)?;
        self.apply_linear_context(frame, scale, library, checkpoint)
    }
    pub fn sample(
        &self,
        local_us: impl Into<super::animation_sample_time::AnimationSampleTime>,
    ) -> Result<Self> {
        let local_us: super::animation_sample_time::AnimationSampleTime = local_us.into();

        self.validate()?;
        let mut p = self.clone();
        if let Some(a) = &self.animation {
            p.basic = super::basic_grade::sample_animation(p.basic, &a.basic, local_us)?;
            p.creative =
                super::creative_adjust::sample_animation(p.creative, &a.creative, local_us)?;
            p.vignette =
                super::color_vignette::sample_animation(p.vignette, &a.vignette, local_us)?;
            p.grade = a.grade.sample(p.grade, local_us)?;
            p.secondary =
                super::hsl_secondary::sample_animation(p.secondary, &a.secondary, local_us)?;
            if let Some(value) = &a.sharpen {
                p.sharpen = super::color_sharpen::sample_animation(value, local_us)?;
            }
        }
        p.animation = None;
        p.validate()?;
        Ok(p)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        self.apply_linear_probe(frame, scale, checkpoint)
    }
    // Only the solver's validated base and bounded finite differences use this path.
    pub(crate) fn apply_linear_probe(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_linear_context(frame, scale, &[], checkpoint)
    }
    // Solver probes inherit a validated library and authored base.
    pub(crate) fn apply_linear_probe_with_library(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        library: &[super::project_luts::ProjectLut],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_linear_context(frame, scale, library, checkpoint)
    }
    fn apply_linear_context(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        library: &[super::project_luts::ProjectLut],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        // Missing library entries bypass the table, preserving orphan references after removal.
        let input_library = self
            .input_lut_library
            .as_ref()
            .map(|id| super::project_luts::resolve_effect_reference(library, &format!("lib:{id}")))
            .transpose()?
            .flatten();
        let look_library = self
            .look_lut_library
            .as_ref()
            .map(|id| super::project_luts::resolve_effect_reference(library, &format!("lib:{id}")))
            .transpose()?
            .flatten();
        ensure!(
            scale.is_finite() && scale > 0.0 && scale <= 1.0,
            "invalid Lumetri pixel scale"
        );
        checkpoint()?;
        if !self.enabled {
            return Ok(());
        }
        let table = if self.basic.enabled {
            match &self.input_lut_builtin {
                Some(id) => Some(super::builtin_luts::resolve(id)?),
                None => match input_library {
                    Some(table) => Some(table),
                    None => self.input_lut.as_ref().map(ColorLut::table).transpose()?,
                },
            }
        } else {
            None
        };
        let (w, h) = frame.dimensions();
        // One signal stage: do not clamp between endpoints, split tone, saturation and vignette.
        frame.map_linear_to_signal(
            &mut |mut rgb, x, y| {
                if let Some(table) = &table {
                    rgb = table
                        .apply(rgb.map(super::basic_grade::display))
                        .map(|q| super::basic_grade::linear(q.clamp(0.0, 1.0)));
                }
                let v = self.basic.signal_from_linear(rgb);
                let saturation = if self.basic.enabled {
                    self.basic.saturation / 100.0
                } else {
                    1.0
                };
                let v = self.creative.signal_with_saturation(v, saturation);
                Ok(self.vignette.signal(v, x, y, w, h))
            },
            checkpoint,
        )?;
        let look_table = if self.creative.enabled {
            match &self.look_lut_builtin {
                Some(id) => Some(super::builtin_luts::resolve(id)?),
                None => match look_library {
                    Some(table) => Some(table),
                    None => self.look_lut.as_ref().map(ColorLut::table).transpose()?,
                },
            }
        } else {
            None
        };
        let mut grade = self.grade;
        if !self.creative.enabled || look_table.is_some() {
            grade.look = 0;
        }
        if !self.wheels_on {
            grade.shadows = Default::default();
            grade.midtones = Default::default();
            grade.highlights = Default::default();
        }
        // Upstream skips the entire advanced pass when it has no active controls.
        // Even an identity signal roundtrip changes premultiplied float pixels.
        if look_table.is_some()
            || grade.has_signal_adjustment()
            || (self.curves_on && self.curves.has_signal_adjustment())
        {
            let look_intensity = self.grade.intensity / 100.0;
            let grade_signal = grade.prepare_signal_unchecked();
            let curves_signal = self.curves.prepare_signal()?;
            frame.map_signal(
                &mut |rgb, _, _| {
                    let mut v = rgb;
                    if let Some(table) = &look_table {
                        let look = table.apply(v);
                        for c in 0..3 {
                            v[c] += (look[c] - v[c]) * look_intensity;
                        }
                    }
                    let v = grade_signal(v);
                    Ok(if self.curves_on { curves_signal(v) } else { v })
                },
                checkpoint,
            )?;
        }
        self.secondary
            .apply_linear_scaled(frame, scale, checkpoint)?;
        if self.creative.enabled {
            frame.sharpen(self.sharpen.amount / 100.0, scale, checkpoint)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn shared_library_roles_match_inline_and_bypass_removed_tables() {
        let table = ColorLut {
            format: super::super::color_lut::TableFormat::Cube,
            text: "LUT_1D_SIZE 2\n0.1 0.2 0.3\n0.8 0.7 0.6\n".into(),
            intensity: 100.0,
        };
        let library = vec![super::super::project_luts::ProjectLut {
            id: "shared".into(),
            name: "Shared".into(),
            lut: table.clone(),
            source_path: None,
        }];
        let surface = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![40, 100, 190, 255, 100, 160, 220, 128],
        };
        for input in [true, false] {
            let mut inline = Lumetri::default();
            inline.grade.intensity = 175.0;
            inline.grade.look = 8;
            if input {
                inline.input_lut = Some(table.clone());
            } else {
                inline.look_lut = Some(table.clone());
            }
            let mut linked = inline.clone();
            if input {
                linked.input_lut = None;
                linked.input_lut_library = Some("shared".into());
            } else {
                linked.look_lut = None;
                linked.look_lut_library = Some("shared".into());
            }
            let mut a = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            let mut b = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            inline.apply_linear(&mut a, 1.0, &mut || Ok(())).unwrap();
            linked
                .apply_linear_with_library(&mut b, 1.0, &library, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                a.to_surface(&mut || Ok(())).unwrap(),
                b.to_surface(&mut || Ok(())).unwrap()
            );
            let mut missing_frame =
                LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            let mut fallback_frame =
                LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            let mut fallback = linked.clone();
            fallback.input_lut_library = None;
            fallback.look_lut_library = None;
            linked
                .apply_linear(&mut missing_frame, 1.0, &mut || Ok(()))
                .unwrap();
            fallback
                .apply_linear(&mut fallback_frame, 1.0, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                missing_frame.to_surface(&mut || Ok(())).unwrap(),
                fallback_frame.to_surface(&mut || Ok(())).unwrap()
            );
            linked.enabled = false;
            linked.validate_library(&[]).unwrap();
            if input {
                linked.input_lut = Some(table.clone());
            } else {
                linked.look_lut = Some(table.clone());
            }
            assert!(linked.validate().is_err());
        }
    }
    #[test]
    fn matches_actual_filmcraft_combined_advanced_renderer() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lumetri.json"))
                .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 56);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: Lumetri = serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: f["width"].as_u64().unwrap() as usize,
                height: f["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            let mut linear = LinearColorFrame::from_surface(&frame, &mut || Ok(())).unwrap();
            p.apply_linear(&mut linear, 1.0, &mut || Ok(())).unwrap();
            frame = linear.to_surface(&mut || Ok(())).unwrap();
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
    fn matches_actual_filmcraft_all_builtin_roles() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-builtin-render.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let input: Vec<u8> = serde_json::from_value(f["input"].clone()).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 68);
        for (index, case) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p: Lumetri = serde_json::from_value(case["parameters"].clone()).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut frame = GroupSurface {
                width: f["width"].as_u64().unwrap() as usize,
                height: f["height"].as_u64().unwrap() as usize,
                rgba: input.clone(),
            };
            let mut linear = LinearColorFrame::from_surface(&frame, &mut || Ok(())).unwrap();
            p.apply_linear(&mut linear, 1.0, &mut || Ok(())).unwrap();
            frame = linear.to_surface(&mut || Ok(())).unwrap();
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
    fn unified_animation_samples_local_clock_and_preserves_baselines() {
        let track = serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000,"value":100.0}]}});
        let mut p = Lumetri::default();
        p.basic.temperature = 23.0;
        p.animation = Some(
            serde_json::from_value(serde_json::json!({
                "basic":{"contrast":track},"creative":{"fadedFilm":track},
                "vignette":{"midpoint":track},"grade":{"intensity":track},
                "secondary":{"saturation":track},"sharpen":track
            }))
            .unwrap(),
        );
        let q = p.sample(500000).unwrap();
        assert_eq!(q.basic.contrast, 50.0);
        assert_eq!(q.basic.temperature, 23.0);
        assert_eq!(q.creative.faded_film, 50.0);
        assert_eq!(q.vignette.midpoint, 50.0);
        assert_eq!(q.grade.intensity, 50.0);
        assert_eq!(q.secondary.saturation, 50.0);
        assert_eq!(q.sharpen.amount, 50.0);
        assert!(q.animation.is_none());
        assert_eq!(p.sample(1000000).unwrap().basic.contrast, 100.0);
        p.basic.exposure = 6.0;
        assert!(p.sample(500000).is_err());
    }
    #[test]
    fn builtin_roles_are_validated_and_disabled_sections_bypass_tables() {
        let mut p = Lumetri::default();
        p.input_lut_builtin = Some("slog3-sgamut3cine-to-rec709".into());
        p.look_lut_builtin = Some(
            super::super::builtin_luts::catalog()
                .unwrap()
                .iter()
                .find(|b| !b.input)
                .unwrap()
                .id
                .clone(),
        );
        p.basic.enabled = false;
        p.creative.enabled = false;
        let image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![72, 131, 207, 128],
        };
        let mut frame = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        p.apply_linear(&mut frame, 1.0, &mut || Ok(())).unwrap();
        assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), image);
        p.input_lut_builtin = Some("missing".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn neutral_alpha_and_hidden_rgb() {
        let image = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![72, 131, 207, 128, 12, 34, 56, 0],
        };
        let mut frame = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        Lumetri::default()
            .apply_linear(&mut frame, 1.0, &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), image);
    }
    #[test]
    fn look_lut_overrides_procedural_look_and_respects_creative_switch() {
        let image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![72, 131, 207, 128],
        };
        let mut p = Lumetri::default();
        p.grade.look = 8;
        p.look_lut = Some(ColorLut {
            format: super::super::color_lut::TableFormat::Cube,
            text: "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n".into(),
            intensity: 100.0,
        });
        for enabled in [true, false] {
            p.creative.enabled = enabled;
            let mut frame = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
            p.apply_linear(&mut frame, 1.0, &mut || Ok(())).unwrap();
            assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), image);
        }
        p.look_lut.as_mut().unwrap().intensity = 50.0;
        assert!(p.validate().is_err());
    }
    #[test]
    fn disabled_sections_preserve_identity_and_validate_authored_values() {
        let image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![72, 131, 207, 128],
        };
        let mut p = Lumetri::default();
        p.basic.enabled = false;
        p.basic.exposure = 5.0;
        p.creative.enabled = false;
        p.creative.faded_film = 100.0;
        p.grade.look = 8;
        p.sharpen.amount = 100.0;
        p.vignette.enabled = false;
        p.vignette.amount = -5.0;
        p.curves_on = false;
        p.curves.master = vec![[0.0, 0.0]];
        p.wheels_on = false;
        p.grade.shadows.x = 1.0;
        let mut frame = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        p.apply_linear(&mut frame, 1.0, &mut || Ok(())).unwrap();
        assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), image);
        p.basic.exposure = 6.0;
        assert!(p.validate().is_err());
    }
}
