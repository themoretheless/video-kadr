//! Filmcraft item metadata edits (MIT, pinned 52318524).
use super::{
    filmcraft_labels::{clip_paths, LABELS},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::{exact_numbers, id},
    filmcraft_project_sort::{item_media, item_rate, keys, source_start, timecode},
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
const LOG_FIELDS: &[&str] = &[
    "Description",
    "Scene",
    "Shot",
    "Log Note",
    "Comment",
    "Tape Name",
    "Client",
    "Camera Angle",
];

fn format_bytes(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{b} bytes"),
    }
}

fn describe(index: &ProjectIndex<'_>, target: u64, item: &Value) -> Value {
    let info = keys(index, target, item, [0; 2]);
    let metadata = &info["metadata"];
    let string = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let mut fields = Vec::new();
    let mut add = |section: &str, name: &str, value: String, editable: bool| {
        fields.push(json!({"section":section,"name":name,"value":value,"editable":editable}));
    };
    add("Clip", "Name", string(&item["name"]), true);
    add("Clip", "Label", string(&item["label"]), true);
    add("Clip", "Media Type", string(&info["mediaType"]), false);
    let rate = item_rate(index, target, item);
    let fps = if info["frameRate"].as_f64().unwrap_or_default() > 0.0 {
        let item_rate = if item.pointer("/kind/Subclip").is_some() {
            filmcraft_time::FrameRate::default()
        } else {
            rate
        };
        format!("{} fps", item_rate.label())
    } else {
        String::new()
    };
    add("Clip", "Frame Rate", fps, false);
    for (name, key) in [
        ("Media Start", "mediaStart"),
        ("Media End", "mediaEnd"),
        ("Media Duration", "mediaDuration"),
    ] {
        add("Clip", name, string(&info["timecodes"][key]), false);
    }
    let media = item_media(index, item);
    let sequence = item.pointer("/kind/Sequence");
    if let Some(marked) = item.pointer("/kind/Media").or(sequence) {
        if let (Some(input), Some(output)) =
            (marked["mark_in"].as_i64(), marked["mark_out"].as_i64())
        {
            let start = source_start(media.map_or(&Value::Null, |m| &m["info"]), rate);
            let input = start.saturating_add(i128::from(input));
            let output = start
                .saturating_add(i128::from(output))
                .saturating_sub(i128::from(rate.frame_duration().0));
            add("Clip", "In Point", timecode(input, rate), false);
            add("Clip", "Out Point", timecode(output, rate), false);
        }
    }
    if let Some(media) = media {
        if !media["info"]["video"].is_null() {
            add("Clip", "Video Info", string(&info["videoInfo"]), false);
            add("Clip", "Video Codec", string(&info["videoCodec"]), false);
            add("Clip", "Color Space", string(&info["colorSpace"]), false);
        }
        if let Some(audio) = media.pointer("/info/audio").filter(|a| !a.is_null()) {
            let value = match (audio["sample_rate"].as_u64(), audio["channels"].as_u64()) {
                (Some(rate), Some(channels)) => format!("{rate} Hz - {channels} ch"),
                _ => String::new(),
            };
            add("Clip", "Audio Info", value, false);
            add("Clip", "Audio Codec", string(&info["audioCodec"]), false);
        }
    }
    if let Some(sequence) = sequence {
        let settings = &sequence["settings"];
        add(
            "Clip",
            "Video Info",
            format!("{} x {}", settings["width"], settings["height"]),
            false,
        );
        add("Clip", "Color Space", string(&info["colorSpace"]), false);
    }
    for name in LOG_FIELDS {
        add("Clip", name, string(&metadata[*name]), true);
    }
    for (name, value) in metadata.as_object().into_iter().flatten() {
        if !LOG_FIELDS.contains(&name.as_str()) {
            add("Clip", name, string(value), true);
        }
    }
    if let Some(media) = media {
        let path = match media.pointer("/media/File/path").and_then(Value::as_str) {
            Some(path) => path.to_owned(),
            None => media
                .pointer("/media/Generator")
                .map_or_else(String::new, |generator| format!("Generator({generator})")),
        };
        add("File", "File Path", path, false);
        add("File", "Format", string(&media["info"]["container"]), false);
        if let Some(size) = media["info"]["file_size"].as_u64() {
            add("File", "File Size", format_bytes(size), false);
        }
        if let Some(bitrate) = media["info"]["video"]["bitrate"].as_u64() {
            add(
                "File",
                "Video Bitrate",
                format!("{:.2} Mbit/s", bitrate as f64 / 1e6),
                false,
            );
        }
        add("File", "Status", string(&info["status"]), false);
    }
    json!({"item":target,"name":item["name"],"label":item["label"],"fields":fields,"metadata":metadata})
}

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
        let mut result = describe(&index, target, original);
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
