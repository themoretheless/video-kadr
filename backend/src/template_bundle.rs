//! Native template packages. Callers must authorize every input file before packing.
//! This codec never resolves a path supplied by an imported document.
mod instantiate;
pub use instantiate::{instantiate, SlotFill};

use crate::domain::{
    composition::{CompositionClipId, CompositionTrack, SourceId, SourceKind},
    concat_video_binding::Recipe,
    nested_composition::NestedCompositionProject,
    project_media::ProjectMediaItem,
};
use crate::project_archive::{ArchiveVideoCacheInput, ArchivedVideoCache, StagedArchiveVideoCache};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
const MANIFEST: &str = "template.json";
const MAX_MANIFEST: u64 = crate::project_archive::MAX_ARCHIVE_MANIFEST_BYTES as u64;
pub const MAX_POSTER_BYTES: usize = 1024 * 1024;
pub const MAX_CONCAT_POSTER_BYTES: usize = 8 * 1024 * 1024;
const MAX_ASSETS: u64 = crate::project_archive::MAX_ARCHIVE_TOTAL_MEDIA_BYTES;
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub project: NestedCompositionProject,
    pub assets: Vec<Asset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub video_caches: Vec<ArchivedVideoCache>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slot_masks: Vec<SlotMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<Poster>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotMask {
    pub timeline_id: String,
    pub clip_id: CompositionClipId,
    pub media_id: SourceId,
    pub recipe: Recipe,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub key: SourceId,
    pub byte_length: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Poster {
    pub byte_length: u64,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}
fn poster_info(bytes: &[u8]) -> Result<Poster> {
    use image::ImageDecoder;
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_POSTER_BYTES,
        "Template poster exceeds byte limit"
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(16 * 1024 * 1024);
    let decoder = image::codecs::png::PngDecoder::with_limits(std::io::Cursor::new(bytes), limits)?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0 && height > 0 && width <= 1024 && height <= 1024,
        "Invalid template poster dimensions"
    );
    image::DynamicImage::from_decoder(decoder)?;
    Ok(Poster {
        byte_length: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
        width,
        height,
    })
}
pub struct Input {
    pub source_id: SourceId,
    pub path: PathBuf,
}
pub struct Plan {
    pub project: NestedCompositionProject,
    pub assets: BTreeMap<SourceId, SourceId>,
    pub slot_masks: Vec<SlotMask>,
}
/// Replace physical bindings with package-local keys and erase slot bindings.
/// Original descriptors remain for editorial authoring; original slot bytes do not ship.
pub fn plan(project: &NestedCompositionProject) -> Result<Plan> {
    project.dependency_order()?;
    let mut next = project.clone();
    let mut slot_masks = Vec::new();
    let mut retired = BTreeSet::new();
    for (timeline_id, node) in &mut next.compositions {
        for track in &mut node.content.tracks {
            if let CompositionTrack::Video { clips, .. } = track {
                for clip in clips {
                    if !next
                        .project_media
                        .get(&clip.source_id)
                        .is_some_and(|item| item.placeholder)
                    {
                        continue;
                    }
                    if let Some(binding) = clip.concat_cutout.take() {
                        for region in &binding.region_tracks {
                            retired.insert(SourceId::parse(&region.snapshot.mask_source_id)?);
                        }
                        slot_masks.push(SlotMask {
                            timeline_id: timeline_id.clone(),
                            clip_id: clip.id.clone(),
                            media_id: clip.source_id.clone(),
                            recipe: Recipe::from_binding(binding)?,
                        });
                    }
                }
            }
        }
    }
    crate::domain::project_media::prune_retired_regions(&mut next, &retired)?;
    for node in next.compositions.values() {
        for (id, source) in &node.content.sources {
            if let Some(item) = next.project_media.get(id) {
                ensure!(
                    item.source == *source,
                    "Template has conflicting logical media descriptors"
                );
            } else {
                let name = node
                    .content
                    .metadata
                    .as_ref()
                    .and_then(|m| m.source_names.get(id))
                    .cloned()
                    .unwrap_or_else(|| id.as_str().into());
                next.project_media.insert(
                    id.clone(),
                    ProjectMediaItem {
                        source: source.clone(),
                        library_source_id: None,
                        name,
                        placeholder: false,
                        concat_metadata: Default::default(),
                    },
                );
            }
        }
    }
    let physical: BTreeSet<_> = next
        .project_media
        .iter()
        .filter(|(_, item)| !item.placeholder)
        .map(|(id, item)| item.library_source_id.clone().unwrap_or_else(|| id.clone()))
        .collect();
    let assets: BTreeMap<_, _> = physical
        .into_iter()
        .enumerate()
        .map(|(index, id)| Ok((id, SourceId::parse(format!("template-asset-{index:04}"))?)))
        .collect::<Result<_>>()?;
    for (id, item) in &mut next.project_media {
        if item.placeholder {
            item.library_source_id = None;
        } else {
            let physical = item.library_source_id.as_ref().unwrap_or(id);
            item.library_source_id = Some(assets[physical].clone());
        }
    }
    for node in next.compositions.values_mut() {
        for track in &mut node.content.tracks {
            if let CompositionTrack::Video { clips, .. } = track {
                for clip in clips {
                    if let Some(binding) = &mut clip.concat_cutout {
                        binding.origin = None;
                    }
                }
            }
        }
    }
    validate_mask_sources(&next)?;
    next.dependency_order()?;
    Ok(Plan {
        project: next,
        assets,
        slot_masks,
    })
}
/// A mask belongs to the exact original bytes. Its source and region movies
/// must be fixed design assets, since placeholders omit their original bytes.
fn validate_mask_sources(project: &NestedCompositionProject) -> Result<()> {
    for node in project.compositions.values() {
        for track in &node.content.tracks {
            if let CompositionTrack::Video { clips, .. } = track {
                for clip in clips {
                    let Some(binding) = &clip.concat_cutout else {
                        continue;
                    };
                    ensure!(
                        binding.origin.is_none(),
                        "Template video mask retains an external origin"
                    );
                    ensure!(
                        project
                            .project_media
                            .get(&clip.source_id)
                            .is_some_and(|item| !item.placeholder
                                && item.source.kind
                                    == crate::domain::composition::SourceKind::Video),
                        "Video masks require a fixed template video asset",
                    );
                    for region in &binding.region_tracks {
                        ensure!(
                            project
                                .project_media
                                .get(&SourceId::parse(region.snapshot.mask_source_id.as_str())?)
                                .is_some_and(|item| !item.placeholder),
                            "Video mask regions require fixed template assets",
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
fn validate(manifest: &Manifest) -> Result<()> {
    ensure!(manifest.schema_version == 1, "Unsupported template version");
    ensure!(
        !manifest.name.trim().is_empty()
            && manifest.name.len() <= crate::db::MAX_COMPOSITION_PROJECT_NAME_BYTES,
        "Invalid template name"
    );
    manifest.project.dependency_order()?;
    let mut expected = BTreeSet::new();
    for node in manifest.project.compositions.values() {
        for (id, source) in &node.content.sources {
            ensure!(
                manifest
                    .project
                    .project_media
                    .get(id)
                    .is_some_and(|item| item.source == *source),
                "Template source has no canonical bin record"
            );
        }
    }
    for item in manifest.project.project_media.values() {
        if item.placeholder {
            ensure!(
                item.library_source_id.is_none(),
                "Template slot retains a physical binding"
            );
        } else {
            expected.insert(
                item.library_source_id
                    .as_ref()
                    .context("Template design asset has no package binding")?
                    .clone(),
            );
        }
    }
    let mut seen = BTreeSet::new();
    let mut bytes = 0u64;
    for asset in &manifest.assets {
        ensure!(seen.insert(asset.key.clone()), "Duplicate template asset");
        ensure!(asset.byte_length > 0, "Empty template asset");
        ensure!(
            asset.sha256.len() == 64
                && asset
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "Invalid template asset digest"
        );
        bytes = bytes
            .checked_add(asset.byte_length)
            .context("Template asset size overflow")?;
    }
    validate_mask_sources(&manifest.project)?;
    let mut seen_masks = BTreeSet::new();
    for mask in &manifest.slot_masks {
        mask.recipe.validate()?;
        ensure!(
            manifest
                .project
                .project_media
                .get(&mask.media_id)
                .is_some_and(|item| item.placeholder && item.source.kind == SourceKind::Video),
            "Video mask recipe requires a video slot"
        );
        let matches = manifest
            .project
            .compositions
            .get(&mask.timeline_id)
            .into_iter()
            .flat_map(|node| &node.content.tracks)
            .filter_map(|track| match track {
                CompositionTrack::Video { clips, .. } => Some(clips),
                _ => None,
            })
            .flatten()
            .filter(|clip| {
                clip.id == mask.clip_id
                    && clip.source_id == mask.media_id
                    && clip.concat_cutout.is_none()
            })
            .count();
        ensure!(
            matches == 1 && seen_masks.insert((&mask.timeline_id, &mask.clip_id)),
            "Invalid or duplicate template mask destination"
        );
    }
    crate::project_archive::video::validate_bindings(
        &serde_json::to_value(&manifest.project)?,
        &manifest.video_caches,
        true,
        |physical, hash| {
            manifest
                .assets
                .iter()
                .any(|asset| asset.key.as_str() == physical && asset.sha256 == hash)
        },
    )?;
    for cache in &manifest.video_caches {
        bytes = bytes
            .checked_add(cache.byte_length)
            .context("Template size overflow")?;
    }
    if let Some(poster) = &manifest.poster {
        ensure!(
            poster.byte_length > 0
                && poster.byte_length <= MAX_POSTER_BYTES as u64
                && poster.width > 0
                && poster.height > 0
                && poster.width <= 1024
                && poster.height <= 1024,
            "Invalid template poster metadata"
        );
        ensure!(
            poster.sha256.len() == 64
                && poster
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "Invalid template poster digest"
        );
        bytes = bytes
            .checked_add(poster.byte_length)
            .context("Template size overflow")?;
    }
    ensure!(
        seen == expected,
        "Template asset inventory differs from design media"
    );
    ensure!(bytes <= MAX_ASSETS, "Template assets exceed byte limit");
    Ok(())
}
fn digest(
    reader: &mut impl Read,
    mut writer: Option<&mut dyn Write>,
    limit: u64,
) -> Result<(u64, String)> {
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .context("Template size overflow")?;
        ensure!(bytes <= limit, "Template asset exceeds byte limit");
        hash.update(&buffer[..n]);
        if let Some(writer) = writer.as_mut() {
            writer.write_all(&buffer[..n])?;
        }
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}
/// Streams only the exact authorized design inventory, never the creator's slot files.
pub fn write<W: Write + Seek>(
    writer: W,
    name: &str,
    project: &NestedCompositionProject,
    inputs: &[Input],
) -> Result<W> {
    write_with_poster(writer, name, project, inputs, None)
}
pub fn write_with_poster<W: Write + Seek>(
    writer: W,
    name: &str,
    project: &NestedCompositionProject,
    inputs: &[Input],
    poster_png: Option<&[u8]>,
) -> Result<W> {
    write_with_resources(writer, name, project, inputs, poster_png, &[], &[])
}

pub(crate) fn write_with_resources<W: Write + Seek>(
    writer: W,
    name: &str,
    project: &NestedCompositionProject,
    inputs: &[Input],
    poster_png: Option<&[u8]>,
    video_caches: &[ArchivedVideoCache],
    video_inputs: &[ArchiveVideoCacheInput],
) -> Result<W> {
    ensure!(
        video_caches.len() == video_inputs.len(),
        "Template video cache payload count differs"
    );
    let poster = poster_png.map(poster_info).transpose()?;
    let prepared = plan(project)?;
    let files: BTreeMap<_, _> = inputs
        .iter()
        .map(|input| (&input.source_id, &input.path))
        .collect();
    ensure!(
        files.len() == inputs.len() && files.keys().copied().eq(prepared.assets.keys()),
        "Template inputs differ from design inventory"
    );
    let mut assets = Vec::new();
    let mut total = 0u64;
    for (source, key) in &prepared.assets {
        let (length, hash) = digest(&mut File::open(files[source])?, None, MAX_ASSETS - total)?;
        total += length;
        assets.push(Asset {
            key: key.clone(),
            byte_length: length,
            sha256: hash,
        });
    }
    let manifest = Manifest {
        schema_version: 1,
        name: name.into(),
        project: prepared.project,
        assets,
        video_caches: video_caches.to_vec(),
        slot_masks: prepared.slot_masks,
        poster,
    };
    validate(&manifest)?;
    let encoded = serde_json::to_vec(&manifest)?;
    ensure!(
        encoded.len() as u64 <= MAX_MANIFEST,
        "Template manifest exceeds byte limit"
    );
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file(MANIFEST, options)?;
    zip.write_all(&encoded)?;
    for ((source, _), asset) in prepared.assets.iter().zip(&manifest.assets) {
        zip.start_file(format!("assets/{}", asset.key.as_str()), options)?;
        let (length, hash) = digest(
            &mut File::open(files[source])?,
            Some(&mut zip),
            asset.byte_length,
        )?;
        ensure!(
            length == asset.byte_length && hash == asset.sha256,
            "Template source changed during packaging"
        );
    }
    if let Some(bytes) = poster_png {
        zip.start_file("poster.png", options)?;
        zip.write_all(bytes)?;
    }
    for (cache, input) in video_caches.iter().zip(video_inputs) {
        ensure!(cache.key == input.key, "Template video cache order differs");
        zip.start_file(format!("video-caches/{}", cache.key), options)?;
        let (length, hash) = digest(
            &mut File::open(&input.path)?,
            Some(&mut zip),
            cache.byte_length,
        )?;
        ensure!(
            length == cache.byte_length && hash == cache.sha256,
            "Template video cache changed during packaging"
        );
    }
    Ok(zip.finish()?)
}
pub struct StagedAsset {
    pub key: SourceId,
    path: PathBuf,
}
impl StagedAsset {
    pub(crate) fn into_archive(
        mut self,
        filename: String,
    ) -> crate::project_archive::StagedArchiveMedia {
        crate::project_archive::StagedArchiveMedia::from_staged_file(
            self.key.as_str().into(),
            filename,
            std::mem::take(&mut self.path),
        )
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for StagedAsset {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
pub struct Parsed {
    pub manifest: Manifest,
    pub assets: Vec<StagedAsset>,
    pub poster_png: Option<Vec<u8>>,
    pub video_caches: Vec<StagedArchiveVideoCache>,
}
/// Stages only allowlisted records under fresh names; never extracts ZIP paths.
pub fn read<R: Read + Seek>(reader: R, staging: &Path) -> Result<Parsed> {
    let mut zip = ZipArchive::new(reader)?;
    let manifest: Manifest = {
        let mut entry = zip.by_index(0)?;
        ensure!(
            entry.name() == MANIFEST && entry.size() <= MAX_MANIFEST,
            "Invalid template manifest record"
        );
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_MANIFEST,
            "Template manifest exceeds byte limit"
        );
        serde_json::from_slice(&bytes)?
    };
    validate(&manifest)?;
    ensure!(
        zip.len()
            == manifest.assets.len()
                + 1
                + usize::from(manifest.poster.is_some())
                + manifest.video_caches.len(),
        "Template contains missing or unexpected records"
    );
    let mut assets = Vec::new();
    for (index, asset) in manifest.assets.iter().enumerate() {
        let mut entry = zip.by_index(index + 1)?;
        ensure!(
            entry.name() == format!("assets/{}", asset.key.as_str())
                && entry.size() == asset.byte_length,
            "Invalid template asset record"
        );
        let staged = StagedAsset {
            key: asset.key.clone(),
            path: staging.join(format!("{}.template-asset", uuid::Uuid::new_v4())),
        };
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&staged.path)?;
        let (length, hash) = digest(&mut entry, Some(&mut file), asset.byte_length)?;
        ensure!(
            length == asset.byte_length && hash == asset.sha256,
            "Template asset verification failed"
        );
        file.flush()?;
        assets.push(staged);
    }
    let poster_png = if let Some(expected) = &manifest.poster {
        let mut entry = zip.by_index(manifest.assets.len() + 1)?;
        ensure!(
            entry.name() == "poster.png" && entry.size() == expected.byte_length,
            "Invalid template poster record"
        );
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_POSTER_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let actual = poster_info(&bytes)?;
        ensure!(
            actual.byte_length == expected.byte_length
                && actual.sha256 == expected.sha256
                && actual.width == expected.width
                && actual.height == expected.height,
            "Template poster verification failed"
        );
        Some(bytes)
    } else {
        None
    };
    let mut video_caches = Vec::new();
    let cache_offset = manifest.assets.len() + 1 + usize::from(manifest.poster.is_some());
    for (index, cache) in manifest.video_caches.iter().enumerate() {
        let mut entry = zip.by_index(cache_offset + index)?;
        ensure!(
            entry.name() == format!("video-caches/{}", cache.key)
                && entry.size() == cache.byte_length,
            "Invalid template video cache record",
        );
        let staged = StagedArchiveVideoCache::new(
            cache.key.clone(),
            staging.join(format!("{}.template-video-cache", uuid::Uuid::new_v4())),
        );
        let mut options = File::options();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(staged.path())?;
        let (length, hash) = digest(&mut entry, Some(&mut file), cache.byte_length)?;
        ensure!(
            length == cache.byte_length && hash == cache.sha256,
            "Template video cache checksum mismatch"
        );
        file.flush()?;
        video_caches.push(staged);
    }
    Ok(Parsed {
        manifest,
        assets,
        poster_png,
        video_caches,
    })
}
