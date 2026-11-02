//! Project inspection adapted from Filmcraft commands.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Session context is explicit.
use super::{filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    #[serde(default)]
    context: SessionContext,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    path: Option<String>,
    #[serde(default)]
    dirty: bool,
    #[serde(default)]
    revision: u64,
    active_sequence: Option<Value>,
    source_item: Option<Value>,
}
pub(super) fn id(value: Option<Value>) -> Result<Option<u64>> {
    value
        .filter(|v| !v.is_null())
        .map(|v| match v {
            Value::String(s) => s.parse().context("invalid inspection context ID"),
            v => v.as_u64().context("invalid inspection context ID"),
        })
        .transpose()
}
fn duration(item: &Value) -> Result<i64> {
    let kind = item["kind"].as_object().context("invalid item kind")?;
    let (name, value) = kind.iter().next().context("missing item kind")?;
    Ok(match name.as_str() {
        "Media" => value["info"]["duration"]
            .as_i64()
            .context("missing media duration")?,
        "Subclip" => value["range"]["duration"]
            .as_i64()
            .context("missing subclip duration")?,
        "AdjustmentLayer" => value["duration"]
            .as_i64()
            .context("missing adjustment duration")?,
        "Graphic" => 3600 * 254_016_000_000,
        "Sequence" => {
            let mut end = 0;
            for (tracks, items) in [
                ("video_tracks", "items"),
                ("audio_tracks", "items"),
                ("caption_tracks", "captions"),
            ] {
                for track in value[tracks].as_array().into_iter().flatten() {
                    for clip in track[items].as_array().into_iter().flatten() {
                        let start = clip["start"].as_i64().context("missing timeline start")?;
                        let duration = clip["duration"]
                            .as_i64()
                            .context("missing timeline duration")?;
                        end = end.max(
                            start
                                .checked_add(duration)
                                .context("inspection duration overflow")?,
                        );
                    }
                }
            }
            end
        }
        _ => anyhow::bail!("unsupported project item kind"),
    })
}
fn type_label(item: &Value) -> Result<&'static str> {
    let kind = item["kind"].as_object().context("missing item kind")?;
    let (name, value) = kind.iter().next().context("empty item kind")?;
    Ok(match name.as_str() {
        "Media" => match value["info"]["kind"]
            .as_str()
            .context("missing media kind")?
        {
            "Movie" => "Movie",
            "AudioOnly" => "Audio",
            "Still" => "Still Image",
            "ImageSequence" => "Image Sequence",
            "Synthetic" => "Synthetic",
            _ => anyhow::bail!("unknown media kind"),
        },
        "Sequence" if !value["multicam"].is_null() => "Multi-Camera Source Sequence",
        "Sequence" if !value["merged"].is_null() => "Merged Clip",
        "Sequence" => "Sequence",
        "Subclip" => "Subclip",
        "AdjustmentLayer" => "Adjustment Layer",
        "Graphic" => "Graphic",
        _ => anyhow::bail!("unknown item kind"),
    })
}
fn bin(value: &Value, index: &ProjectIndex<'_>, depth: usize, nodes: &mut usize) -> Result<Value> {
    ensure!(
        depth <= 64 && *nodes < 100_000,
        "project inspection tree exceeds budget"
    );
    *nodes += 1;
    let mut children = Vec::new();
    for child in value["children"]
        .as_array()
        .context("missing bin children")?
    {
        ensure!(
            child.as_object().is_some_and(|o| o.len() == 1),
            "invalid bin entry"
        );
        if let Some(id) = child.get("Item") {
            let id = id.as_u64().context("invalid bin item ID")?;
            let item = index.item(id);
            children.push(if let Some(item)=item {json!({"item":id,"name":item["name"],"type":type_label(item)?,"duration":duration(item)?,"label":item["label"]})} else {json!({"item":id,"name":null,"type":null,"duration":null,"label":null})});
            *nodes += 1;
            ensure!(*nodes <= 100_000, "project inspection tree exceeds budget");
        } else if let Some(value) = child.get("Bin") {
            children.push(bin(value, index, depth + 1, nodes)?);
        } else {
            anyhow::bail!("invalid bin entry");
        }
    }
    ensure!(
        value["id"].as_u64().is_some() && value["name"].is_string(),
        "invalid bin identity"
    );
    Ok(json!({"id":value["id"],"name":value["name"],"children":children}))
}
/// Preserve exact large IDs/ticks through JavaScript JSON transport.
pub(super) fn exact_numbers(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(exact_numbers),
        Value::Object(values) => values.values_mut().for_each(exact_numbers),
        Value::Number(n)
            if n.as_u64().is_some_and(|v| v > 9_007_199_254_740_991)
                || n.as_i64().is_some_and(|v| v < -9_007_199_254_740_991) =>
        {
            *value = Value::String(n.to_string())
        }
        _ => {}
    }
}
pub fn inspect(params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let project = document.project();
    let context = request.context;
    let mut result = json!({"name":project["name"],"path":context.path,"dirty":context.dirty,"revision":context.revision,"root":bin(&project["root"],&index,0,&mut 0)?,"activeSequence":id(context.active_sequence)?,"sourceItem":id(context.source_item)?});
    exact_numbers(&mut result);
    Ok(result)
}
