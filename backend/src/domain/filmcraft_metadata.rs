//! Filmcraft item metadata edits (MIT, pinned 52318524).
use super::{
    filmcraft_labels::{clip_paths, LABELS},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::{exact_numbers, id},
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const READ_ONLY: &[&str] = &[
    "Media Type",
    "Frame Rate",
    "Media Start",
    "Media End",
    "Media Duration",
    "In Point",
    "Out Point",
    "Video Info",
    "Video Codec",
    "Audio Info",
    "Audio Codec",
    "Color Space",
    "File Path",
    "File Size",
    "Format",
    "Video Bitrate",
    "Status",
    "Poster Frame",
    "Freeform Position",
    "Freeform Stack",
];

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct Selection {
    active_sequence: Option<Value>,
    project_selection: Vec<Value>,
    selection: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    item: Option<Value>,
    clip: Option<Value>,
    field: Option<String>,
    value: Option<Value>,
    #[serde(default)]
    fields: BTreeMap<String, Value>,
    #[serde(default)]
    context: Selection,
}

fn clip_item(document: &SourceProjectDocument, sequence: Option<u64>, clip: u64) -> Result<u64> {
    let (_, path) = clip_paths(document, sequence)
        .into_iter()
        .find(|(id, _)| *id == clip)
        .context("metadata clip not found in active sequence")?;
    document
        .envelope
        .pointer(&path)
        .and_then(|clip| clip["item"].as_u64())
        .context("metadata clip has no item")
}

pub fn run(command: &str, params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let sequence = id(request.context.active_sequence)?
        .or_else(|| document.envelope["project"]["active_sequence"].as_u64());
    let target = if let Some(item) = id(request.item)? {
        item
    } else if let Some(clip) = id(request.clip)? {
        clip_item(&document, sequence, clip)?
    } else if let Some(item) = request.context.project_selection.into_iter().next() {
        id(Some(item))?.context("missing metadata item")?
    } else {
        let clip =
            id(request.context.selection.into_iter().next())?.context("select a metadata item")?;
        clip_item(&document, sequence, clip)?
    };
    let index = ProjectIndex::new(&document)?;
    let original = index.item(target).context("metadata item not found")?;
    if command == "metadata.get" {
        let usage = super::filmcraft_project_sort::usage(&index);
        let fields = super::filmcraft_project_sort::keys(
            &index,
            target,
            original,
            usage.get(&target).copied().unwrap_or_default(),
        );
        let mut result = json!({"item":target,"name":original["name"],"label":original["label"],"fields":fields,"metadata":fields["metadata"]});
        exact_numbers(&mut result);
        return Ok(json!({"result":result}));
    }
    let mut edited = original.clone();
    let mut changes = Vec::new();
    if let Some(field) = request.field {
        changes.push((field, request.value.unwrap_or_else(|| json!(""))));
    }
    changes.extend(request.fields);
    ensure!(!changes.is_empty(), "metadata fields are required");
    for (key, value) in changes {
        ensure!(!key.trim().is_empty(), "empty metadata field");
        ensure!(
            !READ_ONLY.iter().any(|name| name.eq_ignore_ascii_case(&key))
                && !key
                    .to_ascii_lowercase()
                    .starts_with("freeform arrangement: "),
            "metadata field {key} is read-only"
        );
        let value = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        if key.eq_ignore_ascii_case("name") {
            ensure!(!value.trim().is_empty(), "metadata name cannot be empty");
            edited["name"] = json!(value.trim());
        } else if key.eq_ignore_ascii_case("label") {
            let label = LABELS
                .iter()
                .find(|label| label.eq_ignore_ascii_case(value.trim()))
                .context("unknown metadata label")?;
            edited["label"] = json!(label);
        } else if value.is_empty() {
            if let Some(metadata) = edited["metadata"].as_object_mut() {
                metadata.remove(&key);
            }
        } else {
            if edited["metadata"].is_null() {
                edited["metadata"] = json!({});
            }
            edited["metadata"]
                .as_object_mut()
                .context("invalid item metadata")?
                .insert(key, json!(value));
        }
    }
    let changed = edited != *original;
    let mut result = json!({"item":target,"name":edited["name"],"label":edited["label"],"metadata":edited["metadata"],"changed":changed});
    exact_numbers(&mut result);
    if changed {
        document.envelope["project"]["items"][target.to_string()] = edited;
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(
        json!({"result":result,"document":document.envelope,"documentText":text,"metadataEdit":changed}),
    )
}
