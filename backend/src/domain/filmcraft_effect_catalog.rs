//! effects.list from Filmcraft commands.rs / effect.rs (MIT), pinned
//! 5231852443363f001c3f6b396dd9b1e6461ae2be. Catalog access is not render admission.
use serde_json::{json, Value};
use std::sync::OnceLock;
pub fn definitions() -> &'static Vec<Value> {
    static DEFINITIONS: OnceLock<Vec<Value>> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!("filmcraft_effect_definitions.json"))
            .expect("pinned effect definitions decode")
    })
}
fn parameter(p: &Value) -> Value {
    let mut value = json!({"id":p["id"],"label":p["label"],"animatable":p["animatable"],"default":p["default"]});
    let kind = &p["kind"];
    let ty = if let Some(options) = kind.get("Choice") {
        value["options"] = options.clone();
        "choice"
    } else if let Some(float) = kind.get("Float") {
        for field in ["min", "max", "unit"] {
            value[field] = float[field].clone();
        }
        "float"
    } else if kind.get("Curve").is_some() {
        "curve"
    } else {
        match kind.as_str().expect("known parameter kind") {
            "Point" => "point",
            "Color" => "color",
            "Bool" => "bool",
            "Angle" => "angle",
            "Text" => "text",
            "Wheel" => "wheel",
            "Path" => "path",
            _ => unreachable!("unknown pinned parameter kind"),
        }
    };
    value["type"] = json!(ty);
    value
}
pub fn list(params: &Value) -> Value {
    // The source ignores unknown fields and non-string/non-boolean filters.
    let kind = params["kind"].as_str();
    let folder = params["folder"].as_str();
    let detail = params["detail"].as_bool().unwrap_or(false);
    Value::Array(definitions().iter().filter(|d|kind.is_none_or(|kind|d["kind"].as_str().unwrap().eq_ignore_ascii_case(kind))).filter(|d|folder.is_none_or(|folder|path(d).starts_with(folder))).map(|d|{
        let params=d["params"].as_array().unwrap();
        let mut value=json!({"id":d["id"],"name":d["name"],"kind":d["kind"],"category":d["category"],"folder":d["category"].get(1),"path":path(d),"params":params.iter().map(|p|p["id"].clone()).collect::<Vec<_>>(),"badges":{"accelerated":d["accelerated"],"float32":d["float32"],"yuv":d["yuv"]}});
        if detail{value["paramInfo"]=Value::Array(params.iter().map(parameter).collect());}value
    }).collect())
}
fn path(d: &Value) -> String {
    d["category"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("/")
}
