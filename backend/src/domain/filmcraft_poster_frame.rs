//! Filmcraft keyboard::set_poster (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_clip_rename::target_id,
    filmcraft_export_command_range::Clock,
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::{exact_numbers, id},
    filmcraft_source_inspect,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::{FrameRate, Tick};
use serde::Deserialize;
use serde_json::{json, Value};

pub const COMMANDS: &[&str] = &["clip.setPosterFrame", "clip.clearPosterFrame"];
const KEY: &str = "Poster Frame";

pub fn stored_tick(item: &Value) -> Option<String> {
    item.get("metadata")?
        .get(KEY)?
        .as_str()?
        .parse::<i64>()
        .ok()
        .map(|tick| tick.to_string())
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    source_item: Option<Value>,
    source_playhead: Option<Value>,
    playhead: Option<Value>,
    project_selection: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    item: Option<Value>,
    time: Option<Value>,
    frame: Option<Value>,
    seconds: Option<Value>,
    timecode: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}

fn tick(value: Option<Value>) -> Result<i64> {
    value
        .map(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.parse().ok())
                .context("invalid poster clock")
        })
        .transpose()
        .map(|value| value.unwrap_or_default())
}

fn snap(rate: FrameRate, time: i64) -> Result<i64> {
    let rate = rate.sane();
    ensure!(
        (Tick::MIN.0..=Tick::MAX.0).contains(&time),
        "poster time exceeds tick bounds"
    );
    let numerator = i128::from(filmcraft_time::TICKS_PER_SECOND) * i128::from(rate.den);
    let frame = (i128::from(time) * i128::from(rate.num)).div_euclid(numerator);
    let snapped = frame
        .checked_mul(numerator)
        .context("poster frame overflow")?
        .div_euclid(i128::from(rate.num));
    i64::try_from(snapped).context("poster tick overflow")
}

pub(crate) enum PreviewKind {
    Sequence,
    Generator,
    File,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PreviewView {
    #[default]
    Selected,
    Card,
}

/// Match the original preview area and icon-card defaults on the thumbnail clock.
pub(crate) fn preview_clock(
    document: &SourceProjectDocument,
    id: u64,
    view: PreviewView,
) -> Result<(PreviewKind, i64)> {
    let index = ProjectIndex::new(document)?;
    let item = index.item(id).context("poster item is gone")?;
    let kind = if item.pointer("/kind/Sequence").is_some() {
        PreviewKind::Sequence
    } else {
        let media = item
            .pointer("/kind/Media")
            .context("item has no thumbnail")?;
        ensure!(
            media["offline"] != true && !media["info"]["video"].is_null(),
            "item has no available video"
        );
        if media.pointer("/media/File").is_some() {
            PreviewKind::File
        } else if media.pointer("/media/Generator").is_some() {
            PreviewKind::Generator
        } else {
            anyhow::bail!("item media does not support thumbnail preview")
        }
    };
    let time = if let Some(tick) = stored_tick(item) {
        tick.parse()?
    } else {
        let duration = super::filmcraft_project_inspect::duration(item)?;
        match view {
            PreviewView::Selected => duration / 3,
            PreviewView::Card => (i128::from(duration) * 3 / 10) as i64,
        }
    };
    let quarter = filmcraft_time::TICKS_PER_SECOND / 4;
    let rate = filmcraft_source_inspect::frame_rate(&index, id)?;
    Ok((kind, snap(rate, time / quarter * quarter)?))
}

pub fn edit(command: &str, params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let explicit = target_id(request.item)?;
    let source = id(request.context.source_item)?;
    let selected = id(request.context.project_selection.into_iter().next())?;
    let item = explicit
        .or(selected)
        .or(source)
        .context("open a clip in the Source monitor or select one in the Project panel")?;
    let target = index
        .item(item)
        .context("clip.setPosterFrame: no such item")?;
    ensure!(
        target.pointer("/kind/AdjustmentLayer").is_none(),
        "adjustment layers have no poster frame"
    );
    let clear = command == "clip.clearPosterFrame";
    let poster = if clear {
        None
    } else {
        let time = if explicit.is_none() && source == Some(item) {
            tick(request.context.source_playhead)?
        } else {
            let sequence = id(request.context.active_sequence)?
                .and_then(|sequence| index.item(sequence))
                .and_then(|value| value.pointer("/kind/Sequence/settings"));
            let clock = Clock {
                rate: sequence
                    .map(|settings| serde_json::from_value(settings["frame_rate"].clone()))
                    .transpose()?
                    .unwrap_or_default(),
                drop_frame: sequence
                    .and_then(|settings| settings["drop_frame"].as_bool())
                    .unwrap_or_default(),
                playhead: Tick(tick(request.context.playhead)?),
            };
            let times = json!({"atTime":request.time,"atFrame":request.frame,"atSeconds":request.seconds,"atTimecode":request.timecode});
            clock.time(&times, "at")?.unwrap_or_default()
        };
        let rate = if target.pointer("/kind/Subclip").is_some() {
            FrameRate::default()
        } else {
            filmcraft_source_inspect::frame_rate(&index, item)?
        };
        Some(snap(rate, time)?)
    };
    let target = document
        .envelope
        .pointer_mut(&format!("/project/items/{item}"))
        .context("poster target is gone")?;
    if let Some(time) = poster {
        if target["metadata"].is_null() {
            target["metadata"] = json!({});
        }
        target["metadata"]
            .as_object_mut()
            .context("invalid item metadata")?
            .insert(KEY.into(), json!(time.to_string()));
    } else if let Some(metadata) = target["metadata"].as_object_mut() {
        metadata.remove(KEY);
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    let mut result = json!({"item":item,"posterFrame":poster});
    exact_numbers(&mut result);
    Ok(json!({"result":result,"document":document.envelope,"documentText":text}))
}
