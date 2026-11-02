//! Original effects.setParam (Filmcraft MIT, pinned 52318524); explicit snapshots.
use super::{
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceTrackItem,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
    playhead: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    sequence: Option<Value>,
    clip: Value,
    effect: Option<Value>,
    param: String,
    #[serde(default)]
    value: Value,
    media_time: Option<Value>,
    to: Option<Value>,
    interpolation: Option<Value>,
    in_influence: Option<Value>,
    out_influence: Option<Value>,
    mask: Option<Value>,
    time: Option<Value>,
    frame: Option<Value>,
    seconds: Option<Value>,
    timecode: Option<Value>,
    #[serde(default)]
    context: SessionContext,
    #[serde(default)]
    merge: bool,
    #[serde(default)]
    begin: bool,
}
fn point(value: &Value) -> Option<Value> {
    let (x, y) = if let Some(a) = value.as_array() {
        (a.first()?.as_f64()?, a.get(1)?.as_f64()?)
    } else {
        (value["x"].as_f64()?, value["y"].as_f64()?)
    };
    Some(json!({"x":x,"y":y}))
}
pub(super) fn path(value: &Value) -> Result<Value> {
    use super::filmcraft_sequence_inspect::canonical_parameter;
    if let Ok(path) = canonical_parameter(json!({"Path":value})) {
        return Ok(path);
    }
    let array = value
        .as_array()
        .or_else(|| value["vertices"].as_array())
        .context("invalid mask path")?;
    let mut vertices = Vec::new();
    for v in array {
        let vertex = if let Some(p) = v.get("p") {
            json!({"p":point(p).context("invalid path point")?,"t_in":v.get("in").or_else(||v.get("t_in")).and_then(point).unwrap_or(json!({"x":0.,"y":0.})),"t_out":v.get("out").or_else(||v.get("t_out")).and_then(point).unwrap_or(json!({"x":0.,"y":0.}))})
        } else {
            json!({"p":point(v).context("invalid path vertex")?})
        };
        vertices.push(vertex);
    }
    canonical_parameter(
        json!({"Path":{"vertices":vertices,"closed":value["closed"].as_bool().unwrap_or(true)}}),
    )
}
fn convert(template: &Value, value: &Value) -> Result<Value> {
    use super::filmcraft_sequence_inspect::canonical_parameter;
    let tag = template
        .as_object()
        .and_then(|v| v.keys().next())
        .context("invalid parameter template")?;
    let out = match tag.as_str() {
        "Float" => json!({"Float":value.as_f64().context("expected number")?}),
        "Vec2" => {
            let a = value.as_array().context("expected point array")?;
            json!({"Vec2":{"x":a.first().and_then(Value::as_f64).context("expected point x")?,"y":a.get(1).and_then(Value::as_f64).context("expected point y")?}})
        }
        "Color" => {
            let color = if let Some(s) = value.as_str() {
                filmcraft_color::parse_hex(s).context("invalid hex color")?
            } else {
                let a = value.as_array().context("expected color array")?;
                [
                    a.first().and_then(Value::as_f64).context("expected red")? as f32,
                    a.get(1).and_then(Value::as_f64).context("expected green")? as f32,
                    a.get(2).and_then(Value::as_f64).context("expected blue")? as f32,
                    a.get(3).and_then(Value::as_f64).unwrap_or(1.) as f32,
                ]
            };
            ensure!(
                color.iter().all(|v| v.is_finite()),
                "invalid color component"
            );
            json!({"Color":color})
        }
        "Bool" => json!({"Bool":value.as_bool().context("expected boolean")?}),
        "Choice" => json!({"Choice":value.as_u64().context("expected choice index")? as u32}),
        "Text" => json!({"Text":value.as_str().context("expected text")?}),
        "Curve" => {
            let a = value.as_array().context("expected curve array")?;
            let points: Vec<_> = a
                .iter()
                .filter_map(|p| Some([p.get(0)?.as_f64()? as f32, p.get(1)?.as_f64()? as f32]))
                .collect();
            ensure!(
                points.iter().flatten().all(|v| v.is_finite()),
                "invalid curve component"
            );
            json!({"Curve":points})
        }
        "Path" => return path(value),
        _ => anyhow::bail!("unsupported parameter type"),
    };
    canonical_parameter(out)
}
fn check_sequence(seq: &Value) -> Result<()> {
    let settings = &seq["settings"];
    let width = settings["width"].as_u64().context("missing frame width")?;
    let height = settings["height"]
        .as_u64()
        .context("missing frame height")?;
    ensure!(
        width > 0 && height > 0 && width <= 32768 && height <= 32768 && width * height <= 268435456,
        "invalid source frame size"
    );
    ensure!(
        settings["sample_rate"]
            .as_u64()
            .is_some_and(|v| v > 0 && v <= 384000),
        "invalid sample rate"
    );
    ensure!(
        settings["par"].as_array().is_some_and(|p| p.len() == 2
            && p.iter()
                .all(|v| v.as_u64().is_some_and(|v| v > 0 && v <= u32::MAX as u64))),
        "invalid pixel aspect"
    );
    let bounded = |v: i64| v >= filmcraft_time::Tick::MIN.0 && v <= filmcraft_time::Tick::MAX.0;
    for field in ["mark_in", "mark_out"] {
        if !seq[field].is_null() {
            ensure!(
                seq[field].as_i64().is_some_and(bounded),
                "invalid sequence mark"
            );
        }
    }
    let range = |v: &Value| -> Result<(i64, i64)> {
        let start = v["start"].as_i64().context("missing timeline start")?;
        let duration = v["duration"].as_i64().context("missing duration")?;
        let end = start
            .checked_add(duration)
            .context("source range overflow")?;
        ensure!(
            bounded(start) && bounded(duration) && bounded(end),
            "source range exceeds bounds"
        );
        Ok((start, end))
    };
    if !seq["work_area"].is_null() {
        range(&seq["work_area"])?;
    }
    for (tracks, items) in [
        ("video_tracks", "items"),
        ("audio_tracks", "items"),
        ("caption_tracks", "captions"),
    ] {
        let Some(array) = seq.get(tracks) else {
            ensure!(tracks == "caption_tracks", "missing source tracks");
            continue;
        };
        for track in array.as_array().context("invalid source tracks")? {
            let mut previous = None;
            for item in track[items].as_array().context("missing track items")? {
                let (start, end) = range(item)?;
                ensure!(end > start, "non-positive source duration");
                ensure!(previous.is_none_or(|p| p <= start), "source items overlap");
                previous = Some(end);
            }
            if items == "items" {
                for transition in track["transitions"]
                    .as_array()
                    .context("missing transitions")?
                {
                    range(transition)?;
                }
            }
        }
    }
    Ok(())
}
pub fn edit(params: Value) -> Result<Value> {
    edit_command("effects.setParam", params)
}
pub fn edit_command(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(
            command,
            "effects.setParam"
                | "effects.deleteKeyframe"
                | "effects.moveKeyframe"
                | "effects.setInterpolation"
                | "effects.setKeyframe"
                | "effects.toggleAnimation"
                | "effects.addKeyframe"
        ),
        "unsupported parameter command"
    );
    let has_value = params.get("value").is_some();
    let request: Request = serde_json::from_value(params)?;
    // merge/begin govern the source engine's Undo session; snapshots have no implicit history.
    let _history_flags = (request.merge, request.begin);
    let sequence = super::filmcraft_project_inspect::id(request.sequence)?
        .or(super::filmcraft_project_inspect::id(
            request.context.active_sequence,
        )?)
        .context("setParam requires active sequence")?;
    let clip =
        super::filmcraft_project_inspect::id(Some(request.clip))?.context("missing clip ID")?;
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let seq = index
        .item(sequence)
        .and_then(|v| v.pointer("/kind/Sequence"))
        .context("missing sequence")?;
    check_sequence(seq)?;
    let playhead = match request.context.playhead {
        None => 0,
        Some(Value::String(s)) => s.parse()?,
        Some(v) => v.as_i64().context("invalid playhead")?,
    };
    let clock = super::filmcraft_export_command_range::Clock {
        rate: serde_json::from_value(seq["settings"]["frame_rate"].clone())?,
        drop_frame: seq["settings"]["drop_frame"].as_bool().unwrap_or(false),
        playhead: filmcraft_time::Tick(playhead),
    };
    let timeline = if command == "effects.toggleAnimation" {
        playhead
    } else {
        clock.time(&json!({"paramTime":request.time,"paramFrame":request.frame,"paramSeconds":request.seconds,"paramTimecode":request.timecode}),"param")?.unwrap_or(playhead)
    };
    let (clip_path, _locked) =
        super::filmcraft_parameter_edit::location(&document, sequence, clip)?;
    let item = SourceTrackItem::read(document.envelope.pointer(&clip_path).unwrap())?;
    let upper = item
        .end()
        .context("clip end overflow")?
        .checked_sub(1)
        .context("clip end underflow")?
        .max(item.timing.timeline_start);
    let media_time = if let Some(held) = item.timing.frame_hold {
        held
    } else {
        item.clocks(timeline.clamp(item.timing.timeline_start, upper))?
            .frame
    };
    let selector = request.effect.unwrap_or(json!("motion"));
    let effects = item.raw["effects"].as_array().context("missing effects")?;
    let effect_index = match &selector {
        Value::Number(n) => usize::try_from(n.as_u64().unwrap_or(0))
            .ok()
            .filter(|i| *i < effects.len()),
        Value::String(id) => effects.iter().position(|e| e["effect"] == *id),
        _ => None,
    }
    .context("no such effect on clip")?;
    let effect_id = effects[effect_index]["effect"]
        .as_str()
        .context("missing effect identity")?;
    let mut changed = document.clone();
    let effect = &mut changed.envelope.pointer_mut(&clip_path).unwrap()["effects"][effect_index];
    if command == "effects.setParam" && (selector == "motion" || selector == "opacity") {
        effect["enabled"] = json!(true);
    }
    let params = effect["params"]
        .as_object_mut()
        .context("missing effect params")?;
    if command == "effects.setParam" && !params.contains_key(&request.param) {
        if let Some(default) = super::filmcraft_effect_catalog::definitions()
            .iter()
            .find(|d| d["id"] == effect_id)
            .and_then(|d| {
                d["params"]
                    .as_array()?
                    .iter()
                    .find(|p| p["id"] == request.param)
            })
        {
            params.insert(request.param.clone(), json!({"value":default["default"]}));
        }
    }
    let mask = request
        .mask
        .as_ref()
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|v| v as u64)))
        .map(usize::try_from)
        .transpose()?;
    let parameter = if let Some(mask) = mask {
        ensure!(
            ["path", "feather", "opacity", "expansion"].contains(&request.param.as_str()),
            "no such mask parameter"
        );
        effect
            .get_mut("masks")
            .and_then(Value::as_array_mut)
            .and_then(|m| m.get_mut(mask))
            .and_then(|m| m.get_mut(&request.param))
    } else {
        effect["params"].get_mut(&request.param)
    }
    .context("no such effect parameter")?;

    let mut keys = parameter
        .get("keyframes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if command != "effects.setParam" {
        let tick = |v: Option<Value>| -> Option<i64> {
            v.and_then(|v| match v {
                Value::String(s) => s.parse().ok(),
                v => v.as_i64(),
            })
        };
        let at = tick(request.media_time);
        match command {
            "effects.toggleAnimation" => {
                if keys.is_empty() {
                    keys.push(json!({"time":media_time,"value":parameter["value"],"interp":"Linear","in_influence":1./3.,"out_influence":1./3.}));
                } else {
                    parameter["value"] =
                        super::filmcraft_parameter_sample::sample(parameter, media_time)?;
                    keys.clear();
                }
            }
            "effects.addKeyframe" => {
                if let Ok(i) = keys
                    .binary_search_by_key(&media_time, |k| k["time"].as_i64().unwrap_or(i64::MIN))
                {
                    let removed = keys.remove(i);
                    if keys.is_empty() {
                        parameter["value"] = removed["value"].clone();
                    }
                } else {
                    let value = super::filmcraft_parameter_sample::sample(parameter, media_time)?;
                    if keys.is_empty() {
                        parameter["value"] = value.clone();
                    }
                    let i = keys
                        .partition_point(|k| k["time"].as_i64().is_some_and(|t| t < media_time));
                    keys.insert(i,json!({"time":media_time,"value":value,"interp":"Linear","in_influence":1./3.,"out_influence":1./3.}));
                }
            }
            "effects.deleteKeyframe" => {
                let at = at.context("need mediaTime")?;
                if let Some(i) = keys.iter().position(|k| k["time"].as_i64() == Some(at)) {
                    let removed = keys.remove(i);
                    if keys.is_empty() {
                        parameter["value"] = removed["value"].clone();
                    }
                }
            }
            "effects.moveKeyframe" => {
                let at = at.context("need mediaTime")?;
                let to = tick(request.to).context("need to")?;
                if let Some(i) = keys.iter().position(|k| k["time"].as_i64() == Some(at)) {
                    let mut key = keys.remove(i);
                    key["time"] = json!(to);
                    keys.retain(|k| k["time"].as_i64() != Some(to));
                    let i = keys.partition_point(|k| k["time"].as_i64().is_some_and(|t| t < to));
                    keys.insert(i, key);
                }
            }
            "effects.setKeyframe" => {
                let at = at.context("need mediaTime")?;
                let key = keys
                    .iter_mut()
                    .find(|k| k["time"].as_i64() == Some(at))
                    .context("no keyframe at that time")?;
                if has_value {
                    key["value"] = convert(&key["value"], &request.value)?;
                }
                if let Some(x) = request.in_influence.and_then(|v| v.as_f64()) {
                    key["in_influence"] = json!(x.clamp(0.01, 1.));
                }
                if let Some(x) = request.out_influence.and_then(|v| v.as_f64()) {
                    key["out_influence"] = json!(x.clamp(0.01, 1.));
                }
            }
            "effects.setInterpolation" => {
                let at = at.unwrap_or(media_time);
                let kind = match request
                    .interpolation
                    .as_ref()
                    .and_then(Value::as_str)
                    .unwrap_or("linear")
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "bezier" => "Bezier",
                    "autobezier" => "AutoBezier",
                    "continuousbezier" => "ContinuousBezier",
                    "hold" => "Hold",
                    "easein" => "EaseIn",
                    "easeout" => "EaseOut",
                    _ => "Linear",
                };
                let key = keys
                    .iter_mut()
                    .find(|k| k["time"].as_i64() == Some(at))
                    .context("no keyframe at that time")?;
                key["interp"] = json!(kind);
            }
            _ => unreachable!(),
        }
        if keys.is_empty() {
            parameter.as_object_mut().unwrap().remove("keyframes");
        } else {
            parameter["keyframes"] = json!(keys);
        }
    } else {
        let value = convert(&parameter["value"], &request.value)?;
        if keys.is_empty() {
            parameter["value"] = value;
            parameter.as_object_mut().unwrap().remove("keyframes");
        } else {
            ensure!(
                keys.iter().all(|k| k["time"].as_i64().is_some()),
                "invalid source key time"
            );
            match keys.binary_search_by_key(&media_time, |k| k["time"].as_i64().unwrap()) {
                Ok(i) => keys[i]["value"] = value,
                Err(i) => {
                    let interp = keys
                        .get(i.saturating_sub(1))
                        .and_then(|k| k.get("interp"))
                        .cloned()
                        .unwrap_or(json!("Linear"));
                    keys.insert(i,json!({"time":media_time,"value":value,"interp":interp,"in_influence":1./3.,"out_influence":1./3.}));
                }
            }
            parameter["keyframes"] = json!(keys);
        }
    }
    let text = String::from_utf8(changed.encode_preserved()?)?;
    Ok(
        json!({"result":null,"document":changed.envelope,"documentText":text,"sequence":sequence.to_string(),"clip":clip.to_string()}),
    )
}
