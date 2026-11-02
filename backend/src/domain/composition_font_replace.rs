//! Atomic font substitution; preserves authored animation clocks and key metadata.
use super::{
    composition::{Composition, CompositionTrack, VideoEffect},
    graphic_clip::GraphicContent,
    graphic_text::GraphicTextParameters,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FontSelection {
    Family(String),
    Face(FontFace),
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    pub family: String,
    pub style: Option<String>,
}
impl FontSelection {
    fn family(&self) -> &str {
        match self {
            Self::Family(f) => f,
            Self::Face(f) => &f.family,
        }
    }
    fn style(&self) -> Option<&str> {
        match self {
            Self::Family(_) => None,
            Self::Face(f) => f.style.as_deref(),
        }
    }
    fn matches(&self, family: &str, style: &str) -> bool {
        self.family().eq_ignore_ascii_case(family)
            && self.style().is_none_or(|s| s.eq_ignore_ascii_case(style))
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub composition: Composition,
    pub replaced: usize,
}
struct Rule<'a> {
    from: &'a FontSelection,
    to: &'a FontSelection,
}
impl Rule<'_> {
    fn pair(&self, family: &str, style: &str) -> (String, String) {
        if self.from.matches(family, style) {
            (
                self.to.family().into(),
                self.to.style().unwrap_or(style).into(),
            )
        } else {
            (family.into(), style.into())
        }
    }
    fn text(
        &self,
        text: &mut GraphicTextParameters,
        original: Option<&mut Map<String, Value>>,
    ) -> Result<usize> {
        let old = (text.font.clone(), text.font_style.clone());
        let new = self.pair(&old.0, &old.1);
        let mut changed = new != old;
        if let Some(original) = original {
            let mut times = BTreeSet::new();
            for field in ["font", "font_style"] {
                if let Some(keys) = original
                    .get(field)
                    .and_then(|p| p.get("keyframes"))
                    .and_then(Value::as_array)
                {
                    for key in keys {
                        times.insert(
                            key["time"]
                                .as_i64()
                                .ok_or_else(|| anyhow::anyhow!("invalid font key time"))?,
                        );
                    }
                }
            }
            let mut samples = Vec::new();
            for time in times {
                let mut pair = old.clone();
                for (field, target) in [("font", &mut pair.0), ("font_style", &mut pair.1)] {
                    if let Some(param) = original.get(field) {
                        let value = super::graphic_template_animation::sample_parameter(
                            param,
                            i128::from(time),
                        )?;
                        *target = value["Text"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("invalid font parameter"))?
                            .into();
                    }
                }
                let mapped = self.pair(&pair.0, &pair.1);
                changed |= mapped != pair;
                samples.push((time, mapped));
            }
            if changed {
                for (field, base, index) in [("font", &new.0, 0), ("font_style", &new.1, 1)] {
                    let previous = original
                        .get(field)
                        .cloned()
                        .unwrap_or_else(|| json!({"value":{"Text":base}}));
                    let mut parameter = previous.clone();
                    parameter["value"] = json!({"Text":base});
                    if !samples.is_empty() {
                        let keys:Vec<_>=samples.iter().map(|(time,pair)| {
       let mut key=previous.get("keyframes").and_then(Value::as_array).and_then(|keys|keys.iter().find(|k|k["time"].as_i64()==Some(*time))).cloned()
        .unwrap_or_else(||json!({"time":time,"interp":"Hold","out_influence":1.0/3.0,"in_influence":1.0/3.0}));
       key["value"]=json!({"Text":if index==0 {&pair.0}else{&pair.1}});key
      }).collect();
                        parameter["keyframes"] = json!(keys);
                    }
                    original.insert(field.into(), parameter);
                }
            }
        }
        text.font = new.0;
        text.font_style = new.1;
        let mut count = usize::from(changed);
        for run in &mut text.runs {
            if let Some(family) = &run.style.font {
                let style = run.style.font_style.as_deref().unwrap_or(&old.1);
                let mapped = self.pair(family, style);
                if mapped != (family.clone(), style.into()) {
                    run.style.font = Some(mapped.0);
                    if self.to.style().is_some() {
                        run.style.font_style = Some(mapped.1);
                    }
                    count += 1;
                }
            }
        }
        Ok(count)
    }
    fn plain_text_family(&self, family: &mut String) -> Result<usize> {
        if !self.from.matches(family, "Regular") {
            return Ok(0);
        }
        ensure!(
            self.to
                .style()
                .is_none_or(|s| s.eq_ignore_ascii_case("Regular")),
            "ordinary text clips support Regular style only"
        );
        if family == self.to.family() {
            return Ok(0);
        }
        *family = self.to.family().into();
        Ok(1)
    }
    fn effects(&self, effects: &mut [VideoEffect]) -> Result<usize> {
        let mut count = 0;
        for effect in effects {
            if let VideoEffect::GraphicText { parameters, .. } = effect {
                count += self.text(parameters, None)?;
            }
        }
        Ok(count)
    }
}
pub fn replace_fonts(
    composition: &Composition,
    from: &FontSelection,
    to: &FontSelection,
) -> Result<Replacement> {
    ensure!(
        !from.family().trim().is_empty() && !to.family().trim().is_empty(),
        "font families cannot be empty"
    );
    composition.validate()?;
    let rule = Rule { from, to };
    let mut result = composition.clone();
    let mut replaced = 0;
    for track in &mut result.tracks {
        let mut count = 0;
        let locked = match track {
            CompositionTrack::Graphic { clips, locked, .. } => {
                for clip in clips {
                    for layer in &mut clip.graphic.layers {
                        if let GraphicContent::Text { parameters, .. } = &mut layer.content {
                            count += rule.text(parameters, layer.original_parameters.as_mut())?;
                        }
                    }
                    count += rule.effects(&mut clip.effects)?;
                }
                *locked
            }
            CompositionTrack::Video { clips, locked, .. } => {
                for clip in clips {
                    count += rule.effects(&mut clip.effects)?;
                }
                *locked
            }
            CompositionTrack::Image { clips, locked, .. } => {
                for clip in clips {
                    count += rule.effects(&mut clip.effects)?;
                }
                *locked
            }
            CompositionTrack::Text { clips, locked, .. } => {
                for clip in clips {
                    if let Some(title)=&mut clip.concat_title {
                        let before=title.clone();
                        if rule.from.matches(super::concat_project_fonts::family_name(&title.font_family),&title.face_style()){
                            title.font_family=rule.to.family().into();
                            if let Some(style)=rule.to.style(){title.set_face_style(style)?;}
                        }
                        count+=usize::from(*title!=before);
                    }else{count += rule.plain_text_family(&mut clip.style.font_family)?;}
                    count+=rule.effects(&mut clip.effects)?;
                }
                *locked
            }
            CompositionTrack::Audio { locked, .. } | CompositionTrack::Color { locked, .. } => {
                *locked
            }
        };
        ensure!(
            !locked || count == 0,
            "font replacement would change a locked track"
        );
        replaced += count;
    }
    result.validate()?;
    Ok(Replacement {
        composition: result,
        replaced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_text_refuses_same_family_unsupported_style() {
        let from = FontSelection::Family("Arial".into());
        let to = FontSelection::Face(FontFace {
            family: "Arial".into(),
            style: Some("Bold".into()),
        });
        let mut family = "Arial".to_string();
        assert!(Rule {
            from: &from,
            to: &to
        }
        .plain_text_family(&mut family)
        .is_err());
        assert_eq!(family, "Arial");
    }
    #[test]
    fn replacement_is_atomic_on_locked_tracks_and_preserves_input() {
        let mut composition = super::super::composition_fonts::tests::hidden_graphic_composition();
        let original = composition.clone();
        let from = FontSelection::Family("INTER".into());
        let to = FontSelection::Family("External".into());
        assert!(replace_fonts(&composition, &from, &to)
            .unwrap_err()
            .to_string()
            .contains("locked"));
        assert_eq!(composition, original);
        if let CompositionTrack::Graphic { locked, .. } = &mut composition.tracks[0] {
            *locked = false;
        }
        let before = composition.clone();
        let result = replace_fonts(&composition, &from, &to).unwrap();
        assert_eq!(result.replaced, 2);
        assert_eq!(composition, before);
        let fonts = super::super::composition_fonts::fonts_used(&result.composition).unwrap();
        assert_eq!(fonts.len(), 1);
        assert_eq!(fonts[0].family, "External");
        assert_eq!(fonts[0].count, 2);
    }
    #[test]
    fn replacing_animated_faces_keeps_other_families_and_handles() {
        let from = FontSelection::Family("inter".into());
        let to = FontSelection::Face(FontFace {
            family: "Replacement".into(),
            style: Some("Bold".into()),
        });
        let mut text = GraphicTextParameters::default();
        let mut original = json!({"font":{"value":{"Text":"Inter"},"keyframes":[
   {"time":-10,"value":{"Text":"Inter"},"interp":"Linear","out_influence":0.7},
   {"time":10,"value":{"Text":"Other"},"interp":"Hold","in_influence":0.2}]}})
        .as_object()
        .unwrap()
        .clone();
        assert_eq!(
            Rule {
                from: &from,
                to: &to
            }
            .text(&mut text, Some(&mut original))
            .unwrap(),
            1
        );
        assert_eq!(original["font"]["keyframes"][0]["out_influence"], 0.7);
        assert_eq!(
            original["font"]["keyframes"][1]["value"],
            json!({"Text":"Other"})
        );
        assert_eq!(
            original["font_style"]["keyframes"][1]["value"],
            json!({"Text":"Regular"})
        );
        assert_eq!(text.font, "Replacement");
        assert_eq!(text.font_style, "Bold");
    }
}
