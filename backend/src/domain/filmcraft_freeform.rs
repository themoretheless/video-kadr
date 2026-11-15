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
        let text = format_position(x, y, position(item).and_then(|position| position.size));
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

const STACK: &str = "Freeform Stack";
const CARD_SIZE: f32 = 128.0;
const STEP: f32 = CARD_SIZE + 24.0;

pub(crate) fn stack(item: &Value) -> Option<String> {
    item["metadata"][STACK]
        .as_str()?
        .parse::<u64>()
        .ok()
        .map(|id| id.to_string())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackRequest {
    document: Value,
    items: Vec<Value>,
    width: Option<f32>,
}

pub fn stack_cards(params: Value, on: bool) -> Result<Value> {
    let request: StackRequest = serde_json::from_value(params)?;
    let items = request
        .items
        .into_iter()
        .map(|item| match item {
            Value::String(id) => id.parse::<u64>().context("invalid card ID"),
            value => value.as_u64().context("invalid card ID"),
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        items.len() >= if on { 2 } else { 1 },
        "select cards for the stack"
    );
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    ensure!(
        items.iter().all(|id| index.item(*id).is_some()),
        "stack item is gone"
    );
    let anchor = items[0];
    let bins = super::filmcraft_bin_rename::locations(&document)?;
    let children = |path: &str| {
        document
            .envelope
            .pointer(path)
            .and_then(|bin| bin["children"].as_array())
    };
    let bin = bins.iter().find(|bin| {
        children(&bin.path).is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry["Item"].as_u64() == Some(anchor))
        })
    });
    let ids = bin_cards(
        &document,
        &index,
        bin.map_or("/project/root", |bin| bin.path.as_str()),
    );
    let cards = layout(&index, &ids, request.width.unwrap_or(800.0));
    let mut affected = items.clone();
    if !on {
        for id in items {
            for (member, item) in index.items() {
                if stack(item).as_deref() == Some(&id.to_string()) && !affected.contains(&member) {
                    affected.push(member);
                }
            }
        }
    }
    let base = cards
        .iter()
        .find(|(id, _, _)| *id == anchor)
        .map_or((12.0, 12.0), |(_, point, _)| {
            (point.x.round(), point.y.round())
        });
    for (id, point, _) in cards {
        let item = &mut document.envelope["project"]["items"][id.to_string()];
        if item["metadata"].is_null() {
            item["metadata"] = json!({});
        }
        ensure!(item["metadata"].is_object(), "invalid item metadata");
        item["metadata"][POSITION] = json!(format_position(
            point.x,
            point.y,
            point.size.filter(|size| *size != CARD_SIZE)
        ));
    }
    let spread = !on && affected.len() > 1;
    for (n, id) in affected.into_iter().enumerate() {
        let item = &mut document.envelope["project"]["items"][id.to_string()];
        if item["metadata"].is_null() {
            item["metadata"] = json!({});
        }
        let metadata = item["metadata"]
            .as_object_mut()
            .context("invalid item metadata")?;
        if on {
            metadata.insert(STACK.into(), json!(anchor.to_string()));
        } else {
            metadata.remove(STACK);
        }
        if spread {
            let size = position(item).and_then(|point| point.size);
            item["metadata"][POSITION] = json!(format_position(
                (base.0 + n as f32 * STEP).max(0.0),
                base.1.max(0.0),
                size
            ));
        }
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"document":document.envelope,"documentText":text,"freeformEdit":true}))
}

fn bin_cards(document: &SourceProjectDocument, index: &ProjectIndex<'_>, path: &str) -> Vec<u64> {
    document
        .envelope
        .pointer(path)
        .and_then(|bin| bin["children"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["Item"].as_u64())
        .filter(|id| {
            index.item(*id).is_some_and(|item| {
                item.pointer("/kind/Graphic").is_none()
                    || document.project()["source_graphics"]
                        .get(id.to_string())
                        .is_some()
            })
        })
        .collect()
}

fn layout(index: &ProjectIndex<'_>, ids: &[u64], width: f32) -> Vec<(u64, Position, Option<u64>)> {
    let mut cards = Vec::new();
    let mut unplaced = Vec::new();
    let mut bottom = 0.0_f32;
    for &id in ids {
        let item = index.item(id).unwrap();
        let stack = stack(item).and_then(|id| id.parse::<u64>().ok());
        if let Some(position) = position(item) {
            bottom =
                bottom.max(position.y + position.size.unwrap_or(CARD_SIZE) * 9.0 / 16.0 + 40.0);
            cards.push((id, position, stack));
        } else {
            unplaced.push((id, stack));
        }
    }
    let columns = ((width - 12.0) / STEP).floor().max(1.0) as usize;
    let top = if bottom > 0.0 {
        ((bottom + 8.0) / GRID).round() * GRID
    } else {
        12.0
    };
    for (n, (id, stack)) in unplaced.into_iter().enumerate() {
        cards.push((
            id,
            Position {
                x: 12.0 + (n % columns) as f32 * STEP,
                y: top + (n / columns) as f32 * 116.0,
                size: None,
            },
            stack,
        ));
    }
    let anchors: BTreeMap<_, _> = cards
        .iter()
        .filter(|(id, _, stack)| *stack == Some(*id))
        .map(|(id, point, _)| (*id, (point.x, point.y)))
        .collect();
    let mut depth = BTreeMap::<u64, usize>::new();
    for (id, point, stack) in &mut cards {
        if let Some(stack) = stack.filter(|stack| stack != id) {
            if let Some((x, y)) = anchors.get(&stack) {
                let depth = depth.entry(stack).or_default();
                *depth += 1;
                point.x = x + 6.0 * *depth as f32;
                point.y = y + 6.0 * *depth as f32;
            }
        }
    }
    cards
}

fn format_position(x: f32, y: f32, size: Option<f32>) -> String {
    let (x, y) = (x.round(), y.round());
    match size {
        Some(size) => format!("{x},{y},{}", size.round()),
        None => format!("{x},{y}"),
    }
}

const ARRANGEMENT: &str = "Freeform Arrangement: ";

pub(crate) fn arrangement_names(item: &Value) -> Vec<&str> {
    item["metadata"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, _)| key.strip_prefix(ARRANGEMENT))
        .collect()
}

pub const ARRANGEMENT_COMMANDS: &[&str] = &[
    "project.freeform.arrangements",
    "project.freeform.saveArrangement",
    "project.freeform.restoreArrangement",
    "project.freeform.deleteArrangement",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArrangementRequest {
    document: Value,
    name: Option<String>,
    bin: Option<Value>,
    width: Option<f32>,
}

pub fn arrangement(command: &str, params: Value) -> Result<Value> {
    let request: ArrangementRequest = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    let bin = super::filmcraft_project_inspect::id(request.bin)?;
    let bins = super::filmcraft_bin_rename::locations(&document)?;
    let path = bins
        .iter()
        .find(|entry| Some(entry.id) == bin)
        .map_or("/project/root", |entry| entry.path.as_str());
    let ids = bin_cards(&document, &index, path);
    let names = || {
        ids.iter()
            .filter_map(|id| index.item(*id))
            .flat_map(arrangement_names)
            .collect::<std::collections::BTreeSet<_>>()
    };
    if command == ARRANGEMENT_COMMANDS[0] {
        return Ok(json!({"arrangements":names()}));
    }
    let name = request.name.context("arrangement name is required")?;
    let save = command == ARRANGEMENT_COMMANDS[1];
    let restore = command == ARRANGEMENT_COMMANDS[2];
    let name = if save { name.trim() } else { &name };
    ensure!(!save || !name.is_empty(), "arrangement name is empty");
    ensure!(!restore || names().contains(name), "arrangement is absent");
    let key = format!("{ARRANGEMENT}{name}");
    let positions: BTreeMap<_, _> = if save {
        layout(&index, &ids, request.width.unwrap_or(800.0))
            .into_iter()
            .map(|(id, point, _)| {
                (
                    id,
                    format_position(point.x, point.y, Some(point.size.unwrap_or(CARD_SIZE))),
                )
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    for id in ids {
        let item = &mut document.envelope["project"]["items"][id.to_string()];
        if item["metadata"].is_null() {
            if !save {
                continue;
            }
            item["metadata"] = json!({});
        }
        let metadata = item["metadata"]
            .as_object_mut()
            .context("invalid item metadata")?;
        if save {
            metadata.insert(key.clone(), json!(positions[&id]));
        } else if restore {
            if let Some(position) = metadata.get(&key).cloned() {
                metadata.insert(POSITION.into(), position);
            } else {
                metadata.remove(POSITION);
            }
        } else {
            metadata.remove(&key);
        }
    }
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"document":document.envelope,"documentText":text,"freeformEdit":true}))
}
