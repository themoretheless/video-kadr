//! Direct-position form of Filmcraft project.freeform.move (MIT, source 52318524).
use super::{filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const POSITION: &str = "Freeform Position";
const GRID: f32 = 20.0;

#[derive(Serialize)]
pub(crate) struct Position {
    pub x: f32,
    pub y: f32,
    pub size: Option<f32>,
}

pub(crate) fn position(item: &Value) -> Option<Position> {
    let mut parts = item["metadata"][POSITION].as_str()?.split(',').map(|part| {
        part.trim()
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
    });
    Some(Position {
        x: parts.next()??,
        y: parts.next()??,
        size: parts.next().flatten(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveRequest {
    document: Value,
    positions: BTreeMap<String, [f32; 2]>,
    #[serde(default)]
    snap: bool,
}

pub fn move_cards(params: Value) -> Result<Value> {
    let request: MoveRequest = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let mut positions = BTreeMap::new();
    for (id, [x, y]) in request.positions {
        let id = id.parse::<u64>().context("invalid card ID")?;
        let item = index.item(id).context("card item is gone")?;
        ensure!(x.is_finite() && y.is_finite(), "invalid card position");
        let point = |value: f32| {
            let value = if request.snap {
                (value / GRID).round() * GRID
            } else {
                value
            };
            value.max(0.0).round()
        };
        let (x, y) = (point(x), point(y));
        let text = match position(item).and_then(|position| position.size) {
            Some(size) => format!("{x},{y},{}", size.round()),
            None => format!("{x},{y}"),
        };
        ensure!(positions.insert(id, text).is_none(), "duplicate card ID");
    }
    for (id, position) in positions {
        let item = &mut document.envelope["project"]["items"][id.to_string()];
        if item["metadata"].is_null() {
            item["metadata"] = json!({});
        }
        item["metadata"]
            .as_object_mut()
            .context("invalid item metadata")?
            .insert(POSITION.into(), json!(position));
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"document":document.envelope,"documentText":text,"freeformEdit":true}))
}
