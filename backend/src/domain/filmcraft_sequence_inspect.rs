//! Source sequence inspection from Filmcraft commands.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be; explicit snapshot/session context.
use super::{
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::{SourceSequence, SourceTrack},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    item: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    #[serde(default)]
    playheads: BTreeMap<String, Value>,
    #[serde(default)]
    selection: Vec<Value>,
}
// These names/field order intentionally retain the original derived Debug output.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
enum ParamValue {
    Float(f64),
    Vec2(#[serde(deserialize_with = "point")] Vec2),
    Color([f32; 4]),
    Bool(bool),
    Choice(u32),
    Text(String),
    Curve(Vec<[f32; 2]>),
    Path(MaskPath),
}
#[derive(Debug, Default, Deserialize, Serialize)]
#[allow(dead_code)]
struct Vec2 {
    x: f64,
    y: f64,
}
fn point<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec2, D::Error> {
    #[derive(Deserialize)]
    struct Point {
        x: Option<f64>,
        y: Option<f64>,
    }
    let p = Point::deserialize(d)?;
    Ok(Vec2 {
        x: p.x.unwrap_or(f64::NAN),
        y: p.y.unwrap_or(f64::NAN),
    })
}
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct MaskVertex {
    p: Vec2,
    #[serde(default, skip_serializing_if = "zero")]
    t_in: Vec2,
    #[serde(default, skip_serializing_if = "zero")]
    t_out: Vec2,
}
#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct MaskPath {
    vertices: Vec<MaskVertex>,
    #[serde(default = "yes")]
    closed: bool,
}
fn zero(p: &Vec2) -> bool {
    p.x == 0. && p.y == 0.
}
fn yes() -> bool {
    true
}
fn parameters(raw: &Value, debug: bool) -> Result<Value> {
    let mut params = serde_json::Map::new();
    for (name, param) in raw.as_object().context("invalid effect parameters")? {
        let value = param.get("value").context("missing parameter value")?;
        let parsed: ParamValue = serde_json::from_value(value.clone())?;
        params.insert(name.clone(),if debug {json!({"value":format!("{parsed:?}"),"keyframes":match param.get("keyframes"){None=>0,Some(keys)=>keys.as_array().context("invalid parameter keyframes")?.len()}})}else{serde_json::to_value(&parsed)?});
    }
    Ok(Value::Object(params))
}
fn track(track: &SourceTrack<'_>, rate: filmcraft_time::FrameRate) -> Result<Value> {
    let mut items = Vec::new();
    for clip in &track.items {
        let t = clip.timing;
        let raw = clip.raw;
        let end = t
            .timeline_start
            .checked_add(t.duration)
            .context("clip end overflow")?;
        let consumed = (t.duration as f64 * t.speed.abs()).round() as i64;
        let source_out = t
            .source_in
            .checked_add(consumed)
            .context("source out overflow")?;
        let mut effects = Vec::new();
        for effect in raw["effects"].as_array().context("missing clip effects")? {
            effects.push(json!({"effect":effect["effect"],"enabled":effect["enabled"],"masks":match effect.get("masks"){None=>0,Some(masks)=>masks.as_array().context("invalid effect masks")?.len()},"params":parameters(&effect["params"],true)?}));
        }
        items.push(json!({"clip":clip.id,"item":clip.source_id,"name":raw["name"],"start":t.timeline_start,"duration":t.duration,"startFrame":rate.frame_at(filmcraft_time::Tick(t.timeline_start)),"durationFrames":rate.frame_at(filmcraft_time::Tick(t.duration)),"sourceIn":t.source_in,"speed":t.speed,"reverse":t.reverse,"end":end,"endFrame":rate.frame_at(filmcraft_time::Tick(end)),"sourceOut":source_out,"gainDb":raw["gain_db"],"enabled":clip.enabled,"link":raw["link"],"label":raw["label"],"effects":effects}));
    }
    let raw = track.raw;
    let mut transitions = Vec::new();
    for t in raw["transitions"]
        .as_array()
        .context("missing track transitions")?
    {
        transitions.push(json!({"id":t["id"],"effect":t["effect"]["effect"],"start":t["start"],"duration":t["duration"],"from":t["from"],"to":t["to"],"reverse":t.get("reverse").and_then(Value::as_bool).unwrap_or(false),"params":parameters(&t["effect"]["params"],false)?}));
    }
    Ok(
        json!({"id":track.id,"name":raw["name"],"locked":raw["locked"],"syncLock":raw["sync_lock"],"enabled":raw["enabled"],"muted":raw["muted"],"solo":raw["solo"],"items":items,"transitions":transitions}),
    )
}
pub fn inspect(params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let id = super::filmcraft_project_inspect::id(request.item)?
        .or(super::filmcraft_project_inspect::id(
            request.context.active_sequence,
        )?)
        .context("sequence.inspect requires item or activeSequence context")?;
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let seq = SourceSequence::read(&index, id)?;
    let rate: filmcraft_time::FrameRate =
        serde_json::from_value(seq.raw["settings"]["frame_rate"].clone())?;
    ensure!(
        rate.num > 0 && rate.den > 0 && rate.num <= u32::MAX as i64 && rate.den <= u32::MAX as i64,
        "invalid inspected frame rate"
    );
    let duration = seq.duration()?;
    let playhead = request
        .context
        .playheads
        .get(&id.to_string())
        .map(|v| {
            if let Some(s) = v.as_str() {
                s.parse::<i64>().context("invalid playhead")
            } else {
                v.as_i64().context("invalid playhead")
            }
        })
        .transpose()?;
    let selection = request
        .context
        .selection
        .into_iter()
        .map(|v| super::filmcraft_project_inspect::id(Some(v))?.context("invalid selected clip ID"))
        .collect::<Result<Vec<_>>>()?;
    let markers=seq.raw["markers"].as_array().context("missing sequence markers")?.iter().map(|m|json!({"id":m["id"],"start":m["start"],"duration":m["duration"],"kind":m["kind"],"name":m["name"],"comment":m["comment"],"color":m["color"]})).collect::<Vec<_>>();
    let mut result = json!({"id":id,"name":index.item(id).unwrap()["name"],"settings":seq.raw["settings"],"duration":duration,"durationFrames":rate.frame_at(filmcraft_time::Tick(duration)),"playhead":playhead,"in":seq.raw["mark_in"],"out":seq.raw["mark_out"],"split":seq.raw["split"],"markers":markers,"video":seq.video_tracks.iter().map(|t|track(t,rate)).collect::<Result<Vec<_>>>()?,"audio":seq.audio_tracks.iter().map(|t|track(t,rate)).collect::<Result<Vec<_>>>()?,"selection":selection});
    super::filmcraft_project_inspect::exact_numbers(&mut result);
    Ok(result)
}

/// Canonical original ParamValue representation shared by snapshot commands.
pub(super) fn canonical_parameter(value:Value)->Result<Value> {
    Ok(serde_json::to_value(serde_json::from_value::<ParamValue>(value)?)?)
}
