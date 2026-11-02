//! Original Filmcraft builtin graphics templates and native editable controls.
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    graphic_clip::{GraphicClip, GraphicContent, GraphicLayer},
    graphic_responsive_time::ResponsiveTime,
    graphic_roll::Roll,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateControl {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub layer: u64,
    pub param: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicTemplate {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub tags: Vec<String>,
    pub canvas: [u32; 2],
    pub duration_us: u64,
    pub graphic: GraphicClip,
    pub roll: Option<Roll>,
    pub responsive_time: Option<ResponsiveTime>,
    pub controls: Vec<TemplateControl>,
    #[serde(
        default,
        skip_serializing_if = "super::graphic_template_fonts::Resources::is_empty"
    )]
    pub resources: super::graphic_template_fonts::Resources,
}
fn camel(name: &str) -> String {
    let mut upper = false;
    name.chars()
        .filter_map(|ch| {
            if ch == '_' {
                upper = true;
                None
            } else {
                let out = if upper { ch.to_ascii_uppercase() } else { ch };
                upper = false;
                Some(out)
            }
        })
        .collect()
}
pub fn builtin_templates() -> Result<Vec<GraphicTemplate>> {
    let source: Value =
        serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json"))?;
    source["cases"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid builtin template catalog"))?
        .iter()
        .map(from_original)
        .collect()
}
pub fn from_original(raw: &Value) -> Result<GraphicTemplate> {
    super::graphic_template_file::GraphicTemplateFile::read(&serde_json::to_vec(raw)?)?;
    let mut layers = vec![];
    for effect in raw["layers"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing template layers"))?
    {
        let mut parameters = serde_json::Map::new();
        for (name, param) in effect["params"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("missing template parameters"))?
        {
            let tagged = param["value"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("invalid template parameter"))?;
            ensure!(tagged.len() == 1, "invalid template parameter tag");
            let (kind, value) = tagged.iter().next().unwrap();
            parameters.insert(
                camel(name),
                if kind == "Vec2" {
                    if name == "position" && (value["x"].is_null() || value["y"].is_null()) {
                        Value::Null
                    } else {
                        json!([value["x"], value["y"]])
                    }
                } else {
                    value.clone()
                },
            );
        }
        if effect["effect"] == "graphic_text" {
            if let Some(runs) = effect["layer"].get("runs") {
                parameters.insert("runs".into(), super::graphic_template_style_runs::decode(runs)?);
            }
        }
        let content = match effect["effect"].as_str() {
            Some("graphic_text") => GraphicContent::Text {
                parameters: serde_json::from_value(parameters.into())?,
                animation: None,
                source_text_animation: None,
            },
            Some("graphic_shape") => GraphicContent::Shape {
                parameters: serde_json::from_value(parameters.into())?,
                animation: None,
            },
            _ => anyhow::bail!("unsupported builtin graphic layer"),
        };
        layers.push(GraphicLayer {
            original_parameters: effect["params"]
                .as_object()
                .map(|params| {
                    params
                        .iter()
                        .filter(|(_, param)| {
                            param["keyframes"]
                                .as_array()
                                .is_some_and(|keys| !keys.is_empty())
                        })
                        .map(|(name, param)| (name.clone(), param.clone()))
                        .collect()
                })
                .filter(|params: &serde_json::Map<String, Value>| !params.is_empty()),
            uid: effect["layer"]["uid"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("missing layer uid"))?,
            enabled: effect["enabled"].as_bool().unwrap_or(true),
            pin: effect["layer"]
                .get("pin")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?,
            content,
        });
    }
    let graphic = GraphicClip { layers };
    graphic.validate()?;
    let ticks = raw
        .get("duration")
        .cloned()
        .unwrap_or(json!(1270080000000u64))
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("invalid template duration"))?;
    ensure!(ticks % 254016 == 0, "inexact builtin duration conversion");
    let text = |key: &str| -> Result<String> {
        if let Some(value) = raw.get(key) {
            return Ok(value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid template {key}"))?
                .into());
        }
        Ok(if key == "category" {
            "My Templates"
        } else {
            ""
        }
        .into())
    };
    let mut meta = raw.get("graphic").cloned().unwrap_or(json!({}));
    ensure!(meta.is_object(), "invalid original graphic metadata");
    let convert = |value: &mut Value| -> Result<()> {
        let ticks = value
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("invalid original graphic time"))?;
        ensure!(
            ticks % 254016 == 0,
            "original graphic time is not representable in microseconds"
        );
        *value = json!(ticks / 254016);
        Ok(())
    };
    for field in ["intro", "outro", "designIn", "designDuration"] {
        if let Some(value) = meta.get_mut(field) {
            convert(value)?;
        }
    }
    if let Some(roll) = meta.get_mut("roll") {
        for field in ["preroll", "easeIn", "easeOut", "postroll"] {
            if let Some(value) = roll.get_mut(field) {
                convert(value)?;
            }
        }
    }
    let roll = meta
        .as_object_mut()
        .unwrap()
        .remove("roll")
        .map(serde_json::from_value)
        .transpose()?;
    let timing: ResponsiveTime = serde_json::from_value(meta)?;
    Ok(GraphicTemplate {
        id: {
            let id = text("id")?;
            if id.is_empty() {
                format!("user:{}", uuid::Uuid::new_v4())
            } else {
                id
            }
        },
        name: text("name")?,
        category: text("category")?,
        description: text("description")?,
        author: text("author")?,
        license: text("license")?,
        tags: serde_json::from_value(raw.get("tags").cloned().unwrap_or(json!([])))?,
        canvas: serde_json::from_value(raw.get("canvas").cloned().unwrap_or(json!([1920, 1080])))?,
        duration_us: ticks / 254016,
        graphic,
        roll,
        responsive_time: if timing.active() { Some(timing) } else { None },
        controls: serde_json::from_value(raw.get("controls").cloned().unwrap_or(json!([])))?,
        resources: serde_json::from_value(raw.get("resources").cloned().unwrap_or(json!({})))?,
    })
}

fn sampled_parameters(layer: &GraphicLayer, local_us: i128, canvas: [u32; 2]) -> Result<Value> {
    let content=serde_json::to_value(&layer.content)?;
    let mut parameters=if let Some(original)=&layer.original_parameters {
        super::graphic_template_animation::native_parameters(&content["parameters"],original,local_us)?
    } else {content["parameters"].clone()};
    match &layer.content {
        GraphicContent::Shape {animation:Some(animation),..}=>{
            parameters=serde_json::to_value(super::graphic_shape_animation::sample_signed(serde_json::from_value(parameters)?,animation,local_us,(canvas[0],canvas[1]))?)?;
        },
        GraphicContent::Text {animation,source_text_animation,..}=>{
            if let Some(animation)=animation {
                parameters=serde_json::to_value(super::graphic_text_animation::sample_signed(serde_json::from_value(parameters)?,animation,local_us,(canvas[0],canvas[1]))?)?;
            }
            if let Some(animation)=source_text_animation {parameters["text"]=json!(animation.sample_signed(local_us)?);}
        },
        _=>{},
    }
    if parameters["position"].is_null() {parameters["position"]=json!([canvas[0] as f64/2.0,canvas[1] as f64/2.0]);}
    Ok(parameters)
}

impl GraphicTemplate {
    pub fn with_embedded_fonts(&self,license:&str)->Result<Self> {
        self.graphic.validate()?;
        let mut families=std::collections::BTreeSet::new();
        for layer in &self.graphic.layers {
            let GraphicContent::Text {parameters,..}=&layer.content else {continue};
            let mut pairs=std::collections::BTreeSet::from([(parameters.font.clone(),parameters.font_style.clone())]);
            if let Some(original)=&layer.original_parameters {
                let mut times=std::collections::BTreeSet::from([0_i128]);
                for field in ["font","font_style"] {
                    if let Some(keys)=original.get(field).and_then(|param|param.get("keyframes")).and_then(Value::as_array) {
                        for key in keys {times.insert(i128::from(key["time"].as_i64().ok_or_else(||anyhow::anyhow!("invalid font key time"))?));}
                    }
                }
                for time in times {
                    let mut pair=(parameters.font.clone(),parameters.font_style.clone());
                    for (field,target) in [("font",&mut pair.0),("font_style",&mut pair.1)] {
                        if let Some(param)=original.get(field) {
                            let value=super::graphic_template_animation::sample_parameter(param,time)?;
                            *target=value["Text"].as_str().ok_or_else(||anyhow::anyhow!("invalid font parameter"))?.into();
                        }
                    }
                    pairs.insert(pair);
                }
            }
            for (family,style) in pairs {
                families.insert((family.clone(),style.clone()));
                for run in &parameters.runs {
                    families.insert((run.style.font.clone().unwrap_or_else(||family.clone()),run.style.font_style.clone().unwrap_or_else(||style.clone())));
                }
            }
        }
        let mut result=self.clone();
        super::graphic_template_fonts::embed(&mut result.resources,families,license);
        Ok(result)
    }
    pub fn control_values_at(&self, local_us: i128) -> Result<serde_json::Map<String, Value>> {
        self.graphic.validate()?;
        let mut values = serde_json::Map::new();
        for control in &self.controls {
            let layer = self
                .graphic
                .layers
                .iter()
                .find(|layer| layer.uid == control.layer)
                .ok_or_else(|| anyhow::anyhow!("template control layer missing"))?;
            let value = if control.param == "enabled" {
                json!(layer.enabled)
            } else {
                let parameters = sampled_parameters(layer,local_us,self.canvas)?;
                let value = parameters
                    .get(camel(&control.param))
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("template control parameter missing"))?;
                if control.kind == "font" {
                    json!({"family":value,"style":parameters.get("fontStyle").and_then(Value::as_str).unwrap_or("")})
                } else { value }
            };
            values.insert(control.id.clone(), value);
        }
        Ok(values)
    }
    /// Apply a batch at one local clock; failures leave the original template unchanged.
    pub fn set_controls_at(&self, values: &serde_json::Map<String, Value>, local_us: i128) -> Result<Self> {
        let mut next = self.clone();
        for (id, value) in values {
            next = next.set_control_at(id, value.clone(), local_us)?;
        }
        Ok(next)
    }
    pub fn set_control(&self, id: &str, value: Value) -> Result<Self> {
        self.set_control_at(id, value, 0)
    }
    pub fn set_control_at(&self, id: &str, value: Value, local_us: i128) -> Result<Self> {
        let control = self
            .controls
            .iter()
            .find(|c| c.id == id)
            .or_else(|| {
                self.controls
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(id))
            })
            .ok_or_else(|| anyhow::anyhow!("template control not found"))?;
        let value = if control.kind == "color" {
            serde_json::to_value(super::graphic_template_color::color(&value)?)?
        } else { value };
        let value = if control.kind == "slider" {
            let number = value.as_f64().ok_or_else(|| anyhow::anyhow!("invalid slider value"))?;
            ensure!(number.is_finite(), "invalid slider value");
            let min = control.min.unwrap_or(f64::MIN);
            let max = control.max.unwrap_or(f64::MAX);
            ensure!(min.is_finite() && max.is_finite() && min <= max, "invalid slider range");
            let number = number.clamp(min, max);
            if matches!(control.param.as_str(), "shape" | "align" | "caps" | "stroke_type" | "stroke2_type") {
                ensure!(number >= 0.0 && number <= u32::MAX as f64 && number.fract() == 0.0, "choice value must be an unsigned integer");
                json!(number as u32)
            } else { json!(number) }
        } else { value };
        if control.kind == "font" && value.is_object() {
            let mut next = self.clone();
            for (field, param) in [("family", "font"), ("style", "font_style")] {
                if let Some(value) = value.get(field) {
                    ensure!(value.is_string(), "invalid font value");
                    let mut temporary = control.clone();
                    temporary.param = param.into();
                    temporary.kind = "text".into();
                    let controls = next.controls.clone();
                    next.controls = vec![temporary];
                    next = next.set_control_at(&control.id, value.clone(), local_us)?;
                    next.controls = controls;
                }
            }
            return Ok(next);
        }
        let valid = match control.kind.as_str() {
            "text" | "font" => value.is_string(),
            "checkbox" => value.is_boolean(),
            "slider" => value.as_f64().is_some_and(|v| {
                v.is_finite()
                    && control.min.is_none_or(|min| v >= min)
                    && control.max.is_none_or(|max| v <= max)
            }),
            "color" | "position" => value.as_array().is_some_and(|a| {
                a.len() == if control.kind == "color" { 4 } else { 2 }
                    && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))
            }),
            _ => false,
        };
        ensure!(valid, "invalid template control value");
        let mut next = self.clone();
        let layer = next
            .graphic
            .layers
            .iter_mut()
            .find(|layer| layer.uid == control.layer)
            .ok_or_else(|| anyhow::anyhow!("template control layer missing"))?;
        if control.param == "enabled" {
            layer.enabled = value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("invalid visibility"))?;
        } else {
            let mut content = serde_json::to_value(&layer.content)?;
            if control.param == "text" {
                let parameters=sampled_parameters(layer,local_us,self.canvas)?;
                let old = parameters["text"].as_str().ok_or_else(|| anyhow::anyhow!("invalid source text"))?;
                let new = value.as_str().ok_or_else(|| anyhow::anyhow!("invalid new text"))?;
                if let GraphicContent::Text { parameters, .. } = &layer.content {
                    content["parameters"]["runs"] = serde_json::to_value(super::graphic_text_editing::adjust_runs(old, new, &parameters.runs))?;
                }
            }
            content["parameters"][camel(&control.param)] = value.clone();
            if matches!(control.kind.as_str(), "slider" | "color" | "position") {
                super::graphic_template_native_key_edit::edit_existing(&mut content,&control.param,&value,local_us)?;
            }
            layer.content = serde_json::from_value(content)?;
            if control.param == "text" {
                if let GraphicContent::Text { source_text_animation: Some(animation), .. } = &mut layer.content {
                    *animation = animation.set_at(local_us, value.as_str().ok_or_else(||anyhow::anyhow!("invalid new text"))?.into())?;
                }
            }
            if let Some(param) = layer
                .original_parameters
                .as_mut()
                .and_then(|params| params.get_mut(&control.param))
            {
                let kind = param["value"]
                    .as_object()
                    .and_then(|value| value.keys().next())
                    .ok_or_else(|| anyhow::anyhow!("invalid original parameter"))?
                    .clone();
                let value = if kind == "Vec2" {
                    json!({"x":value[0],"y":value[1]})
                } else {
                    value
                };
                let value = json!({kind:value});
                let time = local_us
                    .checked_mul(254016)
                    .ok_or_else(|| anyhow::anyhow!("original key time overflow"))?;
                *param = super::graphic_template_key_edit::set_at(param, value, time)?;
            }
        }
        next.graphic.validate()?;
        Ok(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_builtins_convert_and_keep_control_targets() {
        let templates = builtin_templates().unwrap();
        assert_eq!(templates.len(), 8);
        for t in templates {
            assert!(t.duration_us > 0);
            t.graphic.validate().unwrap();
            for c in &t.controls {
                assert!(t.graphic.layers.iter().any(|l| l.uid == c.layer));
            }
            assert!(t.set_control("missing", Value::Null).is_err());
        }
    }
    #[test]
    fn editable_controls_validate_and_preserve_original() {
        let t = builtin_templates().unwrap().remove(0);
        let edited = t.set_control("name", json!("Привет")).unwrap();
        assert_ne!(edited.graphic, t.graphic);
        assert!(t.set_control("name", json!(123)).is_err());
        let edited = t.set_control("show_role", json!(false)).unwrap();
        assert!(
            !edited
                .graphic
                .layers
                .iter()
                .find(|l| l.uid == 3)
                .unwrap()
                .enabled
        );
        assert!(
            t.graphic
                .layers
                .iter()
                .find(|l| l.uid == 3)
                .unwrap()
                .enabled
        );
    }
}
#[cfg(test)]
#[test]
fn all_38_original_control_values_roundtrip_without_changes() {
    let mut count = 0;
    for template in builtin_templates().unwrap() {
        for control in &template.controls {
            let layer = template
                .graphic
                .layers
                .iter()
                .find(|layer| layer.uid == control.layer)
                .unwrap();
            let value = if control.param == "enabled" {
                json!(layer.enabled)
            } else {
                serde_json::to_value(&layer.content).unwrap()["parameters"][camel(&control.param)]
                    .clone()
            };
            let edited = template.set_control(&control.id, value).unwrap();
            assert_eq!(
                edited.graphic, template.graphic,
                "{} {}",
                template.id, control.id
            );
            count += 1;
        }
    }
    assert_eq!(count, 38);
}

/// Template authorship and editable definitions retained on a placed native graphic clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicTemplateMetadata {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub tags: Vec<String>,
    pub canvas: [u32; 2],
    pub duration_us: u64,
    pub controls: Vec<TemplateControl>,
    #[serde(
        default,
        skip_serializing_if = "super::graphic_template_fonts::Resources::is_empty"
    )]
    pub resources: super::graphic_template_fonts::Resources,
}
impl GraphicTemplateMetadata {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.id.is_empty(), "missing graphic template id");
        ensure!(
            self.duration_us > 0 && self.duration_us <= 9_007_199_254_740_991,
            "invalid template duration"
        );
        ensure!(
            self.canvas.iter().all(|size| *size > 0 && *size <= 32768),
            "invalid template canvas"
        );
        ensure!(self.controls.len() <= 512, "too many template controls");
        ensure!(
            self.resources
                .fonts
                .iter()
                .all(|font| super::graphic_template_fonts::base64_decode(&font.data).is_some()),
            "invalid embedded font encoding"
        );
        let mut ids = std::collections::BTreeSet::new();
        for control in &self.controls {
            ensure!(
                !control.id.is_empty() && ids.insert(&control.id),
                "duplicate or empty template control id"
            );
            ensure!(
                ["text", "font", "slider", "checkbox", "color", "position"]
                    .contains(&control.kind.as_str()),
                "unknown template control kind"
            );
            ensure!(
                control.min.is_none_or(f64::is_finite) && control.max.is_none_or(f64::is_finite),
                "invalid template control bounds"
            );
            ensure!(
                !matches!((control.min,control.max),(Some(min),Some(max)) if min>max),
                "reversed template control bounds"
            );
        }
        Ok(())
    }
}
impl From<&GraphicTemplate> for GraphicTemplateMetadata {
    fn from(t: &GraphicTemplate) -> Self {
        Self {
            id: t.id.clone(),
            name: t.name.clone(),
            category: t.category.clone(),
            description: t.description.clone(),
            author: t.author.clone(),
            license: t.license.clone(),
            tags: t.tags.clone(),
            canvas: t.canvas,
            duration_us: t.duration_us,
            controls: t.controls.clone(),
            resources: t.resources.clone(),
        }
    }
}
#[cfg(test)]
#[test]
fn placed_native_template_metadata_roundtrips_and_refuses_duplicate_controls() {
    use super::composition::{AnimatableValue, TransformSpec};
    use super::graphic_timeline::GraphicTimelineClip;
    for template in builtin_templates().unwrap() {
        let metadata = GraphicTemplateMetadata::from(&template);
        metadata.validate().unwrap();
        let clip:GraphicTimelineClip=serde_json::from_value(json!({"id":"template","timelineStartTick":0,"timelineEndTick":template.duration_us,"width":template.canvas[0],"height":template.canvas[1],"graphic":template.graphic,"template":metadata,"roll":template.roll,"responsiveTime":template.responsive_time,"transform":TransformSpec::default(),"opacity":AnimatableValue::constant(1.)})).unwrap();
        clip.validate().unwrap();
        let round: GraphicTimelineClip =
            serde_json::from_value(serde_json::to_value(&clip).unwrap()).unwrap();
        assert_eq!(clip, round);
        let mut invalid = clip.template.unwrap();
        invalid.controls.push(invalid.controls[0].clone());
        assert!(invalid.validate().is_err());
    }
}

#[cfg(test)]
#[test]
fn template_batch_font_object_and_slider_clamp_keep_original_immutable() {
    let mut template = builtin_templates().unwrap().remove(0);
    let layer = template.controls.iter().find(|c| c.kind == "font").unwrap().layer;
    template.controls.push(TemplateControl { id: "alpha".into(), name:"Alpha".into(),kind:"slider".into(),layer,param:"opacity".into(),min:Some(10.0),max:Some(90.0) });
    let font_id = template.controls.iter().find(|c| c.kind == "font").unwrap().id.clone();
    let original = template.clone();
    let values = json!({font_id.clone():{"family":"Inter","style":"Bold"},"alpha":999}).as_object().unwrap().clone();
    let edited = template.set_controls_at(&values, -250000).unwrap();
    let content = serde_json::to_value(&edited.graphic.layers.iter().find(|l| l.uid==layer).unwrap().content).unwrap();
    assert_eq!(content["parameters"]["font"], "Inter");
    assert_eq!(content["parameters"]["fontStyle"], "Bold");
    assert_eq!(content["parameters"]["opacity"], 90.0);
    assert_eq!(edited.controls, original.controls);
    assert_eq!(template, original);
    let mut bad = values.clone();bad.insert("missing".into(), json!(1));
    assert!(template.set_controls_at(&bad, 0).is_err());
    assert_eq!(template, original);
    assert!(template.set_control(&font_id, json!({"style":23})).is_err());
    let low = template.set_control("alpha", json!(-999)).unwrap();
    let content = serde_json::to_value(&low.graphic.layers.iter().find(|l| l.uid==layer).unwrap().content).unwrap();
    assert_eq!(content["parameters"]["opacity"], 10.0);
}

#[cfg(test)]
#[test]
fn template_font_values_sample_family_and_style_at_signed_clock() {
    let mut template = builtin_templates().unwrap().remove(0);
    let control = template.controls.iter().find(|c| c.kind == "font").unwrap().clone();
    let layer = template.graphic.layers.iter_mut().find(|l| l.uid == control.layer).unwrap();
    layer.original_parameters = Some(json!({
        "font":{"value":{"Text":"Base"},"keyframes":[{"time":-254016000000_i64,"value":{"Text":"Inter"},"interp":"Hold"},{"time":254016000000_i64,"value":{"Text":"Other"},"interp":"Hold"}]},
        "font_style":{"value":{"Text":"Regular"},"keyframes":[{"time":-254016000000_i64,"value":{"Text":"Bold"},"interp":"Hold"},{"time":254016000000_i64,"value":{"Text":"Italic"},"interp":"Hold"}]}
    }).as_object().unwrap().clone());
    assert_eq!(template.control_values_at(-250000).unwrap()[&control.id],json!({"family":"Inter","style":"Bold"}));
    assert_eq!(template.control_values_at(1000000).unwrap()[&control.id],json!({"family":"Other","style":"Italic"}));
    let edited = template.set_control_at(&control.id,json!({"style":"Medium"}),0).unwrap();
    assert_eq!(edited.control_values_at(0).unwrap()[&control.id],json!({"family":"Inter","style":"Medium"}));
    assert_eq!(template.control_values_at(0).unwrap()[&control.id],json!({"family":"Inter","style":"Bold"}));
}

#[cfg(test)]
#[test]
fn template_text_controls_adjust_character_runs_using_sampled_text() {
    let mut template = builtin_templates().unwrap().remove(0);
    let control = template.controls.iter().find(|c| c.kind == "text").unwrap().clone();
    let layer = template.graphic.layers.iter_mut().find(|l| l.uid == control.layer).unwrap();
    if let GraphicContent::Text { parameters, .. } = &mut layer.content {
        parameters.text = "Base".into();
        parameters.runs = serde_json::from_value(json!([{"start":1,"end":2,"style":{"underline":true}}])).unwrap();
    }
    layer.original_parameters = Some(json!({"text":{"value":{"Text":"Base"},"keyframes":[{"time":-254016000000_i64,"value":{"Text":"AБC"},"interp":"Hold"},{"time":254016000000_i64,"value":{"Text":"Other"},"interp":"Hold"}]}}).as_object().unwrap().clone());
    let edited = template.set_control_at(&control.id,json!("A🙂БC"),0).unwrap();
    let layer = edited.graphic.layers.iter().find(|l| l.uid == control.layer).unwrap();
    if let GraphicContent::Text { parameters, .. } = &layer.content {
        assert_eq!(parameters.runs.len(),1);
        assert_eq!((parameters.runs[0].start,parameters.runs[0].end),(2,3));
        assert_eq!(parameters.runs[0].style.underline,Some(true));
    } else { panic!("expected text"); }
    assert_eq!(edited.control_values_at(0).unwrap()[&control.id],json!("A🙂БC"));
    assert_eq!(template.control_values_at(0).unwrap()[&control.id],json!("AБC"));
    let emptied = template.set_control_at(&control.id,json!(""),0).unwrap();
    if let GraphicContent::Text { parameters, .. } = &emptied.graphic.layers.iter().find(|l| l.uid==control.layer).unwrap().content {
        assert!(parameters.runs.is_empty());
    }
}

#[cfg(test)]
#[test]
fn template_discrete_control_values_keep_integer_parameter_types() {
    let mut template = builtin_templates().unwrap().remove(0);
    let layer = template.graphic.layers.iter().find(|layer|matches!(layer.content,GraphicContent::Shape{..})).unwrap().uid;
    template.controls.push(TemplateControl {id:"shape_kind".into(),name:"Shape".into(),kind:"slider".into(),layer,param:"shape".into(),min:Some(0.0),max:Some(3.0)});
    let edited=template.set_control("shape_kind",json!(1)).unwrap();
    let content=serde_json::to_value(&edited.graphic.layers.iter().find(|l|l.uid==layer).unwrap().content).unwrap();
    assert_eq!(content["parameters"]["shape"],json!(1));
    assert!(template.set_control("shape_kind",json!(1.5)).is_err());
    assert!(template.set_control("shape_kind",json!(999)).is_ok());
}

#[cfg(test)]
#[test]
fn template_colors_accept_original_hex_and_rgb_inputs() {
    let template=builtin_templates().unwrap().remove(0);
    let control=template.controls.iter().find(|control|control.kind=="color").unwrap();
    for input in [json!("#11223380"),json!([0.1,0.2,0.3])] {
        let expected=super::graphic_template_color::color(&input).unwrap();
        let edited=template.set_control(&control.id,input).unwrap();
        let actual: [f32;4]=serde_json::from_value(edited.control_values_at(0).unwrap()[&control.id].clone()).unwrap();
        assert_eq!(actual.map(f32::to_bits),expected.map(f32::to_bits));
    }
    assert!(template.set_control(&control.id,json!("#gg1122")).is_err());
}

#[cfg(test)]
#[test]
fn template_text_controls_edit_authored_text_animation_at_playhead() {
    use super::graphic_source_text_animation::{SourceTextAnimation,SourceTextKey};
    let mut template=builtin_templates().unwrap().remove(0);
    let control=template.controls.iter().find(|control|control.kind=="text").unwrap().clone();
    let layer=template.graphic.layers.iter_mut().find(|layer|layer.uid==control.layer).unwrap();
    if let GraphicContent::Text {parameters,source_text_animation,..}=&mut layer.content {
        parameters.text="Base".into();
        parameters.runs=serde_json::from_value(json!([{"start":1,"end":2,"style":{"underline":true}}])).unwrap();
        *source_text_animation=Some(SourceTextAnimation {time_base:1000000,keyframes:vec![SourceTextKey {tick:-1000000,value:"AБC".into()},SourceTextKey {tick:1000000,value:"End".into()}]});
    }
    assert_eq!(template.control_values_at(-250000).unwrap()[&control.id],json!("AБC"));
    let edited=template.set_control_at(&control.id,json!("A🙂БC"),0).unwrap();
    assert_eq!(edited.control_values_at(0).unwrap()[&control.id],json!("A🙂БC"));
    assert_eq!(edited.control_values_at(-1).unwrap()[&control.id],json!("AБC"));
    if let GraphicContent::Text {parameters,source_text_animation,..}=&edited.graphic.layers.iter().find(|layer|layer.uid==control.layer).unwrap().content {
        assert_eq!((parameters.runs[0].start,parameters.runs[0].end),(2,3));
        assert_eq!(source_text_animation.as_ref().unwrap().keyframes.len(),3);
    }
    assert_eq!(template.control_values_at(0).unwrap()[&control.id],json!("AБC"));
}

#[cfg(test)]
#[test]
fn template_numeric_values_sample_authored_tracks_and_auto_position() {
    let mut template=builtin_templates().unwrap().remove(0);
    let canvas=template.canvas;
    let layer=template.graphic.layers.iter_mut().find(|layer|matches!(layer.content,GraphicContent::Shape{..})).unwrap();
    let uid=layer.uid;
    let mut content=serde_json::to_value(&layer.content).unwrap();
    content["parameters"]["position"]=Value::Null;
    content["animation"]=json!({
      "opacity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}},
      "positionX":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":10},{"tick":1000000,"value":30}]}}
    });
    layer.content=serde_json::from_value(content).unwrap();
    for (id,param,kind) in [("native_opacity","opacity","slider"),("native_position","position","position")] {
        template.controls.push(TemplateControl {id:id.into(),name:id.into(),kind:kind.into(),layer:uid,param:param.into(),min:None,max:None});
    }
    let values=template.control_values_at(500000).unwrap();
    assert_eq!(values["native_opacity"],json!(50.0));
    assert_eq!(values["native_position"],json!([20.0,canvas[1] as f64/2.0]));
    assert_eq!(template.control_values_at(0).unwrap()["native_opacity"],json!(0.0));
    let edited=template.set_control_at("native_opacity",json!(64),500000).unwrap();
    assert_eq!(edited.control_values_at(500000).unwrap()["native_opacity"],json!(64.0));
    assert_eq!(template.control_values_at(500000).unwrap()["native_opacity"],json!(50.0));
    let moved=edited.set_control_at("native_position",json!([25,600]),500000).unwrap();
    assert_eq!(moved.control_values_at(500000).unwrap()["native_position"],json!([25.0,600.0]));
    let content=serde_json::to_value(&moved.graphic.layers.iter().find(|layer|layer.uid==uid).unwrap().content).unwrap();
    let track=&content["animation"]["opacity"]["track"];
    assert_eq!(track["timeBase"],1000000);
    assert_eq!(track["interpolation"],"linear");
    assert_eq!(track["keyframes"].as_array().unwrap().len(),3);
    assert_eq!(track["keyframes"][1],json!({"tick":500000,"value":64.0}));
}
