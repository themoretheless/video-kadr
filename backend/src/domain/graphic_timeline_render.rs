//! Graphic timeline rendering: vector layer stack, ordinary effects, clip motion and opacity.
use super::{
    composition::{BlendMode, VideoEffect},
    composition_graph::CompositionReferenceTiming,
    graphic_timeline::GraphicTimelineClip,
    group_surface::GroupSurface,
    group_transform::GroupAffine,
    linear_color_frame::LinearColorFrame,
    nested_composition::CompositionInstance,
};
use anyhow::Result;
pub type GraphicEffectRenderer<'a> = dyn FnMut(&mut LinearColorFrame, &[VideoEffect], u64, &mut dyn FnMut() -> Result<()>) -> Result<()>
    + 'a;
impl GraphicTimelineClip {
    pub fn render_timeline(
        &self,
        output: (u32, u32),
        time: u64,
        effects: &mut GraphicEffectRenderer<'_>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Option<LinearColorFrame>> {
        let Some(local) = self.local_time(time)? else {
            return Ok(None);
        };
        anyhow::ensure!(self.concat_treatment.is_none(), "adjustment treatment requires scene backdrop context");
        if let Some(template) = &self.template {
            super::graphic_template_fonts::register(&template.resources);
        }
        let layer_time = self.layer_time(time)?.expect("visible graphic layer clock");
        let roll = self.roll.as_ref().map(|roll| {
            (
                roll,
                (time - self.timeline_start_tick) as i64,
                (self.timeline_end_tick - self.timeline_start_tick) as i64,
            )
        });
        let instance = CompositionInstance {
            id: self.id.as_str().into(),
            animation_offset_us: self.animation_offset_us,
            timing: CompositionReferenceTiming {
                composition: "graphic".into(),
                timeline_start_us: self.timeline_start_tick,
                source_in_us: 0,
                source_out_us: self.timeline_end_tick - self.timeline_start_tick,
            },
            transform: self.transform.clone(),
            opacity: self.opacity.clone(),
            blend_mode: BlendMode::Normal,
        };
        let pose = instance.pose(time)?;
        if pose.opacity == 0. || pose.scale_x == 0. || pose.scale_y == 0. {
            return Ok(None);
        }
        if let Some(shape) = &self.concat_shape {
            let mut raster = shape.render((self.width, self.height), checkpoint)?;
            let mut center = [0., 0.];
            if let Some(preparation) = super::concat_picture::source_preparation(&self.transform)? {
                let prepared = preparation.prepare(&raster, checkpoint)?;
                raster = prepared.surface;
                center = prepared.center_offset;
            }
            let matrix = GroupAffine::new(
                [output.0 as f64, output.1 as f64],
                [raster.width as f64, raster.height as f64],
                [self.transform.anchor_x, self.transform.anchor_y],
                [pose.x + center[0], pose.y + center[1]],
                [pose.scale_x, pose.scale_y], pose.rotation_degrees,
            )?;
            let mut child = LinearColorFrame::from_surface(&raster, checkpoint)?;
            effects(&mut child, &self.effects, local, checkpoint)?;
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface::transparent(output.0 as usize, output.1 as usize)?, checkpoint,
            )?;
            frame.composite_working(&child, matrix, pose.opacity as f32, BlendMode::Normal, checkpoint)?;
            checkpoint()?;
            return Ok(Some(frame));
        }
        let matrix = GroupAffine::new(
            [output.0 as f64, output.1 as f64],
            [self.width as f64, self.height as f64],
            [self.transform.anchor_x, self.transform.anchor_y],
            [pose.x, pose.y],
            [pose.scale_x, pose.scale_y],
            pose.rotation_degrees,
        )?;
        if self.effects.is_empty() {
            let mut frame = self.graphic.render_with_roll(
                (self.width, self.height),
                output,
                layer_time,
                matrix.0,
                roll,
                checkpoint,
            )?;
            if pose.opacity != 1. {
                frame.map_premultiplied(&mut |p| p.map(|v| v * pose.opacity as f32), checkpoint)?;
            }
            checkpoint()?;
            return Ok(Some(frame));
        }
        let mut child = self.graphic.render_with_roll(
            (self.width, self.height),
            (self.width, self.height),
            layer_time,
            GroupAffine::IDENTITY.0,
            roll,
            checkpoint,
        )?;
        effects(&mut child, &self.effects, local, checkpoint)?;
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface::transparent(output.0 as usize, output.1 as usize)?,
            checkpoint,
        )?;
        frame.composite_working(
            &child,
            matrix,
            pose.opacity as f32,
            BlendMode::Normal,
            checkpoint,
        )?;
        checkpoint()?;
        Ok(Some(frame))
    }
}
#[cfg(test)]
mod tests {
    use super::super::{
        composition::{AnimatableValue, CompositionClipId, TransformSpec},
        graphic_clip::{GraphicClip, GraphicContent, GraphicLayer},
        graphic_shape::GraphicShape,
    };
    use super::*;
    #[test]
    fn retained_local_clock_and_effects_precede_clip_opacity() {
        let graphic = GraphicClip {
            layers: vec![GraphicLayer {
                original_parameters: None,
                pin: None,
                uid: 1,
                enabled: true,
                content: GraphicContent::Shape {
                    parameters: GraphicShape {
                        size: [20., 12.],
                        ..Default::default()
                    },
                    animation: None,
                },
            }],
        };
        let clip = GraphicTimelineClip {
            concat_shape: None, concat_treatment: None, concat_shape_blend: None,
            responsive_time: None,
            roll: None,
            template: None,
            id: CompositionClipId::parse("graphic").unwrap(),
            timeline_start_tick: 1_000_000,
            timeline_end_tick: 2_000_000,
            animation_offset_us: 250_000,
            width: 64,
            height: 48,
            graphic,
            effects: vec![],
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.5),
            enabled: true,
        };
        let expected = clip
            .graphic
            .render(
                (64, 48),
                (64, 48),
                750_000,
                GroupAffine::IDENTITY.0,
                &mut || Ok(()),
            )
            .unwrap();
        let actual = clip
            .render_timeline(
                (64, 48),
                1_500_000,
                &mut |_, _, _, _| panic!("empty effect stack must not invoke renderer"),
                &mut || Ok(()),
            )
            .unwrap()
            .unwrap();
        for (a, b) in actual
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert_eq!(*a, *b * 0.5);
        }
        let mut with_effects = clip.clone();
        with_effects.effects = vec![VideoEffect::HorizontalFlip];
        let mut called = false;
        let output = with_effects
            .render_timeline(
                (64, 48),
                1_500_000,
                &mut |frame, effects, local, check| {
                    called = true;
                    assert_eq!(effects, &[VideoEffect::HorizontalFlip]);
                    assert_eq!(local, 750_000);
                    frame.map_premultiplied(&mut |p| p.map(|v| v * 0.5), check)
                },
                &mut || Ok(()),
            )
            .unwrap()
            .unwrap();
        assert!(called);
        for (a, b) in output
            .pixels()
            .iter()
            .flatten()
            .zip(expected.pixels().iter().flatten())
        {
            assert_eq!(*a, *b * 0.25);
        }
    }
}
