//! Snapshot effect-rack commands from Filmcraft commands.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Caller adopts the returned document.
use super::{filmcraft_effect_catalog::definitions, filmcraft_project_file::SourceProjectDocument};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    document: Value,
    sequence: Option<Value>,
    clip: Value,
    index: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    active_sequence: Option<Value>,
}
fn definition(id: &str) -> Option<&'static Value> {
    definitions().iter().find(|d| d["id"] == id)
}
pub(super) fn fresh(definition: &Value, width: u32, height: u32) -> Value {
    let effect = definition["id"].as_str().unwrap();
    let mut params = serde_json::Map::new();
    for param in definition["params"].as_array().unwrap() {
        let id = param["id"].as_str().unwrap();
        let mut value = param["default"].clone();
        if let Some(point) = value.get_mut("Vec2") {
            let (fx, fy) = match (effect, id) {
                ("corner_pin", "upper_left") => (0., 0.),
                ("corner_pin", "upper_right") => (1., 0.),
                ("corner_pin", "lower_left") => (0., 1.),
                ("corner_pin", "lower_right") => (1., 1.),
                ("gradient", "start") => (0.5, 0.),
                ("gradient", "end") => (0.5, 1.),
                ("lightning", "start") => (0.1, 0.1),
                ("lightning", "end") => (0.9, 0.9),
                ("checkerboard", "anchor") => (0., 0.),
                ("write_on", "brush") => (0.5, 0.5),
                ("volumetric_rays", "center") => (0.5, 0.25),
                _ => (0.5, if id == "end" { 1. } else { 0.5 }),
            };
            if point["x"].is_null() {
                point["x"] = json!(f64::from(width) * fx);
            }
            if point["y"].is_null() {
                point["y"] = json!(f64::from(height) * fy);
            }
        }
        params.insert(id.into(), json!({"value":value}));
    }
    json!({"effect":effect,"enabled":true,"params":params})
}
pub fn edit(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(
            command,
            "effects.remove" | "effects.toggleEnabled" | "effects.reset"
        ),
        "unknown effect-rack command"
    );
    let request: Request = serde_json::from_value(params)?;
    let sequence = super::filmcraft_project_inspect::id(request.sequence)?
        .or(super::filmcraft_project_inspect::id(
            request.context.active_sequence,
        )?)
        .context("effect command requires sequence or activeSequence context")?;
    let clip =
        super::filmcraft_project_inspect::id(Some(request.clip))?.context("missing clip ID")?;
    let index = match request.index.filter(|v| !v.is_null()) {
        Some(Value::String(s)) => s.parse::<u64>().context("invalid effect index")?,
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_f64().map(|v| v as u64))
            .context("invalid effect index")?,
        None => {
            ensure!(command != "effects.remove", "effects.remove requires index");
            0
        }
    };
    let index = usize::try_from(index)?;
    let bytes = match request.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    // Original rack commands find a clip on either A/V track, including locked tracks.
    let (path, _locked) = super::filmcraft_parameter_edit::location(&document, sequence, clip)?;
    let sequence_path = format!("/project/items/{sequence}/kind/Sequence");
    let settings = &document.envelope.pointer(&sequence_path).unwrap()["settings"];
    let width = u32::try_from(
        settings["width"]
            .as_u64()
            .context("invalid sequence width")?,
    )?;
    let height = u32::try_from(
        settings["height"]
            .as_u64()
            .context("invalid sequence height")?,
    )?;
    let mut changed = document.clone();
    let effects = changed.envelope.pointer_mut(&path).unwrap()["effects"]
        .as_array_mut()
        .context("missing clip effects")?;
    if index < effects.len() {
        let id = effects[index]["effect"]
            .as_str()
            .context("missing effect ID")?;
        match command {
            "effects.remove" => {
                ensure!(
                    !definition(id).is_some_and(|d| d["intrinsic"] == true),
                    "intrinsic effects cannot be removed"
                );
                effects.remove(index);
            }
            "effects.toggleEnabled" => {
                let enabled = effects[index]["enabled"]
                    .as_bool()
                    .context("invalid effect enabled state")?;
                effects[index]["enabled"] = json!(!enabled);
            }
            "effects.reset" => {
                if let Some(definition) = definition(id) {
                    effects[index] = fresh(definition, width, height);
                }
            }
            _ => unreachable!(),
        }
    }
    let document_text = String::from_utf8(changed.encode_preserved()?)?;
    Ok(
        json!({"result":null,"document":changed.envelope,"documentText":document_text,"sequence":sequence.to_string(),"clip":clip.to_string()}),
    )
}
