//! Bind trusted verified video stores; no inference, paths or ownership grants.
use super::{
    composition::{
        Composition, CompositionClipId, CompositionSource, CompositionTrack, SourceId, SourceKind,
    },
    concat_project_import::{ReadyVideoBinding, StillBindings, VideoBindings},
    project_media::ProjectMediaItem,
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn prepare(
    timelines: &[Value],
    ready: &VideoBindings,
    physical: &BTreeMap<String, CompositionSource>,
    logical: &BTreeMap<String, CompositionSource>,
    media: &mut BTreeMap<SourceId, ProjectMediaItem>,
    stills: &StillBindings,
    preserve: bool,
) -> Result<VideoBindings> {
    let mut reserved: BTreeSet<SourceId> = logical
        .values()
        .map(|s| s.id.clone())
        .chain(media.keys().cloned())
        .collect();
    let mut other = BTreeSet::new();
    for clips in stills.values() {
        for binding in clips.values() {
            for source in [
                &binding.assets.original,
                &binding.assets.copy,
                &binding.assets.mask,
            ]
            .into_iter()
            .chain(binding.assets.regions.iter())
            {
                other.insert(source.id.clone());
                reserved.insert(source.id.clone());
            }
        }
    }
    for clips in ready.values() {
        for binding in clips.values() {
            for source in &binding.regions {
                reserved.insert(source.id.clone());
            }
        }
    }
    let mut remap = BTreeMap::<SourceId, SourceId>::new();
    let mut descriptors = BTreeMap::new();
    let mut result = ready.clone();
    for (timeline, clips) in &mut result {
        for (clip_id, ready) in clips {
            let clip = timelines
                .iter()
                .find(|t| t.get("id").and_then(Value::as_str) == Some(timeline))
                .and_then(|t| t.get("clips"))
                .and_then(Value::as_array)
                .and_then(|clips| {
                    clips
                        .iter()
                        .find(|c| c.get("id").and_then(Value::as_str) == Some(clip_id))
                })
                .context("unused or stale video cutout binding")?;
            ensure!(
                clip.get("kind").and_then(Value::as_str) == Some("video"),
                "prepared video requires a video clip"
            );
            let id = clip
                .get("mediaId")
                .and_then(Value::as_str)
                .context("video cutout media missing")?;
            let source = physical
                .get(id)
                .context("registered video cutout source missing")?;
            ensure!(
                ready.source == *source && source.kind == SourceKind::Video,
                "prepared video original differs from registered binding"
            );
            let plan = super::concat_cutout_import::plan(
                clip.get("cutout")
                    .context("video cutout settings missing")?,
            )?;
            ensure!(
                plan == ready.source_settings && ready.binding.settings() == plan.correction,
                "video cutout settings changed since preparation"
            );
            let subject = match plan.subject {
                super::concat_cutout_import::Subject::Auto => "auto",
                super::concat_cutout_import::Subject::Person => "person",
                super::concat_cutout_import::Subject::Object => "object",
            };
            ensure!(
                ready.binding.subject == subject,
                "video cutout subject changed since preparation"
            );
            ready.binding.validate()?;
            let mut assets = BTreeMap::new();
            for asset in &ready.regions {
                asset.validate()?;
                ensure!(
                    asset.kind == SourceKind::Video
                        && !asset.has_audio
                        && asset.width == 256
                        && asset.height == 256,
                    "invalid temporal region descriptor"
                );
                if let Some(previous) = descriptors.insert(asset.id.clone(), asset.clone()) {
                    ensure!(previous == *asset, "conflicting temporal region metadata");
                }
                ensure!(
                    assets.insert(asset.id.clone(), asset.clone()).is_none(),
                    "duplicate temporal region descriptor"
                );
            }
            let mut used = BTreeSet::new();
            let target = logical.get(id).context("logical video original missing")?;
            for track in &mut ready.binding.region_tracks {
                ensure!(
                    track.snapshot.source_id == source.id.as_str()
                        && track.snapshot.width == source.width
                        && track.snapshot.height == source.height,
                    "prepared region belongs to another video original"
                );
                let old = SourceId::parse(&track.snapshot.mask_source_id)?;
                ensure!(
                    assets.contains_key(&old),
                    "temporal region descriptor missing"
                );
                used.insert(old.clone());
                let new = if let Some(id) = remap.get(&old) {
                    id.clone()
                } else {
                    let mut id = old.clone();
                    if preserve
                        && (logical.values().any(|s| s.id == old)
                            || media.contains_key(&old)
                            || other.contains(&old))
                    {
                        use sha2::{Digest, Sha256};
                        let mut nonce = 0u64;
                        loop {
                            id = SourceId::parse(format!(
                                "concat-video-region-{:x}",
                                Sha256::digest(format!("{}:{nonce}", old.as_str()))
                            ))?;
                            if reserved.insert(id.clone()) {
                                break;
                            }
                            nonce = nonce
                                .checked_add(1)
                                .context("region ID allocation exhausted")?;
                        }
                    }
                    remap.insert(old.clone(), id.clone());
                    id
                };
                track.snapshot.source_id = target.id.as_str().into();
                track.snapshot.mask_source_id = new.as_str().into();
            }
            ensure!(
                used.len() == assets.len(),
                "unused temporal region descriptor"
            );
            ready.regions = assets
                .into_values()
                .map(|mut asset| -> Result<_> {
                    let physical = asset.id.clone();
                    if let Some(id) = remap.get(&physical) {
                        asset.id = id.clone();
                    }
                    if asset.id != physical {
                        let record = ProjectMediaItem {
                            source: asset.clone(),
                            library_source_id: Some(physical),
                            name: "Видеорегион".into(),
                            placeholder: false,
                            concat_metadata: Default::default(),
                        };
                        if let Some(previous) = media.insert(asset.id.clone(), record.clone()) {
                            ensure!(previous == record, "temporal region binding collision");
                        }
                    }
                    Ok(asset)
                })
                .collect::<Result<_>>()?;
            if let Some(origin)=&mut ready.binding.origin {
                origin.region_sources = std::mem::take(&mut origin.region_sources).into_iter().map(|(key,value)| {
                    let old=SourceId::parse(&key)?;
                    Ok((remap.get(&old).map(|id|id.as_str()).unwrap_or(&key).to_owned(),value))
                }).collect::<Result<_>>()?;
            }
            ready.source = target.clone();
            ready.binding.validate()?;
        }
    }
    Ok(result)
}
pub(crate) fn bind(
    content: &mut Composition,
    clip_id: &CompositionClipId,
    ready: &ReadyVideoBinding,
) -> Result<()> {
    ensure!(
        !content
            .metadata
            .as_ref()
            .is_some_and(|m| m.media_versions.contains_key(clip_id)),
        "prepared video already has another processed version"
    );
    for source in &ready.regions {
        if let Some(previous) = content.sources.get(&source.id) {
            ensure!(previous == source, "temporal region source collision");
        } else {
            content.sources.insert(source.id.clone(), source.clone());
        }
    }
    let clip = content
        .tracks
        .iter_mut()
        .find_map(|track| match track {
            CompositionTrack::Video { clips, .. } => clips.iter_mut().find(|c| &c.id == clip_id),
            _ => None,
        })
        .context("prepared video clip missing")?;
    ensure!(
        clip.source_id == ready.source.id && clip.concat_cutout.is_none(),
        "prepared video source/version changed"
    );
    clip.concat_cutout = Some(ready.binding.clone());
    Ok(())
}
