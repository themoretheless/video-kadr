//! Pure assembly of a fully filled project. Callers own probing/ACL/publication.
use super::{validate, Manifest};
use crate::domain::{
    composition::{CompositionSource, SourceId},
    nested_composition::NestedCompositionProject,
    project_media_fill,
};
use anyhow::{ensure, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Server-probed replacement properties, never client assertions about a file.
pub struct SlotFill {
    pub replacement: Value,
    pub source: CompositionSource,
}

/// Resolve every design record and fill every placeholder before returning a
/// project. No mutation of the template or physical publication occurs here.
pub fn instantiate(
    manifest: &Manifest,
    design: &BTreeMap<SourceId, SourceId>,
    fills: &BTreeMap<SourceId, SlotFill>,
) -> Result<NestedCompositionProject> {
    validate(manifest)?;
    let expected: BTreeSet<_> = manifest.assets.iter().map(|asset| &asset.key).collect();
    ensure!(
        design.keys().collect::<BTreeSet<_>>() == expected,
        "Template design mapping must be complete"
    );
    ensure!(
        design.values().collect::<BTreeSet<_>>().len() == design.len(),
        "Distinct template assets need distinct library identities"
    );
    ensure!(
        design.values().all(|id| !expected.contains(id)),
        "Template design assets need newly allocated library identities"
    );
    let slots: BTreeSet<_> = manifest
        .project
        .project_media
        .iter()
        .filter(|(_, item)| item.placeholder)
        .map(|(id, _)| id)
        .collect();
    ensure!(
        fills.keys().collect::<BTreeSet<_>>() == slots,
        "Every template slot needs exactly one replacement"
    );
    let mut staged = manifest.project.clone();
    for item in staged.project_media.values_mut() {
        if !item.placeholder {
            item.library_source_id = Some(design[item.library_source_id.as_ref().unwrap()].clone());
        }
    }
    for (id, fill) in fills {
        staged = project_media_fill::fill(&staged, id, &fill.replacement, &fill.source)?;
    }
    ensure!(
        staged.project_media.values().all(|item| !item.placeholder),
        "Template has unfilled slots"
    );
    staged.dependency_order()?;
    // Also validate the physical inventory with the exact saved-project rules.
    crate::db::extract_document_source_ids(&serde_json::to_value(&staged)?)?;
    Ok(staged)
}
