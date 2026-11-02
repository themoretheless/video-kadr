//! Filmcraft clip.rename snapshot semantics (MIT), source 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{filmcraft_project_file::SourceProjectDocument, filmcraft_project_inspect::id};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    #[serde(default)]
    selection: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    name: String,
    item: Option<Value>,
    clip: Option<Value>,
    sequence: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
// Source command IDs use u64_p: nonnumeric values are absent and numeric
// floats cast to u64. Decimal strings extend transport for exact large IDs.
fn target_id(value: Option<Value>) -> Result<Option<u64>> {
    match value {
        Some(Value::String(s)) => Ok(Some(s.parse().context("invalid source ID")?)),
        Some(v) => Ok(v.as_u64().or_else(|| v.as_f64().map(|v| v as u64))),
        None => Ok(None),
    }
}
pub fn rename(params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let item = target_id(r.item)?;
    let clip = target_id(r.clip)?.or(id(r.context.selection.into_iter().next())?);
    let sequence = id(r.sequence)?.or(id(r.context.active_sequence)?);
    if let Some(item) = item {
        let target = document
            .envelope
            .pointer_mut(&format!("/project/items/{item}"))
            .context("clip.rename: no such item")?;
        target["name"] = json!(r.name);
    } else if let (Some(clip), Some(sequence)) = (clip, sequence) {
        // Original find_item_mut includes locked audio/video tracks.
        let (path, _) = super::filmcraft_parameter_edit::location(&document, sequence, clip)?;
        document
            .envelope
            .pointer_mut(&path)
            .context("clip.rename: no such clip")?["name"] = json!(r.name);
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"result":null,"document":document.envelope,"documentText":text}))
}
