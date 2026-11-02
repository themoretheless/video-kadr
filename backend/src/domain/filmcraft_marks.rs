//! Ordinary and split In/Out marks adapted from Filmcraft (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be commands.rs / sequence_tools.rs.
use super::{
    filmcraft_clip_rename::target_id, filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex, filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    source_item: Option<Value>,
    playhead: Option<Value>,
    source_playhead: Option<Value>,
}
fn integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}
fn normalize_split(value: &mut Value) {
    if value["split"].is_object() {
        for key in ["videoIn", "videoOut", "audioIn", "audioOut"] {
            if value["split"].get(key).is_none() {
                value["split"][key] = Value::Null;
            }
        }
        if value["split"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_null)
        {
            value.as_object_mut().unwrap().remove("split");
        }
    }
}
fn clear_split(value: &mut Value, input: bool) {
    for key in if input {
        ["videoIn", "audioIn"]
    } else {
        ["videoOut", "audioOut"]
    } {
        value["split"][key] = Value::Null;
    }
}
fn bounded(value: i64) -> bool {
    (Tick::MIN.0..=Tick::MAX.0).contains(&value)
}
fn range(value: &Value) -> Result<i64> {
    let start = value["start"].as_i64().context("invalid timeline start")?;
    let duration = value["duration"]
        .as_i64()
        .context("invalid timeline duration")?;
    let end = start
        .checked_add(duration)
        .context("timeline end overflow")?;
    ensure!(
        bounded(start) && bounded(duration) && bounded(end),
        "timeline outside supported time bounds"
    );
    Ok(end)
}
// Session.edit_sequence validates the entire target, including unchanged structural errors.
pub(super) fn check_sequence(q: &Value) -> Result<()> {
    let settings = &q["settings"];
    let width = settings["width"]
        .as_u64()
        .context("invalid sequence width")?;
    let height = settings["height"]
        .as_u64()
        .context("invalid sequence height")?;
    ensure!(
        width > 0 && height > 0 && width <= 32768 && height <= 32768 && width * height <= 268435456,
        "invalid sequence frame size"
    );
    let rate: FrameRate = serde_json::from_value(settings["frame_rate"].clone())?;
    ensure!(
        rate.num > 0
            && rate.den > 0
            && rate.num <= u32::MAX as i64
            && rate.den <= u32::MAX as i64
            && rate.as_f64() <= 1000.
            && 254016000000i128 * rate.den as i128 / (rate.num as i128) <= Tick::MAX.0 as i128,
        "invalid sequence frame rate"
    );
    ensure!(
        settings["sample_rate"]
            .as_u64()
            .is_some_and(|r| r > 0 && r <= 384000),
        "invalid sequence sample rate"
    );
    ensure!(
        [0, 1].into_iter().all(|i| settings["par"][i]
            .as_u64()
            .is_some_and(|p| p > 0 && p <= u32::MAX as u64)),
        "invalid pixel aspect ratio"
    );
    for mark in ["mark_in", "mark_out"] {
        if !q[mark].is_null() {
            ensure!(
                bounded(q[mark].as_i64().context("invalid sequence mark")?),
                "sequence mark outside supported time bounds"
            );
        }
    }
    if !q["work_area"].is_null() {
        range(&q["work_area"])?;
    }
    for kind in ["video_tracks", "audio_tracks", "caption_tracks"] {
        for track in q[kind].as_array().into_iter().flatten() {
            let mut previous = None;
            for item in track[if kind == "caption_tracks" {
                "captions"
            } else {
                "items"
            }]
            .as_array()
            .into_iter()
            .flatten()
            {
                let end = range(item)?;
                let start = item["start"].as_i64().unwrap();
                ensure!(
                    item["duration"].as_i64().unwrap() > 0 && previous.is_none_or(|p| p <= start),
                    "sequence items overlap or have non-positive duration"
                );
                previous = Some(end);
            }
            if kind != "caption_tracks" {
                for tr in track["transitions"].as_array().into_iter().flatten() {
                    range(tr)?;
                }
            }
        }
    }
    Ok(())
}
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    let fields = params
        .as_object_mut()
        .context("expected marks parameters")?;
    let document = fields
        .remove("document")
        .context("missing marks document")?;
    let context: SessionContext = fields
        .remove("context")
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let bytes = match document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    ProjectIndex::new(&doc)?;
    let source = id(context.source_item)?;
    let active = id(context.active_sequence)?;
    let source_target = params["target"] == "source";
    let direct = command == "project.setMarks";
    let path = if direct || source_target && !command.starts_with("markers.clear") {
        let item = if direct {
            target_id(params.get("item").cloned())?.or(source)
        } else {
            source
        };
        format!("/project/items/{}", item.context("no source item")?)
    } else {
        format!(
            "/project/items/{}/kind/Sequence",
            active.context("no sequence is open")?
        )
    };
    let program = !direct && !(source_target && !command.starts_with("markers.clear"));
    if direct {
        let item = doc
            .envelope
            .pointer_mut(&path)
            .context("no such source item")?;
        for (key, mark, input) in [("in", "mark_in", true), ("out", "mark_out", false)] {
            if let Some(value) = params.get(key) {
                clear_split(item, input);
                let target = if item.pointer("/kind/Media").is_some() {
                    Some("Media")
                } else if item.pointer("/kind/Sequence").is_some() {
                    Some("Sequence")
                } else {
                    None
                };
                if let Some(kind) = target {
                    let target = &mut item["kind"][kind];
                    target[mark] = integer(value).map_or(Value::Null, |i| json!(i));
                    if kind == "Sequence" {
                        clear_split(target, input);
                    }
                }
            }
        }
    } else {
        let playhead = context.playhead.as_ref().and_then(integer).unwrap_or(0);
        let source_playhead = context
            .source_playhead
            .as_ref()
            .and_then(integer)
            .unwrap_or(0);
        let time = if program {
            let q = doc.envelope.pointer(&path).context("no sequence is open")?;
            let rate: FrameRate = serde_json::from_value(q["settings"]["frame_rate"].clone())?;
            if let Some(t) = integer(&params["time"]) {
                t
            } else if let Some(f) = integer(&params["frame"]) {
                rate.tick_of(f).0
            } else if let Some(sec) = params["seconds"].as_f64() {
                Tick::from_seconds_f64(sec).0
            } else if let Some(tc) = params["timecode"].as_str() {
                filmcraft_time::parse_timecode(
                    tc,
                    rate,
                    q["settings"]["drop_frame"].as_bool().unwrap_or(false),
                    rate.frame_at(Tick(playhead)),
                )
                .map(|f| rate.tick_of(f).0)
                .unwrap_or(playhead)
            } else {
                playhead
            }
        } else {
            integer(&params["time"]).unwrap_or(source_playhead)
        };
        let target = doc
            .envelope
            .pointer_mut(&path)
            .context("missing marks target")?;
        match command {
            "markers.markIn" | "markers.markOut" => {
                let input = command == "markers.markIn";
                let mark = if input { "mark_in" } else { "mark_out" };
                if program {
                    target[mark] = json!(time);
                    clear_split(target, input);
                    let other = if input { "mark_out" } else { "mark_in" };
                    if target[other]
                        .as_i64()
                        .is_some_and(|v| if input { v < time } else { v > time })
                    {
                        target[other] = Value::Null;
                    }
                } else {
                    clear_split(target, input);
                    for kind in ["Media", "Sequence"] {
                        if let Some(q) = target["kind"].get_mut(kind) {
                            q[mark] = json!(time);
                            if kind == "Sequence" {
                                clear_split(q, input);
                            }
                        }
                    }
                }
            }
            "markers.clearIn" | "markers.clearOut" | "markers.clearInOut" => {
                if command != "markers.clearOut" {
                    target["mark_in"] = Value::Null;
                    clear_split(target, true);
                }
                if command != "markers.clearIn" {
                    target["mark_out"] = Value::Null;
                    clear_split(target, false);
                }
            }
            "markers.markSplitVideoIn"
            | "markers.markSplitVideoOut"
            | "markers.markSplitAudioIn"
            | "markers.markSplitAudioOut" => {
                let video = command.contains("Video");
                let input = command.ends_with("In");
                let (key, other) = match (video, input) {
                    (true, true) => ("videoIn", "videoOut"),
                    (true, false) => ("videoOut", "videoIn"),
                    (false, true) => ("audioIn", "audioOut"),
                    (false, false) => ("audioOut", "audioIn"),
                };
                target["split"][key] = json!(time);
                if target["split"][other].as_i64().is_some_and(|v| {
                    if input {
                        v < time
                    } else {
                        v > time
                    }
                }) {
                    target["split"][other] = Value::Null;
                }
            }
            _ => anyhow::bail!("unsupported marks command"),
        }
    }
    let target = doc.envelope.pointer_mut(&path).unwrap();
    normalize_split(target);
    if direct || !program {
        if let Some(q) = target.pointer_mut("/kind/Sequence") {
            normalize_split(q);
        }
    }
    if program {
        check_sequence(doc.envelope.pointer(&path).unwrap())?;
    }
    let result = if command.starts_with("markers.markSplit") {
        doc.envelope
            .pointer(&path)
            .unwrap()
            .get("split")
            .cloned()
            .unwrap_or_else(
                || json!({"videoIn":null,"videoOut":null,"audioIn":null,"audioOut":null}),
            )
    } else {
        Value::Null
    };
    let mut result = result;
    super::filmcraft_project_inspect::exact_numbers(&mut result);
    Ok(
        json!({"result":result,"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope,"markEdit":true}),
    )
}

fn describe_marks(ordinary: &Value, split: &Value) -> Value {
    let tick = |value: &Value| value.as_i64().map(|t| t.to_string());
    json!({"in":tick(&ordinary["mark_in"]),"out":tick(&ordinary["mark_out"]),"split":{"videoIn":tick(&split["videoIn"]),"videoOut":tick(&split["videoOut"]),"audioIn":tick(&split["audioIn"]),"audioOut":tick(&split["audioOut"])}})
}
pub fn source_marks(item: &Value) -> Value {
    describe_marks(
        item.pointer("/kind/Media")
            .or_else(|| item.pointer("/kind/Sequence"))
            .unwrap_or(&Value::Null),
        &item["split"],
    )
}
pub fn sequence_marks(sequence: &Value) -> Value {
    describe_marks(sequence, &sequence["split"])
}
