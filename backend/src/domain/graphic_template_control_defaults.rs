//! Original graphic effect soft slider ranges (pinned Filmcraft registry snapshot).
use serde_json::Value;
use std::sync::OnceLock;
pub(super) fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("filmcraft_graphic_control_catalog.json"))
            .expect("pinned graphic control catalogue")
    })
}
pub fn apply(controls: &mut [Value], layers: &[Value]) {
    for control in controls {
        if control["kind"] != "slider" {
            continue;
        }
        let Some(layer) = layers
            .iter()
            .find(|layer| layer["layer"]["uid"] == control["layer"])
        else {
            continue;
        };
        let (Some(effect), Some(param)) = (layer["effect"].as_str(), control["param"].as_str())
        else {
            continue;
        };
        let Some(bounds) = catalog()["effects"][effect][param].get("Float") else {
            continue;
        };
        if control.get("min").is_none_or(Value::is_null) {
            control["min"] = bounds["soft_min"].clone();
        }
        if control.get("max").is_none_or(Value::is_null) {
            control["max"] = bounds["soft_max"].clone();
        }
    }
}
