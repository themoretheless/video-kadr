//! Export authored native graphics to Filmcraft .fcgt without flattening animations.
use anyhow::{ensure, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
const RATE: i128 = 254016000000;
fn snake(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            result.push('_');
            result.push(ch.to_ascii_lowercase())
        } else {
            result.push(ch)
        }
    }
    result
}
fn original_time(tick: i64, base: u64) -> Result<i64> {
    ensure!(base > 0, "invalid animation time base");
    let scaled = i128::from(tick) * RATE;
    ensure!(
        scaled % i128::from(base) == 0,
        "animation key is not representable in Filmcraft ticks"
    );
    Ok(i64::try_from(scaled / i128::from(base))?)
}
fn us(value: &Value) -> Result<Value> {
    Ok(json!(i64::try_from(
        i128::from(
            value
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("invalid graphic time"))?
        ) * 254016
    )?))
}
fn tag(name: &str, value: &Value) -> Value {
    let kind = if value.is_boolean() {
        "Bool"
    } else if value.is_string() {
        "Text"
    } else if name == "points" {
        "Curve"
    } else if name.ends_with("_color") {
        "Color"
    } else if matches!(name, "position" | "anchor") || name == "size" && value.is_array() {
        "Vec2"
    } else if matches!(
        name,
        "shape" | "stroke_type" | "stroke2_type" | "align" | "caps"
    ) {
        "Choice"
    } else {
        "Float"
    };
    json!({kind:if kind=="Vec2" {json!({"x":value[0],"y":value[1]})}else{value.clone()}})
}
fn binding(channel: &str) -> (String, usize) {
    for (group, field) in [
        ("fill", "fill_color"),
        ("stroke2", "stroke2_color"),
        ("stroke", "stroke_color"),
        ("background", "background_color"),
        ("shadow", "shadow_color"),
    ] {
        if let Some(axis) = channel.strip_prefix(group) {
            if let Some(index) = ["R", "G", "B", "A"].iter().position(|value| *value == axis) {
                return (field.into(), index);
            }
        }
    }
    for group in ["position", "anchor", "size"] {
        if let Some(axis) = channel.strip_prefix(group) {
            if let Some(index) = ["X", "Y"].iter().position(|value| *value == axis) {
                return (group.into(), index);
            }
        }
    }
    (snake(channel), 0)
}
fn component(value: &mut Value, index: usize, new: Value) -> Result<()> {
    let tagged = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("invalid parameter"))?;
    let (kind, value) = tagged
        .iter_mut()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing parameter tag"))?;
    match kind.as_str() {
        "Float" => *value = new,
        "Vec2" => {
            ensure!(index < 2, "invalid point component");
            value[if index == 0 { "x" } else { "y" }] = new
        }
        "Color" => {
            let values = value
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("invalid color"))?;
            ensure!(index < values.len(), "invalid color component");
            values[index] = new
        }
        _ => anyhow::bail!("unsupported animated parameter kind {kind}"),
    };
    Ok(())
}
fn key_shape(track: &Value) -> Result<Vec<(i64, String)>> {
    ensure!(
        track
            .get("extrapolation")
            .is_none_or(|v| v.is_null() || v["before"] == "hold" && v["after"] == "hold"),
        "Filmcraft does not represent native extrapolation"
    );
    let base = track["timeBase"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("invalid time base"))?;
    let keys = track["keyframes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid keys"))?;
    ensure!(!keys.is_empty(), "empty animation");
    keys.iter()
        .map(|key| {
            ensure!(
                key.get("tangents").is_none_or(Value::is_null),
                "native manual tangents require an exact Filmcraft conversion"
            );
            let mode = key
                .get("interpolation")
                .filter(|v| !v.is_null())
                .unwrap_or(&track["interpolation"])
                .as_str()
                .ok_or_else(|| {
                    anyhow::anyhow!("native curve requires an exact Filmcraft conversion")
                })?;
            ensure!(
                matches!(mode, "linear" | "hold"),
                "native curve {mode} requires an exact Filmcraft conversion"
            );
            Ok((
                original_time(
                    key["tick"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("invalid key time"))?,
                    base,
                )?,
                if mode == "linear" { "Linear" } else { "Hold" }.into(),
            ))
        })
        .collect()
}
fn animate(params: &mut Map<String, Value>, animation: &Value, canvas: &Value) -> Result<()> {
    let Some(animation) = animation.as_object() else {
        return Ok(());
    };
    let mut groups: BTreeMap<String, BTreeMap<usize, &Value>> = BTreeMap::new();
    for (channel, value) in animation {
        let (field, index) = binding(channel);
        groups.entry(field).or_default().insert(index, value);
    }
    for (field, channels) in groups {
        let param = params
            .get_mut(&field)
            .ok_or_else(|| anyhow::anyhow!("unknown animated parameter {field}"))?;
        if field == "position"
            && (param["value"]["Vec2"]["x"].is_null() || param["value"]["Vec2"]["y"].is_null())
        {
            param["value"] = json!({"Vec2":{"x":canvas[0].as_f64().unwrap_or(1920.0)/2.0,"y":canvas[1].as_f64().unwrap_or(1080.0)/2.0}});
        }
        for (index, value) in &channels {
            if value["mode"] == "constant" {
                component(&mut param["value"], *index, value["value"].clone())?;
                if let Some(keys) = param.get_mut("keyframes").and_then(Value::as_array_mut) {
                    for key in keys {
                        component(&mut key["value"], *index, value["value"].clone())?;
                    }
                }
            }
        }
        let dynamic = channels
            .iter()
            .filter(|(_, value)| value["mode"] == "keyframes")
            .collect::<Vec<_>>();
        if dynamic.is_empty() {
            continue;
        }
        let shape = key_shape(&dynamic[0].1["track"])?;
        for (_, value) in &dynamic {
            ensure!(
                key_shape(&value["track"])? == shape,
                "independent vector/color curves cannot be represented by one Filmcraft parameter"
            );
        }
        let dimension = if param["value"].get("Vec2").is_some() {
            2
        } else if param["value"].get("Color").is_some() {
            4
        } else {
            1
        };
        let original = param
            .get("keyframes")
            .and_then(Value::as_array)
            .filter(|keys| !keys.is_empty());
        if let Some(original) = original {
            if channels.len() < dimension {
                ensure!(
                    original.len() == shape.len()
                        && original
                            .iter()
                            .zip(&shape)
                            .all(|(key, (time, interp))| key["time"] == *time
                                && key["interp"] == *interp),
                    "native component curve differs from retained original parameter"
                );
            }
        }
        let mut keys = vec![];
        for (index, (time, interp)) in shape.iter().enumerate() {
            let mut value = original
                .and_then(|keys| keys.get(index))
                .map(|key| key["value"].clone())
                .unwrap_or_else(|| param["value"].clone());
            for (component_index, channel) in &dynamic {
                component(
                    &mut value,
                    **component_index,
                    channel["track"]["keyframes"][index]["value"].clone(),
                )?;
            }
            keys.push(json!({"time":time,"value":value,"interp":interp,"out_influence":1.0/3.0,"in_influence":1.0/3.0}));
        }
        param["keyframes"] = json!(keys);
    }
    Ok(())
}
pub fn export_native(template: &Value, animation_offset_us: u64) -> Result<Value> {
    export_native_with_control_defaults(template,animation_offset_us,true)
}
pub fn export_native_with_control_defaults(template:&Value,animation_offset_us:u64,auto_text_controls:bool)->Result<Value> {

    let source = template["graphic"]["layers"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing graphic layers"))?;
    ensure!(!source.is_empty(), "graphic has no layers");
    let offset = i128::from(animation_offset_us) * 254016;
    let canvas = template["canvas"].clone();
    let mut layers = vec![];
    let mut default_controls=vec![];
    let mut assigned = source
        .iter()
        .filter_map(|layer| layer["uid"].as_u64())
        .max()
        .unwrap_or(0);
    for layer in source {
        let content = &layer["content"];
        let kind = content["kind"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing graphic kind"))?;
        ensure!(matches!(kind, "text" | "shape"), "invalid graphic kind");
        let mut params = Map::new();
        for (name, value) in content["parameters"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("missing parameters"))?
        {
            if name != "runs" {
                let field = snake(name);
                params.insert(field.clone(), json!({"value":tag(&field,value)}));
            }
        }
        if let Some(original) = layer["originalParameters"].as_object() {
            params.extend(original.clone());
        }
        animate(&mut params, &content["animation"], &canvas)?;
        if let Some(text) = content
            .get("sourceTextAnimation")
            .filter(|value| !value.is_null())
        {
            let base = text["timeBase"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("invalid source text clock"))?;
            let keys=text["keyframes"].as_array().ok_or_else(||anyhow::anyhow!("invalid source text keys"))?.iter().map(|key|Ok(json!({"time":original_time(key["tick"].as_i64().ok_or_else(||anyhow::anyhow!("invalid text key time"))?,base)?,"value":{"Text":key["value"]},"interp":"Hold","out_influence":1.0/3.0,"in_influence":1.0/3.0}))).collect::<Result<Vec<_>>>()?;
            params
                .get_mut("text")
                .ok_or_else(|| anyhow::anyhow!("missing source text"))?["keyframes"] = json!(keys);
        }
        let default_name=if kind=="text" {
            let sample=|field:&str|->Result<String>{
                let Some(param)=params.get(field) else{return Ok(String::new())};
                Ok(super::graphic_template_animation::sample_parameter(param,0)?["Text"].as_str().unwrap_or("").into())
            };
            let name=sample("name")?;
            Some(if !name.is_empty(){name}else{let text=sample("text")?;let first=text.lines().next().unwrap_or("").trim();if first.is_empty(){"Text".into()}else{first.chars().take(40).collect()}})
        }else{None};
        for param in params.values_mut() {
            if let Some(keys) = param.get_mut("keyframes").and_then(Value::as_array_mut) {
                for key in keys {
                    key["time"] = json!(i64::try_from(
                        i128::from(
                            key["time"]
                                .as_i64()
                                .ok_or_else(|| anyhow::anyhow!("invalid original key time"))?
                        ) - offset
                    )?);
                }
            }
        }
        let mut uid = layer["uid"].as_u64().unwrap_or(0);
        if uid == 0 {
            assigned = assigned
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("layer uid overflow"))?;
            uid = assigned;
        }
        if let Some(name)=default_name {default_controls.push(json!({"id":format!("text{}",default_controls.len()+1),"name":name,"kind":"text","layer":uid,"param":"text"}));}
        let mut extra = json!({"uid":uid});
        if let Some(pin) = layer.get("pin").filter(|value| !value.is_null()) {
            extra["pin"] = pin.clone();
        }
        if let Some(runs) = content["parameters"]
            .get("runs")
            .filter(|value| value.as_array().is_some_and(|v| !v.is_empty()))
        {
            extra["runs"] = super::graphic_template_style_runs::encode(runs)?;
        }
        layers.push(json!({"effect":format!("graphic_{kind}"),"enabled":layer["enabled"],"params":params,"layer":extra}));
    }
    let mut meta = json!({});
    if let Some(timing) = template
        .get("responsiveTime")
        .filter(|value| !value.is_null())
    {
        for field in ["intro", "outro", "designIn", "designDuration"] {
            meta[field] = us(&timing[field])?;
        }
        if timing["intro"].as_i64().unwrap_or(0) > 0 || timing["outro"].as_i64().unwrap_or(0) > 0 {
            meta["designIn"] = json!(i64::try_from(
                i128::from(meta["designIn"].as_i64().unwrap()) - offset
            )?);
        }
    }
    if let Some(roll) = template.get("roll").filter(|value| !value.is_null()) {
        let mut roll = roll.clone();
        for field in ["preroll", "easeIn", "easeOut", "postroll"] {
            roll[field] = us(&roll[field])?;
        }
        meta["roll"] = roll;
    }
    let duration = template["durationUs"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("invalid template duration"))?;
    ensure!(duration > 0, "empty template duration");
    let mut result = template.clone();
    let object = result
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("invalid template"))?;
    for field in ["durationUs", "roll", "responsiveTime"] {
        object.remove(field);
    }
    let mut slug = String::new();
    for ch in object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
    {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase())
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-')
        }
    }
    let slug = slug.trim_end_matches('-');
    object.insert(
        "id".into(),
        json!(format!(
            "user:{}",
            if slug.is_empty() { "template" } else { slug }
        )),
    );
    if auto_text_controls && object.get("controls").and_then(Value::as_array).is_none_or(|controls|controls.is_empty()) {
        object.insert("controls".into(),json!(default_controls));
    }
    if let Some(controls)=object.get_mut("controls").and_then(Value::as_array_mut) {
        super::graphic_template_control_defaults::apply(controls,&layers);
    }
    object.insert("format".into(), json!("filmcraft.graphicsTemplate"));
    object.insert("version".into(), json!(1));
    object.insert(
        "duration".into(),
        json!(i64::try_from(i128::from(duration) * 254016)?),
    );
    object.insert("layers".into(), json!(layers));
    object.insert("graphic".into(), meta);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn template() -> Value {
        json!({"id":"user:test","name":"Title","category":"Titles","description":"","author":"","license":"MIT","tags":[],"canvas":[64,48],"durationUs":2000000,"graphic":{"layers":[{"uid":1,"enabled":true,"content":{"kind":"text","parameters":{"text":"Привет","position":[10,20],"font":"Inter","fontStyle":"Regular","fillColor":[0.1,0.2,0.3,1.0],"runs":[{"start":0,"end":2,"style":{"underline":true}}]},"sourceTextAnimation":{"timeBase":1000000,"keyframes":[{"tick":0,"value":"Начало"},{"tick":1000000,"value":"Конец"}]},"animation":{"positionX":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":10},{"tick":1000000,"value":30}]}},"positionY":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":20},{"tick":1000000,"value":40}]}}}}}]},"controls":[{"id":"text","name":"Text","kind":"text","layer":1,"param":"text"}],"roll":{"mode":"crawlLeft","startOffScreen":true,"endOffScreen":true,"preroll":100000,"easeIn":200000,"easeOut":300000,"postroll":400000},"responsiveTime":{"intro":250000,"outro":250000,"designIn":-500000,"designDuration":2000000},"resources":{"fonts":[{"family":"Test","data":"TWFu","license":"OFL-1.1"}]}})
    }
    #[test]
    fn default_text_controls_use_original_zero_time_names_and_can_be_disabled() {
        let mut native=template();native["controls"]=json!([]);
        let exported=export_native(&native,1_000_000).unwrap();
        assert_eq!(exported["controls"][0],json!({"id":"text1","name":"Начало","kind":"text","layer":1,"param":"text"}));
        assert_eq!(export_native_with_control_defaults(&native,1_000_000,false).unwrap()["controls"],json!([]));
        native["graphic"]["layers"][0]["content"]["parameters"]["name"]=json!("Authored name");
        assert_eq!(export_native(&native,0).unwrap()["controls"][0]["name"],"Authored name");
    }
    #[test]
    fn default_text_name_trims_first_line_and_limits_unicode_characters() {
        let mut native=template();native["controls"]=json!([]);
        native["graphic"]["layers"][0]["content"].as_object_mut().unwrap().remove("sourceTextAnimation");
        native["graphic"]["layers"][0]["content"]["parameters"]["text"]=json!(format!("  {}  \nOther", "🙂".repeat(41)));
        assert_eq!(export_native(&native,0).unwrap()["controls"][0]["name"],"🙂".repeat(40));
        native["graphic"]["layers"][0]["content"]["parameters"]["text"]=json!(" \nOther");
        assert_eq!(export_native(&native,0).unwrap()["controls"][0]["name"],"Text");
    }
    #[test]
    fn slider_defaults_use_original_soft_bounds_and_preserve_authored_halves() {
        let mut native=template();native["graphic"]["layers"][0]["content"]["parameters"]["size"]=json!(100);
        native["graphic"]["layers"][0]["content"]["parameters"]["rotation"]=json!(0);
        native["controls"]=json!([
            {"id":"size","kind":"slider","name":"Size","layer":1,"param":"size","min":9,"max":null},
            {"id":"opacity","kind":"slider","name":"Opacity","layer":1,"param":"opacity","max":99},
            {"id":"angle","kind":"slider","name":"Angle","layer":1,"param":"rotation"},
            {"id":"point","kind":"slider","name":"Point","layer":1,"param":"position"}
        ]);
        let before=native.clone();let exported=export_native(&native,0).unwrap();
        assert_eq!(exported["controls"][0]["min"],9);assert_eq!(exported["controls"][0]["max"],400.);
        assert_eq!(exported["controls"][1]["min"],0.);assert_eq!(exported["controls"][1]["max"],99);
        assert!(exported["controls"][2].get("min").is_none());assert!(exported["controls"][3].get("max").is_none());
        assert_eq!(native,before);
    }
    #[test]
    fn exports_original_clock_vector_text_runs_resources_and_trim() {
        let native = template();
        let exported = export_native(&native, 250000).unwrap();
        assert_eq!(exported["duration"], json!(508032000000i64));
        assert_eq!(
            exported["layers"][0]["params"]["position"]["keyframes"][0]["time"],
            json!(-63504000000i64)
        );
        assert_eq!(
            exported["layers"][0]["params"]["position"]["keyframes"][1]["value"],
            json!({"Vec2":{"x":30,"y":40}})
        );
        assert_eq!(
            exported["layers"][0]["params"]["text"]["keyframes"][1]["value"],
            json!({"Text":"Конец"})
        );
        assert_eq!(
            exported["layers"][0]["layer"]["runs"],
            json!([{"start":0,"end":2,"underline":true}])
        );
        assert_eq!(exported["resources"], native["resources"]);
        assert_eq!(exported["graphic"]["designIn"], json!(-190512000000i64));
        assert_eq!(
            exported["graphic"]["roll"]["preroll"],
            json!(25401600000i64)
        );
        assert!(exported.get("durationUs").is_none());
        for param in exported["layers"][0]["params"].as_object().unwrap().values(){assert!(param.get("keyframes").is_none_or(Value::is_array));}
    }
    #[test]
    fn refuses_unrepresentable_native_curves_instead_of_flattening() {
        let mut native = template();
        native["graphic"]["layers"][0]["content"]["animation"]["positionX"]["track"]
            ["interpolation"] = json!("bounce_out");
        assert!(export_native(&native, 0).is_err());
        let mut native = template();
        native["graphic"]["layers"][0]["content"]["animation"]["positionX"]["track"]["keyframes"]
            [1]["tick"] = json!(750000);
        assert!(export_native(&native, 0).is_err());
    }
}
