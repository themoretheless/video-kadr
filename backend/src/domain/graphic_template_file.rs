//! Lossless Filmcraft .fcgt JSON document boundary.
//! Format and validation adapted from Filmcraft gtemplate.rs (MIT; docs/licenses/filmcraft-MIT.txt).
//! Rendering conversion is separate: keep animations/resources intact at this boundary.
use anyhow::{ensure, Result};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphicTemplateFile(Value);
impl GraphicTemplateFile {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        ensure!(
            !bytes.starts_with(b"PK\x03\x04") && !bytes.starts_with(b"PK\x05\x06"),
            "foreign ZIP template package; expected Filmcraft .fcgt"
        );
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        let value: Value = serde_json::from_slice(bytes)?;
        ensure!(
            value["format"] == "filmcraft.graphicsTemplate",
            "not a Filmcraft graphics template"
        );
        let version = value["version"].as_u64().unwrap_or(0);
        ensure!(
            version == 1,
            "unsupported graphics template version {version}"
        );
        ensure!(
            value["name"].as_str().is_some_and(|s| !s.trim().is_empty()),
            "template has no name"
        );
        let canvas = value
            .get("canvas")
            .cloned()
            .unwrap_or(serde_json::json!([1920, 1080]));
        let canvas = canvas
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid canvas"))?;
        ensure!(
            canvas.len() == 2
                && canvas
                    .iter()
                    .all(|n| n.as_u64().is_some_and(|n| n > 0 && n <= 16384)),
            "invalid canvas"
        );
        let layers = value["layers"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing layers"))?;
        ensure!(!layers.is_empty(), "template has no layers");
        let mut uids = HashSet::new();
        for layer in layers {
            ensure!(
                matches!(
                    layer["effect"].as_str(),
                    Some("graphic_text" | "graphic_shape")
                ),
                "not a graphic layer"
            );
            let uid = layer["layer"]["uid"].as_u64().unwrap_or(0);
            ensure!(uid != 0 && uids.insert(uid), "layer has no unique uid");
            ensure!(layer["params"].is_object(), "invalid layer parameters");
            for param in layer["params"].as_object().unwrap().values() {
                validate_value(&param["value"])?;
                if let Some(keys) = param.get("keyframes") {
                    for key in keys
                        .as_array()
                        .ok_or_else(|| anyhow::anyhow!("invalid keyframes"))?
                    {
                        ensure!(key["time"].as_i64().is_some(), "invalid key time");
                        validate_value(&key["value"])?;
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
                            "invalid interpolation"
                        );
                        for name in ["out_influence", "in_influence"] {
                            if let Some(value) = key.get(name) {
                                ensure!(value.as_f64().is_some(), "invalid key influence");
                            }
                        }
                    }
                }
            }
        }
        let mut ids = HashSet::new();
        if let Some(controls) = value.get("controls") {
            for control in controls
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid controls"))?
            {
                let id = control["id"].as_str().unwrap_or("");
                ensure!(!id.is_empty() && ids.insert(id), "control has no unique id");
                let layer = layers
                    .iter()
                    .find(|layer| layer["layer"]["uid"] == control["layer"])
                    .ok_or_else(|| anyhow::anyhow!("control points at missing layer"))?;
                let param = control["param"].as_str().unwrap_or("");
                ensure!(
                    param == "enabled" || layer["params"].get(param).is_some(),
                    "control parameter missing"
                );
                let kind = control
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                ensure!(
                    matches!(
                        kind,
                        "text" | "color" | "slider" | "checkbox" | "font" | "position"
                    ),
                    "unknown control kind"
                );
            }
        }
        if let Some(fonts) = value.get("resources").and_then(|v| v.get("fonts")) {
            for font in fonts
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid font resources"))?
            {
                let data = font.get("data").and_then(Value::as_str).unwrap_or("");
                // Original decoder skips ASCII whitespace and stops at first padding.
                ensure!(
                    data.bytes()
                        .filter(|b| !b.is_ascii_whitespace())
                        .take_while(|b| *b != b'=')
                        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/'),
                    "font is not valid base64"
                );
            }
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> &Value {
        &self.0
    }
    pub fn write(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(&self.0)?)
    }
}
pub(super) fn validate_value(value: &Value) -> Result<()> {
    let tagged = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid parameter value"))?;
    ensure!(tagged.len() == 1, "invalid parameter tag");
    let (kind, value) = tagged.iter().next().unwrap();
    let valid = match kind.as_str() {
        "Float" => value.as_f64().is_some(),
        "Bool" => value.is_boolean(),
        "Choice" => value.as_u64().is_some_and(|v| v <= u32::MAX as u64),
        "Text" => value.is_string(),
        "Vec2" => {
            value.is_object()
                && ["x", "y"].iter().all(|axis| {
                    value
                        .get(*axis)
                        .is_some_and(|v| v.is_null() || v.as_f64().is_some())
                })
        }
        "Color" => value
            .as_array()
            .is_some_and(|v| v.len() == 4 && v.iter().all(|v| v.as_f64().is_some())),
        "Curve" => value.as_array().is_some_and(|v| {
            v.iter().all(|v| {
                v.as_array()
                    .is_some_and(|v| v.len() == 2 && v.iter().all(|v| v.as_f64().is_some()))
            })
        }),
        "Path" => {
            value.is_object()
                && value.get("closed").is_none_or(Value::is_boolean)
                && value
                    .get("vertices")
                    .and_then(Value::as_array)
                    .is_some_and(|vertices| {
                        vertices.iter().all(|vertex| {
                            let point = |point: &Value| {
                                point.is_object()
                                    && ["x", "y"].iter().all(|axis| {
                                        point.get(*axis).and_then(Value::as_f64).is_some()
                                    })
                            };
                            vertex.is_object()
                                && point(&vertex["p"])
                                && ["t_in", "t_out"]
                                    .iter()
                                    .all(|field| vertex.get(*field).is_none_or(point))
                        })
                    })
        }
        _ => false,
    };
    ensure!(valid, "invalid {kind} parameter value");
    Ok(())
}
#[cfg(test)]
#[test]
fn original_file_admission_and_animated_resource_preservation() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-template-files.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 96);
    for case in cases {
        let raw = &case["document"];
        let mut bytes = serde_json::to_vec(raw).unwrap();
        if case["bom"] == true {
            bytes.splice(0..0, [0xef, 0xbb, 0xbf]);
        }
        let result = GraphicTemplateFile::read(&bytes);
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{} {}",
            case["id"],
            case["variant"]
        );
        if let Ok(file) = result {
            assert_eq!(file.value(), raw);
            assert_eq!(
                GraphicTemplateFile::read(&file.write().unwrap()).unwrap(),
                file
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_templates_lossless_file_roundtrip() {
        let source: Value =
            serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
        let cases = source["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 8);
        for raw in cases {
            let file = GraphicTemplateFile::read(&serde_json::to_vec(raw).unwrap()).unwrap();
            assert_eq!(file.value(), raw);
            assert_eq!(
                GraphicTemplateFile::read(&file.write().unwrap()).unwrap(),
                file
            );
            let mut resource = raw.clone();
            resource["resources"] = serde_json::json!({"fonts":[{"family":"Test","style":"Regular","license":"OFL-1.1","data":"TWFu"}]});
            let file = GraphicTemplateFile::read(&serde_json::to_vec(&resource).unwrap()).unwrap();
            assert_eq!(file.value(), &resource);
        }
    }
    #[test]
    fn refuses_foreign_newer_and_broken_references() {
        for bytes in [
            b"PK\x03\x04rest".as_slice(),
            b"PK\x05\x06rest",
            b"{}",
            b"\xff",
        ] {
            assert!(GraphicTemplateFile::read(bytes).is_err());
        }
        let source: Value =
            serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
        for mutation in ["version", "uid", "target", "param", "font"] {
            let mut raw = source["cases"][0].clone();
            match mutation {
                "version" => raw["version"] = serde_json::json!(99),
                "uid" => {
                    raw["layers"][1]["layer"]["uid"] = raw["layers"][0]["layer"]["uid"].clone()
                }
                "target" => raw["controls"][0]["layer"] = serde_json::json!(999),
                "param" => raw["controls"][0]["param"] = serde_json::json!("absent"),
                _ => raw["resources"] = serde_json::json!({"fonts":[{"data":"@@@"}]}),
            }
            assert!(
                GraphicTemplateFile::read(&serde_json::to_vec(&raw).unwrap()).is_err(),
                "{mutation}"
            );
        }
    }
}
