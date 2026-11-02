//! Project-wide media records. Placeholder marking preserves creator media and
//! all timeline placements; filling/rebinding is a separate native operation.
use super::composition::{CompositionSource, SourceId, MAX_SOURCES};
use super::nested_composition::NestedCompositionProject;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMediaItem {
    pub source: CompositionSource,
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholder: bool,
    /// Retained Concat authoring properties; never file-path or render authority.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub concat_metadata: BTreeMap<String, serde_json::Value>,
}
const CONCAT_METADATA_FIELDS: &[&str] = &[
    "frameRate",
    "frameRateFraction",
    "videoCodec",
    "audioCodec",
    "audioTracks",
    "origin",
    "colorRange",
    "colorSpace",
];
pub fn capture_concat_metadata(item: &serde_json::Value) -> BTreeMap<String, serde_json::Value> {
    CONCAT_METADATA_FIELDS
        .iter()
        .filter_map(|key| {
            let value = item.get(*key)?;
            if value.is_null()
                || value.as_array().is_some_and(Vec::is_empty)
                || (*key == "colorSpace" && value == "sdr")
            {
                None
            } else {
                Some(((*key).to_owned(), value.clone()))
            }
        })
        .collect()
}
fn validate_concat_metadata(values: &BTreeMap<String, serde_json::Value>) -> Result<()> {
    ensure!(
        values
            .keys()
            .all(|key| CONCAT_METADATA_FIELDS.contains(&key.as_str())),
        "unknown Concat media property"
    );
    ensure!(
        serde_json::to_vec(values)?.len() <= 64 * 1024,
        "Concat media properties exceed 64 KiB"
    );
    ensure!(
        values
            .get("colorSpace")
            .map_or(true, |value| value == "sdr"),
        "native HDR source mapping is not implemented"
    );
    ensure!(
        values
            .get("colorRange")
            .map_or(true, serde_json::Value::is_null),
        "native source color-range mapping is not implemented"
    );
    Ok(())
}
pub fn validate(items: &BTreeMap<SourceId, ProjectMediaItem>) -> Result<()> {
    ensure!(items.len() <= MAX_SOURCES, "too many project media records");
    for (id, item) in items {
        ensure!(
            id == &item.source.id,
            "project media key and source ID differ"
        );
        ensure!(
            !item.name.trim().is_empty() && item.name.chars().count() <= 512,
            "invalid project media name"
        );
        item.source.validate()?;
        validate_concat_metadata(&item.concat_metadata)?;
    }
    Ok(())
}
/// Unknown targets and unchanged flags are no-ops, as SetMediaPlaceholder is.
/// A record remains in the bin after unmarking or removing its last placement.
pub fn set_placeholder(
    project: &NestedCompositionProject,
    id: &SourceId,
    placeholder: bool,
) -> Result<(NestedCompositionProject, bool)> {
    project.dependency_order()?;
    let old = project.project_media.get(id);
    if old.map_or(false, |entry| entry.placeholder) == placeholder {
        return Ok((project.clone(), false));
    }
    let entry = old.cloned().or_else(|| {
        project.compositions.values().find_map(|node| {
            let source = node.content.sources.get(id)?.clone();
            let name = node
                .content
                .metadata
                .as_ref()
                .and_then(|m| m.source_names.get(id))
                .filter(|name| !name.trim().is_empty() && name.chars().count() <= 512)
                .cloned()
                .unwrap_or_else(|| id.as_str().into());
            Some(ProjectMediaItem {
                source,
                name,
                placeholder: false,
                concat_metadata: Default::default(),
            })
        })
    });
    let Some(mut entry) = entry else {
        return Ok((project.clone(), false));
    };
    entry.placeholder = placeholder;
    let mut next = project.clone();
    next.project_media.insert(id.clone(), entry);
    next.dependency_order()?;
    Ok((next, true))
}
