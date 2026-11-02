//! Filmcraft-compatible template color input (MIT; docs/licenses/filmcraft-MIT.txt).
use anyhow::{ensure, Result};
use serde_json::Value;

pub fn color(value: &Value) -> Result<[f32; 4]> {
    let result = if let Some(text) = value.as_str() {
        let text = text.trim().trim_start_matches('#');
        ensure!(matches!(text.len(), 6 | 8), "invalid hexadecimal color");
        let byte = |index| -> Result<f32> {
            let pair = text.get(index..index+2).ok_or_else(|| anyhow::anyhow!("invalid hexadecimal color"))?;
            Ok(u8::from_str_radix(pair,16)? as f32 / 255.0)
        };
        [byte(0)?,byte(2)?,byte(4)?,if text.len()==8 {byte(6)?}else{1.0}]
    } else {
        let array=value.as_array().ok_or_else(||anyhow::anyhow!("invalid color"))?;
        let component=|index| -> Result<f32> {Ok(array.get(index).and_then(Value::as_f64).ok_or_else(||anyhow::anyhow!("invalid color component"))? as f32)};
        [component(0)?,component(1)?,component(2)?,array.get(3).and_then(Value::as_f64).unwrap_or(1.0) as f32]
    };
    ensure!(result.iter().all(|value|value.is_finite()), "non-finite color");
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_hex_inputs_match_every_component_bit() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-template-color-input.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let actual=color(&case["input"]);
            if case["bits"].is_null() {assert!(actual.is_err(),"{}",case["input"])}
            else {assert_eq!(serde_json::to_value(actual.unwrap().map(f32::to_bits)).unwrap(),case["bits"],"{}",case["input"])}
        }
        assert_eq!(color(&serde_json::json!([0.1,0.2,0.3])).unwrap(),[0.1,0.2,0.3,1.0]);
        assert_eq!(color(&serde_json::json!([0.1,0.2,0.3,0.4])).unwrap(),[0.1,0.2,0.3,0.4]);
        assert!(color(&serde_json::json!([1e300,0,0])).is_err());
    }
}
