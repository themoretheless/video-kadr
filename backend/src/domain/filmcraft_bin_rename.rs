//! Filmcraft project.renameBin semantics (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{filmcraft_clip_rename::target_id, filmcraft_project_file::SourceProjectDocument};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub(super) struct BinLocation {
    pub(super) id: u64,
    pub(super) path: String,
    pub(super) name: String,
    pub(super) parent: Option<u64>,
    depth: usize,
}
pub(super) fn locations(document: &SourceProjectDocument) -> Result<Vec<BinLocation>> {
    let mut pending = vec![("/project/root".to_string(), None, 0)];
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    while let Some((path, parent, depth)) = pending.pop() {
        let bin = document
            .envelope
            .pointer(&path)
            .context("missing project bin")?;
        let id = bin["id"].as_u64().context("invalid project bin ID")?;
        ensure!(seen.insert(id), "ambiguous project bin ID");
        let name = bin["name"]
            .as_str()
            .context("invalid project bin name")?
            .to_string();
        let children = bin["children"]
            .as_array()
            .context("invalid project bin children")?;
        for (index, child) in children.iter().enumerate().rev() {
            let entry = child.as_object().context("invalid project bin entry")?;
            ensure!(entry.len() == 1, "invalid project bin entry");
            if entry.contains_key("Bin") {
                pending.push((format!("{path}/children/{index}/Bin"), Some(id), depth + 1));
            } else {
                ensure!(
                    entry.get("Item").and_then(Value::as_u64).is_some(),
                    "invalid project bin entry"
                );
            }
        }
        result.push(BinLocation {
            id,
            path,
            name,
            parent,
            depth,
        });
    }
    Ok(result)
}
pub fn list(document: &SourceProjectDocument) -> Result<Value> {
    Ok(Value::Array(locations(document)?.iter().map(|b|json!({"id":b.id.to_string(),"name":b.name,"parentId":b.parent.map(|v|v.to_string()),"depth":b.depth,"itemIds":document.envelope.pointer(&b.path).unwrap()["children"].as_array().unwrap().iter().filter_map(|child|child.get("Item").and_then(Value::as_u64).map(|id|id.to_string())).collect::<Vec<_>>()})).collect()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    bin: Option<Value>,
    name: String,
}
pub fn rename(params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let bin = target_id(r.bin)?.context("project.renameBin: need bin")?;
    let name = r.name.trim();
    ensure!(!name.is_empty(), "project.renameBin: need a non-empty name");
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let bins = locations(&document)?;
    ensure!(
        bins.first().map(|b| b.id) != Some(bin),
        "project.renameBin: the project's root bin is named after the project"
    );
    let path = &bins
        .iter()
        .find(|b| b.id == bin)
        .context("project.renameBin: no such bin")?
        .path;
    document.envelope.pointer_mut(path).unwrap()["name"] = json!(name);
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(
        json!({"result":{"bin":bin.to_string(),"name":name},"document":document.envelope,"documentText":text}),
    )
}
