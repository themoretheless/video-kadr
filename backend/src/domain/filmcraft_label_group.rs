//! Original edit.selectLabelGroup (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! Selection query: the caller adopts returned context, never a document edit.
use super::{
    filmcraft_labels::{clip_paths, context_ids, LABELS},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    #[serde(default)]
    selection: Vec<Value>,
    #[serde(default)]
    project_selection: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    sequence: Option<Value>,
    #[serde(default)]
    context: SessionContext,
    #[serde(rename = "items")]
    _items: Option<Value>,
    #[serde(rename = "clips")]
    _clips: Option<Value>,
}
fn label(item: &Value) -> Result<String> {
    let label = item["label"].as_str().context("missing source label")?;
    ensure!(LABELS.contains(&label), "invalid source label");
    Ok(label.to_string())
}
fn tree_items(bin: &Value, all: &mut Vec<u64>) -> Result<()> {
    for child in bin["children"].as_array().context("invalid bin children")? {
        if let Some(id) = child["Item"].as_u64() {
            all.push(id)
        } else if let Some(bin) = child.get("Bin") {
            tree_items(bin, all)?
        }
    }
    Ok(())
}
pub fn select(params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let doc = SourceProjectDocument::read(&bytes)?;
    ProjectIndex::new(&doc)?;
    let seq = id(r.sequence)?.or(id(r.context.active_sequence)?);
    let mut selection = context_ids(r.context.selection)?;
    let mut project = context_ids(r.context.project_selection)?;
    ensure!(
        !selection.is_empty() || !project.is_empty(),
        "select clips or project items"
    );
    let result = if !selection.is_empty() {
        let seq = seq.context("no active sequence")?;
        ensure!(
            doc.envelope
                .pointer(&format!("/project/items/{seq}/kind/Sequence"))
                .is_some(),
            "no active sequence"
        );
        let paths = clip_paths(&doc, Some(seq));
        let mut labels = BTreeSet::new();
        for id in &selection {
            if let Some((_, path)) = paths.iter().find(|(clip, _)| clip == id) {
                labels.insert(label(doc.envelope.pointer(path).unwrap())?);
            }
        }
        selection = paths
            .into_iter()
            .filter_map(
                |(id, path)| match label(doc.envelope.pointer(&path).unwrap()) {
                    Ok(l) if labels.contains(&l) => Some(Ok(id)),
                    Ok(_) => None,
                    Err(e) => Some(Err(e)),
                },
            )
            .collect::<Result<Vec<_>>>()?;
        json!({"clips":selection.iter().map(|id|id.to_string()).collect::<Vec<_>>()})
    } else {
        let mut labels = BTreeSet::new();
        for id in &project {
            if let Some(item) = doc.envelope["project"]["items"].get(id.to_string()) {
                labels.insert(label(item)?);
            }
        }
        super::filmcraft_bin_rename::locations(&doc)?;
        let mut all = Vec::new();
        tree_items(&doc.envelope["project"]["root"], &mut all)?;
        project = all
            .into_iter()
            .filter_map(|id| {
                doc.envelope["project"]["items"]
                    .get(id.to_string())
                    .map(|item| (id, label(item)))
            })
            .filter_map(|(id, l)| match l {
                Ok(l) if labels.contains(&l) => Some(Ok(id)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>>>()?;
        json!({"items":project.iter().map(|id|id.to_string()).collect::<Vec<_>>()})
    };
    Ok(
        json!({"result":result,"context":{"selection":selection.iter().map(|id|id.to_string()).collect::<Vec<_>>(),"projectSelection":project.iter().map(|id|id.to_string()).collect::<Vec<_>>()}}),
    )
}
