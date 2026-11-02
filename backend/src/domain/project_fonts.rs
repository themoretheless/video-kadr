//! Font operations across stored composition assets, independently of instance count.
use super::{
    composition_font_replace::{self, FontSelection},
    composition_fonts::{self, FontUsage},
    nested_composition::NestedCompositionProject,
};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

/// Every stored asset contributes once, including currently unreferenced compositions.
pub fn fonts_used(project: &NestedCompositionProject) -> Result<Vec<FontUsage>> {
    project.dependency_order()?;
    let mut combined: BTreeMap<(String, String), FontUsage> = BTreeMap::new();
    for document in project.compositions.values() {
        for usage in composition_fonts::fonts_used(&document.content)? {
            let key = (
                usage.family.to_ascii_lowercase(),
                usage.style.to_ascii_lowercase(),
            );
            if let Some(previous) = combined.get_mut(&key) {
                previous.count = previous
                    .count
                    .checked_add(usage.count)
                    .ok_or_else(|| anyhow::anyhow!("font dependency count overflow"))?;
            } else {
                combined.insert(key, usage);
            }
        }
    }
    Ok(combined.into_values().collect())
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub project: NestedCompositionProject,
    pub replaced: usize,
}
/// Clone first so one locked or invalid asset refuses the entire operation.
pub fn replace_fonts(
    project: &NestedCompositionProject,
    from: &FontSelection,
    to: &FontSelection,
) -> Result<Replacement> {
    project.dependency_order()?;
    let mut result = project.clone();
    let mut replaced = 0_usize;
    for document in result.compositions.values_mut() {
        let changed = composition_font_replace::replace_fonts(&document.content, from, to)?;
        document.content = changed.composition;
        replaced = replaced
            .checked_add(changed.replaced)
            .ok_or_else(|| anyhow::anyhow!("font replacement count overflow"))?;
    }
    result.dependency_order()?;
    Ok(Replacement {
        project: result,
        replaced,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        composition::{
            AnimatableValue, BlendMode, CanvasSpec, Composition, CompositionTrack, TransformSpec,
        },
        composition_graph::CompositionReferenceTiming,
        nested_composition::{CompositionInstance, NestedCompositionDocument},
    };
    fn fixture() -> NestedCompositionProject {
        let mut content = crate::domain::composition_fonts::tests::hidden_graphic_composition();
        if let CompositionTrack::Graphic { locked, .. } = &mut content.tracks[0] {
            *locked = false;
        }
        let child = NestedCompositionDocument {
            duration_us: Some(1_000_000),
            content,
            instances: vec![],
        };
        let mut root = NestedCompositionDocument {
            duration_us: Some(1_000_000),
            content: Composition::new(CanvasSpec::default()),
            instances: vec![],
        };
        for id in ["first", "second"] {
            root.instances.push(CompositionInstance {
                id: id.into(),
                timing: CompositionReferenceTiming {
                    composition: "child".into(),
                    timeline_start_us: 0,
                    source_in_us: 0,
                    source_out_us: 1_000_000,
                },
                animation_offset_us: 0,
                transform: TransformSpec::default(),
                opacity: AnimatableValue::constant(1.),
                blend_mode: BlendMode::Normal,
            });
        }
        NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts: vec![],
            schema_version: 1,
            root: "root".into(),
            compositions: BTreeMap::from([
                ("root".into(), root),
                ("child".into(), child.clone()),
                ("unused".into(), child),
            ]),
        }
    }
    #[test]
    fn shared_and_unused_assets_are_counted_once_and_replaced_atomically() {
        let project = fixture();
        let before = project.clone();
        let fonts = fonts_used(&project).unwrap();
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].count, 4);
        let from = FontSelection::Family("Inter".into());
        let to = FontSelection::Family("External".into());
        let result = replace_fonts(&project, &from, &to).unwrap();
        assert_eq!(result.replaced, 4);
        assert_eq!(fonts_used(&result.project).unwrap()[0].family, "External");
        assert_eq!(
            result.project.compositions["root"].instances,
            project.compositions["root"].instances
        );
        assert_eq!(project, before);
        let mut locked = project.clone();
        if let CompositionTrack::Graphic { locked, .. } = &mut locked
            .compositions
            .get_mut("unused")
            .unwrap()
            .content
            .tracks[0]
        {
            *locked = true;
        }
        let original = locked.clone();
        assert!(replace_fonts(&locked, &from, &to).is_err());
        assert_eq!(locked, original);
    }
}
