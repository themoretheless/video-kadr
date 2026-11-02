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
