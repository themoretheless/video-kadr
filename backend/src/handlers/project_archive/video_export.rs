//! Collect every saved video binding into bounded, self-contained archive records.
use super::TemporaryFile;
use crate::{
    db::CompositionProject,
    domain::{composition::CompositionTrack, nested_composition::NestedCompositionProject},
    project_archive::{
        ArchiveVideoCacheInput, ArchivedVideoCache, MAX_ARCHIVE_ENTRY_BYTES,
        MAX_ARCHIVE_TOTAL_MEDIA_BYTES,
    },
    state::AppState,
};
use anyhow::{ensure, Result};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub(super) struct Collected {
    pub document: Value,
    pub entries: Vec<ArchivedVideoCache>,
    pub inputs: Vec<ArchiveVideoCacheInput>,
    pub authorities: Vec<crate::handlers::concat_video_bound::SavedProject>,
    _files: Vec<TemporaryFile>,
}
pub(super) fn collect<'a>(
    state: &'a AppState,
    actor: &'a str,
    project: &'a CompositionProject,
    media_bytes: u64,
    cancel: &'a CancellationToken,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Collected>> + Send + 'a>> {
    Box::pin(async move {
        ensure!(
            media_bytes <= MAX_ARCHIVE_TOTAL_MEDIA_BYTES,
            "archive media exceeds aggregate limit"
        );
        let mut document = project.document.clone();
        let contents: Vec<_> =
            if let Some(nodes) = document.get("compositions").and_then(Value::as_object) {
                nodes.values().filter_map(|n| n.get("content")).collect()
            } else {
                vec![&document]
            };
        let any_binding = contents
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
            .any(|clip| {
                clip.get("concatCutout")
                    .is_some_and(|binding| !binding.is_null())
            });
        if !any_binding {
            return Ok(Collected {
                document,
                entries: vec![],
                inputs: vec![],
                authorities: vec![],
                _files: vec![],
            });
        }

        let mut native: NestedCompositionProject = if document.get("compositions").is_some() {
            serde_json::from_value(document.clone())?
        } else {
            serde_json::from_value(
                serde_json::json!({"schemaVersion":1,"root":"archive","compositions":{"archive":{"content":document,"instances":[]}}}),
            )?
        };
        native.dependency_order()?;
        let mut has_masks = false;
        for node in native.compositions.values_mut() {
            for track in &mut node.content.tracks {
                if let CompositionTrack::Video { clips, .. } = track {
                    for clip in clips {
                        has_masks |= clip.concat_cutout.is_some();
                        clip.enabled = true;
                    }
                }
            }
        }
        if !has_masks {
            return Ok(Collected {
                document,
                entries: vec![],
                inputs: vec![],
                authorities: vec![],
                _files: vec![],
            });
        }
        let _permit =
            crate::handlers::concat_cutout_analyse::acquire_analysis(state, cancel).await?;
        let context = crate::handlers::concat_video_bound::SavedProject {
            project_id: project.id.clone(),
            expected_revision: project.revision,
        };
        let mut masks = crate::handlers::concat_video_bound::BoundMasks::load(
            state,
            actor,
            Some(&context),
            &native,
            cancel,
        )
        .await?;
        let mut authorities = masks.authorities();
        if !authorities
            .iter()
            .any(|p| p.project_id == context.project_id)
        {
            authorities.push(context);
        }
        let mut entries = vec![];
        let mut inputs = vec![];
        let mut files = vec![];
        let mut remaining = MAX_ARCHIVE_TOTAL_MEDIA_BYTES - media_bytes;
        let mut seen = std::collections::BTreeMap::new();
        for (source, cache) in masks.archive_caches() {
            let identity = cache.portable_identity()?;
            let tuple = (
                source.as_str().to_owned(),
                identity.video_hash.clone(),
                identity.model_hash.clone(),
                identity.runtime_hash.clone(),
            );
            if let Some(old) = seen.insert(tuple, identity.cache_identity.clone()) {
                ensure!(
                    old == identity.cache_identity,
                    "conflicting archive stores for one video binding"
                );
                continue;
            }
            let file = TemporaryFile::new(
                state
                    .staging_dir()
                    .join(format!("{}.veproj-export-video", uuid::Uuid::new_v4())),
            );
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let output = options.open(file.path()).await?;
            cache
                .write_portable(output, remaining.min(MAX_ARCHIVE_ENTRY_BYTES), cancel)
                .await?;
            let (byte_length, sha256) =
                crate::project_archive::fingerprint_file(file.path()).await?;
            remaining = remaining
                .checked_sub(byte_length)
                .ok_or_else(|| anyhow::anyhow!("archive video budget exceeded"))?;
            let key = format!("video-cache-{}", entries.len());
            entries.push(ArchivedVideoCache {
                key: key.clone(),
                source_id: source.as_str().into(),
                identity,
                byte_length,
                sha256,
            });
            inputs.push(ArchiveVideoCacheInput {
                key,
                path: file.path().to_owned(),
            });
            files.push(file);
        }
        // Origin projects are read authorities, not portable source references.
        let contents: Vec<_> = if let Some(nodes) = document
            .get_mut("compositions")
            .and_then(Value::as_object_mut)
        {
            nodes
                .values_mut()
                .filter_map(|n| n.get_mut("content"))
                .collect()
        } else {
            vec![&mut document]
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
        crate::handlers::concat_video_bound::recheck_authorities(state, actor, &authorities)
            .await?;
        ensure!(!cancel.is_cancelled(), "archive video collection cancelled");
        Ok(Collected {
            document,
            entries,
            inputs,
            authorities,
            _files: files,
        })
    })
}
