//! Verify imported origins before saving; registration is part of the DB save transaction.
use super::concat_video_cache::{self as cache, PreparedTransfer, ProjectSource, RegionTransfer};
use crate::{domain::concat_video_binding::Binding, state::AppState};
use anyhow::{ensure, Result};
use serde_json::Value;

pub(crate) fn has_origins(document: &Value) -> bool {
    let contents: Vec<_> =
        if let Some(nodes) = document.get("compositions").and_then(Value::as_object) {
            nodes.values().filter_map(|n| n.get("content")).collect()
        } else {
            vec![document]
        };
    contents.iter().any(|content| {
        content
            .get("tracks")
            .and_then(Value::as_array)
            .is_some_and(|tracks| {
                tracks.iter().any(|t| {
                    t.get("kind").and_then(Value::as_str) == Some("video")
                        && t.get("clips")
                            .and_then(Value::as_array)
                            .is_some_and(|clips| {
                                clips.iter().any(|c| {
                                    c.get("concatCutout")
                                        .and_then(|b| b.get("origin"))
                                        .is_some_and(|o| !o.is_null())
                                })
                            })
                })
            })
    })
}
pub(crate) fn prepare<'a>(
    state: &'a AppState,
    actor: &'a str,
    document: &'a Value,
    cancel: &'a tokio_util::sync::CancellationToken,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<(Value, Vec<PreparedTransfer>)>> + Send + 'a>,
> {
    Box::pin(async move {
        let mut final_document = document.clone();
        let mut requests = vec![];
        let contents: Vec<_> = if let Some(nodes) = final_document
            .get_mut("compositions")
            .and_then(Value::as_object_mut)
        {
            nodes
                .values_mut()
                .filter_map(|n| n.get_mut("content"))
                .collect()
        } else {
            vec![&mut final_document]
        };
        for content in contents {
            let Some(tracks) = content.get_mut("tracks").and_then(Value::as_array_mut) else {
                continue;
            };
            for track in tracks {
                if track.get("kind").and_then(Value::as_str) != Some("video") {
                    continue;
                }
                let Some(clips) = track.get_mut("clips").and_then(Value::as_array_mut) else {
                    continue;
                };
                for clip in clips {
                    let source = clip
                        .get("sourceId")
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("video source missing"))?
                        .to_owned();
                    let Some(raw) = clip
                        .get_mut("concatCutout")
                        .filter(|b| b.get("origin").is_some_and(|o| !o.is_null()))
                    else {
                        continue;
                    };
                    let binding: Binding = serde_json::from_value(raw.clone())?;
                    binding.validate()?;
                    raw.as_object_mut()
                        .ok_or_else(|| anyhow::anyhow!("video binding missing"))?
                        .remove("origin");
                    requests.push((source, binding));
                }
            }
        }
        let mut transfers = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for (source, binding) in requests {
            let origin = binding.origin.as_ref().unwrap();
            let identity = serde_json::to_string(&(&source, &binding))?;
            if !seen.insert(identity) {
                continue;
            }
            let regions = binding
                .region_tracks
                .iter()
                .map(|track| {
                    ensure!(
                        track.snapshot.source_id == source,
                        "imported region source mismatch"
                    );
                    let mut snapshot = track.snapshot.clone();
                    snapshot.source_id = origin.source_id.clone();
                    snapshot.mask_source_id = origin
                        .region_sources
                        .get(&snapshot.mask_source_id)
                        .cloned()
                        .unwrap_or(snapshot.mask_source_id);
                    Ok(RegionTransfer {
                        snapshot,
                        target_mask_source_id: track.snapshot.mask_source_id.clone(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let prepared = cache::prepare_transfer(
                state,
                actor,
                ProjectSource {
                    project: &origin.project_id,
                    source_id: &origin.source_id,
                    revision: origin.expected_revision,
                },
                &source,
                &final_document,
                &binding.runtime_hash,
                &binding.model_hash,
                &regions,
                cancel,
            )
            .await?;
            ensure!(
                prepared.video_hash() == binding.source_hash,
                "imported source hash differs from verified store"
            );
            transfers.push(prepared);
        }
        ensure!(!cancel.is_cancelled(), "video save cancelled");
        Ok((final_document, transfers))
    })
}
