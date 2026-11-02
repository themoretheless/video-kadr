//! Filmcraft export control descriptors: source layer indices/names and parameter aliases.
use anyhow::{ensure, Result};
use serde_json::{json, Value};
fn display_name(layer: &Value, index: usize) -> Result<String> {
    let sample = |field: &str| -> Result<String> {
        let Some(param) = layer["params"].get(field) else { return Ok(String::new()) };
        Ok(super::graphic_template_animation::sample_parameter(param, 0)?["Text"].as_str().unwrap_or("").into())
    };
    let name = sample("name")?;
    if !name.is_empty() { return Ok(name) }
    if layer["effect"] != "graphic_text" { return Ok(format!("Shape {:02}", index + 1)) }
    let text = sample("text")?;
    let first = text.lines().next().unwrap_or("").trim();
    Ok(if first.is_empty() { "Text".into() } else { first.chars().take(40).collect() })
}
fn slug(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() { result.push(ch.to_ascii_lowercase()) }
        else if !result.is_empty() && !result.ends_with('-') { result.push('-') }
    }
    let result = result.trim_end_matches('-');
    if result.is_empty() { "template".into() } else { result.into() }
}
fn kind(param: &str) -> &'static str {
    match param {
        "font" | "font_style" => "font",
        "enabled" => "checkbox",
        "position" | "anchor" => "position",
        "text" | "name" => "text",
        p if p.ends_with("_color") => "color",
        p => {
            let effects=&super::graphic_template_control_defaults::catalog()["effects"];
            match effects["graphic_text"].get(p).or_else(||effects["graphic_shape"].get(p)).and_then(Value::as_str) {
                Some("Bool")=>"checkbox",Some("Color")=>"color",Some("Point")=>"position",Some("Text")=>"text",_=>"slider",
            }
        },
    }
}
pub fn resolve(source_at_zero: &Value, descriptors: &[Value]) -> Result<Vec<Value>> {
    let layers = source_at_zero["layers"].as_array().ok_or_else(||anyhow::anyhow!("missing graphic layers"))?;
    let names = layers.iter().enumerate().map(|(index,layer)|display_name(layer,index)).collect::<Result<Vec<_>>>()?;
    let mut controls: Vec<Value> = Vec::new();
    for (index, descriptor) in descriptors.iter().enumerate() {
        let layer_index = match &descriptor["layer"] {
            Value::Number(number) => usize::try_from(number.as_u64().unwrap_or(0))?,
            Value::String(name) => names.iter().position(|candidate|candidate.eq_ignore_ascii_case(name)).ok_or_else(||anyhow::anyhow!("no layer named `{name}`"))?,
            _ => anyhow::bail!("each control needs layer"),
        };
        let layer = layers.get(layer_index).ok_or_else(||anyhow::anyhow!("no layer {layer_index}"))?;
        let param = match descriptor["param"].as_str().unwrap_or("text") {
            "color" | "fillColor" => "fill_color", "fontSize" => "size", "visible" | "show" => "enabled", p => p,
        };
        ensure!(param == "enabled" || layer["params"].get(param).is_some(), "layer {layer_index} has no property `{param}`");
        let explicit = descriptor["kind"].as_str().unwrap_or("").to_ascii_lowercase();
        let explicit = if explicit == "colour" { "color" } else { &explicit };
        let kind = if ["text","font","slider","checkbox","color","position"].contains(&explicit) { explicit } else { kind(param) };
        let name = descriptor["name"].as_str().map(str::to_string).unwrap_or_else(||format!("{} {}",names[layer_index],param.replace('_'," ")));
        let id = descriptor["id"].as_str().map(str::to_string).unwrap_or_else(|| {
            let base = slug(&name).replace('-',"_");
            if controls.iter().any(|control|control["id"] == base) { format!("{base}_{index}") } else { base }
        });
        controls.push(json!({"id":id,"name":name,"kind":kind,"layer":layer["layer"]["uid"],"param":param,"min":descriptor["min"].as_f64(),"max":descriptor["max"].as_f64()}));
    }
    super::graphic_template_control_defaults::apply(&mut controls,layers);
    Ok(controls)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphic_template_controls_match_original_engine_exports() {
        let fixture: Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-control-authoring.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),150);
        for (index,case) in cases.iter().enumerate() {
            let result=resolve(&fixture["document"],case["descriptors"].as_array().unwrap());
            if case["error"]==true {assert!(result.is_err(),"case{index}");}
            else {assert_eq!(json!(result.unwrap()),case["controls"],"case{index}");}
        }
    }
    #[test]
    fn graphic_template_authoring_aliases_names_ids_types_and_bounds() {
        let template = super::super::graphic_templates::builtin_templates().unwrap().remove(0);
        let native = serde_json::to_value(&template).unwrap();
        let original = super::super::graphic_template_export::export_native(&native,0).unwrap();
        let text_index = original["layers"].as_array().unwrap().iter().position(|layer|layer["effect"]=="graphic_text").unwrap();
        let name=display_name(&original["layers"][text_index],text_index).unwrap();
        let descriptors=json!([{"layer":name.to_ascii_uppercase(),"param":"fontSize"},{"layer":text_index,"param":"fontSize"},{"layer":text_index,"param":"fillColor","kind":"COLOUR","id":"paint"},{"layer":text_index,"param":"show"},{"layer":text_index,"param":"anchor"}]);
        let controls=resolve(&original,descriptors.as_array().unwrap()).unwrap();
        assert_eq!(controls[0]["param"],"size");assert_eq!(controls[0]["kind"],"slider");assert_eq!(controls[0]["min"],json!(6.0));assert_eq!(controls[0]["max"],json!(400.0));
        assert_eq!(controls[1]["id"],format!("{}_1",controls[0]["id"].as_str().unwrap()));
        assert_eq!(controls[2]["kind"],"color");assert_eq!(controls[2]["id"],"paint");
        assert_eq!(controls[3]["kind"],"checkbox");assert_eq!(controls[4]["kind"],"position");
        assert!(resolve(&original,&[json!({"layer":"missing"})]).is_err());
        assert!(resolve(&original,&[json!({"layer":text_index,"param":"missing"})]).is_err());
        assert_eq!(serde_json::to_value(&template).unwrap(),native);
    }
}
