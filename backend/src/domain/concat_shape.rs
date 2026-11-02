//! Retained Concat shape style and original source rasterizer adapter.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    #[default]
    Square,
    Circle,
    Triangle,
    Parallelogram,
    Trapezoid,
    Line,
    Arrow,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeStyle {
    pub kind: ShapeKind,
    pub size: f64,
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
}
impl Default for ShapeStyle {
    fn default() -> Self {
        Self {
            kind: ShapeKind::Square,
            size: 0.35,
            fill: "#ffffff".into(),
            stroke: String::new(),
            stroke_width: 0.012,
        }
    }
}
impl ShapeStyle {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.size.is_finite() && (0.01..=2.).contains(&self.size),
            "invalid Concat shape size"
        );
        ensure!(
            self.stroke_width.is_finite() && (0.0..=0.5).contains(&self.stroke_width),
            "invalid Concat shape stroke width"
        );
        let color = |s: &str| {
            s.is_empty()
                || (matches!(s.len(), 7 | 9)
                    && s.starts_with('#')
                    && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
        };
        ensure!(
            color(&self.fill) && color(&self.stroke),
            "invalid Concat shape color"
        );
        Ok(())
    }
    pub fn original(&self) -> concat_text::ShapeStyle {
        use concat_text::Figure;
        concat_text::ShapeStyle {
            figure: match self.kind {
                ShapeKind::Square => Figure::Square,
                ShapeKind::Circle => Figure::Circle,
                ShapeKind::Triangle => Figure::Triangle,
                ShapeKind::Parallelogram => Figure::Parallelogram,
                ShapeKind::Trapezoid => Figure::Trapezoid,
                ShapeKind::Line => Figure::Line,
                ShapeKind::Arrow => Figure::Arrow,
            },
            size: self.size,
            fill: self.fill.clone(),
            stroke: self.stroke.clone(),
            stroke_width: self.stroke_width,
        }
    }
    pub fn render(
        &self,
        canvas: (u32, u32),
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<GroupSurface> {
        self.validate()?;
        ensure!(
            canvas.0 > 0 && canvas.0 <= 3840 && canvas.1 > 0 && canvas.1 <= 2160,
            "invalid Concat shape canvas"
        );
        checkpoint()?;
        let frame = concat_text::render_shape_frame(&self.original(), canvas.0, canvas.1)?;
        checkpoint()?;
        Ok(GroupSurface {
            width: frame.width as usize,
            height: frame.height as usize,
            rgba: frame.rgba,
        })
    }
}

/// Paint a retained figure through the original source compositor against the scene.
pub fn composite(
    clip: &super::graphic_timeline::GraphicTimelineClip,
    surface: &mut GroupSurface,
    time: u64,
    effects: &mut super::concat_title::TitleEffects<'_>,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    use super::{
        composition::BlendMode, composition_graph::CompositionReferenceTiming,
        group_transform::GroupAffine, nested_composition::CompositionInstance,
    };
    let Some(local) = clip.local_time(time)? else {
        return Ok(());
    };
    let shape = clip
        .concat_shape
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("retained Concat shape missing"))?;
    let instance = CompositionInstance {
        id: clip.id.as_str().into(),
        animation_offset_us: clip.animation_offset_us,
        timing: CompositionReferenceTiming {
            composition: "shape".into(),
            timeline_start_us: clip.timeline_start_tick,
            source_in_us: 0,
            source_out_us: clip.timeline_end_tick - clip.timeline_start_tick,
        },
        transform: clip.transform.clone(),
        opacity: clip.opacity.clone(),
        blend_mode: BlendMode::Normal,
    };
    let pose = instance.pose(time)?;
    if pose.opacity == 0. || pose.scale_x == 0. || pose.scale_y == 0. {
        return Ok(());
    }
    let mut picture = shape.render((clip.width, clip.height), checkpoint)?;
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
        effects(&mut picture, &clip.effects, local, matrix, checkpoint)?;
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
        clip.concat_shape_blend.unwrap_or(BlendMode::Normal),
        checkpoint,
    )
}
