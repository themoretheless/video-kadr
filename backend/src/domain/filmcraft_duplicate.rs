//! Filmcraft edit.duplicate (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_bin_rename::locations, filmcraft_labels::context_ids,
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    #[serde(default)]
    project_selection: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    #[serde(default)]
    context: SessionContext,
    #[serde(rename = "items")]
    _items: Option<Value>,
}
pub(super) fn nest_cycle(doc: &SourceProjectDocument) -> Result<Option<u64>> {
    let index = ProjectIndex::new(doc)?;
    let sequences: BTreeSet<_> = index.sequences().map(|(id, _)| id).collect();
    let mut graph = BTreeMap::new();
    for (id, q) in index.sequences() {
        let inner: BTreeSet<_> = ["video_tracks", "audio_tracks"]
            .into_iter()
            .flat_map(|kind| q[kind].as_array().into_iter().flatten())
            .flat_map(|track| track["items"].as_array().into_iter().flatten())
            .filter_map(|clip| clip["item"].as_u64())
            .filter(|id| sequences.contains(id))
            .collect();
        if !inner.is_empty() {
            graph.insert(id, inner);
        }
    }
    loop {
        let leaves: Vec<_> = graph
            .iter()
            .filter(|(_, inner)| inner.iter().all(|id| !graph.contains_key(id)))
            .map(|(id, _)| *id)
            .collect();
        if leaves.is_empty() {
            break;
        }
        for id in leaves {
            graph.remove(&id);
        }
    }
    for start in graph.keys() {
        let mut seen = BTreeSet::new();
        let mut todo: Vec<_> = graph[start].iter().copied().collect();
        while let Some(id) = todo.pop() {
            if id == *start {
                return Ok(Some(*start));
            }
            if seen.insert(id) {
                todo.extend(graph.get(&id).into_iter().flatten().copied());
            }
        }
    }
    Ok(None)
}
pub fn duplicate(params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let selected = context_ids(r.context.project_selection)?;
    ensure!(!selected.is_empty(), "select an item in the Project panel");
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    ProjectIndex::new(&doc)?;
    let bins = locations(&doc)?;
    let existing_cycle = nest_cycle(&doc)?;
    let mut out = Vec::new();
    let mut copies = Vec::new();
    for source in selected {
        // Resolve against the evolving project, as the original does: a later request
        // may name an ID created by an earlier iteration.
        let Some(item) = doc.envelope["project"]["items"]
            .get(source.to_string())
            .cloned()
        else {
            continue;
        };
        let next = doc.envelope["project"]["next_id"]
            .as_u64()
            .context("missing project next ID")?;
        let following = next.checked_add(1).context("project ID space exhausted")?;
        ensure!(
            !bins.iter().any(|bin| bin.id == next)
                && doc.envelope["project"]["items"]
                    .get(next.to_string())
                    .is_none(),
            "project next ID already allocated"
        );
        let name = item["name"].as_str().context("missing item name")?;
        let label = item["label"].as_str().context("missing item label")?;
        let copy = json!({"id":next,"name":format!("{name} Copy"),"label":label,"kind":item["kind"],"metadata":{},"created":next});
        doc.envelope["project"]["items"][next.to_string()] = copy;
        doc.envelope["project"]["root"]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!({"Item":next}));
        doc.envelope["project"]["next_id"] = json!(following);
        out.push(next.to_string());
        copies.push(json!({"id":next.to_string(),"sourceId":source.to_string()}));
    }
    if existing_cycle.is_none() {
        if let Some(id) = nest_cycle(&doc)? {
            let name = doc.envelope["project"]["items"][id.to_string()]["name"]
                .as_str()
                .unwrap_or_default();
            anyhow::bail!("a sequence cannot be nested inside itself (\u{201c}{name}\u{201d})");
        }
    }
    Ok(
        json!({"result":{"items":out},"context":{"projectSelection":out},"copies":copies,"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}
