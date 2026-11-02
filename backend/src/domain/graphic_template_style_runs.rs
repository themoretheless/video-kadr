//! Original .fcgt StyleRun flattens character style; native runs contain a style object.
use anyhow::{Result, ensure};
use serde_json::{json, Value};
pub fn decode(runs: &Value) -> Result<Value> {
    let runs=runs.as_array().ok_or_else(||anyhow::anyhow!("invalid character runs"))?;
    runs.iter().map(|run| {
        let mut fields=run.as_object().ok_or_else(||anyhow::anyhow!("invalid character run"))?.clone();
        let start=fields.remove("start").unwrap_or(Value::Null);
        let end=fields.remove("end").unwrap_or(Value::Null);
        // Preserve native-shaped files from earlier video-kadr exports as well.
        let style=fields.remove("style").unwrap_or(Value::Object(fields));
        ensure!(style.is_object(),"invalid character style");
        Ok(json!({"start":start,"end":end,"style":style}))
    }).collect::<Result<Vec<_>>>().map(Value::Array)
}
pub fn encode(runs: &Value) -> Result<Value> {
    let runs=runs.as_array().ok_or_else(||anyhow::anyhow!("invalid character runs"))?;
    runs.iter().map(|run| {
        let mut fields=run["style"].as_object().ok_or_else(||anyhow::anyhow!("invalid native character style"))?.clone();
        fields.retain(|_,value|!value.is_null());
        fields.insert("start".into(),run["start"].clone());
        fields.insert("end".into(),run["end"].clone());
        Ok(Value::Object(fields))
    }).collect::<Result<Vec<_>>>().map(Value::Array)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphic_template_character_styles_roundtrip_original_caption_runs() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-upgrade.json")).unwrap();
        let mut styled=0;
        for case in fixture["cases"].as_array().unwrap() {
            let original=&case["document"];
            let native=super::super::graphic_templates::from_original(original).unwrap();
            let exported=super::super::graphic_template_export::export_native(&serde_json::to_value(&native).unwrap(),0).unwrap();
            for (layer,output) in original["layers"].as_array().unwrap().iter().zip(exported["layers"].as_array().unwrap()) {
                if let Some(runs)=layer["layer"].get("runs") {
                    styled+=1;
                    assert_eq!(&output["layer"]["runs"],runs);
                    let typed:Vec<super::super::graphic_text_runs::CharacterStyleRun>=serde_json::from_value(decode(runs).unwrap()).unwrap();
                    assert!(typed.iter().all(|run|!run.style.is_empty()));
                }
            }
        }
        assert!(styled>=6);
    }
}
