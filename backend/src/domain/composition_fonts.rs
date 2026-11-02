//! Font dependencies of the complete composition, including inactive authored content.
use super::{
    composition::{Composition, CompositionTrack, VideoEffect},
    graphic_clip::GraphicContent,
    graphic_text::GraphicTextParameters,
};
use anyhow::Result;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontUsage {
    pub family: String,
    pub style: String,
    /// Authored dependencies, not rendered glyph or frame counts.
    pub count: usize,
}
#[derive(Default)]
struct Inventory(BTreeMap<(String, String), FontUsage>);
impl Inventory {
    fn add(&mut self, family: &str, style: &str) {
        let family = if family.is_empty() { "Inter" } else { family };
        let style = if style.is_empty() { "Regular" } else { style };
        let item = self
            .0
            .entry((family.to_ascii_lowercase(), style.to_ascii_lowercase()))
            .or_insert_with(|| FontUsage {
                family: family.into(),
                style: style.into(),
                count: 0,
            });
        item.count += 1;
    }
    fn text(
        &mut self,
        text: &GraphicTextParameters,
        original: Option<&Map<String, Value>>,
    ) -> Result<()> {
        let mut pairs = BTreeSet::from([(text.font.clone(), text.font_style.clone())]);
        if let Some(original) = original {
            let mut times = BTreeSet::from([0_i128]);
            for field in ["font", "font_style"] {
                if let Some(keys) = original
                    .get(field)
                    .and_then(|p| p.get("keyframes"))
                    .and_then(Value::as_array)
                {
                    for key in keys {
                        times.insert(i128::from(
                            key["time"]
                                .as_i64()
                                .ok_or_else(|| anyhow::anyhow!("invalid font key time"))?,
                        ));
                    }
                }
            }
            for time in times {
                let mut pair = (text.font.clone(), text.font_style.clone());
                for (field, target) in [("font", &mut pair.0), ("font_style", &mut pair.1)] {
                    if let Some(parameter) = original.get(field) {
                        let value =
                            super::graphic_template_animation::sample_parameter(parameter, time)?;
                        *target = value["Text"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid font parameter"))?
                            .into();
                    }
                }
                pairs.insert(pair);
            }
        }
        for (family, style) in pairs {
            self.add(&family, &style);
            for run in &text.runs {
                if run.style.font.is_some() || run.style.font_style.is_some() {
                    self.add(
                        run.style.font.as_deref().unwrap_or(&family),
                        run.style.font_style.as_deref().unwrap_or(&style),
                    );
                }
            }
        }
        Ok(())
    }
    fn effects(&mut self, effects: &[VideoEffect]) -> Result<()> {
        for effect in effects {
            if let VideoEffect::GraphicText { parameters, .. } = effect {
                self.text(parameters, None)?;
            }
        }
        Ok(())
    }
}
/// Includes hidden/disabled clips: they still retain font dependencies when saved.
pub fn fonts_used(composition: &Composition) -> Result<Vec<FontUsage>> {
    composition.validate()?;
    let mut inventory = Inventory::default();
    for track in &composition.tracks {
        match track {
            CompositionTrack::Graphic { clips, .. } => {
                for clip in clips {
                    for layer in &clip.graphic.layers {
                        if let GraphicContent::Text { parameters, .. } = &layer.content {
                            inventory.text(parameters, layer.original_parameters.as_ref())?;
                        }
                    }
                    inventory.effects(&clip.effects)?;
                }
            }
            CompositionTrack::Video { clips, .. } => {
                for clip in clips {
                    inventory.effects(&clip.effects)?;
                }
            }
            CompositionTrack::Image { clips, .. } => {
                for clip in clips {
                    inventory.effects(&clip.effects)?;
                }
            }
            CompositionTrack::Text { clips, .. } => {
                for clip in clips {
                    if let Some(title)=&clip.concat_title {inventory.add(super::concat_project_fonts::family_name(&title.font_family),&title.face_style());}else{inventory.add(&clip.style.font_family, "Regular");}
                    inventory.effects(&clip.effects)?;
                }
            }
            CompositionTrack::Audio { .. } | CompositionTrack::Color { .. } => {}
        }
    }
    Ok(inventory.0.into_values().collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn hidden_graphic_composition() -> Composition {
        use super::super::composition::{
            AnimatableValue, CanvasSpec, CompositionClipId, TrackId, TransformSpec,
        };
        use super::super::graphic_clip::{GraphicClip, GraphicLayer};
        let canvas: CanvasSpec = serde_json::from_value(serde_json::json!({"width":80,"height":64,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut composition = Composition::new(canvas);
        let text = GraphicTextParameters::default();
        let shape = super::super::graphic_templates::builtin_templates()
            .unwrap()
            .remove(0)
            .graphic
            .layers
            .into_iter()
            .find(|layer| matches!(layer.content, GraphicContent::Shape { .. }))
            .unwrap();
        composition.tracks.push(CompositionTrack::Graphic {
            id: TrackId::new(),
            name: "Hidden graphics".into(),
            hidden: true,
            locked: true,
            clips: vec![super::super::graphic_timeline::GraphicTimelineClip {
                concat_shape: None, concat_treatment: None, concat_shape_blend: None,
            id: CompositionClipId::new(),
                timeline_start_tick: 0,
                timeline_end_tick: 1_000_000,
                animation_offset_us: 0,
                template: None,
                roll: None,
                responsive_time: None,
                width: 80,
                height: 64,
                graphic: GraphicClip {
                    layers: vec![
                        shape,
                        GraphicLayer {
                            uid: 999,
                            enabled: false,
                            original_parameters: None,
                            pin: None,
                            content: GraphicContent::Text {
                                parameters: text.clone(),
                                animation: None,
                                source_text_animation: None,
                            },
                        },
                    ],
                },
                effects: vec![VideoEffect::GraphicText {
                    parameters: text,
                    animation: None,
                    source_text_animation: None,
                }],
                transform: TransformSpec::default(),
                opacity: AnimatableValue::constant(1.0),
                enabled: false,
            }],
        });
        composition
    }
    #[test]
    fn hidden_graphics_and_text_effects_count_fonts_but_shapes_do_not() {
        let composition = hidden_graphic_composition();
        let original = composition.clone();
        assert_eq!(
            fonts_used(&composition).unwrap(),
            vec![FontUsage {
                family: "Inter".into(),
                style: "Regular".into(),
                count: 2
            }]
        );
        assert_eq!(composition, original);
    }
    #[test]
    fn animation_run_dependencies_and_alias_counts_are_retained() {
        let mut text = GraphicTextParameters::default();
        text.runs = serde_json::from_value(serde_json::json!([
            {"start":0,"end":1,"style":{"font":"inter","fontStyle":"regular"}},
            {"start":1,"end":2,"style":{"fontStyle":"Bold"}},
            {"start":2,"end":3,"style":{"size":20}}
        ]))
        .unwrap();
        let original = serde_json::json!({"font":{"value":{"Text":"Inter"},"keyframes":[
            {"time":0,"value":{"Text":"External"},"interp":"Hold"},
            {"time":254016,"value":{"Text":"Inter"},"interp":"Hold"}]}});
        let mut inventory = Inventory::default();
        inventory.text(&text, original.as_object()).unwrap();
        let usages: Vec<_> = inventory.0.into_values().collect();
        assert_eq!(
            usages
                .iter()
                .find(|u| u.family == "External" && u.style == "Regular")
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            usages
                .iter()
                .find(|u| u.family == "External" && u.style == "Bold")
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            usages
                .iter()
                .find(|u| u.family.eq_ignore_ascii_case("Inter")
                    && u.style.eq_ignore_ascii_case("Regular"))
                .unwrap()
                .count,
            3
        );
        assert_eq!(usages.len(), 4);
    }
}
