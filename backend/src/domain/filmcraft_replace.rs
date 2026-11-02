//! Replace With Clip from Filmcraft clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Snapshot edits never open media paths.
use super::{
    filmcraft_labels::{clip_paths, context_ids},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use filmcraft_time::FrameRate;
use serde_json::{json, Value};
fn scalar(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|n| n as u64))
        .or_else(|| v.as_str()?.parse().ok())
}
fn integer(v: &Value) -> Result<i64> {
    v.as_i64()
        .or_else(|| v.as_str()?.parse().ok())
        .context("invalid replacement clock")
}
fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).context("replacement clock overflow")
}
fn rounded(v: f64) -> Result<i64> {
    let v = v.round();
    ensure!(
        v.is_finite() && v >= i64::MIN as f64 && v < 9223372036854775808.,
        "replacement clock overflow"
    );
    Ok(v as i64)
}
fn frame_tick(r: FrameRate, frame: i64) -> Result<i64> {
    ensure!(r.num > 0 && r.den > 0, "invalid replacement frame rate");
    let n = (frame as i128)
        .checked_mul(254016000000)
        .and_then(|n| n.checked_mul(r.den as i128))
        .context("replacement frame clock overflow")?;
    Ok(n.div_euclid(r.num as i128) as i64)
}
fn eligible(item: &Value) -> bool {
    ["Media", "Subclip", "Sequence", "AdjustmentLayer"]
        .iter()
        .any(|k| item["kind"].get(k).is_some())
}
fn media_end(index: &ProjectIndex<'_>, mut item: u64) -> Result<Option<i64>> {
    for _ in 0..=16 {
        let Some(pi) = index.item(item) else {
            return Ok(None);
        };
        let k = &pi["kind"];
        if let Some(m) = k.get("Media") {
            return if matches!(m["info"]["kind"].as_str(), Some("Still" | "Synthetic")) {
                Ok(None)
            } else {
                Ok(Some(integer(&m["info"]["duration"])?))
            };
        }
        if let Some(q) = k.get("Sequence") {
            return Ok(Some(super::filmcraft_source_edit::duration(q)?));
        }
        if let Some(sub) = k.get("Subclip") {
            if sub["restrict_trims"].as_bool().unwrap_or(false) {
                return Ok(Some(add(
                    integer(&sub["range"]["start"])?,
                    integer(&sub["range"]["duration"])?,
                )?));
            }
            item = sub["parent"].as_u64().context("invalid subclip parent")?;
            continue;
        }
        return Ok(None);
    }
    Ok(None)
}
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    ensure!(
        matches!(
            command,
            "clip.replaceFromSource" | "clip.replaceFromSourceMatchFrame" | "clip.replaceFromBin"
        ),
        "unknown replacement command"
    );
    let bin = command == "clip.replaceFromBin";
    let matching = command == "clip.replaceFromSourceMatchFrame";
    let fields = params
        .as_object_mut()
        .context("invalid replacement parameters")?;
    let doc = fields.remove("document").context("missing document")?;
    let ctx = fields.remove("context").unwrap_or_else(|| json!({}));
    let active = id(fields.remove("sequence"))?
        .or(id(ctx.get("activeSequence").cloned())?)
        .context("no active sequence")?;
    let bytes = match doc {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    let qpath = format!("/project/items/{active}/kind/Sequence");
    let q = doc.envelope.pointer(&qpath).context("no active sequence")?;
    let rate: FrameRate = serde_json::from_value(q["settings"]["frame_rate"].clone())?;
    let frame = frame_tick(rate, 1)?.max(1);
    let paths = clip_paths(&doc, Some(active));
    let selection = context_ids(ctx["selection"].as_array().cloned().unwrap_or_default())?;
    let named = params["clips"].as_array().map(|a| {
        a.iter()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .collect::<Vec<_>>()
    });
    ensure!(
        !selection.is_empty()
            || named
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| paths.iter().any(|(c, _)| c == id))),
        "no selected clips"
    );
    let project = context_ids(
        ctx["projectSelection"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    )?;
    let index = ProjectIndex::new(&doc)?;
    let explicit = scalar(&params["item"]);
    if bin {
        ensure!(
            project
                .iter()
                .any(|id| index.item(*id).is_some_and(eligible))
                || explicit.and_then(|id| index.item(id)).is_some_and(eligible),
            "no replacement bin selection"
        )
    }
    let item = if bin {
        explicit.or_else(|| {
            project
                .iter()
                .copied()
                .find(|id| index.item(*id).is_some_and(eligible))
        })
    } else {
        id(ctx.get("sourceItem").cloned())?
    }
    .context("no replacement clip")?;
    let pi = index
        .item(item)
        .context("no such replacement item")?
        .clone();
    let k = &pi["kind"];
    let video = k
        .get("Media")
        .map_or(true, |m| !m["info"]["video"].is_null());
    let audio = k
        .get("Media")
        .is_some_and(|m| !m["info"]["audio"].is_null())
        || k.get("Sequence")
            .is_some_and(|s| s["audio_tracks"].as_array().is_some_and(|a| !a.is_empty()));
    let in_point = if let Some(sub) = k.get("Subclip") {
        integer(&sub["range"]["start"])?
    } else {
        k.get("Media")
            .or_else(|| k.get("Sequence"))
            .and_then(|v| v["mark_in"].as_i64())
            .unwrap_or(0)
    };
    let keep = bin && params["keepSourceIn"].as_bool().unwrap_or(false);
    let within = if let Some(sub) = k
        .get("Subclip")
        .filter(|s| s["restrict_trims"].as_bool().unwrap_or(false))
    {
        let start = integer(&sub["range"]["start"])?;
        let end = add(start, integer(&sub["range"]["duration"])?)?;
        let frame = frame_tick(super::filmcraft_subclip::item_rate(&index, item)?, 1)?;
        Some((
            start,
            end.checked_sub(frame)
                .context("subclip last frame overflow")?
                .max(start),
        ))
    } else {
        None
    };
    let source_ph = ctx
        .get("sourcePlayhead")
        .map(integer)
        .transpose()?
        .unwrap_or(0);
    let ph = if let Some(v) = ctx.get("playhead") {
        integer(v)?
    } else {
        frame_tick(
            rate,
            ctx.get("playheadFrames")
                .map(integer)
                .transpose()?
                .unwrap_or(0),
        )?
    };
    let mut targets = named.unwrap_or_else(|| {
        scalar(&params["clip"])
            .map(|c| vec![c])
            .unwrap_or(selection)
    });
    ensure!(targets.len() <= 4096, "too many replacement targets");
    if ctx["linkedSelection"].as_bool().unwrap_or(true) {
        let links: Vec<_> = targets
            .iter()
            .filter_map(|id| {
                paths
                    .iter()
                    .find(|(c, _)| c == id)
                    .and_then(|(_, p)| doc.envelope.pointer(p)?["link"].as_u64())
            })
            .collect();
        for (id, path) in &paths {
            if doc
                .envelope
                .pointer(path)
                .and_then(|v| v["link"].as_u64())
                .is_some_and(|link| links.contains(&link))
                && !targets.contains(id)
            {
                targets.push(*id)
            }
        }
    }
    let channels = k
        .pointer("/Media/interpret/audio_channels/clips/0")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let old_cycle = super::filmcraft_duplicate::nest_cycle(&doc)?;
    let (mut replaced, mut clamped) = (vec![], vec![]);
    for c in targets {
        let Some((_, path)) = paths.iter().find(|(id, _)| *id == c) else {
            continue;
        };
        let track_path = path.rsplit_once("/items/").context("invalid clip path")?.0;
        let kind = doc
            .envelope
            .pointer(track_path)
            .and_then(|v| v["kind"].as_str())
            .unwrap_or("Video");
        let is_audio = kind == "Audio";
        if if is_audio { !audio } else { !video } {
            continue;
        }
        let before = doc.envelope.pointer(path).unwrap();
        let start = if matching {
            let delta = ph
                .checked_sub(integer(&before["start"])?)
                .context("match frame offset overflow")?;
            source_ph
                .checked_sub(rounded(
                    delta as f64
                        * before["speed"]
                            .as_f64()
                            .context("invalid clip speed")?
                            .abs(),
                )?)
                .context("match frame source overflow")?
        } else if keep {
            let old = integer(&before["source_in"])?;
            let next = within.map_or(old, |(a, b)| old.clamp(a, b));
            if old != next {
                clamped.push(c)
            }
            next
        } else {
            in_point
        };
        ensure!(
            start >= 0,
            "not enough media before the Source monitor playhead to match the frame"
        );
        let it = doc.envelope.pointer_mut(path).unwrap();
        it["item"] = json!(item);
        it["name"] = pi["name"].clone();
        it["label"] = pi["label"].clone();
        it["source_in"] = json!(start);
        it["frame_hold"] = Value::Null;
        it.as_object_mut().unwrap().remove("multicam");
        if is_audio {
            if channels.is_empty() {
                it.as_object_mut().unwrap().remove("source_channels");
            } else {
                it["source_channels"] = json!(channels);
            }
        }
        replaced.push(c);
    }
    ensure!(
        !replaced.is_empty(),
        "replacement has no video or audio for selected clips"
    );
    super::filmcraft_marks::check_sequence(doc.envelope.pointer(&qpath).unwrap())?;
    ensure!(
        old_cycle.is_some() || super::filmcraft_duplicate::nest_cycle(&doc)?.is_none(),
        "replacement creates a nested sequence cycle"
    );
    let index = ProjectIndex::new(&doc)?;
    let mut short = vec![];
    for c in &replaced {
        let path = &paths.iter().find(|(id, _)| id == c).unwrap().1;
        let it = doc.envelope.pointer(path).unwrap();
        let Some(end) = media_end(&index, item)? else {
            continue;
        };
        let speed = it["speed"]
            .as_f64()
            .context("invalid replacement speed")?
            .abs();
        let duration = integer(&it["duration"])?;
        let out = add(
            integer(&it["source_in"])?,
            rounded(duration as f64 * speed)?,
        )?;
        let over = out.saturating_sub(end);
        if over <= 0 || !it["frame_hold"].is_null() {
            continue;
        }
        let length = duration.max(0);
        let ticks = (over as f64 / speed.max(1e-9)).round().min(length as f64) as i64;
        let frames = ticks.min(length).saturating_add(frame - 1) / frame;
        if frames > 0 {
            short.push(json!({"clip":c.to_string(),"shortByFrames":frames.to_string()}))
        }
    }
    let mut result = json!({"clips":replaced.iter().map(u64::to_string).collect::<Vec<_>>(),"item":item.to_string(),"short":short});
    if keep {
        result["sourceInClamped"] = json!(clamped.iter().map(u64::to_string).collect::<Vec<_>>())
    }
    let text = String::from_utf8(doc.encode_preserved()?)?;
    Ok(
        json!({"result":result,"document":doc.envelope,"documentText":text,"replacementEdit":true,"sequence":active.to_string()}),
    )
}
