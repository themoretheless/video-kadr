//! Read-only template inventory. Media identifiers are editorial, never paths.
use super::{
    composition::{CompositionTrack, SourceId, SourceKind},
    nested_composition::NestedCompositionProject,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSlot {
    pub media_id: SourceId,
    pub name: String,
    pub kind: SourceKind,
    pub seconds: f64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub schema_version: u32,
    pub composition_id: String,
    pub width: u32,
    pub height: u32,
    pub slots: Vec<TemplateSlot>,
}

/// Sum authored placements, including hidden/muted clips, not their source spans.
/// Slots absent from the selected timeline remain available with zero seconds.
pub fn inspect(
    project: &NestedCompositionProject,
    composition: Option<&str>,
) -> Result<TemplateInfo> {
    project.dependency_order()?;
    let id = composition.unwrap_or(&project.root);
    let node = project
        .compositions
        .get(id)
        .context("Template timeline not found")?;
    let mut spans = BTreeMap::<SourceId, (u64, u64)>::new();
    let mut add = |source: &SourceId, start: u64, duration: u64| -> Result<()> {
        let span = spans.entry(source.clone()).or_insert((start, 0));
        span.0 = span.0.min(start);
        span.1 = span
            .1
            .checked_add(duration)
            .context("Template slot duration overflow")?;
        Ok(())
    };
    for track in &node.content.tracks {
        match track {
            CompositionTrack::Video { clips, .. } => {
                for clip in clips {
                    add(
                        &clip.source_id,
                        clip.placement.timeline_start_tick,
                        clip.placement.timeline_duration_ticks()?,
                    )?;
                }
            }
            CompositionTrack::Audio { clips, .. } => {
                for clip in clips {
                    add(
                        &clip.source_id,
                        clip.placement.timeline_start_tick,
                        clip.placement.timeline_duration_ticks()?,
                    )?;
                }
            }
            CompositionTrack::Image { clips, .. } => {
                for clip in clips {
                    add(
                        &clip.source_id,
                        clip.timeline_start_tick,
                        clip.duration_ticks,
                    )?;
                }
            }
            _ => {}
        }
    }
    let mut slots: Vec<_> = project
        .project_media
        .iter()
        .filter(|(_, item)| item.placeholder)
        .map(|(id, item)| {
            let (first, duration) = spans.get(id).copied().unwrap_or((u64::MAX, 0));
            (
                first,
                TemplateSlot {
                    media_id: id.clone(),
                    name: item.name.clone(),
                    kind: item.source.kind,
                    seconds: duration as f64 / node.content.time_base as f64,
                },
            )
        })
        .collect();
    // Stable equal-time ordering follows the canonical native media registry.
    slots.sort_by_key(|(first, _)| *first);
    Ok(TemplateInfo {
        schema_version: 1,
        composition_id: id.into(),
        width: node.content.canvas.width,
        height: node.content.canvas.height,
        slots: slots.into_iter().map(|(_, slot)| slot).collect(),
    })
}
