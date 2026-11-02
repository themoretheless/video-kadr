//! Filmcraft Param::set_at semantics for imported graphics (MIT attribution in docs/licenses).
use anyhow::{ensure, Result};
use serde_json::{json, Value};
pub fn set_at(param: &Value, value: Value, time: i128) -> Result<Value> {
    let time = i64::try_from(time)?;
    let mut result = param.clone();
    let keys = param
        .get("keyframes")
        .map(|keys| {
            keys.as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid keyframes"))
        })
        .transpose()?;
    if keys.is_none_or(|keys| keys.is_empty()) {
        result["value"] = value;
        return Ok(result);
    }
    let keys = keys.unwrap();
    for key in keys {
        ensure!(key["time"].as_i64().is_some(), "invalid key time");
    }
    for pair in keys.windows(2) {
        ensure!(
            pair[0]["time"].as_i64().unwrap() < pair[1]["time"].as_i64().unwrap(),
            "key times must increase"
        );
    }
    let index = keys.binary_search_by_key(&time, |key| key["time"].as_i64().unwrap());
    let keys = result["keyframes"].as_array_mut().unwrap();
    match index {
        Ok(index) => keys[index]["value"] = value,
        Err(index) => {
            let interp = keys
                .get(index.saturating_sub(1))
                .and_then(|key| key.get("interp"))
                .cloned()
                .unwrap_or(json!("Linear"));
            keys.insert(index,json!({"time":time,"value":value,"interp":interp,"out_influence":1.0/3.0,"in_influence":1.0/3.0}));
        }
    }
    Ok(result)
}
#[cfg(test)]
#[test]
fn original_key_edits_preserve_existing_handles_and_copy_insertion_interpolation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-template-key-edit.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 560);
    for (index, case) in cases.iter().enumerate() {
        let source = case["param"].clone();
        assert_eq!(
            set_at(
                &source,
                case["value"].clone(),
                case["time"].as_i64().unwrap() as i128
            )
            .unwrap(),
            case["edited"],
            "case{index}"
        );
        assert_eq!(source, case["param"]);
    }
    assert!(set_at(&cases[0]["param"], Value::Null, i128::MAX).is_err());
}
