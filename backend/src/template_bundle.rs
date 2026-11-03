//! Native template packages. Callers must authorize every input file before packing.
//! This codec never resolves a path supplied by an imported document.
use crate::domain::{
    composition::{CompositionTrack, SourceId},
    nested_composition::NestedCompositionProject,
    project_media::ProjectMediaItem,
};
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
const MAX_ASSETS: u64 = crate::project_archive::MAX_ARCHIVE_TOTAL_MEDIA_BYTES;
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub project: NestedCompositionProject,
    pub assets: Vec<Asset>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub key: SourceId,
    pub byte_length: u64,
    pub sha256: String,
}
pub struct Input {
    pub source_id: SourceId,
    pub path: PathBuf,
}
pub struct Plan {
    pub project: NestedCompositionProject,
    pub assets: BTreeMap<SourceId, SourceId>,
}
/// Replace physical bindings with package-local keys and erase slot bindings.
/// Original descriptors remain for editorial authoring; original slot bytes do not ship.
pub fn plan(project: &NestedCompositionProject) -> Result<Plan> {
    project.dependency_order()?;
    let mut next = project.clone();
    for node in project.compositions.values() {
        for track in &node.content.tracks {
            if let CompositionTrack::Video { clips, .. } = track {
                ensure!(
                    clips.iter().all(|clip| clip.concat_cutout.is_none()),
                    "Video mask stores require template cache packaging"
                );
            }
        }
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
    next.dependency_order()?;
    Ok(Plan {
        project: next,
        assets,
    })
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
        for track in &node.content.tracks {
            if let CompositionTrack::Video { clips, .. } = track {
                ensure!(
                    clips.iter().all(|clip| clip.concat_cutout.is_none()),
                    "Video mask stores require template cache packaging"
                );
            }
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
    Ok(zip.finish()?)
}
pub struct StagedAsset {
    pub key: SourceId,
    path: PathBuf,
}
impl StagedAsset {
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
        zip.len() == manifest.assets.len() + 1,
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
    Ok(Parsed { manifest, assets })
}
