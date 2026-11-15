//! Filmcraft saved search bins (MIT, pinned 52318524).
use super::{
    filmcraft_bin_rename::locations,
    filmcraft_find::{matching_ids, Query},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_inspect::id,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub const COMMANDS: &[&str] = &[
    "file.newSearchBin",
    "project.searchBinItems",
    "project.editSearchBin",
    "project.deleteSearchBin",
];

pub(crate) fn list(document: &SourceProjectDocument) -> Vec<Value> {
    document.project()["search_bins"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|bin| {
            Some(json!({"id":bin["id"].as_u64()?.to_string(),"name":bin["name"].as_str()?}))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    bin: Option<Value>,
    name: Option<String>,
    query: Option<Query>,
}

fn contents(document: &SourceProjectDocument, bin: &Value) -> Result<Value> {
    let id = bin["id"].as_u64().context("invalid search bin ID")?;
    let query: Query = serde_json::from_value(bin["query"].clone())?;
    let items = matching_ids(document, &query)?;
    Ok(json!({"bin":id.to_string(),"name":bin["name"],"query":query.browser_view(),"items":items}))
}

pub fn run(command: &str, params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let project = document.project();
    let entries = match &project["search_bins"] {
        Value::Null => &[][..],
        value => value
            .as_array()
            .context("invalid project search bins")?
            .as_slice(),
    };
    let query = request.query.map(Query::normalized).transpose()?;
    let reply = if command == COMMANDS[0] {
        let query = query.context("search conditions are required")?;
        let text = query.first_text().context("enter text to search for")?;
        let name = request
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(text);
        let next = project["next_id"]
            .as_u64()
            .context("missing project next ID")?;
        let following = next.checked_add(1).context("project ID space exhausted")?;
        ensure!(
            !entries
                .iter()
                .any(|entry| entry["id"].as_u64() == Some(next))
                && !locations(&document)?.iter().any(|bin| bin.id == next)
                && project["items"].get(next.to_string()).is_none(),
            "project next ID already allocated"
        );
        let entry = json!({"id":next,"name":name,"query":query});
        let reply = contents(&document, &entry)?;
        let project = &mut document.envelope["project"];
        if project["search_bins"].is_null() {
            project["search_bins"] = json!([]);
        }
        project["search_bins"].as_array_mut().unwrap().push(entry);
        project["next_id"] = json!(following);
        reply
    } else {
        let bin = id(request.bin)?.context("search bin ID is required")?;
        let mut positions = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry["id"].as_u64() == Some(bin));
        let (position, entry) = positions.next().context("search bin not found")?;
        ensure!(positions.next().is_none(), "ambiguous search bin ID");
        if command == COMMANDS[1] {
            return contents(&document, entry);
        }
        if command == COMMANDS[2] {
            let mut entry = entry.clone();
            if let Some(name) = request.name {
                entry["name"] = json!(name);
            }
            if let Some(query) = query {
                entry["query"] = json!(query);
            }
            let reply = contents(&document, &entry)?;
            document.envelope["project"]["search_bins"][position] = entry;
            reply
        } else {
            document.envelope["project"]["search_bins"]
                .as_array_mut()
                .unwrap()
                .remove(position);
            json!({"bin":bin.to_string()})
        }
    };
    let text = String::from_utf8(document.encode_preserved()?)?;
    Ok(json!({"result":reply,"documentText":text}))
}
