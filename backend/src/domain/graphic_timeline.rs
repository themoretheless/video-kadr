//! Source-free graphic clip placement and authored layer stack.
use super::{
    composition::{AnimatableValue, CompositionClipId, TransformSpec, VideoEffect},
    graphic_clip::GraphicClip,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
fn enabled() -> bool {
    true
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicTimelineClip {
    pub id: CompositionClipId,
    pub timeline_start_tick: u64,
    pub timeline_end_tick: u64,
    /// Retained layer-local time after a trim or split.
    #[serde(default)]
    pub animation_offset_us: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<super::graphic_templates::GraphicTemplateMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roll: Option<super::graphic_roll::Roll>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsive_time: Option<super::graphic_responsive_time::ResponsiveTime>,
    pub width: u32,
    pub height: u32,
    pub graphic: GraphicClip,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concat_shape: Option<super::concat_shape::ShapeStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concat_treatment: Option<super::concat_treatment::Treatment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concat_shape_blend: Option<super::composition::BlendMode>,
    #[serde(default)]
    pub effects: Vec<VideoEffect>,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
impl GraphicTimelineClip {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.timeline_end_tick > self.timeline_start_tick
                && self.timeline_end_tick <= 9_007_199_254_740_991,
            "invalid graphic clip placement"
        );
        ensure!(
            self.width > 0 && self.width <= 3840 && self.height > 0 && self.height <= 2160,
            "invalid graphic clip canvas"
        );
        ensure!(
            self.animation_offset_us
                .checked_add(self.timeline_end_tick - self.timeline_start_tick)
                .is_some(),
            "graphic clip clock overflow"
        );
        self.graphic.validate()?;
        if let Some(treatment) = &self.concat_treatment {
            treatment.validate()?;
            ensure!(self.concat_shape.is_none() && self.graphic.layers.is_empty() && self.template.is_none() && self.roll.is_none() && self.responsive_time.is_none(), "treatment cannot replace graphic content");
            ensure!(self.transform == super::composition::TransformSpec::default() && self.opacity == super::composition::AnimatableValue::constant(1.), "treatment has no authored picture pose");
            ensure!(self.animation_offset_us.checked_add(self.timeline_end_tick-self.timeline_start_tick).is_some_and(|end|end<=treatment.duration_ticks), "treatment placement exceeds retained clock");
        }
        if let Some(shape) = &self.concat_shape {
            shape.validate()?;
            ensure!(matches!(self.concat_shape_blend.unwrap_or(super::composition::BlendMode::Normal),super::composition::BlendMode::Normal|super::composition::BlendMode::Multiply|super::composition::BlendMode::Screen|super::composition::BlendMode::Addition|super::composition::BlendMode::Lighten|super::composition::BlendMode::Darken), "unsupported Concat shape blend");
            ensure!(self.graphic.layers.is_empty() && self.template.is_none() && self.roll.is_none() && self.responsive_time.is_none(), "Concat shape cannot replace authored graphic layers or template timing");
        }
        if let Some(template) = &self.template {
            template.validate()?;
        }
        ensure!(self.concat_shape.is_some() || self.concat_shape_blend.is_none(), "Concat shape blend requires a retained shape");
        self.transform.validate()?;
        self.opacity.validate()?;
        ensure!(
            super::composition::animatable_values_in(&self.opacity, 0., 1., true),
            "graphic clip opacity out of range"
        );
        for effect in &self.effects {
            effect.validate()?;
        }
        Ok(())
    }
    pub fn layer_time(&self, timeline: u64) -> Result<Option<i128>> {
        let Some(local) = self.local_time(timeline)? else {
            return Ok(None);
        };
        if let Some(meta) = self.responsive_time.filter(|meta| meta.active()) {
            let u = i64::try_from(timeline - self.timeline_start_tick)?;
            let duration = i64::try_from(self.timeline_end_tick - self.timeline_start_tick)?;
            return Ok(Some(i128::from(meta.remap(u, duration)?)));
        }
        Ok(Some(i128::from(local)))
    }
    pub fn local_time(&self, timeline: u64) -> Result<Option<u64>> {
        self.validate()?;
        if !self.enabled
            || timeline < self.timeline_start_tick
            || timeline >= self.timeline_end_tick
        {
            return Ok(None);
        }
        Ok(Some(
            (timeline - self.timeline_start_tick)
                .checked_add(self.animation_offset_us)
                .ok_or_else(|| anyhow::anyhow!("graphic clip clock overflow"))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trim_offset_preserves_half_open_graphic_clip_clock_and_roundtrip() {
        let mut clip = GraphicTimelineClip {
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
            graphic: GraphicClip::default(),
            effects: vec![],
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.),
            enabled: true,
        };
        assert_eq!(clip.local_time(999_999).unwrap(), None);
        assert_eq!(clip.local_time(1_000_000).unwrap(), Some(250_000));
        assert_eq!(clip.local_time(1_500_000).unwrap(), Some(750_000));
        assert_eq!(clip.local_time(2_000_000).unwrap(), None);
        let round: GraphicTimelineClip =
            serde_json::from_str(&serde_json::to_string(&clip).unwrap()).unwrap();
        assert_eq!(round, clip);
        clip.timeline_start_tick = 1_500_000;
        clip.animation_offset_us = 750_000;
        assert_eq!(clip.local_time(1_500_000).unwrap(), Some(750_000));
        clip.enabled = false;
        assert_eq!(clip.local_time(1_500_000).unwrap(), None);
        clip.animation_offset_us = u64::MAX;
        assert!(clip.validate().is_err());
    }
}
