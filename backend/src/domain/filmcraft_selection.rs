//! Original matching/project selection commands (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_labels::{clip_paths, context_ids},
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
    clips: Option<Value>,
    clip: Option<Value>,
    items: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
fn array(v: Option<&Value>) -> Option<Vec<u64>> {
    v?.as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect()
    })
}
fn scalar(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .or_else(|| v.as_str()?.parse().ok())
}
fn strings(ids: &[u64]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}
pub fn select(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(
            command,
            "edit.selectAllMatching"
                | "project.selectAll"
                | "project.deselectAll"
                | "project.select"
        ),
        "unknown selection command"
    );
    let r: Request = serde_json::from_value(params)?;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let doc = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&doc)?;
    let mut selection = context_ids(r.context.selection)?;
    let mut project = context_ids(r.context.project_selection)?;
    let result = match command {
        "project.selectAll" => {
            project = index.items().map(|(id, _)| id).collect();
            json!({"selected":project.len()})
        }
        "project.deselectAll" => {
            project.clear();
            Value::Null
        }
        "project.select" => {
            project = array(r.items.as_ref()).unwrap_or_default();
            Value::Null
        }
        _ => {
            let sequence = id(r.sequence)?
                .or(id(r.context.active_sequence)?)
                .context("no active sequence")?;
            ensure!(
                doc.envelope
                    .pointer(&format!("/project/items/{sequence}/kind/Sequence"))
                    .is_some(),
                "no active sequence"
            );
            let paths = clip_paths(&doc, Some(sequence));
            let explicit = array(r.clips.as_ref());
            let named = explicit
                .as_ref()
                .into_iter()
                .flatten()
                .any(|id| paths.iter().any(|(clip, _)| clip == id));
            ensure!(!selection.is_empty() || named, "nothing selected");
            let seeds = explicit.unwrap_or_else(|| {
                scalar(r.clip.as_ref())
                    .map(|id| vec![id])
                    .unwrap_or_else(|| selection.clone())
            });
            let sources: BTreeSet<_> = seeds
                .iter()
                .filter_map(|id| paths.iter().find(|(clip, _)| clip == id))
                .filter_map(|(_, path)| doc.envelope.pointer(path)?["item"].as_u64())
                .collect();
            selection = paths
                .iter()
                .filter_map(|(id, path)| {
                    doc.envelope.pointer(path)?["item"]
                        .as_u64()
                        .filter(|source| sources.contains(source))
                        .map(|_| *id)
                })
                .collect();
            json!({"clips":strings(&selection)})
        }
    };
    Ok(
        json!({"result":result,"context":{"selection":strings(&selection),"projectSelection":strings(&project)}}),
    )
}
