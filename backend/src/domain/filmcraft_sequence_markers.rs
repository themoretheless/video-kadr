//! Sequence marker commands from Filmcraft (MIT, pinned 52318524).
use super::{
    filmcraft_labels::LABELS,
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
    filmcraft_project_sort::{item_rate, timecode},
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
use serde::Deserialize;
use serde_json::{json, Value};

pub const COMMANDS: &[&str] = &[
    "markers.add",
    "markers.addRange",
    "markers.addRangeInOut",
    "markers.addChapter",
    "markers.addFlashCue",
    "markers.edit",
    "markers.clearCurrent",
    "markers.clearAll",
    "markers.goNext",
    "markers.goPrev",
];

pub(crate) fn describe(sequence: &Value, rate: FrameRate) -> Vec<Value> {
    sequence["markers"].as_array().into_iter().flatten().filter_map(|marker| {
        let start = marker["start"].as_i64()?;
        Some(json!({
            "id":marker["id"].as_u64()?.to_string(),"start":start.to_string(),
            "duration":marker["duration"].as_i64().unwrap_or_default().to_string(),
            "frame":rate.frame_at(Tick(start)).to_string(),
            "durationFrames":rate.frame_at(Tick(marker["duration"].as_i64().unwrap_or_default())).to_string(),
            "timecode":timecode(i128::from(start), rate),"kind":marker["kind"],
            "name":marker["name"],"comment":marker["comment"],"color":marker["color"]
        }))
    }).collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    sequence: Option<Value>,
    marker: Option<Value>,
    time: Option<Value>,
    frame: Option<Value>,
    duration: Option<Value>,
    duration_frames: Option<Value>,
    name: Option<String>,
    comment: Option<String>,
    color: Option<String>,
}

fn integer(value: Option<&Value>) -> Result<Option<i64>> {
    value
        .map(|v| {
            v.as_i64()
                .or_else(|| v.as_str()?.parse().ok())
                .context("invalid marker clock")
        })
        .transpose()
}

pub(super) fn ticks(rate: FrameRate, frame: i64) -> Result<i64> {
    let tick = i128::from(frame)
        .checked_mul(i128::from(TICKS_PER_SECOND))
        .and_then(|ticks| ticks.checked_mul(i128::from(rate.den)))
        .context("marker clock overflow")?
        .div_euclid(i128::from(rate.num));
    i64::try_from(tick).context("marker clock exceeds i64")
}

pub fn run(command: &str, params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let sequence = id(request.sequence)?
        .or_else(|| document.project()["active_sequence"].as_u64())
        .context("select a sequence")?;
    let item = index.item(sequence).context("missing marker sequence")?;
    let q = item
        .pointer("/kind/Sequence")
        .context("marker target is not a sequence")?;
    let rate = item_rate(&index, sequence, item);
    let frame = integer(request.frame.as_ref())?;
    let time = integer(request.time.as_ref())?
        .or(frame.map(|frame| ticks(rate, frame)).transpose()?)
        .unwrap_or(0);
    let duration =
        integer(request.duration.as_ref())?.or(integer(request.duration_frames.as_ref())?
            .map(|frame| ticks(rate, frame))
            .transpose()?);
    let mut markers = q["markers"].as_array().cloned().unwrap_or_default();
    if matches!(command, "markers.goNext" | "markers.goPrev") {
        let candidates = markers.iter().filter_map(|m| m["start"].as_i64());
        let target = if command == "markers.goNext" {
            candidates.filter(|t| *t > time).min()
        } else {
            candidates.filter(|t| *t < time).max()
        };
        let result = target.map(|time| json!({"sequence":sequence.to_string(),"time":time.to_string(),
            "timeFrames":rate.frame_at(Tick(time)).to_string(),"timecode":timecode(i128::from(time), rate)}));
        return Ok(json!({"result":result}));
    }
    let color = request.color.as_deref().and_then(|value| {
        LABELS
            .iter()
            .copied()
            .find(|name| name.eq_ignore_ascii_case(value))
    });
    let marker_id = id(request.marker)?;
    let mut allocated = None;
    let result = match command {
        "markers.clearAll" => {
            markers.clear();
            Value::Null
        }
        "markers.clearCurrent" => {
            markers.retain(|m| match marker_id {
                Some(id) => m["id"].as_u64() != Some(id),
                None => m["start"].as_i64() != Some(time),
            });
            Value::Null
        }
        "markers.edit" => {
            let marker_id = marker_id.context("select a marker to edit")?;
            let marker = markers
                .iter_mut()
                .find(|m| m["id"].as_u64() == Some(marker_id))
                .context("marker not found")?;
            if let Some(name) = request.name {
                marker["name"] = json!(name);
            }
            if let Some(comment) = request.comment {
                marker["comment"] = json!(comment);
            }
            if let Some(color) = color {
                marker["color"] = json!(color);
            }
            if let Some(duration) = duration {
                marker["duration"] = json!(duration);
            }
            json!({"marker":marker_id.to_string()})
        }
        _ => {
            let (kind, start, duration) = match command {
                "markers.add" => ("Comment", time, duration.unwrap_or_default()),
                "markers.addRange" => {
                    let duration = duration.unwrap_or(ticks(rate, rate.timecode_base())?);
                    ensure!(duration > 0, "range marker duration must be positive");
                    ("Comment", time, duration)
                }
                "markers.addRangeInOut" => {
                    let input = q["mark_in"].as_i64().context("mark an In point first")?;
                    let output = q["mark_out"].as_i64().context("mark an Out point first")?;
                    let duration = output
                        .checked_sub(input)
                        .and_then(|span| span.checked_add(rate.frame_duration().0))
                        .context("range marker clock overflow")?;
                    ("Comment", input, duration)
                }
                "markers.addChapter" => ("Chapter", time, 0),
                "markers.addFlashCue" => (
                    "FlashCue",
                    ticks(rate, rate.frame_at(Tick(time)))?,
                    duration.unwrap_or_default(),
                ),
                _ => anyhow::bail!("unknown marker command"),
            };
            let next = document.project()["next_id"]
                .as_u64()
                .context("missing project next ID")?;
            ensure!(
                index.item(next).is_none()
                    && !index.sequences().any(|(_, seq)| seq["markers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|m| m["id"].as_u64() == Some(next))),
                "project next ID already allocated"
            );
            allocated = Some(next.checked_add(1).context("project ID space exhausted")?);
            let name = request.name.unwrap_or_else(|| {
                let number = markers.iter().filter(|m| m["kind"] == kind).count() + 1;
                match kind {
                    "Chapter" => format!("Chapter {number}"),
                    "FlashCue" => format!("Cue Point {number}"),
                    _ => String::new(),
                }
            });
            markers.push(json!({"id":next,"start":start,"duration":duration,"name":name,
                "comment":request.comment.unwrap_or_default(),"kind":kind,"color":color.unwrap_or("Green")}));
            markers.sort_by_key(|m| m["start"].as_i64().unwrap_or_default());
            json!({"marker":next.to_string()})
        }
    };
    document.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"]["markers"] =
        json!(markers);
    if let Some(next) = allocated {
        document.envelope["project"]["next_id"] = json!(next);
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"result":result,"documentText":text,"sequenceMarkerEdit":true}))
}
