//! Authored Filmcraft rich text layer parameters (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    graphic_shape::GraphicShape,
    graphic_text_layout::GraphicText,
    graphic_text_render::TextLayer,
    graphic_text_runs::{byte_runs, CharacterStyleRun},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicTextParameters {
    pub name: String,
    pub fill: bool,
    pub fill_color: [f32; 4],
    pub stroke: bool,
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_type: u32,
    pub stroke2: bool,
    pub stroke2_color: [f32; 4],
    pub stroke2_width: f32,
    pub stroke2_type: u32,
    pub background: bool,
    pub background_color: [f32; 4],
    pub background_opacity: f32,
    pub background_size: f32,
    pub background_radius: f32,
    pub shadow: bool,
    pub shadow_color: [f32; 4],
    pub shadow_opacity: f32,
    pub shadow_angle: f32,
    pub shadow_distance: f32,
    pub shadow_size: f32,
    pub shadow_blur: f32,
    pub position: Option<[f64; 2]>,
    pub anchor: [f64; 2],
    pub scale: f32,
    pub scale_width: f32,
    pub uniform_scale: bool,
    pub rotation: f32,
    pub opacity: f32,
    pub text: String,
    pub font: String,
    pub font_style: String,
    pub size: f32,
    pub align: u32,
    pub tracking: f32,
    pub kerning: bool,
    pub ligatures: bool,
    pub leading: f32,
    pub baseline_shift: f32,
    pub faux_bold: bool,
    pub faux_italic: bool,
    pub caps: u32,
    pub underline: bool,
    pub box_width: f32,
    pub box_height: f32,
    pub vertical: bool,
    pub runs: Vec<CharacterStyleRun>,
}
impl Default for GraphicTextParameters {
    fn default() -> Self {
        let common = GraphicShape::default();
        Self {
            name: common.name,
            fill: common.fill,
            fill_color: common.fill_color,
            stroke: common.stroke,
            stroke_color: common.stroke_color,
            stroke_width: common.stroke_width,
            stroke_type: common.stroke_type,
            stroke2: common.stroke2,
            stroke2_color: common.stroke2_color,
            stroke2_width: common.stroke2_width,
            stroke2_type: common.stroke2_type,
            background: common.background,
            background_color: common.background_color,
            background_opacity: common.background_opacity,
            background_size: common.background_size,
            background_radius: common.background_radius,
            shadow: common.shadow,
            shadow_color: common.shadow_color,
            shadow_opacity: common.shadow_opacity,
            shadow_angle: common.shadow_angle,
            shadow_distance: common.shadow_distance,
            shadow_size: common.shadow_size,
            shadow_blur: common.shadow_blur,
            position: common.position,
            anchor: common.anchor,
            scale: common.scale,
            scale_width: common.scale_width,
            uniform_scale: common.uniform_scale,
            rotation: common.rotation,
            opacity: common.opacity,
            text: String::new(),
            font: "Inter".into(),
            font_style: "Regular".into(),
            size: 100.,
            align: 0,
            tracking: 0.,
            kerning: true,
            ligatures: true,
            leading: 0.,
            baseline_shift: 0.,
            faux_bold: false,
            faux_italic: false,
            caps: 0,
            underline: false,
            box_width: 0.,
            box_height: 0.,
            vertical: false,
            runs: vec![],
        }
    }
}
impl GraphicTextParameters {
    fn common(&self) -> GraphicShape {
        GraphicShape {
            name: self.name.clone(),
            fill: self.fill.clone(),
            fill_color: self.fill_color.clone(),
            stroke: self.stroke.clone(),
            stroke_color: self.stroke_color.clone(),
            stroke_width: self.stroke_width.clone(),
            stroke_type: self.stroke_type.clone(),
            stroke2: self.stroke2.clone(),
            stroke2_color: self.stroke2_color.clone(),
            stroke2_width: self.stroke2_width.clone(),
            stroke2_type: self.stroke2_type.clone(),
            background: self.background.clone(),
            background_color: self.background_color.clone(),
            background_opacity: self.background_opacity.clone(),
            background_size: self.background_size.clone(),
            background_radius: self.background_radius.clone(),
            shadow: self.shadow.clone(),
            shadow_color: self.shadow_color.clone(),
            shadow_opacity: self.shadow_opacity.clone(),
            shadow_angle: self.shadow_angle.clone(),
            shadow_distance: self.shadow_distance.clone(),
            shadow_size: self.shadow_size.clone(),
            shadow_blur: self.shadow_blur.clone(),
            position: self.position.clone(),
            anchor: self.anchor.clone(),
            scale: self.scale.clone(),
            scale_width: self.scale_width.clone(),
            uniform_scale: self.uniform_scale.clone(),
            rotation: self.rotation.clone(),
            opacity: self.opacity.clone(),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.common().validate()?;
        for (v, lo, hi) in [
            (self.size, 1., 2000.),
            (self.tracking, -1000., 10000.),
            (self.leading, -5000., 5000.),
            (self.baseline_shift, -5000., 5000.),
            (self.box_width, 0., 100000.),
            (self.box_height, 0., 100000.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "graphic text parameter out of range"
            );
        }
        self.text_properties().validate()
    }
    fn text_properties(&self) -> GraphicText {
        GraphicText {
            text: self.text.clone(),
            font: self.font.clone(),
            style: self.font_style.clone(),
            size: self.size.clone(),
            align: self.align.clone(),
            tracking: self.tracking.clone(),
            kerning: self.kerning.clone(),
            ligatures: self.ligatures.clone(),
            leading: self.leading.clone(),
            baseline_shift: self.baseline_shift.clone(),
            faux_bold: self.faux_bold.clone(),
            faux_italic: self.faux_italic.clone(),
            caps: self.caps.clone(),
            underline: self.underline.clone(),
            box_width: self.box_width.clone(),
            box_height: self.box_height.clone(),
            vertical: self.vertical.clone(),
            runs: byte_runs(&self.text, &self.runs),
        }
    }
    pub fn evaluate(
        &self,
        enabled: bool,
        canvas: (u32, u32),
    ) -> Result<(TextLayer, filmcraft_text::Xform)> {
        self.validate()?;
        let (common, matrix) = self.common().evaluate(enabled, canvas)?;
        Ok((
            TextLayer {
                enabled,
                text: self.text_properties(),
                appearance: common.appearance,
                opacity: common.opacity,
            },
            matrix,
        ))
    }
}
