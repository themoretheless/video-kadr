//! Exact source-project parameter and effect-mask authoring. No JavaScript or microsecond clock lowering.
use super::{
    filmcraft_effect_scalar::{self, Evaluation},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceTrackItem,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Address {
    pub sequence_id: String,
    pub clip_id: String,
    pub effect_index: usize,
    pub effect_kind: String,
    pub parameter: String,
    #[serde(default)]
    pub mask_index: Option<usize>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    SetBase {
        value: Value,
    },
    Upsert {
        time: String,
        value: Value,
    },
    AtPlayhead {
        value: Value,
    },
    Delete {
        time: String,
    },
    Move {
        time: String,
        destination: String,
    },
    Interpolation {
        time: String,
        interpolation: String,
    },
    Influence {
        time: String,
        incoming: f64,
        outgoing: f64,
    },
}
fn scalar(value: &Value) -> Result<()> {
    let value = value.as_object().context("expected scalar value")?;
    ensure!(value.len() == 1, "ambiguous scalar value");
    ensure!(
        value
            .get("Float")
            .is_some_and(|v| v.as_f64().is_some_and(f64::is_finite))
            || value
                .get("Choice")
                .is_some_and(|v| v.as_u64().is_some_and(|v| v <= u32::MAX as u64))
            || value.get("Bool").is_some_and(Value::is_boolean),
        "unsupported scalar value"
    );
    Ok(())
}
/// Supported original ParamValue variants. Null Vec2 components are source
/// automatic coordinates, not zero. Preserve them until a geometry-aware render.
fn parameter_value(value: &Value) -> Result<()> {
    if scalar(value).is_ok() {
        return Ok(());
    }
    let tagged = value
        .as_object()
        .context("expected tagged parameter value")?;
    ensure!(tagged.len() == 1, "ambiguous parameter value");
    if let Some(point) = tagged.get("Vec2") {
        let point = point.as_object().context("expected point components")?;
        ensure!(
            point.len() == 2 && point.contains_key("x") && point.contains_key("y"),
            "expected exactly x/y point components"
        );
        ensure!(
            point
                .values()
                .all(|v| v.is_null() || v.as_f64().is_some_and(f64::is_finite)),
            "invalid point component"
        );
        return Ok(());
    }
    if let Some(color) = tagged.get("Color") {
        let color = color.as_array().context("expected RGBA components")?;
        ensure!(
            color.len() == 4
                && color
                    .iter()
                    .all(|v| v.as_f64().is_some_and(|n| (n as f32).is_finite())),
            "expected four finite f32 color channels"
        );
        return Ok(());
    }
    if let Some(text) = tagged.get("Text") {
        ensure!(
            text.as_str().is_some_and(|s| s.len() <= 65536),
            "invalid or oversized parameter text"
        );
        return Ok(());
    }
    if let Some(curve) = tagged.get("Curve") {
        let curve = curve.as_array().context("expected curve control points")?;
        ensure!(curve.len() <= 4096, "curve point budget exceeded");
        ensure!(
            curve
                .iter()
                .all(|p| p.as_array().is_some_and(|p| p.len() == 2
                    && p.iter()
                        .all(|v| v.as_f64().is_some_and(|n| (n as f32).is_finite())))),
            "expected finite f32 curve pairs"
        );
        return Ok(());
    }
    if let Some(path) = tagged.get("Path") {
        let path = path.as_object().context("invalid source path")?;
        if let Some(closed) = path.get("closed") {
            ensure!(closed.is_boolean(), "invalid path closure");
        }
        let vertices = path
            .get("vertices")
            .and_then(Value::as_array)
            .context("missing path vertices")?;
        ensure!(vertices.len() <= 4096, "path vertex budget exceeded");
        for vertex in vertices {
            let vertex = vertex.as_object().context("invalid path vertex")?;
            for name in ["p", "t_in", "t_out"] {
                if name != "p" && !vertex.contains_key(name) {
                    continue;
                }
                let point = vertex
                    .get(name)
                    .and_then(Value::as_object)
                    .context("invalid path point/tangent")?;
                ensure!(
                    ["x", "y"].iter().all(|axis| point
                        .get(*axis)
                        .is_some_and(|v| v.as_f64().is_some_and(f64::is_finite))),
                    "path coordinates must be finite, not auto"
                );
            }
        }
        return Ok(());
    }
    anyhow::bail!("unsupported parameter value")
}
fn interpolation(value: &str) -> Result<()> {
    ensure!(
        [
            "Linear",
            "Hold",
            "Bezier",
            "AutoBezier",
            "ContinuousBezier",
            "EaseIn",
            "EaseOut"
        ]
        .contains(&value),
        "invalid source interpolation"
    );
    Ok(())
}
fn validate(parameter: &Value) -> Result<()> {
    parameter_value(parameter.get("value").context("missing parameter value")?)?;
    let Some(keys) = parameter.get("keyframes") else {
        return Ok(());
    };
    let keys = keys.as_array().context("invalid source keyframes")?;
    ensure!(keys.len() <= 4096, "source keyframe budget exceeded");
    let mut previous = None;
    for key in keys {
        let time = key["time"].as_i64().context("invalid signed key time")?;
        ensure!(
            previous.is_none_or(|old| old < time),
            "source keys must be strictly ordered"
        );
        previous = Some(time);
        parameter_value(&key["value"])?;
        interpolation(
            key["interp"]
                .as_str()
                .context("missing source interpolation")?,
        )?;
        for field in ["in_influence", "out_influence"] {
            if let Some(value) = key.get(field) {
                ensure!(
                    value.as_f64().is_some_and(f64::is_finite),
                    "nonfinite source influence"
                );
            }
        }
    }
    Ok(())
}
pub(super) fn location(
    document: &SourceProjectDocument,
    sequence: u64,
    clip: u64,
) -> Result<(String, bool)> {
    let index = ProjectIndex::new(document)?;
    let seq = index
        .item(sequence)
        .and_then(|value| value.pointer("/kind/Sequence"))
        .context("source sequence missing")?;
    let mut found = None;
    for kind in ["video_tracks", "audio_tracks"] {
        for (track_index, track) in seq[kind]
            .as_array()
            .context("source tracks missing")?
            .iter()
            .enumerate()
        {
            for (item_index, item) in track["items"]
                .as_array()
                .context("source items missing")?
                .iter()
                .enumerate()
            {
                if item["id"].as_u64() == Some(clip) {
                    ensure!(found.is_none(), "ambiguous source clip identity");
                    found=Some((format!("/project/items/{sequence}/kind/Sequence/{kind}/{track_index}/items/{item_index}"),track["locked"].as_bool().unwrap_or(false)));
                }
            }
        }
    }
    found.context("source clip missing")
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
pub(super) fn parameter_path(
    document: &SourceProjectDocument,
    address: &Address,
) -> Result<(String, bool)> {
    ensure!(
        address.effect_index < 1024
            && !address.effect_kind.is_empty()
            && address.effect_kind.len() <= 128
            && !address.parameter.is_empty()
            && address.parameter.len() <= 256,
        "invalid parameter address"
    );
    let (clip, locked) = location(
        document,
        address.sequence_id.parse()?,
        address.clip_id.parse()?,
    )?;
    let effect = document
        .envelope
        .pointer(&format!("{clip}/effects/{}", address.effect_index))
        .context("source effect missing")?;
    ensure!(
        effect["effect"].as_str() == Some(address.effect_kind.as_str()),
        "source effect identity changed"
    );
    let path = if let Some(mask) = address.mask_index {
        ensure!(
            mask < 1024
                && ["path", "feather", "opacity", "expansion"]
                    .contains(&address.parameter.as_str()),
            "invalid mask parameter address"
        );
        format!(
            "{clip}/effects/{}/masks/{mask}/{}",
            address.effect_index,
            escape(&address.parameter)
        )
    } else {
        format!(
            "{clip}/effects/{}/params/{}",
            address.effect_index,
            escape(&address.parameter)
        )
    };
    validate(
        document
            .envelope
            .pointer(&path)
            .context("source parameter missing")?,
    )?;
    Ok((path, locked))
}
fn describe_parameter(name: &str, parameter: &Value, tick: i64) -> Result<Option<Value>> {
    if parameter_value(&parameter["value"]).is_err() {
        return Ok(None);
    }
    validate(parameter)?;
    let keys = parameter["keyframes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|key| {
            let mut key = key.clone();
            key["time"] = json!(key["time"].as_i64().unwrap().to_string());
            key
        })
        .collect::<Vec<_>>();
    // Sampling consumes only the known scalar fields; authoring keeps the
    // original object (including future extension fields) untouched.
    let mut sample_parameter = json!({"value": parameter["value"]});
    if let Some(keys) = parameter["keyframes"].as_array() {
        sample_parameter["keyframes"] = json!(keys
            .iter()
            .map(|key| {
                let mut known = serde_json::Map::new();
                for name in ["time", "value", "interp", "in_influence", "out_influence"] {
                    if let Some(value) = key.get(name) {
                        known.insert(name.into(), value.clone());
                    }
                }
                Value::Object(known)
            })
            .collect::<Vec<_>>());
    }
    let sampled = filmcraft_effect_scalar::convert(&sample_parameter, Evaluation::EffectParameter)
        .ok()
        .map(|curve| filmcraft_effect_scalar::sample(&curve, tick));
    let sampled_components = if parameter["value"].get("Vec2").is_some() {
        super::filmcraft_component_curve::point(&sample_parameter)
            .ok()
            .map(|curves| {
                curves
                    .iter()
                    .map(|curve| filmcraft_effect_scalar::sample(curve, tick))
                    .collect::<Vec<_>>()
            })
    } else if parameter["value"].get("Color").is_some() {
        super::filmcraft_component_curve::color(&sample_parameter)
            .ok()
            .map(|curves| {
                curves
                    .iter()
                    .map(|curve| f64::from(filmcraft_effect_scalar::sample(curve, tick) as f32))
                    .collect::<Vec<_>>()
            })
    } else {
        None
    };
    let sampled_value =
        if parameter["value"].get("Text").is_some() || parameter["value"].get("Curve").is_some() {
            let tag = if parameter["value"].get("Text").is_some() {
                "Text"
            } else {
                "Curve"
            };
            let keys = parameter["keyframes"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if keys.iter().all(|key| key["value"].get(tag).is_some()) {
                Some(
                    keys.iter()
                        .rev()
                        .find(|key| key["time"].as_i64().unwrap() <= tick)
                        .or_else(|| keys.first())
                        .map_or(&parameter["value"], |key| &key["value"])
                        .clone(),
                )
            } else {
                None
            }
        } else {
            None
        };
    Ok(Some(
        json!({"name":name,"value":parameter["value"],"keys":keys,"sampled":sampled,"sampledComponents":sampled_components,"sampledValue":sampled_value}),
    ))
}
pub fn inspect(
    document: &SourceProjectDocument,
    sequence: u64,
    clip: u64,
    timeline: i64,
) -> Result<Value> {
    let (path, locked) = location(document, sequence, clip)?;
    let item = document.envelope.pointer(&path).unwrap();
    let clocks = SourceTrackItem::read(item)?.clocks(timeline)?;
    let effects = item["effects"]
        .as_array()
        .context("source effects missing")?;
    ensure!(effects.len() <= 1024, "source effect budget exceeded");
    let mut result = vec![];
    for (index, effect) in effects.iter().enumerate() {
        let Some(params) = effect["params"].as_object() else {
            continue;
        };
        ensure!(params.len() <= 1024, "source parameter budget exceeded");
        let mut parameters = vec![];
        for (name, parameter) in params {
            if let Some(parameter) = describe_parameter(name, parameter, clocks.parameters)? {
                parameters.push(parameter);
            }
        }
        let mut masks = vec![];
        if let Some(raw_masks) = effect.get("masks") {
            let raw_masks = raw_masks.as_array().context("invalid effect masks")?;
            ensure!(raw_masks.len() <= 1024, "mask authoring budget exceeded");
            for (index, mask) in raw_masks.iter().enumerate() {
                let mut parameters = vec![];
                for name in ["path", "feather", "opacity", "expansion"] {
                    if let Some(parameter) = mask.get(name) {
                        if let Some(parameter) =
                            describe_parameter(name, parameter, clocks.parameters)?
                        {
                            parameters.push(parameter);
                        }
                    }
                }
                masks.push(json!({"index":index,"name":mask["name"],"mode":mask["mode"].as_str().unwrap_or("Add"),"inverted":mask["inverted"].as_bool().unwrap_or(false),"trackMethod":mask["track_method"].as_str().unwrap_or("PositionScaleRotation"),"parameters":parameters}));
            }
        }
        result.push(json!({"index":index,"kind":effect["effect"],"enabled":effect["enabled"],"maskable":maskable_effect(effect["effect"].as_str().unwrap_or("")),"parameters":parameters,"masks":masks}));
    }
    Ok(
        json!({"locked":locked,"timelineTicks":timeline.to_string(),"parameterTicks":clocks.parameters.to_string(),"frameTicks":clocks.frame.to_string(),"generatorTicks":clocks.generator.to_string(),"effects":result}),
    )
}
pub fn edit(
    document: &SourceProjectDocument,
    address: &Address,
    command: Command,
    timeline: i64,
) -> Result<SourceProjectDocument> {
    let (path, locked) = parameter_path(document, address)?;
    ensure!(!locked, "source track is locked");
    let mut changed = document.clone();
    let parameter = changed.envelope.pointer_mut(&path).unwrap();
    if let Command::SetBase { value } = command {
        parameter_value(&value)?;
        parameter["value"] = value;
    } else {
        let mut keys = parameter["keyframes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let at = |time: &str| -> Result<usize> {
            let time = time.parse::<i64>()?;
            keys.iter()
                .position(|key| key["time"].as_i64() == Some(time))
                .context("source key missing")
        };
        match command {
            Command::SetBase { .. } => unreachable!(),
            Command::Upsert { time, value } => {
                parameter_value(&value)?;
                let time = time.parse::<i64>()?;
                if let Some(key) = keys
                    .iter_mut()
                    .find(|key| key["time"].as_i64() == Some(time))
                {
                    key["value"] = value
                } else {
                    keys.push(json!({"time":time,"value":value,"interp":"Linear","in_influence":1./3.,"out_influence":1./3.}));
                }
            }
            Command::AtPlayhead { value } => {
                let (clip_path, _) = location(
                    document,
                    address.sequence_id.parse()?,
                    address.clip_id.parse()?,
                )?;
                let time = SourceTrackItem::read(document.envelope.pointer(&clip_path).unwrap())?
                    .clocks(timeline)?
                    .parameters;
                parameter_value(&value)?;
                if let Some(key) = keys
                    .iter_mut()
                    .find(|key| key["time"].as_i64() == Some(time))
                {
                    key["value"] = value
                } else {
                    keys.push(json!({"time":time,"value":value,"interp":"Linear","in_influence":1./3.,"out_influence":1./3.}));
                }
            }
            Command::Delete { time } => {
                let index = at(&time)?;
                let removed = keys.remove(index);
                if keys.is_empty() {
                    parameter["value"] = removed["value"].clone();
                }
            }
            Command::Move { time, destination } => {
                let index = at(&time)?;
                let destination = destination.parse::<i64>()?;
                ensure!(
                    !keys
                        .iter()
                        .enumerate()
                        .any(|(i, key)| i != index && key["time"].as_i64() == Some(destination)),
                    "source key collision"
                );
                keys[index]["time"] = json!(destination);
            }
            Command::Interpolation {
                time,
                interpolation: mode,
            } => {
                let index = at(&time)?;
                interpolation(&mode)?;
                keys[index]["interp"] = json!(mode);
            }
            Command::Influence {
                time,
                incoming,
                outgoing,
            } => {
                let index = at(&time)?;
                ensure!(
                    incoming.is_finite() && outgoing.is_finite(),
                    "nonfinite influence"
                );
                keys[index]["in_influence"] = json!(incoming);
                keys[index]["out_influence"] = json!(outgoing);
            }
        }
        keys.sort_by_key(|key| key["time"].as_i64().unwrap());
        parameter["keyframes"] = json!(keys);
    }
    validate(parameter)?;
    ProjectIndex::new(&changed)?;
    Ok(changed)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectAddress {
    pub sequence_id: String,
    pub clip_id: String,
    pub effect_index: usize,
    pub effect_kind: String,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum MaskShape {
    Rectangle { x0: f64, y0: f64, x1: f64, y1: f64 },
    Ellipse { x0: f64, y0: f64, x1: f64, y1: f64 },
    FreeDraw,
    Path { path: Value },
}
#[derive(Clone, Copy, Deserialize, serde::Serialize)]
pub enum MaskMode {
    None,
    Add,
    Subtract,
    Intersect,
    Lighten,
    Darken,
    Difference,
}
#[derive(Clone, Copy, Deserialize, serde::Serialize)]
pub enum MaskTrackMethod {
    Position,
    PositionRotation,
    PositionScaleRotation,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum MaskCommand {
    Create {
        name: String,
        shape: MaskShape,
    },
    Delete {
        mask_index: usize,
    },
    Move {
        mask_index: usize,
        destination: usize,
    },
    Settings {
        mask_index: usize,
        name: Option<String>,
        mode: Option<MaskMode>,
        inverted: Option<bool>,
        track_method: Option<MaskTrackMethod>,
    },
}
pub(super) fn new_mask(name: String, shape: MaskShape) -> Result<Value> {
    ensure!(name.len() <= 4096, "mask name exceeds authoring budget");
    let path = match shape {
        MaskShape::FreeDraw => json!({"vertices":[],"closed":false}),
        MaskShape::Path { path } => {
            parameter_value(&json!({"Path":path}))?;
            let vertices = path["vertices"]
                .as_array()
                .context("missing mask vertices")?;
            ensure!(
                path["closed"].as_bool() == Some(false) || vertices.len() >= 3,
                "a closed mask needs at least three vertices"
            );
            path
        }
        MaskShape::Rectangle { x0, y0, x1, y1 } | MaskShape::Ellipse { x0, y0, x1, y1 } => {
            ensure!(
                [x0, y0, x1, y1].iter().all(|n| n.is_finite()) && x1 > x0 && y1 > y0,
                "mask bounds must be finite and increasing"
            );
            if matches!(shape, MaskShape::Rectangle { .. }) {
                json!({"closed":true,"vertices":[{"p":{"x":x0,"y":y0}},{"p":{"x":x1,"y":y0}},{"p":{"x":x1,"y":y1}},{"p":{"x":x0,"y":y1}}]})
            } else {
                // Original MaskPath::ellipse: top/right/bottom/left with relative
                // mirrored cubic handles, in full-resolution clip pixel space.
                let (cx, cy, rx, ry) = (
                    x0 / 2. + x1 / 2.,
                    y0 / 2. + y1 / 2.,
                    x1 / 2. - x0 / 2.,
                    y1 / 2. - y0 / 2.,
                );
                let k = 0.552_284_749_830_793_4;
                let vertex = |x: f64, y: f64, tx: f64, ty: f64| json!({"p":{"x":x,"y":y},"t_in":{"x":-tx,"y":-ty},"t_out":{"x":tx,"y":ty}});
                json!({"closed":true,"vertices":[vertex(cx,cy-ry,rx*k,0.),vertex(cx+rx,cy,0.,ry*k),vertex(cx,cy+ry,-rx*k,0.),vertex(cx-rx,cy,0.,-ry*k)]})
            }
        }
    };
    parameter_value(&json!({"Path":path}))?;
    Ok(
        json!({"name":name,"path":{"value":{"Path":path}},"feather":{"value":{"Float":10.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}},"inverted":false,"mode":"Add","track_method":"PositionScaleRotation"}),
    )
}
/// Pinned original EffectKind::Video admission, excluding intrinsic Motion.
pub fn maskable_effect(kind: &str) -> bool {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("filmcraft_maskable_effects.json"))
            .expect("embedded mask effect catalogue")["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    })
    .iter()
    .any(|id| id == kind)
}
pub fn edit_mask(
    document: &SourceProjectDocument,
    address: &EffectAddress,
    command: MaskCommand,
) -> Result<SourceProjectDocument> {
    ensure!(
        address.effect_index < 1024
            && !address.effect_kind.is_empty()
            && address.effect_kind.len() <= 128,
        "invalid source effect address"
    );
    let (clip, locked) = location(
        document,
        address.sequence_id.parse()?,
        address.clip_id.parse()?,
    )?;
    ensure!(!locked, "source track is locked");
    let path = format!("{clip}/effects/{}", address.effect_index);
    let effect = document
        .envelope
        .pointer(&path)
        .context("source effect missing")?;
    ensure!(
        effect["effect"].as_str() == Some(address.effect_kind.as_str()),
        "source effect identity changed"
    );
    let mut masks = effect
        .get("masks")
        .map(|v| v.as_array().cloned().context("invalid effect masks"))
        .transpose()?
        .unwrap_or_default();
    ensure!(masks.len() <= 1024, "mask authoring budget exceeded");
    match command {
        MaskCommand::Create { name, shape } => {
            ensure!(
                maskable_effect(&address.effect_kind),
                "masks require an original video effect other than Motion"
            );
            ensure!(masks.len() < 1024, "mask authoring budget exceeded");
            masks.push(new_mask(name, shape)?);
        }
        MaskCommand::Delete { mask_index } => {
            ensure!(mask_index < masks.len(), "source mask missing");
            masks.remove(mask_index);
        }
        MaskCommand::Move {
            mask_index,
            destination,
        } => {
            ensure!(
                mask_index < masks.len() && destination < masks.len(),
                "source mask move out of bounds"
            );
            let mask = masks.remove(mask_index);
            masks.insert(destination, mask);
        }
        MaskCommand::Settings {
            mask_index,
            name,
            mode,
            inverted,
            track_method,
        } => {
            let mask = masks
                .get_mut(mask_index)
                .and_then(Value::as_object_mut)
                .context("source mask missing")?;
            if let Some(name) = name {
                ensure!(name.len() <= 4096, "mask name exceeds authoring budget");
                mask.insert("name".into(), json!(name));
            }
            if let Some(mode) = mode {
                mask.insert("mode".into(), json!(mode));
            }
            if let Some(inverted) = inverted {
                mask.insert("inverted".into(), json!(inverted));
            }
            if let Some(method) = track_method {
                mask.insert("track_method".into(), json!(method));
            }
        }
    }
    let mut changed = document.clone();
    changed.envelope.pointer_mut(&path).unwrap()["masks"] = json!(masks);
    ProjectIndex::new(&changed)?;
    Ok(changed)
}
