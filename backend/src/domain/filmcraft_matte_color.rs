//! Filmcraft project.matteColor (MIT, pinned 52318524).
use super::{
    filmcraft_clip_rename::target_id,
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::{exact_numbers, id},
};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

const COLOR: &str = "/kind/Media/media/Generator/ColorMatte/color";

pub(crate) fn describe(item: &Value) -> Option<String> {
    let color: [f32; 4] = serde_json::from_value(item.pointer(COLOR)?.clone()).ok()?;
    Some(filmcraft_color::to_hex(color))
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct ContextSnapshot {
    project_selection: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document: Value,
    item: Option<Value>,
    color: String,
    #[serde(default)]
    context: ContextSnapshot,
}

pub fn edit(params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let color =
        filmcraft_color::parse_hex(&request.color).context("invalid Color Matte hex color")?;
    let item = match (
        target_id(request.item)?,
        request.context.project_selection.as_slice(),
    ) {
        (Some(item), _) => Some(item),
        (None, [item]) => id(Some(item.clone()))?,
        _ => None,
    }
    .context("select one Color Matte or pass its item ID")?;
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let mut document = SourceProjectDocument::read(&bytes)?;
    let index = ProjectIndex::new(&document)?;
    index
        .item(item)
        .and_then(|item| item.pointer(COLOR))
        .context("item is not a Color Matte")?;
    *document.envelope["project"]["items"][item.to_string()]
        .pointer_mut(COLOR)
        .context("Color Matte is gone")? = json!(color);
    let text = String::from_utf8(document.encode_preserved()?)?;
    let mut result = json!({"item":item,"color":filmcraft_color::to_hex(color)});
    exact_numbers(&mut result);
    Ok(json!({"result":result,"document":document.envelope,"documentText":text,"matteEdit":true}))
}
