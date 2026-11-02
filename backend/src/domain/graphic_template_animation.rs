//! Original Filmcraft parameter sampling for imported .fcgt layers.
//! Adapted from project/keyframe.rs and mask.rs; docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde_json::{json, Value};
fn number(value: &Value) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}
fn components(value: &Value) -> Result<Option<Vec<f64>>> {
    let tagged = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid parameter value"))?;
    ensure!(tagged.len() == 1, "invalid parameter tag");
    let (kind, value) = tagged.iter().next().unwrap();
    Ok(match kind.as_str() {
        "Float" => Some(vec![number(value)]),
        "Vec2" => Some(vec![number(&value["x"]), number(&value["y"])]),
        "Color" => Some(
            value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid color"))?
                .iter()
                .map(|v| number(v) as f32 as f64)
                .collect(),
        ),
        "Path" => {
            let mut out = vec![];
            for vertex in value["vertices"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid path"))?
            {
                for field in ["p", "t_in", "t_out"] {
                    for axis in ["x", "y"] {
                        out.push(if vertex.get(field).is_none() && field != "p" {
                            0.0
                        } else {
                            number(&vertex[field][axis])
                        });
                    }
                }
            }
            Some(out)
        }
        _ => None,
    })
}
fn from_components(template: &Value, values: &[f64]) -> Result<Value> {
    let mut result = template.clone();
    let kind = template.as_object().unwrap().keys().next().unwrap();
    match kind.as_str() {
        "Float" => result[kind] = json!(values[0]),
        "Vec2" => {
            result[kind]["x"] = json!(values[0]);
            result[kind]["y"] = json!(values[1]);
        }
        "Color" => result[kind] = json!(values.iter().map(|v| *v as f32).collect::<Vec<_>>()),
        "Path" => {
            let vertices = result[kind]["vertices"].as_array_mut().unwrap();
            for (vertex, values) in vertices.iter_mut().zip(values.chunks_exact(6)) {
                for (field, pair) in ["p", "t_in", "t_out"].iter().zip(values.chunks_exact(2)) {
                    if *field != "p" && pair[0] == 0.0 && pair[1] == 0.0 {
                        vertex.as_object_mut().unwrap().remove(*field);
                    } else {
                        vertex[*field] = json!({"x":pair[0],"y":pair[1]});
                    }
                }
            }
        }
        _ => {}
    }
    Ok(result)
}
pub fn sample_parameter(param: &Value, time: i128) -> Result<Value> {
    let Some(keys) = param.get("keyframes") else {
        return Ok(param["value"].clone());
    };
    let keys = keys
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid keyframes"))?;
    if keys.is_empty() {
        return Ok(param["value"].clone());
    }
    let tick = |key: &Value| -> Result<i128> {
        Ok(key["time"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("invalid key time"))? as i128)
    };
    for pair in keys.windows(2) {
        ensure!(tick(&pair[0])? < tick(&pair[1])?, "key times must increase")
    }
    if time <= tick(&keys[0])? {
        return Ok(keys[0]["value"].clone());
    }
    if time >= tick(keys.last().unwrap())? {
        return Ok(keys.last().unwrap()["value"].clone());
    }
    let index = keys.partition_point(|key| key["time"].as_i64().unwrap() as i128 <= time) - 1;
    let (a, b) = (&keys[index], &keys[index + 1]);
    if time == tick(a)? || a["interp"] == "Hold" {
        return Ok(a["value"].clone());
    }
    let Some(left) = components(&a["value"])? else {
        return Ok(a["value"].clone());
    };
    let Some(right) = components(&b["value"])? else {
        return Ok(a["value"].clone());
    };
    if left.len() != right.len() {
        return Ok(a["value"].clone());
    }
    let u = (time - tick(a)?) as f64 / (tick(b)? - tick(a)?) as f64;
    let out = matches!(
        a["interp"].as_str(),
        Some("Bezier" | "AutoBezier" | "ContinuousBezier" | "EaseOut")
    );
    let incoming = matches!(
        b["interp"].as_str(),
        Some("Bezier" | "AutoBezier" | "ContinuousBezier" | "EaseIn")
    ) || matches!(a["interp"].as_str(), Some("Bezier" | "ContinuousBezier"));
    let e = if !out && !incoming {
        u
    } else {
        let x1 = if out {
            a["out_influence"]
                .as_f64()
                .unwrap_or(1.0 / 3.0)
                .clamp(0.01, 1.0)
        } else {
            1.0 / 3.0
        };
        let y1 = if out { 0.0 } else { 1.0 / 3.0 };
        let x2 = if incoming {
            1.0 - b["in_influence"]
                .as_f64()
                .unwrap_or(1.0 / 3.0)
                .clamp(0.01, 1.0)
        } else {
            2.0 / 3.0
        };
        let y2 = if incoming { 1.0 } else { 2.0 / 3.0 };
        cubic_bezier_y_for_x(x1, y1, x2, y2, u)
    };
    from_components(
        &a["value"],
        &left
            .iter()
            .zip(right)
            .map(|(a, b)| a + (b - a) * e)
            .collect::<Vec<_>>(),
    )
}
pub fn cubic_bezier_y_for_x(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let bx =
        |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s * s * x2 + s * s * s;
    let by =
        |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s * s * y2 + s * s * s;
    let dbx = |s: f64| {
        3.0 * (1.0 - s) * (1.0 - s) * x1
            + 6.0 * (1.0 - s) * s * (x2 - x1)
            + 3.0 * s * s * (1.0 - x2)
    };
    let mut s = x;
    for _ in 0..8 {
        let d = dbx(s);
        if d.abs() < 1e-9 {
            break;
        }
        let ns = s - (bx(s) - x) / d;
        if !(0.0..=1.0).contains(&ns) {
            break;
        }
        s = ns;
    }
    if (bx(s) - x).abs() > 1e-7 {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..60 {
            s = (lo + hi) / 2.0;
            if bx(s) < x {
                lo = s
            } else {
                hi = s
            }
        }
    }
    by(s)
}

#[cfg(test)]
#[test]
fn original_template_parameter_animation_matches_all_curves_and_types() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-template-animation.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14112);
    for (index, case) in cases.iter().enumerate() {
        let actual =
            sample_parameter(&case["param"], case["time"].as_i64().unwrap() as i128).unwrap();
        assert_eq!(actual, case["value"], "case{index} kind{}", case["kind"]);
    }
}

/// Resolve imported parameters at the exact original clock before native layer conversion.
/// Layer extras (pins and character styles), resources and the input document are retained.
pub fn sample_document(document: &Value, local_us: i128) -> Result<Value> {
    let time = local_us
        .checked_mul(254016)
        .ok_or_else(|| anyhow::anyhow!("original graphic clock overflow"))?;
    let mut snapshot = document.clone();
    for layer in snapshot["layers"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("missing graphic layers"))?
    {
        for param in layer["params"]
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("missing graphic parameters"))?
            .values_mut()
        {
            let sampled = sample_parameter(param, time)?;
            param["value"] = sampled;
            param
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("invalid graphic parameter"))?
                .remove("keyframes");
        }
    }
    Ok(snapshot)
}
#[cfg(test)]
#[test]
fn document_sampling_keeps_original_resources_and_extras_without_mutation() {
    let source: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-template-files.json"
    ))
    .unwrap();
    let cases = source["cases"].as_array().unwrap();
    for case in cases.iter().filter(|case| case["accepted"] == true) {
        let original = case["document"].clone();
        for local in [-1000000i128, 0, 1000000] {
            let sampled = sample_document(&original, local).unwrap();
            assert_eq!(sampled["resources"], original["resources"]);
            for (layer, source) in sampled["layers"]
                .as_array()
                .unwrap()
                .iter()
                .zip(original["layers"].as_array().unwrap())
            {
                assert_eq!(layer["layer"], source["layer"]);
                for (name, param) in layer["params"].as_object().unwrap() {
                    assert!(param.get("keyframes").is_none());
                    assert_eq!(
                        param["value"],
                        sample_parameter(&source["params"][name], local * 254016).unwrap()
                    );
                }
            }
        }
        assert_eq!(original, case["document"]);
    }
    assert!(sample_document(&cases[0]["document"], i128::MAX).is_err());
}

/// Overlay original animated parameters on the current native authored base.
pub fn native_parameters(
    base: &Value,
    params: &serde_json::Map<String, Value>,
    local_us: i128,
) -> Result<Value> {
    let time = local_us
        .checked_mul(254016)
        .ok_or_else(|| anyhow::anyhow!("original graphic clock overflow"))?;
    native_parameters_at_ticks(base, params, time)
}
pub fn native_parameters_at_ticks(base: &Value, params: &serde_json::Map<String, Value>, time: i128) -> Result<Value> {
    let mut result = base.clone();
    for (name, param) in params {
        let value = sample_parameter(param, time)?;
        let tagged = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("invalid parameter value"))?;
        let (kind, value) = tagged
            .iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing parameter value"))?;
        let mut upper = false;
        let key: String = name
            .chars()
            .filter_map(|ch| {
                if ch == '_' {
                    upper = true;
                    None
                } else {
                    let ch = if upper { ch.to_ascii_uppercase() } else { ch };
                    upper = false;
                    Some(ch)
                }
            })
            .collect();
        ensure!(
            result.get(&key).is_some(),
            "unknown native graphic parameter {name}"
        );
        result[&key] = if kind == "Vec2" {
            if name == "position" && (value["x"].is_null() || value["y"].is_null()) {
                Value::Null
            } else {
                json!([value["x"], value["y"]])
            }
        } else {
            value.clone()
        };
    }
    Ok(result)
}
pub fn validate_parameters(params: &serde_json::Map<String, Value>) -> Result<()> {
    for param in params.values() {
        super::graphic_template_file::validate_value(&param["value"])?;
        if let Some(keys) = param.get("keyframes") {
            for key in keys
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid keyframes"))?
            {
                ensure!(key["time"].as_i64().is_some(), "invalid key time");
                super::graphic_template_file::validate_value(&key["value"])?;
                ensure!(
                    key["value"].as_object().unwrap().keys().next()
                        == param["value"].as_object().unwrap().keys().next(),
                    "mixed parameter key types"
                );
                ensure!(
                    matches!(
                        key["interp"].as_str(),
                        Some(
                            "Linear"
                                | "Hold"
                                | "Bezier"
                                | "AutoBezier"
                                | "ContinuousBezier"
                                | "EaseIn"
                                | "EaseOut"
                        )
                    ),
                    "invalid key interpolation"
                );
            }
        }
        sample_parameter(param, 0)?;
    }
    Ok(())
}
