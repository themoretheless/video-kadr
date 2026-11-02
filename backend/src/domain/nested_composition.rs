//! Persisted native project model for child-composition instances.
use super::composition::{
    AnimatableValue, BlendMode, Composition, TransformSpec, DEFAULT_TIME_BASE,
};
use super::composition_graph::{CompositionGraph, CompositionReferenceTiming};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionInstance {
    pub id: String,
    pub timing: CompositionReferenceTiming,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub animation_offset_us: u64,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    pub blend_mode: BlendMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedCompositionDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_us: Option<u64>,
    pub content: Composition,
    pub instances: Vec<CompositionInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedCompositionProject {
    #[serde(default, skip_serializing_if="BTreeMap::is_empty")]
    pub project_media: BTreeMap<super::composition::SourceId, super::project_media::ProjectMediaItem>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub fonts:Vec<super::concat_project_fonts::FontResource>,
    #[serde(default, skip_serializing_if="Vec::is_empty")]
    pub timeline_order: Vec<String>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub luts:Vec<super::project_luts::ProjectLut>,
    pub schema_version: u32,
    pub root: String,
    pub compositions: BTreeMap<String, NestedCompositionDocument>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NestedGroupPose {
    pub local_time_us: u64,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}

pub(crate) fn sample_value(value: &AnimatableValue, local_us: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<f64> {
    let local_us: super::animation_sample_time::AnimationSampleTime = local_us.into();

    Ok(match value {
        AnimatableValue::Constant { value } => *value,
        AnimatableValue::Keyframes { track } => {
            local_us.sample_track(track)?
        }
    })
}
impl CompositionInstance {
    pub(crate) fn pose(&self, parent_time_us: u64) -> Result<NestedGroupPose> {
        let local = (parent_time_us - self.timing.timeline_start_us)
            .checked_add(self.animation_offset_us)
            .ok_or_else(|| anyhow::anyhow!("group animation clock overflow"))?;
        if let Some(animation)=&self.transform.concat_animation {
            let pose=animation.sample(local)?;
            return Ok(NestedGroupPose{local_time_us:local,x:pose.x,y:pose.y,scale_x:pose.scale_x,scale_y:pose.scale_y,rotation_degrees:pose.rotation_degrees,opacity:pose.opacity});
        }
        let (x, y) = if let Some(position) = &self.transform.position {
            let point = position
                .compile()
                .map_err(anyhow::Error::msg)?
                .sample_at(local.into())
                .map_err(anyhow::Error::msg)?;
            (point.x, point.y)
        } else {
            (
                sample_value(&self.transform.x, local)?,
                sample_value(&self.transform.y, local)?,
            )
        };
        Ok(NestedGroupPose {
            local_time_us: local,
            x,
            y,
            scale_x: sample_value(&self.transform.scale_x, local)?,
            scale_y: sample_value(&self.transform.scale_y, local)?,
            rotation_degrees: sample_value(&self.transform.rotation_degrees, local)?,
            opacity: sample_value(&self.opacity, local)?.clamp(0.0, 1.0),
        })
    }
}

/// Each entry retains group boundaries rather than incorrectly flattening
/// nonlinear blend/opacity operations into individual child media layers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NestedFrameEntry {
    pub composition: String,
    pub source_time_us: u64,
    pub instance_path: Vec<String>,
    pub ancestors: Vec<CompositionInstance>,
    pub ancestor_poses: Vec<NestedGroupPose>,
    pub canvas_to_root: super::group_transform::GroupAffine,
}

pub(crate) fn effect_track_dependencies(content:&super::composition::Composition)->BTreeSet<usize>{
    let visual:Vec<_>=content.tracks.iter().enumerate().filter(|(_,track)|matches!(track,super::composition::CompositionTrack::Image{..}|super::composition::CompositionTrack::Video{..}|super::composition::CompositionTrack::Color{..}|super::composition::CompositionTrack::Graphic{..})).map(|(index,_)|index).collect();
    let mut needed=BTreeSet::new();
    let mut pending:Vec<_>=content.tracks.iter().enumerate().filter_map(|(index,track)|match track{super::composition::CompositionTrack::Text{hidden:false,..}|super::composition::CompositionTrack::Graphic{hidden:false,..}|super::composition::CompositionTrack::Image{hidden:false,..}|super::composition::CompositionTrack::Video{hidden:false,..}=>Some(index),_=>None}).collect();
    while let Some(index)=pending.pop(){
        if !needed.insert(index){continue;}
        let effects:Vec<_>=match &content.tracks[index]{super::composition::CompositionTrack::Text{clips,..}=>clips.iter().flat_map(|c|c.effects.iter()).collect(),super::composition::CompositionTrack::Graphic{clips,..}=>clips.iter().flat_map(|c|c.effects.iter()).collect(),super::composition::CompositionTrack::Image{clips,..}=>clips.iter().flat_map(|c|c.effects.iter()).collect(),super::composition::CompositionTrack::Video{clips,..}=>clips.iter().flat_map(|c|c.effects.iter()).collect(),_=>vec![]};
        for effect in effects{let layer=match effect{super::composition::VideoEffect::TrackMatte{parameters}=>parameters.matte,super::composition::VideoEffect::GradientWipe{parameters,..}=>parameters.layer,super::composition::VideoEffect::CompoundBlur{parameters,..}=>parameters.layer,_=>0};if layer>0{if let Some(index)=visual.get((layer-1)as usize){pending.push(*index);}}}
    }
    needed
}

impl NestedCompositionProject {
    /// Assemble flat media surfaces followed by authored child instances.
    /// The caller supplies the existing media renderer for each local clock.
    pub fn render_surfaces<F>(
        &self,
        time_us: u64,
        media: F,
    ) -> Result<super::group_surface::GroupSurface>
    where
        F: FnMut(&str, &Composition, u64) -> Result<super::group_surface::GroupSurface>,
    {
        self.render_surfaces_checked(time_us, media, &mut || Ok(()))
    }

    pub fn render_surfaces_checked<F>(
        &self,
        time_us: u64,
        media: F,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<super::group_surface::GroupSurface>
    where
        F: FnMut(&str, &Composition, u64) -> Result<super::group_surface::GroupSurface>,
    {
        self.render_surfaces_from(time_us, &self.root, media, checkpoint)
    }

    pub fn render_surfaces_from<F>(
        &self,
        time_us: u64,
        composition: &str,
        mut media: F,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<super::group_surface::GroupSurface>
    where
        F: FnMut(&str, &Composition, u64) -> Result<super::group_surface::GroupSurface>,
    {
        checkpoint()?;
        self.dependency_order()?;
        ensure!(
            self.compositions.contains_key(composition),
            "preview composition missing"
        );
        ensure!(
            time_us <= 9_007_199_254_740_991,
            "root frame time exceeds exact wire range"
        );
        let mut visited = 0;
        let _fonts=super::concat_project_fonts::Scope::enter(&self.fonts)?;
        self.render_surface(composition, time_us, &mut media, &mut visited, checkpoint)
    }

    fn render_surface<F>(
        &self,
        id: &str,
        time: u64,
        media: &mut F,
        visited: &mut usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<super::group_surface::GroupSurface>
    where
        F: FnMut(&str, &Composition, u64) -> Result<super::group_surface::GroupSurface>,
    {
        use super::{group_surface::GroupSurface, group_transform::GroupAffine};
        checkpoint()?;
        *visited += 1;
        ensure!(
            *visited <= 4096,
            "nested render exceeds 4096 active instances"
        );
        let document = &self.compositions[id];
        let canvas = document.content.canvas;
        let mut surface = GroupSurface::transparent(canvas.width as usize, canvas.height as usize)?;
        // Source overhang is transparent; never hold a child's final frame implicitly.
        if time >= self.document_duration_us(id)? {
            return Ok(surface);
        }
        let base = media(id, &document.content, time)?;
        ensure!(
            base.width == surface.width && base.height == surface.height,
            "media renderer canvas mismatch"
        );
        surface.composite_checked(
            &base,
            GroupAffine::IDENTITY,
            1.0,
            BlendMode::Normal,
            checkpoint,
        )?;
        for instance in &document.instances {
            if let Some(child_time) = instance.timing.source_time_us(time)? {
                let pose = instance.pose(time)?;
                if pose.opacity == 0.0 || pose.scale_x == 0.0 || pose.scale_y == 0.0 {
                    continue;
                }
                let child = self.render_surface(
                    &instance.timing.composition,
                    child_time,
                    media,
                    visited,
                    checkpoint,
                )?;
                let matrix = GroupAffine::new(
                    [surface.width as f64, surface.height as f64],
                    [child.width as f64, child.height as f64],
                    [instance.transform.anchor_x, instance.transform.anchor_y],
                    [pose.x, pose.y],
                    [pose.scale_x, pose.scale_y],
                    pose.rotation_degrees,
                )?;
                surface.composite_checked(
                    &child,
                    matrix,
                    pose.opacity,
                    instance.blend_mode,
                    checkpoint,
                )?;
            }
        }
        Ok(surface)
    }

    /// Conservative source requirements over a focused half-open export interval.
    /// Animated invisibility is retained; constant-zero layers and inactive intervals are pruned.
    pub fn range_image_sources(&self, composition: &str, range: std::ops::Range<u64>, rendered: &BTreeSet<String>) -> Result<BTreeSet<super::composition::SourceId>> {
        self.dependency_order()?;
        ensure!(self.compositions.contains_key(composition) && range.start < range.end && range.end <= 9_007_199_254_740_991, "invalid export source range");
        let mut sources = BTreeSet::new();
        self.collect_range_images(composition, range, rendered, &mut sources, &mut 0)?;
        Ok(sources)
    }
    fn collect_range_images(&self, id: &str, range: std::ops::Range<u64>, rendered: &BTreeSet<String>, sources: &mut BTreeSet<super::composition::SourceId>, visited: &mut usize) -> Result<()> {
        *visited += 1;
        ensure!(*visited <= 4096, "export source range exceeds instance budget");
        let end = range.end.min(self.document_duration_us(id)?);
        if range.start >= end { return Ok(()) }
        let document = self.compositions.get(id).ok_or_else(|| anyhow::anyhow!("export composition missing"))?;
        let zero = |value: &AnimatableValue| matches!(value, AnimatableValue::Constant { value } if *value == 0.0);
        let invisible = |opacity: &AnimatableValue, transform: &TransformSpec| transform.concat_animation.is_none() && (zero(opacity) || zero(&transform.scale_x) || zero(&transform.scale_y));
        if !rendered.contains(id) {
            let needed=effect_track_dependencies(&document.content);
            for (index,track) in document.content.tracks.iter().enumerate() {
                match track {
                    super::composition::CompositionTrack::Image { hidden, clips, .. } => {
                        if *hidden && !needed.contains(&index){continue;}
                        for clip in clips.iter().filter(|clip| clip.enabled) {
                            if !invisible(&clip.opacity, &clip.transform) && range.start < clip.timeline_start_tick + clip.duration_ticks && clip.timeline_start_tick < end { sources.insert(clip.source_id.clone()); }
                        }
                    }
                    super::composition::CompositionTrack::Video { hidden, clips, transitions, .. } => {
                        if *hidden && !needed.contains(&index){continue;}
                        for clip in clips.iter().filter(|clip| clip.enabled) {
                            if !invisible(&clip.opacity, &clip.transform) && range.start < clip.placement.timeline_end_tick()? && clip.placement.timeline_start_tick < end { sources.insert(clip.source_id.clone()); }
                        }
                        if let Some(indices)=super::concat_project_transitions::visual_range_clip_indices(clips,transitions,&document.content.canvas,range.start as i128*254016,end as i128*254016)?{
                            sources.extend(indices.into_iter().map(|index|clips[index].source_id.clone()));
                            continue;
                        }
                        // Transition handles can be visible beyond either ordinary clip interval.
                        for transition in transitions{
                            let(start,duration)=super::transition_preview_sources::resolved_span(clips,transition)?;
                            let first=(range.start as i128*254016).max(start as i128);let finish=(end as i128*254016).min(start as i128+duration as i128);
                            if first>=finish{continue;}
                            for endpoint in [transition.from_clip_id.as_ref(),transition.to_clip_id.as_ref()].into_iter().flatten(){
                                if let Some(clip)=clips.iter().find(|clip|&clip.id==endpoint&&clip.enabled){
                                    if super::video_clock::transition_source_sample_tick(&clip.placement,clip.playback_mode,first)?.is_some()||super::video_clock::transition_source_sample_tick(&clip.placement,clip.playback_mode,finish-1)?.is_some(){sources.insert(clip.source_id.clone());}
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for instance in &document.instances {
            if invisible(&instance.opacity, &instance.transform) {continue}
            let start = range.start.max(instance.timing.timeline_start_us);
            let finish = end.min(instance.timing.timeline_end_us()?);
            if start >= finish {continue}
            let child_start = instance.timing.source_in_us.checked_add(start-instance.timing.timeline_start_us).ok_or_else(|| anyhow::anyhow!("export source clock overflow"))?;
            let child_end = instance.timing.source_in_us.checked_add(finish-instance.timing.timeline_start_us).ok_or_else(|| anyhow::anyhow!("export source clock overflow"))?;
            self.collect_range_images(&instance.timing.composition, child_start..child_end, rendered, sources, visited)?;
        }
        Ok(())
    }

    /// Resolve image/video authorization requirements for the actual focused frame.
    pub fn frame_image_sources(
        &self,
        composition: &str,
        time_us: u64,
        rendered: &BTreeSet<String>,
    ) -> Result<BTreeSet<super::composition::SourceId>> {
        self.dependency_order()?;
        ensure!(
            self.compositions.contains_key(composition),
            "preview composition missing"
        );
        ensure!(
            time_us <= 9_007_199_254_740_991,
            "frame time exceeds exact wire range"
        );
        let mut sources = BTreeSet::new();
        let mut visited = 0;
        self.collect_frame_images(composition, time_us, rendered, &mut sources, &mut visited)?;
        Ok(sources)
    }

    fn collect_frame_images(
        &self,
        id: &str,
        time: u64,
        rendered: &BTreeSet<String>,
        sources: &mut BTreeSet<super::composition::SourceId>,
        visited: &mut usize,
    ) -> Result<()> {
        *visited += 1;
        ensure!(
            *visited <= 4096,
            "nested frame exceeds 4096 active instances"
        );
        if time >= self.document_duration_us(id)? {
            return Ok(());
        }
        let document = &self.compositions[id];
        if !rendered.contains(id) {
            let needed=effect_track_dependencies(&document.content);
            for (index,track) in document.content.tracks.iter().enumerate() {
                if let super::composition::CompositionTrack::Video {
                    hidden,
                    clips,
                    transitions,
                    ..
                } = track
                {
                    if *hidden && !needed.contains(&index){continue;}
                    if let Some(layers)=super::concat_project_transitions::visual_layers(clips,transitions,&document.content.canvas,time as i128*254016)?{
                        sources.extend(layers.into_iter().map(|layer|clips[layer.clip_index].source_id.clone()));
                        continue;
                    }
                    if let Some(handles) = super::transition_preview_sources::active_sources(clips,transitions,time)? {
                        sources.extend(handles);
                        continue;
                    }
                    for clip in clips.iter().filter(|clip| clip.enabled) {
                        if super::video_clock::source_tick(
                            &clip.placement,
                            clip.playback_mode,
                            time,
                        )?
                        .is_none()
                        {
                            continue;
                        }
                        let source_local=if clip.placement.concat_retime.is_some(){Some(super::video_effect_timing::sample(clip,time as i128*254016)?.ok_or_else(||anyhow::anyhow!("active source effect clock missing"))?.generator as f64/254016000000.)}else{None};
                        let at=|value:&AnimatableValue|->Result<f64>{if let Some(seconds)=source_local {Ok(match value{AnimatableValue::Constant{value}=>*value,AnimatableValue::Keyframes{track}=>track.sample_seconds(seconds)?})}else{sample_value(value,time-clip.placement.timeline_start_tick)}};
                        if clip.transform.concat_animation.is_none() && (at(&clip.opacity)?.clamp(0.0, 1.0) == 0.0
                            || at(&clip.transform.scale_x)? == 0.0
                            || at(&clip.transform.scale_y)? == 0.0)
                        {
                            continue;
                        }
                        sources.insert(clip.source_id.clone());
                    }
                }
                if let super::composition::CompositionTrack::Image {
                    hidden,
                    clips,
                    ..
                } = track
                {
                    if *hidden && !needed.contains(&index){continue;}
                    for clip in clips.iter().filter(|clip| {
                        clip.enabled
                            && time >= clip.timeline_start_tick
                            && time < clip.timeline_start_tick + clip.duration_ticks
                    }) {
                        let local = time - clip.timeline_start_tick;
                        if clip.transform.concat_animation.is_none() && (sample_value(&clip.opacity, local)?.clamp(0.0, 1.0) == 0.0
                            || sample_value(&clip.transform.scale_x, local)? == 0.0
                            || sample_value(&clip.transform.scale_y, local)? == 0.0)
                        {
                            continue;
                        }
                        sources.insert(clip.source_id.clone());
                    }
                }
            }
        }
        for instance in &document.instances {
            if let Some(child_time) = instance.timing.source_time_us(time)? {
                let pose = instance.pose(time)?;
                if pose.opacity == 0.0 || pose.scale_x == 0.0 || pose.scale_y == 0.0 {
                    continue;
                }
                self.collect_frame_images(
                    &instance.timing.composition,
                    child_time,
                    rendered,
                    sources,
                    visited,
                )?;
            }
        }
        Ok(())
    }

    pub fn split_instance(
        &self,
        parent: &str,
        instance_id: &str,
        right_id: &str,
        time_us: u64,
    ) -> Result<Self> {
        self.dependency_order()?;
        let mut result = self.clone();
        let document = result
            .compositions
            .get_mut(parent)
            .ok_or_else(|| anyhow::anyhow!("parent composition missing"))?;
        let index = document
            .instances
            .iter()
            .position(|instance| instance.id == instance_id)
            .ok_or_else(|| anyhow::anyhow!("instance missing"))?;
        let original = document.instances[index].clone();
        let (left_timing, right_timing) = original.timing.split(time_us)?;
        let mut right = original.clone();
        right.id = right_id.into();
        right.animation_offset_us = original
            .animation_offset_us
            .checked_add(time_us - original.timing.timeline_start_us)
            .ok_or_else(|| anyhow::anyhow!("group animation clock overflow"))?;
        right.timing = right_timing;
        document.instances[index].timing = left_timing;
        document.instances.insert(index + 1, right);
        result.dependency_order()?;
        Ok(result)
    }

    /// Wrap all authored tracks and nested instances in a new child document.
    /// Immutable return allows callers to commit the group operation as one edit.
    pub fn group_document(&self, parent: &str, child: &str, instance_id: &str) -> Result<Self> {
        self.dependency_order()?;
        ensure!(
            !self.compositions.contains_key(child),
            "child composition already exists"
        );
        let content = self
            .compositions
            .get(parent)
            .ok_or_else(|| anyhow::anyhow!("parent composition is missing"))?;
        let duration = self.document_duration_us(parent)?;
        ensure!(duration > 0, "cannot group an empty composition");
        let child_document = content.clone();
        let mut grouped = NestedCompositionDocument {
            duration_us: None,
            content: Composition::new(content.content.canvas),
            instances: vec![],
        };
        grouped.instances.push(CompositionInstance {
            id: instance_id.into(),
            timing: CompositionReferenceTiming {
                composition: child.into(),
                timeline_start_us: 0,
                source_in_us: 0,
                source_out_us: duration,
            },
            animation_offset_us: 0,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
        });
        let mut result = self.clone();
        result.compositions.insert(child.into(), child_document);
        if !result.timeline_order.is_empty() {
            let index = result.timeline_order.iter().position(|id| id == parent).unwrap();
            result.timeline_order.insert(index + 1, child.into());
        }
        result.compositions.insert(parent.into(), grouped);
        result.dependency_order()?;
        Ok(result)
    }

    pub fn durations_us(&self) -> Result<BTreeMap<String, u64>> {
        self.dependency_order()?;
        self.compositions
            .keys()
            .map(|id| Ok((id.clone(), self.document_duration_us(id)?)))
            .collect()
    }

    fn document_duration_us(&self, id: &str) -> Result<u64> {
        let document = self
            .compositions
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("composition is missing"))?;
        if let Some(duration) = document.duration_us {
            ensure!(
                duration <= 9_007_199_254_740_991,
                "composition duration exceeds exact wire range"
            );
            return Ok(duration);
        }
        let mut duration = document.content.native_duration_ticks()?;
        for instance in &document.instances {
            duration = duration.max(instance.timing.timeline_end_us()?);
        }
        Ok(duration)
    }

    pub fn frame_plan(&self, root_time_us: u64) -> Result<Vec<NestedFrameEntry>> {
        self.dependency_order()?;
        ensure!(
            root_time_us <= 9_007_199_254_740_991,
            "root frame time exceeds exact wire range"
        );
        let mut output = Vec::new();
        self.plan_frame(&self.root, root_time_us, &[], &[], &[], &mut output)?;
        Ok(output)
    }

    fn plan_frame(
        &self,
        id: &str,
        time: u64,
        path: &[String],
        ancestors: &[CompositionInstance],
        poses: &[NestedGroupPose],
        output: &mut Vec<NestedFrameEntry>,
    ) -> Result<()> {
        ensure!(
            output.len() < 4096,
            "nested frame exceeds 4096 active instances"
        );
        let mut canvas_to_root = super::group_transform::GroupAffine::IDENTITY;
        let mut parent_id = self.root.as_str();
        for (instance, pose) in ancestors.iter().zip(poses) {
            let parent = self.compositions[parent_id].content.canvas;
            let child = self.compositions[&instance.timing.composition]
                .content
                .canvas;
            let matrix = super::group_transform::GroupAffine::new(
                [parent.width as f64, parent.height as f64],
                [child.width as f64, child.height as f64],
                [instance.transform.anchor_x, instance.transform.anchor_y],
                [pose.x, pose.y],
                [pose.scale_x, pose.scale_y],
                pose.rotation_degrees,
            )?;
            canvas_to_root = canvas_to_root.compose(matrix)?;
            parent_id = &instance.timing.composition;
        }
        output.push(NestedFrameEntry {
            composition: id.into(),
            source_time_us: time,
            instance_path: path.to_vec(),
            ancestors: ancestors.to_vec(),
            ancestor_poses: poses.to_vec(),
            canvas_to_root,
        });
        for instance in &self.compositions[id].instances {
            if let Some(child_time) = instance.timing.source_time_us(time)? {
                let mut child_path = path.to_vec();
                child_path.push(instance.id.clone());
                let mut child_ancestors = ancestors.to_vec();
                child_ancestors.push(instance.clone());
                let mut child_poses = poses.to_vec();
                child_poses.push(instance.pose(time)?);
                self.plan_frame(
                    &instance.timing.composition,
                    child_time,
                    &child_path,
                    &child_ancestors,
                    &child_poses,
                    output,
                )?;
            }
        }
        Ok(())
    }

    pub fn dependency_order(&self) -> Result<Vec<String>> {
        super::project_media::validate(&self.project_media)?;
        if !self.timeline_order.is_empty() {
            let ordered: BTreeSet<_> = self.timeline_order.iter().collect();
            let ids: BTreeSet<_> = self.compositions.keys().collect();
            ensure!(self.timeline_order.len() == ids.len() && ordered == ids, "timeline order must contain every composition exactly once");
        }
        super::project_luts::validate(&self.luts)?;
        super::concat_project_fonts::bank(&self.fonts)?;
        ensure!(
            self.schema_version == 1,
            "unsupported nested project schema"
        );
        let mut references = BTreeMap::new();
        for (id, document) in &self.compositions {
            document.content.validate_native_surface()?;
            for track in &document.content.tracks {
                let effects = match track {
                    super::composition::CompositionTrack::Video { clips, .. } => clips.iter().flat_map(|clip| &clip.effects).collect::<Vec<_>>(),
                    super::composition::CompositionTrack::Image { clips, .. } => clips.iter().flat_map(|clip| &clip.effects).collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                {
                    for effect in effects {
                        if let super::composition::VideoEffect::Lumetri { parameters } = effect {
                            parameters.validate_library(&self.luts)?;
                        }
                    }
                }
            }
            self.document_duration_us(id)?;
            ensure!(
                document.content.time_base == DEFAULT_TIME_BASE,
                "nested timing requires microsecond composition clocks"
            );
            ensure!(
                document.instances.len() <= 512,
                "too many composition instances"
            );
            let mut ids = BTreeSet::new();
            for instance in &document.instances {
                ensure!(
                    !instance.id.is_empty()
                        && instance.id.len() <= 128
                        && !instance.id.chars().any(char::is_control)
                        && ids.insert(&instance.id),
                    "invalid or duplicate composition instance id"
                );
                instance.timing.timeline_end_us()?;
                ensure!(
                    instance
                        .animation_offset_us
                        .checked_add(instance.timing.source_out_us - instance.timing.source_in_us)
                        .is_some_and(|end| end <= 9_007_199_254_740_991),
                    "group animation clock exceeds exact wire range"
                );
                instance.transform.validate()?;
                instance.opacity.validate()?;
            }
            references.insert(
                id.clone(),
                document
                    .instances
                    .iter()
                    .map(|instance| instance.timing.composition.clone())
                    .collect(),
            );
        }
        CompositionGraph {
            root: self.root.clone(),
            references,
        }
        .dependency_order()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::CanvasSpec;
    fn fixture() -> NestedCompositionProject {
        let child = NestedCompositionDocument {
            duration_us: None,
            content: Composition::new(CanvasSpec::default()),
            instances: vec![],
        };
        let mut root = child.clone();
        root.instances.push(CompositionInstance {
            id: "group".into(),
            timing: CompositionReferenceTiming {
                composition: "child".into(),
                timeline_start_us: 500000,
                source_in_us: 100000,
                source_out_us: 1100000,
            },
            animation_offset_us: 0,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.5),
            blend_mode: BlendMode::Normal,
        });
        NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts:vec![],
            schema_version: 1,
            root: "root".into(),
            compositions: BTreeMap::from([("root".into(), root), ("child".into(), child)]),
        }
    }
    #[test]
    fn timeline_order_roundtrips_without_changing_frame_plan_and_rejects_invalid_lists() {
        let mut project = fixture();
        let before = serde_json::to_value(project.frame_plan(750000).unwrap()).unwrap();
        project.timeline_order = vec!["root".into(), "child".into()];
        assert_eq!(serde_json::to_value(project.frame_plan(750000).unwrap()).unwrap(), before);
        let encoded = serde_json::to_value(&project).unwrap();
        assert_eq!(encoded["timelineOrder"], serde_json::json!(["root", "child"]));
        let restored: NestedCompositionProject = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored.timeline_order, project.timeline_order);
        assert_eq!(serde_json::to_value(restored.frame_plan(750000).unwrap()).unwrap(), before);
        for order in [vec!["root"], vec!["root", "root"], vec!["root", "missing"]] {
            project.timeline_order = order.into_iter().map(str::to_owned).collect();
            assert!(project.dependency_order().is_err());
        }
        project.timeline_order = vec!["root".into(), "child".into()];
        let grouped = project.group_document("root", "grouped", "new-group").unwrap();
        assert_eq!(grouped.timeline_order, ["root", "grouped", "child"]);
        assert!(grouped.dependency_order().is_ok());
        project.timeline_order.clear();
        assert!(project.dependency_order().is_ok());
        assert!(serde_json::to_value(project).unwrap().get("timelineOrder").is_none());
    }

    #[test]
    fn gradient_wipe_hidden_dependencies_follow_transitive_cycles_for_preview_and_export(){
        let mut project=fixture();let root=project.compositions.get_mut("root").unwrap();root.instances.clear();root.duration_us=Some(1000000);
        root.content.sources=serde_json::from_value(serde_json::json!({"a":{"id":"a","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"b":{"id":"b","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"c":{"id":"c","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let tracks:Vec<_>=[("a",false,2),("b",true,1),("c",true,0)].into_iter().map(|(id,hidden,layer)|serde_json::json!({"kind":"image","id":id,"name":id,"hidden":hidden,"clips":[{"id":id,"sourceId":id,"timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"gradient_wipe","parameters":{"layer":layer,"completion":50.}}]}]})).collect();
        root.content.tracks=serde_json::from_value(serde_json::json!(tracks)).unwrap();
        let empty=BTreeSet::new();let expected:BTreeSet<super::super::composition::SourceId>=serde_json::from_value(serde_json::json!(["a","b"])).unwrap();
        assert_eq!(project.frame_image_sources("root",500000,&empty).unwrap(),expected);
        assert_eq!(project.range_image_sources("root",0..1000000,&empty).unwrap(),expected);
    }
    #[test]
    fn compound_blur_hidden_dependencies_follow_transitive_cycles_for_preview_and_export(){
        let mut project=fixture();let root=project.compositions.get_mut("root").unwrap();root.instances.clear();root.duration_us=Some(1000000);
        root.content.sources=serde_json::from_value(serde_json::json!({"a":{"id":"a","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"b":{"id":"b","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"c":{"id":"c","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let tracks:Vec<_>=[("a",false,2),("b",true,1),("c",true,0)].into_iter().map(|(id,hidden,layer)|serde_json::json!({"kind":"image","id":id,"name":id,"hidden":hidden,"clips":[{"id":id,"sourceId":id,"timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{"layer":layer,"max":4.25}}]}]})).collect();
        root.content.tracks=serde_json::from_value(serde_json::json!(tracks)).unwrap();
        let empty=BTreeSet::new();let expected:BTreeSet<super::super::composition::SourceId>=serde_json::from_value(serde_json::json!(["a","b"])).unwrap();
        assert_eq!(project.frame_image_sources("root",500000,&empty).unwrap(),expected);
        assert_eq!(project.range_image_sources("root",0..1000000,&empty).unwrap(),expected);
    }
    #[test]
    fn track_matte_hidden_dependencies_follow_transitive_cycles_for_preview_and_export(){
        let mut project=fixture();let root=project.compositions.get_mut("root").unwrap();root.instances.clear();root.duration_us=Some(1000000);
        root.content.sources=serde_json::from_value(serde_json::json!({"a":{"id":"a","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"b":{"id":"b","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false},"c":{"id":"c","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let tracks:Vec<_>=[("a",false,2),("b",true,1),("c",true,0)].into_iter().map(|(id,hidden,layer)|serde_json::json!({"kind":"image","id":id,"name":id,"hidden":hidden,"clips":[{"id":id,"sourceId":id,"timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"track_matte","parameters":{"matte":layer}}]}]})).collect();
        root.content.tracks=serde_json::from_value(serde_json::json!(tracks)).unwrap();
        let empty=BTreeSet::new();let expected:BTreeSet<super::super::composition::SourceId>=serde_json::from_value(serde_json::json!(["a","b"])).unwrap();
        assert_eq!(project.frame_image_sources("root",500000,&empty).unwrap(),expected);
        assert_eq!(project.range_image_sources("root",0..1000000,&empty).unwrap(),expected);
    }
    #[test]
    fn frame_images_follow_focused_child_clocks_visibility_and_output_overrides() {
        let mut project = fixture();
        let child = project.compositions.get_mut("child").unwrap();
        child.duration_us = Some(1500000);
        child.content.sources = serde_json::from_value(serde_json::json!({"picture":{"id":"picture","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        child.content.tracks = serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"photo","sourceId":"picture","timelineStartTick":300000,"durationTicks":200000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}])).unwrap();
        let empty = BTreeSet::new();
        assert_eq!(project.range_image_sources("root", 800000..900000, &empty).unwrap().iter().map(|id| id.as_str()).collect::<Vec<_>>(), ["picture"]);
        for range in [0..500000, 900000..1000000] {
            assert!(project.range_image_sources("root", range, &empty).unwrap().is_empty());
        }
        assert!(project.range_image_sources("root", 800000..900000, &BTreeSet::from(["child".into()])).unwrap().is_empty());
        assert!(project.range_image_sources("root", 1..1, &empty).is_err());
        assert_eq!(
            project
                .frame_image_sources("root", 800000, &empty)
                .unwrap()
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            ["picture"]
        );
        assert!(project
            .frame_image_sources("root", 499999, &empty)
            .unwrap()
            .is_empty());
        assert!(project
            .frame_image_sources("root", 900000, &empty)
            .unwrap()
            .is_empty());
        assert!(project
            .frame_image_sources("child", 500000, &empty)
            .unwrap()
            .is_empty());
        assert!(project
            .frame_image_sources("root", 800000, &BTreeSet::from(["child".into()]))
            .unwrap()
            .is_empty());
        project.compositions.get_mut("root").unwrap().instances[0].opacity =
            AnimatableValue::constant(0.0);
        assert!(project.range_image_sources("root", 800000..900000, &empty).unwrap().is_empty());
        assert!(project
            .frame_image_sources("root", 800000, &empty)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn focused_child_preview_keeps_original_project_root_and_ignores_parent_pose() {
        let mut project = fixture();
        for document in project.compositions.values_mut() {
            document.duration_us = Some(1500000);
            document.content.canvas.width = 2;
            document.content.canvas.height = 2;
        }
        let surface = project
            .render_surfaces_from(
                400000,
                "child",
                |id, _, time| {
                    assert_eq!(id, "child");
                    assert_eq!(time, 400000);
                    Ok(super::super::group_surface::GroupSurface {
                        width: 2,
                        height: 2,
                        rgba: [0, 0, 255, 255].repeat(4),
                    })
                },
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(surface.rgba, [0, 0, 255, 255].repeat(4));
        assert_eq!(project.root, "root");
        assert!(project
            .render_surfaces_from(0, "missing", |_, _, _| panic!("invalid focus"), &mut || Ok(
                ()
            ))
            .is_err());
    }
    #[test]
    fn splitting_group_preserves_source_and_authored_animation_clocks() {
        let mut project = fixture();
        let animation: AnimatableValue = serde_json::from_value(serde_json::json!({ "mode": "keyframes", "track": { "timeBase": 1000000, "interpolation": "linear", "keyframes": [{ "tick": 0, "value": 0.0 }, { "tick": 1000000, "value": 1.0 }] } })).unwrap();
        project.compositions.get_mut("root").unwrap().instances[0].opacity = animation.clone();
        project.compositions.get_mut("root").unwrap().instances[0]
            .transform
            .x = animation;
        let split = project
            .split_instance("root", "group", "right", 800000)
            .unwrap();
        let split = split
            .split_instance("root", "right", "tail", 1100000)
            .unwrap();
        for time in [500000, 799999, 800000, 1099999, 1100000, 1499999] {
            let before = project.frame_plan(time).unwrap();
            let after = split.frame_plan(time).unwrap();
            assert_eq!(before[1].source_time_us, after[1].source_time_us);
            assert_eq!(before[1].ancestor_poses, after[1].ancestor_poses);
            assert_eq!(before[1].canvas_to_root, after[1].canvas_to_root);
        }
        assert_eq!(
            split.compositions["root"].instances[2].animation_offset_us,
            600000
        );
        assert_eq!(project.compositions["root"].instances.len(), 1);
        assert!(project
            .split_instance("root", "group", "group", 800000)
            .is_err());
        assert!(project
            .split_instance("root", "group", "right", 500000)
            .is_err());
        let restored: NestedCompositionProject =
            serde_json::from_str(&serde_json::to_string(&split).unwrap()).unwrap();
        assert_eq!(restored, split);
    }
    #[test]
    fn recursive_surfaces_apply_group_alpha_and_blank_source_overhang() {
        use super::super::group_surface::GroupSurface;
        let mut project = fixture();
        for document in project.compositions.values_mut() {
            document.content.canvas.width = 2;
            document.content.canvas.height = 2;
            document.duration_us = Some(1500000);
        }
        let mut calls = vec![];
        let frame = project
            .render_surfaces(800000, |id, _, time| {
                calls.push((id.to_owned(), time));
                Ok(GroupSurface {
                    width: 2,
                    height: 2,
                    rgba: if id == "child" {
                        [0, 0, 255, 255].repeat(4)
                    } else {
                        [0, 0, 0, 0].repeat(4)
                    },
                })
            })
            .unwrap();
        assert_eq!(calls, [("root".into(), 800000), ("child".into(), 400000)]);
        assert_eq!(frame.rgba, [0, 0, 255, 128].repeat(4));
        project.compositions.get_mut("child").unwrap().duration_us = Some(400000);
        let frame = project
            .render_surfaces(800000, |id, _, _| {
                assert_eq!(id, "root");
                GroupSurface::transparent(2, 2)
            })
            .unwrap();
        assert_eq!(frame.rgba, [0, 0, 0, 0].repeat(4));
        let frame = project
            .render_surfaces(1500000, |_, _, _| panic!("past root duration"))
            .unwrap();
        assert_eq!(frame.rgba, [0, 0, 0, 0].repeat(4));
    }

    #[test]
    fn persisted_instances_derive_dependencies_and_preserve_visual_parameters() {
        let project = fixture();
        assert_eq!(project.dependency_order().unwrap(), ["child", "root"]);
        let saved = serde_json::to_string(&project).unwrap();
        let restored: NestedCompositionProject = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, project);
        assert_eq!(
            restored.dependency_order().unwrap(),
            project.dependency_order().unwrap()
        );
    }
    #[test]
    fn nested_frame_mapping_preserves_instance_paths_and_compositing_boundaries() {
        let mut project = fixture();
        let leaf = project.compositions["child"].clone();
        project.compositions.insert("leaf".into(), leaf);
        let mut instance = project.compositions["root"].instances[0].clone();
        instance.id = "inner".into();
        instance.timing.composition = "leaf".into();
        instance.timing.timeline_start_us = 200000;
        instance.timing.source_in_us = 700000;
        instance.timing.source_out_us = 1700000;
        project
            .compositions
            .get_mut("child")
            .unwrap()
            .instances
            .push(instance);
        let frames = project.frame_plan(800000).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[1].source_time_us, 400000);
        assert_eq!(frames[2].source_time_us, 900000);
        assert_eq!(frames[2].instance_path, ["group", "inner"]);
        assert_eq!(frames[2].ancestors.len(), 2);
        assert_eq!(
            frames[2]
                .ancestor_poses
                .iter()
                .map(|pose| pose.local_time_us)
                .collect::<Vec<_>>(),
            [300000, 200000]
        );
        assert_eq!(frames[2].ancestor_poses[0].opacity, 0.5);
        assert_eq!(
            frames[2].ancestors[0].opacity,
            AnimatableValue::constant(0.5)
        );
        assert_eq!(project.frame_plan(499999).unwrap().len(), 1);
        assert_eq!(project.frame_plan(1500000).unwrap().len(), 1);
    }

    #[test]
    fn grouping_preserves_child_clocks_and_is_an_immutable_single_edit() {
        let original = fixture();
        let grouped = original.group_document("root", "grouped", "outer").unwrap();
        assert_eq!(
            grouped.compositions["grouped"],
            original.compositions["root"]
        );
        assert_eq!(original.compositions.len(), 2);
        let frame = grouped.frame_plan(800000).unwrap();
        assert_eq!(
            frame
                .iter()
                .map(|entry| entry.source_time_us)
                .collect::<Vec<_>>(),
            [800000, 800000, 400000]
        );
        assert_eq!(frame[2].instance_path, ["outer", "group"]);
        assert!(original.group_document("root", "child", "outer").is_err());
        assert!(original.group_document("root", "new", "").is_err());
    }

    #[test]
    fn pinned_duration_overrides_derived_span_and_survives_grouping() {
        let mut project = fixture();
        assert_eq!(project.durations_us().unwrap()["root"], 1500000);
        project.compositions.get_mut("root").unwrap().duration_us = Some(750000);
        assert_eq!(project.durations_us().unwrap()["root"], 750000);
        let grouped = project
            .group_document("root", "grouped", "new-group")
            .unwrap();
        assert_eq!(
            grouped.compositions["root"].instances[0]
                .timing
                .source_out_us,
            750000
        );
        assert_eq!(grouped.compositions["grouped"].duration_us, Some(750000));
        project.compositions.get_mut("root").unwrap().duration_us = Some(u64::MAX);
        assert!(project.dependency_order().is_err());
    }

    #[test]
    fn group_animation_samples_instance_local_time_not_child_source_time() {
        let mut project = fixture();
        let animation: AnimatableValue = serde_json::from_value(serde_json::json!({ "mode": "keyframes", "track": { "timeBase": 1000000, "interpolation": "linear", "keyframes": [{ "tick": 0, "value": 0.0 }, { "tick": 1000000, "value": 1.0 }] } })).unwrap();
        let group = &mut project.compositions.get_mut("root").unwrap().instances[0];
        group.opacity = animation.clone();
        group.transform.x = animation;
        let frames = project.frame_plan(800000).unwrap();
        let pose = &frames[1].ancestor_poses[0];
        assert_eq!(pose.local_time_us, 300000);
        assert_eq!(frames[1].source_time_us, 400000);
        assert!((pose.opacity - 0.3).abs() < 1e-12);
        assert!((pose.x - 0.3).abs() < 1e-12);
    }

    #[test]
    fn project_rejects_dangling_references_duplicate_instances_and_bad_transform() {
        let mut project = fixture();
        project.compositions.remove("child");
        assert!(project.dependency_order().is_err());
        let mut project = fixture();
        let root = project.compositions.get_mut("root").unwrap();
        root.instances.push(root.instances[0].clone());
        assert!(project.dependency_order().is_err());
        let mut project = fixture();
        project.compositions.get_mut("root").unwrap().instances[0]
            .transform
            .anchor_x = f64::NAN;
        assert!(project.dependency_order().is_err());
    }
}
