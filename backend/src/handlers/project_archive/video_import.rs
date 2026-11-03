//! Fully verify uploaded stores before granting independent target ownership.
use super::PreparedSource;
use crate::{
    domain::concat_video_binding::Binding,
    handlers::concat_video_cache::{self, PreparedTransfer},
    project_archive::{ArchivedVideoCache, StagedArchiveVideoCache},
    state::AppState,
};
use anyhow::{ensure, Result};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf};
use tokio_util::sync::CancellationToken;
pub(super) struct Imported {
    pub registrations: Vec<PreparedTransfer>,
    directories: Vec<PathBuf>,
    committed: bool,
}
impl Imported {
    pub fn commit(&mut self) {
        self.committed = true;
    }
}
impl Drop for Imported {
    fn drop(&mut self) {
        if !self.committed {
            for directory in &self.directories {
                let _ = std::fs::remove_dir_all(directory);
            }
        }
    }
}
pub(super) fn remove_origins(document: &mut Value) {
    let contents: Vec<_> = if let Some(nodes) = document
        .get_mut("compositions")
        .and_then(Value::as_object_mut)
    {
        nodes
            .values_mut()
            .filter_map(|n| n.get_mut("content"))
            .collect()
    } else {
        vec![document]
    };
    for content in contents {
        if let Some(tracks) = content.get_mut("tracks").and_then(Value::as_array_mut) {
            for track in tracks {
                if track.get("kind").and_then(Value::as_str) != Some("video") {
                    continue;
                }
                if let Some(clips) = track.get_mut("clips").and_then(Value::as_array_mut) {
                    for clip in clips {
                        if let Some(binding) =
                            clip.get_mut("concatCutout").and_then(Value::as_object_mut)
                        {
                            binding.remove("origin");
                        }
                    }
                }
            }
        }
    }
}
pub(super) fn prepare<'a>(
    state: &'a AppState,
    actor: &'a str,
    old_document: &'a Value,
    document: &'a Value,
    mapping: &'a BTreeMap<String, String>,
    media: &'a [PreparedSource],
    entries: &'a [ArchivedVideoCache],
    payloads: &'a [StagedArchiveVideoCache],
    cancel: &'a CancellationToken,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Imported>> + Send + 'a>> {
    Box::pin(async move {
        let mut imported = Imported {
            registrations: vec![],
            directories: vec![],
            committed: false,
        };
        ensure!(
            entries.len() == payloads.len(),
            "archive cache payload count changed"
        );
        if entries.is_empty() {
            return Ok(imported);
        }
        let _permit =
            crate::handlers::concat_cutout_analyse::acquire_analysis(state, cancel).await?;
        if document.get("compositions").is_none() {
            serde_json::from_value::<crate::domain::composition::Composition>(document.clone())?
                .validate_native_surface()?;
        } else {
            crate::db::extract_document_source_ids(document)?;
        }
        let paths: BTreeMap<_, _> = media
            .iter()
            .map(|source| (source.new_id.clone(), source.staged.path().to_owned()))
            .collect();
        let contents: Vec<_> =
            if let Some(nodes) = document.get("compositions").and_then(Value::as_object) {
                nodes.values().filter_map(|n| n.get("content")).collect()
            } else {
                vec![document]
            };
        let clips: Vec<_> = contents
            .iter()
            .flat_map(|content| {
                content
                    .get("tracks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter(|track| track.get("kind").and_then(Value::as_str) == Some("video"))
            .flat_map(|track| {
                track
                    .get("clips")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .collect();
        let root = state.storage.join("concat-video-caches");
        tokio::fs::create_dir_all(&root).await?;
        for (entry, payload) in entries.iter().zip(payloads) {
            ensure!(entry.key == payload.key(), "archive cache key changed");
            let bound = old_document
                .get("projectMedia")
                .and_then(|items| items.get(&entry.source_id))
                .and_then(|item| item.get("librarySourceId"))
                .is_some_and(Value::is_string);
            let source = if bound {
                entry.source_id.clone()
            } else {
                mapping
                    .get(&entry.source_id)
                    .ok_or_else(|| anyhow::anyhow!("archive video source mapping missing"))?
                    .clone()
            };
            let physical = crate::handlers::concat_cutout_access::library_id(document, &source)?;
            let original = paths
                .get(&physical)
                .ok_or_else(|| anyhow::anyhow!("archive original bytes missing"))?;
            let mut regions = vec![];
            let mut matched = false;
            for clip in &clips {
                if clip.get("sourceId").and_then(Value::as_str) != Some(source.as_str()) {
                    continue;
                }
                let Some(raw) = clip.get("concatCutout").filter(|value| !value.is_null()) else {
                    continue;
                };
                let binding: Binding = serde_json::from_value(raw.clone())?;
                if binding.source_hash != entry.identity.video_hash
                    || binding.model_hash != entry.identity.model_hash
                    || binding.runtime_hash != entry.identity.runtime_hash
                {
                    continue;
                }
                matched = true;
                for track in binding.region_tracks {
                    let library = crate::handlers::concat_cutout_access::library_id(
                        document,
                        &track.snapshot.mask_source_id,
                    )?;
                    let path = paths
                        .get(&library)
                        .ok_or_else(|| anyhow::anyhow!("archived region bytes missing"))?;
                    regions.push((track.snapshot, library, path.clone()));
                }
            }
            ensure!(matched, "archive video binding lost during relink");
            let mut portable = crate::services::concat_video_analysis::portable::restore(
                tokio::fs::File::open(payload.path()).await?,
                &entry.identity,
                &state.staging_dir(),
                entry.byte_length,
                cancel,
            )
            .await?;
            let receipt = portable
                .publish(
                    &root,
                    crate::project_archive::MAX_ARCHIVE_ENTRY_BYTES,
                    cancel,
                )
                .await?;
            imported.directories.push(receipt.directory.clone());
            let registration = concat_video_cache::prepare_archive(
                state,
                actor,
                &source,
                document,
                original,
                &receipt,
                &entry.identity,
                &regions,
                cancel,
            )
            .await?;
            imported.registrations.push(registration);
        }
        ensure!(!cancel.is_cancelled(), "archive cache import cancelled");
        Ok(imported)
    })
}
