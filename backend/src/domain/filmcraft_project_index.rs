//! Borrowed source-project relationships used before native sequence conversion.
//! Retains complete source nodes; does not admit a project into rendering.
use super::filmcraft_project_file::SourceProjectDocument;
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

pub struct ProjectIndex<'a> {
    items: BTreeMap<u64, &'a Value>,
}
#[derive(Debug)]
pub struct ResolvedMedia<'a> {
    pub item_id: u64,
    pub media: &'a Value,
    /// The outermost subclip range, exactly as authored on the source clock.
    pub range: Option<&'a Value>,
}
impl<'a> ProjectIndex<'a> {
    pub fn new(document: &'a SourceProjectDocument) -> Result<Self> {
        let source = document
            .project()
            .get("items")
            .and_then(Value::as_object)
            .context("missing Filmcraft project items")?;
        let mut items = BTreeMap::new();
        for (key, value) in source {
            let id = key.parse::<u64>().context("invalid Filmcraft item ID")?;
            ensure!(
                value.get("id").and_then(Value::as_u64) == Some(id),
                "Filmcraft item ID differs from its key"
            );
            let kind = value
                .get("kind")
                .and_then(Value::as_object)
                .context("invalid Filmcraft item kind")?;
            ensure!(
                kind.len() == 1
                    && kind.keys().all(|k| matches!(
                        k.as_str(),
                        "Media" | "Sequence" | "Subclip" | "Graphic" | "AdjustmentLayer"
                    )),
                "unknown Filmcraft item kind"
            );
            ensure!(
                kind.values().all(Value::is_object),
                "invalid Filmcraft item payload"
            );
            ensure!(
                items.insert(id, value).is_none(),
                "duplicate Filmcraft item ID"
            );
        }
        Ok(Self { items })
    }
    /// All validated project items, in numeric source-ID order.
    pub fn items(&self) -> impl Iterator<Item = (u64, &'a Value)> + '_ {
        self.items.iter().map(|(id, item)| (*id, *item))
    }
    pub fn item(&self, id: u64) -> Option<&'a Value> {
        self.items.get(&id).copied()
    }
    pub fn sequences(&self) -> impl Iterator<Item = (u64, &'a Value)> + '_ {
        self.items.iter().filter_map(|(id, item)| {
            item.pointer("/kind/Sequence")
                .map(|sequence| (*id, sequence))
        })
    }
    /// Match original Project::resolve_media's sixteen-node bound and outer range.
    pub fn resolve_media(&self, mut id: u64) -> Option<ResolvedMedia<'a>> {
        let mut range = None;
        for _ in 0..16 {
            let kind = self.item(id)?.get("kind")?;
            if let Some(media) = kind.get("Media") {
                return Some(ResolvedMedia {
                    item_id: id,
                    media,
                    range,
                });
            }
            let subclip = kind.get("Subclip")?;
            range = range.or(Some(subclip.get("range")?));
            id = subclip.get("parent")?.as_u64()?;
        }
        None
    }
    /// Graphic/adjustment/nested sequence dimensions and bounded subclip ancestry.
    pub fn source_size(&self, mut id: u64) -> Option<(u32, u32)> {
        for _ in 0..16 {
            let kind = self.item(id)?.get("kind")?;
            if let Some(subclip) = kind.get("Subclip") {
                id = subclip.get("parent")?.as_u64()?;
                continue;
            }
            let size = if let Some(media) = kind.get("Media") {
                media.pointer("/info/video")?
            } else if let Some(sequence) = kind.get("Sequence") {
                sequence.get("settings")?
            } else {
                kind.get("Graphic")
                    .or_else(|| kind.get("AdjustmentLayer"))?
            };
            return Some((
                u32::try_from(size.get("width")?.as_u64()?).ok()?,
                u32::try_from(size.get("height")?.as_u64()?).ok()?,
            ));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_source_resolution_and_sequence_index_preserve_authoring_nodes() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-project-index.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let document =
            SourceProjectDocument::read(&serde_json::to_vec(&fixture["document"]).unwrap())
                .unwrap();
        let index = ProjectIndex::new(&document).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 26);
        for case in cases {
            let id = case["id"].as_u64().unwrap();
            assert_eq!(
                serde_json::to_value(index.source_size(id)).unwrap(),
                case["size"],
                "size {id}"
            );
            let resolved = index
                .resolve_media(id)
                .map(|r| serde_json::json!({"item":r.item_id,"media":r.media,"range":r.range}));
            assert_eq!(
                serde_json::to_value(resolved).unwrap(),
                case["resolved"],
                "media {id}"
            );
        }
        let sequences: Vec<u64> = index.sequences().map(|(id, _)| id).collect();
        assert_eq!(
            serde_json::to_value(sequences).unwrap(),
            fixture["sequences"]
        );
        let restored = SourceProjectDocument::read(&document.encode_preserved().unwrap()).unwrap();
        assert_eq!(restored.envelope, document.envelope);
        let mut corrupt = document.clone();
        let item = corrupt.envelope["project"]["items"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        item["id"] = serde_json::json!(9999);
        assert!(ProjectIndex::new(&corrupt).is_err());
    }
}
