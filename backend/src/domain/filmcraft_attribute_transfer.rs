//! Effect transfer primitives adapted from Filmcraft clip_ops.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

/// Snapshot command admission mirrors Session::enabled_for: named valid clips may
/// enable a command, while the original selection remains the execution fallback.
pub fn edit(command: &str, mut params: Value) -> Result<Value> {
    use super::{
        filmcraft_labels::{clip_paths, context_ids},
        filmcraft_project_file::SourceProjectDocument,
        filmcraft_project_index::ProjectIndex,
        filmcraft_project_inspect::id,
    };
    use serde_json::json;
    ensure!(
        matches!(command, "edit.pasteAttributes" | "edit.removeAttributes"),
        "unknown attribute command"
    );
    let fields = params
        .as_object_mut()
        .context("invalid attribute parameters")?;
    let document = fields.remove("document").context("missing document")?;
    let context = fields.remove("context").unwrap_or_else(|| json!({}));
    let active = id(fields.remove("sequence"))?
        .or(id(context.get("activeSequence").cloned())?)
        .context("no active sequence")?;
    let bytes = match document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let qpath = format!("/project/items/{active}/kind/Sequence");
    let sequence = document
        .envelope
        .pointer(&qpath)
        .context("no active sequence")?;
    let paths = clip_paths(&document, Some(active));
    let selection = context_ids(context["selection"].as_array().cloned().unwrap_or_default())?;
    let array_ids = |v: &Value| {
        v.as_array().map(|a| {
            a.iter()
                .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .collect::<Vec<u64>>()
        })
    };
    let named = array_ids(&params["clips"]);
    ensure!(
        !selection.is_empty()
            || named
                .as_ref()
                .is_some_and(|ids| ids.iter().any(|id| paths.iter().any(|(c, _)| c == id))),
        "no selected clips"
    );
    let scalar = |v: &Value| {
        v.as_u64()
            .or_else(|| v.as_f64().map(|n| n as u64))
            .or_else(|| v.as_str()?.parse().ok())
    };
    let mut targets = named.unwrap_or_else(|| {
        scalar(&params["clip"])
            .map(|id| vec![id])
            .unwrap_or(selection)
    });
    ensure!(targets.len() <= 4096, "too many attribute targets");
    if context["linkedSelection"].as_bool().unwrap_or(true) {
        let links: Vec<_> = targets
            .iter()
            .filter_map(|id| {
                paths
                    .iter()
                    .find(|(c, _)| c == id)
                    .and_then(|(_, p)| document.envelope.pointer(p)?["link"].as_u64())
            })
            .collect();
        for (id, path) in &paths {
            if document
                .envelope
                .pointer(path)
                .and_then(|v| v["link"].as_u64())
                .is_some_and(|link| links.contains(&link))
                && !targets.contains(id)
            {
                targets.push(*id);
            }
        }
    }
    let clipboard = if command == "edit.pasteAttributes" {
        let text = params["clipboardText"]
            .as_str()
            .context("missing clipboard text")?;
        let clipboard: Value = serde_json::from_str(text)?;
        super::filmcraft_trim::validate_clipboard(&clipboard)?;
        ensure!(
            !clipboard["clips"].as_array().unwrap().is_empty(),
            "clipboard is empty"
        );
        Some(clipboard)
    } else {
        None
    };
    let width = u32::try_from(
        sequence["settings"]["width"]
            .as_u64()
            .context("invalid frame width")?,
    )?;
    let height = u32::try_from(
        sequence["settings"]["height"]
            .as_u64()
            .context("invalid frame height")?,
    )?;
    let sizes = {
        let index = ProjectIndex::new(&document)?;
        paths
            .iter()
            .map(|(id, path)| {
                (
                    *id,
                    document
                        .envelope
                        .pointer(path)
                        .and_then(|v| v["item"].as_u64())
                        .and_then(|item| index.source_size(item))
                        .unwrap_or((width, height)),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let attributes = Attributes::new(&params);
    let mut count = 0;
    for target in targets {
        let Some((_, path)) = paths.iter().find(|(id, _)| *id == target) else {
            continue;
        };
        let before = document.envelope.pointer(path).unwrap();
        let after = if let Some(clipboard) = &clipboard {
            let track_path = path.rsplit_once("/items/").context("invalid clip path")?.0;
            let kind = document
                .envelope
                .pointer(track_path)
                .and_then(|v| v["kind"].as_str())
                .unwrap_or("Video");
            let Some(source) = clipboard["clips"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry[0] == kind)
            else {
                continue;
            };
            attributes.paste(&source[2], before)?
        } else {
            attributes.remove(before, (width, height), sizes[&target])?
        };
        // Original PartialEq considers unresolved automatic Vec2 (NaN) unequal.
        fn auto_nan(v: &Value) -> bool {
            match v {
                Value::Object(o) => {
                    o.get("Vec2")
                        .is_some_and(|p| p["x"].is_null() || p["y"].is_null())
                        || o.values().any(auto_nan)
                }
                Value::Array(a) => a.iter().any(auto_nan),
                _ => false,
            }
        }
        if &after != before || auto_nan(&after) || auto_nan(before) {
            count += 1;
        }
        *document.envelope.pointer_mut(path).unwrap() = after;
    }
    super::filmcraft_marks::check_sequence(document.envelope.pointer(&qpath).unwrap())?;
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(
        json!({"result":{"clips":count},"document":document.envelope,"documentText":text,"sequence":active.to_string(),"attributeEdit":true}),
    )
}

/// Media-time span consumed by a clip, independent of reverse and frame hold.
fn span(clip: &Value) -> Result<(i64, i64)> {
    let start = clip["source_in"].as_i64().context("invalid source in")?;
    let duration = clip["duration"].as_i64().context("invalid clip duration")?;
    let speed = clip["speed"].as_f64().context("invalid clip speed")?;
    let consumed = (duration as f64 * speed.abs()).round();
    ensure!(
        consumed.is_finite() && consumed >= i64::MIN as f64 && consumed < 9223372036854775808.0,
        "source span overflow"
    );
    let end = start
        .checked_add(consumed as i64)
        .context("source out overflow")?;
    Ok((start, end))
}

/// Preserve every keyframe field except time, including mask path animation.
/// Work on a clone so a rejected timestamp cannot leave a partially retimed effect.
pub fn retime_effect(effect: &Value, from: &Value, to: &Value, scale: bool) -> Result<Value> {
    let (f0, f1) = span(from)?;
    let (t0, t1) = span(to)?;
    let source_span = f1.checked_sub(f0).context("source span overflow")?;
    let target_span = t1.checked_sub(t0).context("target span overflow")?;
    let map = |time: i64| -> Result<i64> {
        let offset = time.checked_sub(f0).context("keyframe offset overflow")?;
        let offset = if scale && source_span > 0 {
            let mapped = (offset as f64 / source_span as f64 * target_span as f64).round();
            ensure!(
                mapped.is_finite() && mapped >= i64::MIN as f64 && mapped < 9223372036854775808.0,
                "keyframe scale overflow"
            );
            mapped as i64
        } else {
            offset
        };
        t0.checked_add(offset).context("keyframe time overflow")
    };
    let retime = |param: &mut Value| -> Result<()> {
        if let Some(keys) = param.get_mut("keyframes") {
            for key in keys.as_array_mut().context("invalid keyframes")? {
                let time = key["time"].as_i64().context("invalid keyframe time")?;
                key["time"] = Value::from(map(time)?);
            }
        }
        Ok(())
    };
    let mut result = effect.clone();
    if let Some(params) = result.get_mut("params") {
        for param in params
            .as_object_mut()
            .context("invalid effect parameters")?
            .values_mut()
        {
            retime(param)?;
        }
    }
    if let Some(masks) = result.get_mut("masks") {
        for mask in masks.as_array_mut().context("invalid masks")? {
            for name in ["path", "feather", "opacity", "expansion"] {
                if let Some(param) = mask.get_mut(name) {
                    retime(param)?;
                }
            }
        }
    }
    Ok(result)
}

/// Original attribute chooser: wrong flag types retain their default of true.
pub struct Attributes {
    intrinsic: Vec<&'static str>,
    effects: Option<Vec<String>>,
    scale: bool,
}
impl Attributes {
    pub fn new(params: &Value) -> Self {
        let intrinsic = [
            ("motion", "motion"),
            ("opacity", "opacity"),
            ("timeRemapping", "time_remap"),
            ("volume", "volume"),
            ("channelVolume", "channel_volume"),
            ("panner", "panner"),
        ]
        .into_iter()
        .filter(|(flag, _)| params[*flag].as_bool().unwrap_or(true))
        .map(|(_, id)| id)
        .collect();
        let effects = match params.get("effects") {
            Some(Value::Bool(false)) => Some(Vec::new()),
            Some(Value::Array(ids)) => Some(
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            ),
            _ => None,
        };
        Self {
            intrinsic,
            effects,
            scale: params["scaleTimes"].as_bool().unwrap_or(true),
        }
    }
    fn standard(&self, effect: &Value) -> bool {
        let Some(id) = effect["effect"].as_str() else {
            return false;
        };
        id != "graphic_text"
            && id != "graphic_shape"
            && effect["essential"] != true
            && super::filmcraft_effect_catalog::definitions()
                .iter()
                .any(|d| d["id"] == id && d["intrinsic"] != true)
            && self
                .effects
                .as_ref()
                .map_or(true, |ids| ids.iter().any(|s| s == id))
    }
    /// Replace the first intrinsic instance; append standard effects in source order.
    pub fn paste(&self, from: &Value, to: &Value) -> Result<Value> {
        let mut result = to.clone();
        let effects = result["effects"]
            .as_array_mut()
            .context("missing target effects")?;
        let sources = from["effects"]
            .as_array()
            .context("missing source effects")?;
        for id in &self.intrinsic {
            if let Some(effect) = sources.iter().find(|e| e["effect"] == *id) {
                let effect = retime_effect(effect, from, to, self.scale)?;
                if let Some(slot) = effects.iter_mut().find(|e| e["effect"] == *id) {
                    *slot = effect;
                } else {
                    effects.push(effect);
                }
            }
        }
        for effect in sources.iter().filter(|e| self.standard(e)) {
            effects.push(retime_effect(effect, from, to, self.scale)?);
        }
        Ok(result)
    }
    /// Reset selected intrinsic instances with source-aware automatic anchor points.
    pub fn remove(&self, clip: &Value, frame: (u32, u32), source: (u32, u32)) -> Result<Value> {
        let mut result = clip.clone();
        let effects = result["effects"]
            .as_array_mut()
            .context("missing target effects")?;
        for id in &self.intrinsic {
            if let Some(slot) = effects.iter_mut().find(|e| e["effect"] == *id) {
                if let Some(def) = super::filmcraft_effect_catalog::definitions()
                    .iter()
                    .find(|d| d["id"] == *id)
                {
                    *slot = super::filmcraft_effect_edit::fresh(def, frame.0, frame.1);
                    if *id == "motion" {
                        let source_defaults =
                            super::filmcraft_effect_edit::fresh(def, source.0, source.1);
                        slot["params"]["anchor"] = source_defaults["params"]["anchor"].clone();
                    }
                }
            }
        }
        effects.retain(|e| !self.standard(e));
        Ok(result)
    }
}
