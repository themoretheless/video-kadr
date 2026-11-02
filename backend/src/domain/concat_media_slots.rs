//! Concat template-media document edits. File paths are inert document data.
//! Independent implementation of the pinned command contract; no source I/O.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};

pub(crate) fn admit(document: &Value) -> Result<()> {
    ensure!(
        serde_json::to_vec(document)?.len() <= 4 * 1024 * 1024,
        "Concat document exceeds 4 MiB"
    );
    ensure!(
        document
            .get("version")
            .map_or(true, |v| v.as_u64() == Some(1)),
        "unsupported Concat version"
    );
    let media = document["media"]
        .as_array()
        .context("missing media array")?;
    ensure!(media.len() <= 32, "too many media records");
    let mut ids = std::collections::BTreeSet::new();
    for item in media {
        let id = item["id"].as_str().context("missing media ID")?;
        ensure!(
            !id.is_empty() && id.len() <= 128 && ids.insert(id),
            "invalid or duplicate media ID"
        );
    }
    let timelines = document["timelines"]
        .as_array()
        .context("missing timelines array")?;
    ensure!(timelines.len() <= 32, "too many timelines");
    for timeline in timelines {
        let clips = timeline["clips"]
            .as_array()
            .context("missing clips array")?;
        ensure!(
            clips.len() <= 512 && clips.iter().all(Value::is_object),
            "invalid timeline clips"
        );
    }
    Ok(())
}

pub fn set_placeholder(document: &Value, id: &str, placeholder: bool) -> Result<(Value, bool)> {
    admit(document)?;
    let mut next = document.clone();
    let Some(item) = next["media"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["id"] == id)
    else {
        return Ok((next, false));
    };
    if item["placeholder"].as_bool().unwrap_or(false) == placeholder {
        return Ok((next, false));
    }
    if placeholder {
        item["placeholder"] = json!(true);
    } else {
        item.as_object_mut()
            .context("invalid media record")?
            .remove("placeholder");
    }
    Ok((next, true))
}

/// Keeps logical identity and every authored clip field except source In,
/// kind, display name and audio stream. Retained duration may exceed new media.
/// Native render admission is intentionally handled by subsequent conversion.
pub fn fill(document: &Value, id: &str, replacement: &Value) -> Result<Value> {
    admit(document)?;
    let index = document["media"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["id"] == id)
        .context("SlotGone")?;
    ensure!(document["media"][index]["placeholder"] == true, "NotASlot");
    let item = replacement
        .as_object()
        .context("replacement must be an object")?;
    for key in ["path", "name"] {
        let value = item
            .get(key)
            .and_then(Value::as_str)
            .context("replacement path/name missing")?;
        ensure!(
            value.len() <= 4096 && !value.contains('\0'),
            "invalid replacement path/name"
        );
    }
    let kind = replacement["kind"]
        .as_str()
        .context("missing replacement kind")?;
    ensure!(
        matches!(kind, "video" | "audio" | "image"),
        "invalid replacement kind"
    );
    for key in ["duration", "frameRate"] {
        if let Some(value) = item.get(key).filter(|v| !v.is_null()) {
            ensure!(
                value.as_f64().is_some_and(|v| v.is_finite() && v >= 0.),
                "invalid replacement {key}"
            );
        }
    }
    for key in ["width", "height"] {
        if let Some(value) = item.get(key).filter(|v| !v.is_null()) {
            ensure!(
                value.as_u64().is_some_and(|v| v <= u32::MAX as u64),
                "invalid replacement {key}"
            );
        }
    }
    if let Some(value) = item.get("hasAudio") {
        ensure!(value.is_boolean(), "invalid replacement hasAudio");
    }
    if let Some(value) = item.get("audioTracks") {
        ensure!(value.is_array(), "invalid replacement audioTracks");
    }
    for key in ["frameRateFraction", "videoCodec", "audioCodec"] {
        if let Some(value) = item.get(key) {
            ensure!(
                value.is_null() || value.is_string(),
                "invalid replacement {key}"
            );
        }
    }
    if let Some(space) = item.get("colorSpace") {
        ensure!(
            matches!(space.as_str(), Some("sdr" | "hlg" | "pq")),
            "invalid replacement colorSpace"
        );
    }
    let mut next = document.clone();
    let media = next["media"][index]
        .as_object_mut()
        .context("invalid slot record")?;
    for key in [
        "path",
        "name",
        "duration",
        "kind",
        "width",
        "height",
        "frameRate",
        "frameRateFraction",
        "videoCodec",
        "audioCodec",
    ] {
        media.insert(
            key.to_owned(),
            item.get(key).cloned().unwrap_or(Value::Null),
        );
    }
    // The source model owns these as f64 even when a caller sends an integer.
    for key in ["duration", "frameRate"] {
        if let Some(number) = item.get(key).and_then(Value::as_f64) {
            media.insert(key.into(), json!(number));
        }
    }
    media.insert(
        "hasAudio".into(),
        item.get("hasAudio").cloned().unwrap_or(json!(false)),
    );
    match item
        .get("audioTracks")
        .filter(|value| value.as_array().is_some_and(|tracks| !tracks.is_empty()))
    {
        Some(value) => {
            media.insert("audioTracks".into(), value.clone());
        }
        None => {
            media.remove("audioTracks");
        }
    }
    match item.get("colorSpace").filter(|value| *value != "sdr") {
        Some(value) => {
            media.insert("colorSpace".into(), value.clone());
        }
        None => {
            media.remove("colorSpace");
        }
    }
    media.remove("placeholder");
    for timeline in next["timelines"].as_array_mut().unwrap() {
        for clip in timeline["clips"].as_array_mut().unwrap() {
            if clip["mediaId"] != id {
                continue;
            }
            clip["sourceStart"] = json!(0.);
            clip["kind"] = json!(kind);
            clip["name"] = replacement["name"].clone();
            clip.as_object_mut().unwrap().remove("audioStream");
        }
    }
    if serde_json::to_vec(&next)?.len() > 4 * 1024 * 1024 {
        bail!("filled document exceeds 4 MiB");
    }
    Ok(next)
}
