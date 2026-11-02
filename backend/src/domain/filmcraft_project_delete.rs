//! Original project.delete (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_bin_edit::remove_item,
    filmcraft_bin_rename::locations,
    filmcraft_labels::{clip_paths, context_ids},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    source_item: Option<Value>,
    #[serde(default)]
    selection: Vec<Value>,
    #[serde(default)]
    project_selection: Vec<Value>,
    #[serde(default)]
    open_sequences: Vec<Value>,
    #[serde(default)]
    caption_selection: Vec<Value>,
    selected_mask: Option<Value>,
    timeline_views: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    items: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
fn item_ids(bin: &Value, ids: &mut BTreeSet<u64>) {
    for child in bin["children"].as_array().unwrap() {
        if let Some(id) = child["Item"].as_u64() {
            ids.insert(id);
        } else if let Some(b) = child.get("Bin") {
            item_ids(b, ids)
        }
    }
}
fn remove_bin(bin: &mut Value, id: u64) -> bool {
    let children = bin["children"].as_array_mut().unwrap();
    if let Some(at) = children
        .iter()
        .position(|entry| entry.pointer("/Bin/id").and_then(Value::as_u64) == Some(id))
    {
        children.remove(at);
        return true;
    }
    children
        .iter_mut()
        .filter_map(|child| child.get_mut("Bin"))
        .any(|b| remove_bin(b, id))
}
fn strings(ids: &[u64]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}
pub fn delete(params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    ProjectIndex::new(&doc)?;
    let bins = locations(&doc)?;
    let root = bins[0].id;
    let project = context_ids(r.context.project_selection)?;
    let explicit = r.items.as_ref().and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect::<Vec<_>>()
    });
    let named_item = explicit.as_ref().into_iter().flatten().any(|id| {
        doc.envelope["project"]["items"]
            .get(id.to_string())
            .is_some()
    });
    ensure!(
        !project.is_empty() || named_item,
        "select an item in the Project panel"
    );
    let asked = explicit.unwrap_or(project);
    let named: Vec<_> = asked
        .iter()
        .filter_map(|id| bins.iter().find(|bin| bin.id == *id && *id != root))
        .collect();
    let top: Vec<_> = named
        .iter()
        .copied()
        .filter(|b| {
            !named
                .iter()
                .any(|other| other.id != b.id && b.path.starts_with(&format!("{}/", other.path)))
        })
        .collect();
    let mut items: BTreeSet<_> = asked
        .into_iter()
        .filter(|id| !named.iter().any(|b| b.id == *id))
        .collect();
    for bin in &top {
        item_ids(doc.envelope.pointer(&bin.path).unwrap(), &mut items);
    }
    let removed_items: Vec<_> = items
        .iter()
        .copied()
        .filter(|id| {
            doc.envelope["project"]["items"]
                .get(id.to_string())
                .is_some()
        })
        .collect();
    for bin in &top {
        remove_bin(&mut doc.envelope["project"]["root"], bin.id);
    }
    for id in &items {
        doc.envelope["project"]["items"]
            .as_object_mut()
            .unwrap()
            .remove(&id.to_string());
        remove_item(&mut doc.envelope["project"]["root"], *id);
    }
    for item in doc.envelope["project"]["items"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        if let Some(q) = item.pointer_mut("/kind/Sequence") {
            for kind in ["video_tracks", "audio_tracks"] {
                for track in q[kind].as_array_mut().into_iter().flatten() {
                    track["items"]
                        .as_array_mut()
                        .context("missing track items")?
                        .retain(|clip| {
                            !clip["item"].as_u64().is_some_and(|id| items.contains(&id))
                        });
                    let ids: BTreeSet<_> = track["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|clip| clip["id"].as_u64())
                        .collect();
                    if let Some(transitions) = track["transitions"].as_array_mut() {
                        transitions.retain(|tr| {
                            let from = tr["from"].as_u64();
                            let to = tr["to"].as_u64();
                            from.is_none_or(|id| ids.contains(&id))
                                && to.is_none_or(|id| ids.contains(&id))
                                && (from.is_some() || to.is_some())
                        });
                    }
                }
            }
        }
    }
    let old_active = id(r.context.active_sequence)?;
    let mut active = old_active;
    let mut selection = context_ids(r.context.selection)?;
    let mut open = context_ids(r.context.open_sequences)?;
    let mut captions = context_ids(r.context.caption_selection)?;
    let mut source = id(r.context.source_item)?;
    let mask = if let Some(mask) = r.context.selected_mask {
        let clip = id(mask.get("clip").cloned())?.context("invalid selected mask clip")?;
        let effect = usize::try_from(
            mask["effect"]
                .as_u64()
                .context("invalid selected mask effect")?,
        )?;
        let m = usize::try_from(
            mask["mask"]
                .as_u64()
                .context("invalid selected mask index")?,
        )?;
        clip_paths(&doc, old_active)
            .iter()
            .find(|(id, _)| *id == clip)
            .and_then(|(_, path)| {
                doc.envelope.pointer(path)?["effects"]
                    .as_array()?
                    .get(effect)?["masks"]
                    .as_array()
            })
            .filter(|masks| m < masks.len())
            .map(|_| json!({"clip":clip.to_string(),"effect":effect,"mask":m}))
    } else {
        None
    };
    let index = ProjectIndex::new(&doc)?;
    let seqs: BTreeSet<_> = index.sequences().map(|(id, _)| id).collect();
    if active.is_some_and(|id| !seqs.contains(&id)) {
        active = seqs.first().copied();
    }
    open.retain(|id| seqs.contains(id));
    if source.is_some_and(|id| index.item(id).is_none()) {
        source = None;
    }
    let paths = clip_paths(&doc, active);
    selection.retain(|id| paths.iter().any(|(clip, _)| clip == id));
    let mut caption_ids = BTreeSet::new();
    if let Some(seq) = active
        .and_then(|id| index.item(id))
        .and_then(|item| item.pointer("/kind/Sequence"))
    {
        for track in seq["caption_tracks"].as_array().into_iter().flatten() {
            for caption in track["captions"].as_array().into_iter().flatten() {
                if let Some(id) = caption["id"].as_u64() {
                    caption_ids.insert(id);
                }
            }
        }
    }
    captions.retain(|id| caption_ids.contains(id));
    let mut views = BTreeMap::new();
    if let Some(v) = r.context.timeline_views {
        for (key, value) in v.as_object().context("invalid timeline views")? {
            let id = key.parse::<u64>().context("invalid timeline view ID")?;
            if seqs.contains(&id) {
                views.insert(id.to_string(), value.clone());
            }
        }
    }
    let remaining: BTreeSet<_> = locations(&doc)?.iter().map(|bin| bin.id).collect();
    let removed_bins: Vec<_> = bins
        .iter()
        .filter(|bin| !remaining.contains(&bin.id))
        .map(|bin| bin.id)
        .collect();
    Ok(
        json!({"result":{"items":items.len(),"bins":top.len()},"removedItemIds":strings(&removed_items),"removedBinIds":strings(&removed_bins),"context":{"activeSequence":active.map(|id|id.to_string()),"sourceItem":source.map(|id|id.to_string()),"selection":strings(&selection),"projectSelection":[],"openSequences":strings(&open),"captionSelection":strings(&captions),"selectedMask":mask,"timelineViews":views},"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}
