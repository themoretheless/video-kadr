//! Original Filmcraft bin creation/organization commands (MIT), pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    filmcraft_bin_rename::locations, filmcraft_clip_rename::target_id,
    filmcraft_project_file::SourceProjectDocument, filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionContext {
    #[serde(default)]
    project_selection: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    name: Option<Value>,
    parent: Option<Value>,
    bin: Option<Value>,
    items: Option<Value>,
    item: Option<Value>,
    #[serde(default)]
    context: SessionContext,
}
fn parent_of(bin: &Value, id: u64) -> Option<u64> {
    for child in bin["children"].as_array()? {
        if child.get("Item").and_then(Value::as_u64) == Some(id) {
            return bin["id"].as_u64();
        }
        if let Some(child) = child.get("Bin") {
            if let Some(found) = parent_of(child, id) {
                return Some(found);
            }
        }
    }
    None
}
pub(super) fn remove_item(bin: &mut Value, id: u64) -> bool {
    let children = bin["children"].as_array_mut().unwrap();
    let before = children.len();
    children.retain(|child| child.get("Item").and_then(Value::as_u64) != Some(id));
    if children.len() != before {
        return true;
    }
    children
        .iter_mut()
        .filter_map(|v| v.get_mut("Bin"))
        .any(|bin| remove_item(bin, id))
}
fn move_items(doc: &mut SourceProjectDocument, items: &[u64], target: u64) -> Result<usize> {
    ensure!(
        locations(doc)?.iter().any(|b| b.id == target),
        "project.moveToBin: no such bin"
    );
    let mut moved = 0;
    for id in items {
        if doc.envelope["project"]["items"]
            .get(id.to_string())
            .is_none()
        {
            continue;
        }
        if !remove_item(&mut doc.envelope["project"]["root"], *id) {
            continue;
        }
        // Removing an Item may shift the target Bin entry; resolve after every removal.
        let path = locations(doc)?
            .into_iter()
            .find(|b| b.id == target)
            .unwrap()
            .path;
        doc.envelope.pointer_mut(&path).unwrap()["children"]
            .as_array_mut()
            .unwrap()
            .push(json!({"Item":id}));
        moved += 1;
    }
    Ok(moved)
}
fn new_bin(doc: &mut SourceProjectDocument, name: &str, parent: Option<u64>) -> Result<u64> {
    let bins = locations(doc)?;
    let next = doc.envelope["project"]["next_id"]
        .as_u64()
        .context("missing project next ID")?;
    let following = next.checked_add(1).context("project ID space exhausted")?;
    ensure!(
        !bins.iter().any(|b| b.id == next)
            && doc.envelope["project"]["items"]
                .get(next.to_string())
                .is_none(),
        "project next ID already allocated"
    );
    let path = bins
        .iter()
        .find(|b| Some(b.id) == parent)
        .unwrap_or(&bins[0])
        .path
        .clone();
    doc.envelope.pointer_mut(&path).unwrap()["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Bin":{"id":next,"name":name,"children":[]}}));
    doc.envelope["project"]["next_id"] = json!(following);
    Ok(next)
}
pub fn edit(command: &str, params: Value) -> Result<Value> {
    ensure!(
        matches!(
            command,
            "file.newBin" | "file.newBinFromSelection" | "project.moveToBin"
        ),
        "unknown bin operation"
    );
    let r: Request = serde_json::from_value(params)?;
    let bytes = match r.document {
        Value::String(s) => s.into_bytes(),
        v => serde_json::to_vec(&v)?,
    };
    let mut doc = SourceProjectDocument::read(&bytes)?;
    let bins = locations(&doc)?;
    let root = bins[0].id;
    if command == "file.newBinFromSelection" && r.context.project_selection.is_empty() {
        // Session.execute temporarily derives Project selection only from valid
        // explicitly named array items; the command schema does not advertise item.
        let named = r
            .items
            .as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .any(|id| {
                doc.envelope["project"]["items"]
                    .get(id.to_string())
                    .is_some()
            });
        ensure!(named, "select an item in the Project panel");
    }

    let result = if command == "file.newBin" {
        let name = r.name.as_ref().and_then(Value::as_str).unwrap_or("New Bin");
        json!({"bin":new_bin(&mut doc,name,target_id(r.parent)?)?.to_string()})
    } else {
        let selected = match r.items.as_ref().and_then(Value::as_array) {
            Some(a) => a
                .iter()
                .filter_map(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .collect(),
            None => {
                if command == "file.newBinFromSelection" {
                    match target_id(r.item)? {
                        Some(id) => vec![id],
                        None => r
                            .context
                            .project_selection
                            .into_iter()
                            .map(|v| {
                                id(Some(v)).and_then(|v| v.context("missing project selection ID"))
                            })
                            .collect::<Result<Vec<_>>>()?,
                    }
                } else {
                    r.context
                        .project_selection
                        .into_iter()
                        .map(|v| {
                            id(Some(v)).and_then(|v| v.context("missing project selection ID"))
                        })
                        .collect::<Result<Vec<_>>>()?
                }
            }
        };
        ensure!(!selected.is_empty(), "select items in the Project panel");
        if command == "file.newBinFromSelection" {
            let parent =
                parent_of(&doc.envelope["project"]["root"], selected[0]).filter(|id| *id != root);
            let name = match r.name.as_ref().and_then(Value::as_str) {
                Some(s) => s.to_string(),
                None => {
                    let mut n = 1;
                    while bins.iter().skip(1).any(|b| b.name == format!("Bin {n:02}")) {
                        n += 1
                    }
                    format!("Bin {n:02}")
                }
            };
            let bin = new_bin(&mut doc, &name, parent)?;
            move_items(&mut doc, &selected, bin)?;
            json!({"bin":bin.to_string(),"moved":selected.len()})
        } else {
            json!({"moved":move_items(&mut doc,&selected,target_id(r.bin)?.unwrap_or(root))?})
        }
    };
    Ok(
        json!({"result":result,"documentText":String::from_utf8(doc.encode_preserved()?)?,"document":doc.envelope}),
    )
}
