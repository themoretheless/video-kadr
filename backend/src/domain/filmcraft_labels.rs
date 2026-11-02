//! Filmcraft edit.label and sixteen named label commands (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
pub const LABELS: [&str; 16] = [
    "Violet",
    "Iris",
    "Caribbean",
    "Lavender",
    "Cerulean",
    "Forest",
    "Rose",
    "Mango",
    "Purple",
    "Blue",
    "Teal",
    "Magenta",
    "Tan",
    "Green",
    "Brown",
    "Yellow",
];
/// Original Label::rgb values, kept distinct from marker_rgb.
pub fn palette() -> Value {
    const RGB: [[u8; 3]; 16] = [
        [0x38, 0x0e, 0xa7],
        [0x1d, 0x4a, 0x64],
        [0x35, 0x54, 0x18],
        [0x6b, 0x1c, 0x82],
        [0x23, 0x53, 0x5a],
        [0x40, 0x4a, 0x11],
        [0x80, 0x18, 0x36],
        [0x80, 0x3f, 0x17],
        [0x59, 0x0d, 0xb0],
        [0x19, 0x2d, 0x94],
        [0x1f, 0x4d, 0x45],
        [0x79, 0x1c, 0x56],
        [0x6c, 0x5b, 0x47],
        [0x29, 0x5c, 0x2d],
        [0x58, 0x3d, 0x14],
        [0x6e, 0x66, 0x28],
    ];
    json!(LABELS
        .iter()
        .zip(RGB)
        .map(|(name, rgb)| json!({"name":name,"rgb":rgb}))
        .collect::<Vec<_>>())
}
fn present<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}
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
    label: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    items: Option<Value>,
    #[serde(default, deserialize_with = "present")]
    clips: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
pub(super) fn context_ids(v: Vec<Value>) -> Result<Vec<u64>> {
    v.into_iter()
        .map(|v| id(Some(v))?.context("missing selection ID"))
        .collect()
}
fn array_ids(v: &Option<Value>) -> Option<Vec<u64>> {
    v.as_ref()?.as_array().map(|v| {
        v.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect()
    })
}
pub(super) fn clip_paths(doc: &SourceProjectDocument, seq: Option<u64>) -> Vec<(u64, String)> {
    let Some(seq) = seq else { return vec![] };
    let Some(q) = doc
        .envelope
        .pointer(&format!("/project/items/{seq}/kind/Sequence"))
    else {
        return vec![];
    };
    let mut paths = Vec::new();
    for tracks in ["video_tracks", "audio_tracks"] {
        for (ti, t) in q[tracks].as_array().into_iter().flatten().enumerate() {
            for (ci, c) in t["items"].as_array().into_iter().flatten().enumerate() {
                if let Some(id) = c["id"].as_u64() {
                    paths.push((
                        id,
                        format!("/project/items/{seq}/kind/Sequence/{tracks}/{ti}/items/{ci}"),
                    ))
                }
            }
        }
    }
    paths
}
pub fn edit(command: &str, params: Value) -> Result<Value> {
    let r: Request = serde_json::from_value(params)?;
    let generic = command == "edit.label";
    let label = if generic {
        let name = r
            .label
            .as_ref()
            .and_then(Value::as_str)
            .context("edit.label: unknown label")?;
        LABELS
            .iter()
            .find(|l| l.eq_ignore_ascii_case(name))
            .copied()
            .context("edit.label: unknown label")?
    } else {
        let suffix = command
            .strip_prefix("edit.label.")
            .context("unknown label command")?;
        LABELS
            .iter()
            .find(|l| l.to_ascii_lowercase() == suffix)
            .copied()
            .context("unknown label command")?
    };
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    ProjectIndex::new(&doc)?;
    let seq = id(r.sequence)?.or(id(r.context.active_sequence)?);
    let paths = clip_paths(&doc, seq);
    let project_selection = context_ids(r.context.project_selection)?;
    let selection = context_ids(r.context.selection)?;
    let named_items = array_ids(&r.items);
    let named_clips = array_ids(&r.clips);
    if !generic && project_selection.is_empty() && selection.is_empty() {
        let named_item = named_items.as_ref().into_iter().flatten().any(|i| {
            doc.envelope["project"]["items"]
                .get(i.to_string())
                .is_some()
        });
        let named_clip = named_clips
            .as_ref()
            .into_iter()
            .flatten()
            .any(|id| paths.iter().any(|(clip, _)| clip == id));
        ensure!(named_item || named_clip, "select clips or project items");
    }
    let items = if generic {
        project_selection
    } else {
        named_items.unwrap_or_else(|| {
            if r.clips.is_some() {
                vec![]
            } else {
                project_selection
            }
        })
    };
    let clips = if generic {
        selection
    } else {
        named_clips.unwrap_or_else(|| if r.items.is_some() { vec![] } else { selection })
    };
    ensure!(
        generic || !items.is_empty() || !clips.is_empty(),
        "select clips or project items"
    );
    let mut count = 0;
    for item in items {
        if let Some(item) = doc.envelope["project"]["items"].get_mut(item.to_string()) {
            item["label"] = json!(label);
            count += 1
        }
    }
    for clip in clips {
        if let Some((_, path)) = paths.iter().find(|(id, _)| *id == clip) {
            doc.envelope.pointer_mut(path).unwrap()["label"] = json!(label);
            count += 1
        }
    }
    let result = if generic {
        Value::Null
    } else {
        json!({"label":label,"labelled":count})
    };
    Ok(
        json!({"result":result,"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}
