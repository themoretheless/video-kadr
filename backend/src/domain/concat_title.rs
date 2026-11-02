//! Retained Concat title typography; original rasterizer lives in vendor/concat-text.
use super::{
    composition::{BlendMode, TextAlignment, TextClip},
    composition_graph::CompositionReferenceTiming,
    group_surface::GroupSurface,
    group_transform::GroupAffine,
    linear_color_frame::LinearColorFrame,
    nested_composition::CompositionInstance,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleStyle {
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: f64,
    pub italic: bool,
    pub color: String,
    pub align: TextAlignment,
    pub opacity: f64,
    pub stroke_width: f64,
    pub stroke_color: String,
    pub shadow: bool,
    pub background: String,
    pub background_radius: f64,
    pub background_padding_x: f64,
    pub background_padding_y: f64,
    pub line_height: f64,
    pub tracking: f64,
    pub max_width: f64,
    pub max_height: f64,
}
impl Default for TitleStyle {
    fn default() -> Self {
        Self {
            font_family: "\"Cabinet Grotesk\"".into(),
            font_size: 0.09,
            font_weight: 700.,
            italic: false,
            color: "#ffffff".into(),
            align: TextAlignment::Center,
            opacity: 1.,
            stroke_width: 0.,
            stroke_color: "#000000".into(),
            shadow: true,
            background: String::new(),
            background_radius: 0.0135,
            background_padding_x: 0.0315,
            background_padding_y: 0.018,
            line_height: 1.2,
            tracking: 0.,
            max_width: 0.,
            max_height: 0.,
        }
    }
}
impl TitleStyle {
    pub fn face_style(&self) -> String {
        let weight = ((self.font_weight / 100.).round() as i32).clamp(1, 9);
        let name = match weight {
            1 => "Thin",
            2 => "ExtraLight",
            3 => "Light",
            4 => "Regular",
            5 => "Medium",
            6 => "SemiBold",
            7 => "Bold",
            8 => "ExtraBold",
            _ => "Black",
        };
        if self.italic {
            if weight == 4 {
                "Italic".into()
            } else {
                format!("{name} Italic")
            }
        } else {
            name.into()
        }
    }
    pub fn set_face_style(&mut self, style: &str) -> Result<()> {
        let mut name = style
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect::<String>()
            .to_lowercase();
        let italic = name.ends_with("italic") || name.ends_with("oblique");
        if italic {
            let count = if name.ends_with("italic") { 6 } else { 7 };
            name.truncate(name.len() - count);
        }
        let weight = match name.as_str() {
            "thin" => 100.,
            "extralight" | "ultralight" => 200.,
            "light" => 300.,
            "" | "regular" | "normal" | "roman" => 400.,
            "medium" => 500.,
            "semibold" | "demibold" => 600.,
            "bold" => 700.,
            "extrabold" | "ultrabold" => 800.,
            "black" | "heavy" => 900.,
            _ => anyhow::bail!("Title face style cannot be represented by CSS weight and italic"),
        };
        self.font_weight = weight;
        self.italic = italic;
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.font_family.is_empty()
                && self.font_family.len() <= 256
                && !self.font_family.contains('\0'),
            "invalid title font family"
        );
        ensure!(
            [
                self.font_size,
                self.font_weight,
                self.opacity,
                self.stroke_width,
                self.background_radius,
                self.background_padding_x,
                self.background_padding_y,
                self.line_height,
                self.tracking,
                self.max_width,
                self.max_height
            ]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64),
            "invalid title numeric value"
        );
        ensure!(
            (0.01..=1.).contains(&self.font_size)
                && (100.0..=900.).contains(&self.font_weight)
                && (0.0..=1.).contains(&self.opacity)
                && self.line_height >= 0.5,
            "invalid title size/weight/opacity/line height"
        );
        ensure!(
            [
                self.stroke_width,
                self.background_radius,
                self.background_padding_x,
                self.background_padding_y,
                self.max_width,
                self.max_height
            ]
            .iter()
            .all(|v| *v >= 0.),
            "negative title dimension"
        );
        let color = |s: &str| {
            matches!(s.len(), 7 | 9)
                && s.starts_with('#')
                && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        };
        ensure!(
            color(&self.color)
                && color(&self.stroke_color)
                && (self.background.is_empty() || color(&self.background)),
            "invalid title color"
        );
        Ok(())
    }
    pub fn original(&self, text: &str) -> concat_text::TitleStyle {
        concat_text::TitleStyle {
            content: text.into(),
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            font_weight: self.font_weight,
            italic: self.italic,
            color: self.color.clone(),
            align: match self.align {
                TextAlignment::Left => concat_text::Align::Left,
                TextAlignment::Center => concat_text::Align::Center,
                TextAlignment::Right => concat_text::Align::Right,
            },
            stroke_width: self.stroke_width,
            stroke_color: self.stroke_color.clone(),
            shadow: self.shadow,
            background: self.background.clone(),
            background_radius: self.background_radius,
            background_padding_x: self.background_padding_x,
            background_padding_y: self.background_padding_y,
            line_height: self.line_height,
            tracking: self.tracking,
            max_width: self.max_width,
            max_height: self.max_height,
        }
    }
}
pub fn fonts() -> &'static concat_text::Fonts {
    static FONTS: OnceLock<concat_text::Fonts> = OnceLock::new();
    FONTS.get_or_init(concat_text::Fonts::new)
}
pub fn picture(
    clip: &TextClip,
    style: &TitleStyle,
    canvas: (u32, u32),
    time: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<(GroupSurface, super::nested_composition::NestedGroupPose)>> {
    if !clip.enabled || time < clip.timeline_start_tick || time >= clip.timeline_end_tick {
        return Ok(None);
    }
    style.validate()?;
    ensure!(
        canvas.0 > 0 && canvas.0 <= 3840 && canvas.1 > 0 && canvas.1 <= 2160,
        "invalid title canvas"
    );
    checkpoint()?;
    let instance = CompositionInstance {
        id: clip.id.as_str().into(),
        animation_offset_us: 0,
        timing: CompositionReferenceTiming {
            composition: "title".into(),
            timeline_start_us: clip.timeline_start_tick,
            source_in_us: 0,
            source_out_us: clip.timeline_end_tick - clip.timeline_start_tick,
        },
        transform: clip.transform.clone(),
        opacity: clip.opacity.clone(),
        blend_mode: BlendMode::Normal,
    };
    let mut pose = instance.pose(time)?;
    let opacity = pose.opacity * style.opacity;
    if opacity == 0. || pose.scale_x == 0. || pose.scale_y == 0. {
        return Ok(None);
    }
    let mut animated_style = style.clone();
    if let Some(track) = &clip.color_animation {
        let channel = |at: usize| {
            u8::from_str_radix(&style.color[at..at + 2], 16).map_err(anyhow::Error::from)
        };
        let default = super::animated_color::Color {
            r: channel(1)?,
            g: channel(3)?,
            b: channel(5)?,
            a: if style.color.len() == 9 {
                channel(7)?
            } else {
                255
            },
        };
        let color = super::animated_color::PreparedColorTrack::new(
            super::animated_color::ColorSampleRequest {
                track: track.clone(),
                default,
                times_us: vec![],
            },
        )
        .map_err(anyhow::Error::msg)?
        .sample((time - clip.timeline_start_tick) as f64)
        .map_err(anyhow::Error::msg)?;
        animated_style.color = format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            color.r, color.g, color.b, color.a
        );
    }
    let rendered = concat_text::render_frame(
        super::concat_project_fonts::current()
            .as_deref()
            .unwrap_or_else(|| fonts()),
        &animated_style.original(&clip.text),
        canvas.0,
        canvas.1,
    )?;
    checkpoint()?;
    pose.opacity = opacity;
    Ok(Some((
        GroupSurface {
            width: canvas.0 as usize,
            height: canvas.1 as usize,
            rgba: rendered.rgba,
        },
        pose,
    )))
}

pub type TitleEffects<'a> = dyn FnMut(
        &mut GroupSurface,
        &[super::composition::VideoEffect],
        u64,
        GroupAffine,
        &mut dyn FnMut() -> Result<()>,
    ) -> Result<()>
    + 'a;

pub fn composite(
    clip: &TextClip,
    style: &TitleStyle,
    surface: &mut GroupSurface,
    time: u64,
    effects: &mut TitleEffects<'_>,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    let Some((mut picture, pose)) = picture(
        clip,
        style,
        (surface.width as u32, surface.height as u32),
        time,
        checkpoint,
    )?
    else {
        return Ok(());
    };
    let mut preparation = super::concat_picture::source_preparation(&clip.transform)?.unwrap_or(
        super::concat_picture::Preparation {
            crop: [0.; 4],
            canvas: [surface.width, surface.height],
            flip_h: false,
            flip_v: false,
        },
    );
    if !clip.effects.is_empty() {
        let prepared = preparation.prepare(&picture, checkpoint)?;
        picture = prepared.surface;
        let matrix = GroupAffine::new(
            [surface.width as f64, surface.height as f64],
            [picture.width as f64, picture.height as f64],
            [clip.transform.anchor_x, clip.transform.anchor_y],
            [
                pose.x + prepared.center_offset[0],
                pose.y + prepared.center_offset[1],
            ],
            [pose.scale_x, pose.scale_y],
            pose.rotation_degrees,
        )?;
        effects(
            &mut picture,
            &clip.effects,
            time - clip.timeline_start_tick,
            matrix,
            checkpoint,
        )?;
        preparation.crop = [0.; 4];
        preparation.flip_h = false;
        preparation.flip_v = false;
    }
    preparation.composite_reference(
        &picture,
        surface,
        super::concat_clip_animation::Pose {
            scale_x: pose.scale_x,
            scale_y: pose.scale_y,
            x: pose.x,
            y: pose.y,
            rotation_degrees: pose.rotation_degrees,
            opacity: pose.opacity,
        },
        clip.blend_mode.unwrap_or(BlendMode::Normal),
        checkpoint,
    )
}

pub fn render(
    clip: &TextClip,
    style: &TitleStyle,
    canvas: (u32, u32),
    time: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<LinearColorFrame>> {
    ensure!(
        clip.effects.is_empty(),
        "Retained title effects require scene rendering context"
    );
    let Some((mut picture, pose)) = picture(clip, style, canvas, time, checkpoint)? else {
        return Ok(None);
    };
    let offset =
        super::concat_picture::prepare_transform_source(&clip.transform, &mut picture, checkpoint)?;
    let child = LinearColorFrame::from_surface(&picture, checkpoint)?;
    let matrix = GroupAffine::new(
        [canvas.0 as f64, canvas.1 as f64],
        [picture.width as f64, picture.height as f64],
        [clip.transform.anchor_x, clip.transform.anchor_y],
        [pose.x + offset[0], pose.y + offset[1]],
        [pose.scale_x, pose.scale_y],
        pose.rotation_degrees,
    )?;
    let mut frame = LinearColorFrame::from_surface(
        &GroupSurface::transparent(canvas.0 as usize, canvas.1 as usize)?,
        checkpoint,
    )?;
    frame.composite_working(
        &child,
        matrix,
        pose.opacity as f32,
        BlendMode::Normal,
        checkpoint,
    )?;
    Ok(Some(frame))
}
