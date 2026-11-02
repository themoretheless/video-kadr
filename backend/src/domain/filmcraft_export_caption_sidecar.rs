//! Source engine/export_tools.rs::write_sidecar and captions::document_from_track.
//! MIT, pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use anyhow::{Context, Result};
use filmcraft_captions::{Cue, Document, Format, WriteOptions};
use filmcraft_time::{FrameRate, Tick};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidecar {
    pub extension: String,
    pub bytes_base64: String,
}

fn optional_text(value: Option<&Value>) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => anyhow::bail!("invalid caption metadata"),
    }
}

/// Only the first track is exported, even when disabled. Export end does not clip cues.
pub fn make(sequence: &Value, offset: i64, kind: &str) -> Result<Option<Sidecar>> {
    let tracks = sequence["caption_tracks"]
        .as_array()
        .context("missing caption tracks")?;
    let Some(track) = tracks.first() else {
        return Ok(None);
    };
    let format = Format::from_name(kind).context("unknown caption sidecar format")?;
    let mut cues = Vec::new();
    for cue in track["captions"]
        .as_array()
        .context("invalid caption track")?
    {
        let start = cue["start"].as_i64().context("invalid caption start")?;
        let duration = cue["duration"]
            .as_i64()
            .context("invalid caption duration")?;
        let end = start
            .checked_add(duration)
            .context("caption end overflow")?;
        if end <= offset {
            continue;
        }
        cues.push(Cue {
            start: Tick(
                start
                    .checked_sub(offset)
                    .context("caption offset overflow")?
                    .max(0),
            ),
            end: Tick(end.checked_sub(offset).context("caption offset overflow")?),
            text: cue["text"].as_str().context("invalid caption text")?.into(),
            speaker: optional_text(cue.get("speaker"))?,
            id: optional_text(cue.get("cue_id"))?,
            settings: optional_text(cue.get("settings"))?.unwrap_or_default(),
        });
    }
    let blocks = match track.get("vtt_blocks") {
        None => Vec::new(),
        Some(blocks) => blocks
            .as_array()
            .context("invalid WebVTT blocks")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .context("invalid WebVTT block")
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let rate: FrameRate = serde_json::from_value(sequence["settings"]["frame_rate"].clone())?;
    let document = Document {
        cues,
        blocks,
        warnings: Vec::new(),
    };
    let bytes = filmcraft_captions::write(
        &document,
        format,
        WriteOptions {
            rate: Some(rate),
            drop_frame: sequence["settings"]["drop_frame"]
                .as_bool()
                .unwrap_or(false),
        },
    );
    Ok(Some(Sidecar {
        extension: format.extension().into(),
        bytes_base64: super::graphic_template_fonts::base64_encode(&bytes),
    }))
}
