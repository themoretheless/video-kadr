//! Resolve requested families after registering font resources carried by the document.
use super::{
    composition::{Composition, CompositionTrack},
    composition_fonts::{self, FontUsage},
    nested_composition::NestedCompositionProject,
};
use anyhow::Result;
use serde::Serialize;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontStatus {
    #[serde(flatten)]
    pub usage: FontUsage,
    pub missing: bool,
}
fn register_resources(composition: &Composition) {
    for track in &composition.tracks {
        if let CompositionTrack::Graphic { clips, .. } = track {
            for clip in clips {
                if let Some(template) = &clip.template {
                    super::graphic_template_fonts::register(&template.resources);
                }
            }
        }
    }
}
fn resolve(usages: Vec<FontUsage>) -> Vec<FontStatus> {
    usages
        .into_iter()
        .map(|usage| {
            let missing = filmcraft_text::fonts::resolve(&usage.family, &usage.style).missing;
            FontStatus { usage, missing }
        })
        .collect()
}
pub fn composition_fonts(composition: &Composition) -> Result<Vec<FontStatus>> {
    let usages = composition_fonts::fonts_used(composition)?;
    register_resources(composition);
    Ok(resolve(usages))
}
pub fn project_fonts(project: &NestedCompositionProject) -> Result<Vec<FontStatus>> {
    let usages = super::project_fonts::fonts_used(project)?;
    // All resources precede all resolutions: a shared family may be embedded in another asset.
    for document in project.compositions.values() {
        register_resources(&document.content);
    }
    let mut rich=std::collections::BTreeMap::<(String,String),usize>::new();
    for key in project.compositions.values().flat_map(|document|document.content.tracks.iter()).filter_map(|track|if let CompositionTrack::Text{clips,..}=track{Some(clips)}else{None}).flatten().filter_map(|clip|clip.concat_title.as_ref()).map(|title|(super::concat_project_fonts::family_name(&title.font_family).to_lowercase(),title.face_style().to_lowercase())){*rich.entry(key).or_default()+=1;}
    let names=if rich.is_empty(){vec![]}else{super::concat_project_fonts::bank(&project.fonts)?.map(|fonts|fonts.families()).unwrap_or_else(||super::concat_title::fonts().families())};
    Ok(resolve(usages).into_iter().map(|mut status|{if let Some(count)=rich.get(&(status.usage.family.to_lowercase(),status.usage.style.to_lowercase())){let source_missing=!names.iter().any(|family|family.eq_ignore_ascii_case(&status.usage.family));status.missing=source_missing||(status.usage.count>*count&&status.missing);}status}).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_template_face_is_registered_before_missing_status_is_resolved() {
        let mut bytes =
            include_bytes!("../../vendor/filmcraft-text/assets/fonts/Inter-Regular.ttf").to_vec();
        let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        for record in 0..count {
            let offset = 12 + record * 16;
            if &bytes[offset..offset + 4] == b"name" {
                let start =
                    u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
                let length = u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().unwrap())
                    as usize;
                let old = [0, b'I', 0, b'n', 0, b't', 0, b'e', 0, b'r'];
                let new = [0, b'K', 0, b'd', 0, b'r', 0, b'Y', 0, b'9'];
                for index in start..start + length - old.len() + 1 {
                    if bytes[index..index + old.len()] == old {
                        bytes[index..index + old.len()].copy_from_slice(&new);
                    }
                }
            }
        }
        assert!(filmcraft_text::fonts::resolve("KdrY9", "Regular").missing);
        let mut template = super::super::graphic_templates::builtin_templates()
            .unwrap()
            .remove(0);
        template.controls.clear();
        template.resources.fonts = vec![super::super::graphic_template_fonts::FontResource {
            family: "KdrY9".into(),
            style: "Regular".into(),
            license: "test".into(),
            file: String::new(),
            data: super::super::graphic_template_fonts::base64_encode(&bytes),
        }];
        let mut composition = super::super::composition_fonts::tests::hidden_graphic_composition();
        if let CompositionTrack::Graphic { clips, .. } = &mut composition.tracks[0] {
            clips[0].template = Some((&template).into());
            if let super::super::graphic_clip::GraphicContent::Text { parameters, .. } =
                &mut clips[0].graphic.layers[1].content
            {
                parameters.font = "KdrY9".into();
            }
        }
        let fonts = composition_fonts(&composition).unwrap();
        assert!(
            !fonts
                .iter()
                .find(|font| font.usage.family == "KdrY9")
                .unwrap()
                .missing
        );
    }
    #[test]
    fn missing_requested_family_is_reported_without_rewriting_it_to_fallback() {
        let mut composition = super::super::composition_fonts::tests::hidden_graphic_composition();
        if let CompositionTrack::Graphic { clips, .. } = &mut composition.tracks[0] {
            if let super::super::graphic_clip::GraphicContent::Text { parameters, .. } =
                &mut clips[0].graphic.layers[1].content
            {
                parameters.font = "Missing-Kadr-Status-1502".into();
            }
        }
        let before = composition.clone();
        let fonts = composition_fonts(&composition).unwrap();
        assert!(
            !fonts
                .iter()
                .find(|font| font.usage.family == "Inter")
                .unwrap()
                .missing
        );
        assert!(
            fonts
                .iter()
                .find(|font| font.usage.family == "Missing-Kadr-Status-1502")
                .unwrap()
                .missing
        );
        assert_eq!(composition, before);
    }
}
