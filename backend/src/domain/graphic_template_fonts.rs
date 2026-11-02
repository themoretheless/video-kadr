//! Embedded .fcgt font resources; Filmcraft MIT, docs/licenses/filmcraft-MIT.txt.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontResource {
    pub family: String,
    pub style: String,
    pub license: String,
    pub file: String,
    pub data: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resources {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontResource>,
}
impl Resources {
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}
/// Embed resolved external faces, preserving resources already carried by the template.
pub fn embed(resources:&mut Resources,families:impl IntoIterator<Item=(String,String)>,license:&str) {
    register(resources);
    let mut seen=HashSet::new();
    for (family,style) in families {
        let resolved=filmcraft_text::fonts::resolve(&family,&style);
        if resolved.missing||!seen.insert(resolved.face) {continue}
        let face=filmcraft_text::fonts::face(resolved.face);
        if face.info.origin=="bundled"||resources.fonts.iter().any(|font|font.family==face.info.family&&font.style==face.info.style) {continue}
        if let Some(bytes)=face.data() {
            resources.fonts.push(FontResource {family:face.info.family.clone(),style:face.info.style.clone(),license:license.into(),file:String::new(),data:base64_encode(&bytes)});
        }
    }
}
/// Register once per font file, matching the original engine's FNV-1a content identity.
pub fn register(resources: &Resources) -> usize {
    static SEEN: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    let mut count = 0;
    for resource in &resources.fonts {
        let Some(bytes) = base64_decode(&resource.data) else {
            continue;
        };
        let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ *byte as u64).wrapping_mul(0x100_0000_01b3)
        });
        let mut seen = SEEN
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if seen.insert(hash) {
            count += filmcraft_text::fonts::add_font_data(bytes).len();
        }
    }
    count
}
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= c.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for b in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if b == b'=' {
            break;
        }
        let v = B64.iter().position(|&x| x == b)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_real_ttf_registers_new_family_once_and_remains_parseable() {
        let mut bytes =
            include_bytes!("../../vendor/filmcraft-text/assets/fonts/Inter-Regular.ttf").to_vec();
        // Rename only the SFNT name table for an isolated test family, preserving string lengths.
        let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        for record in 0..count {
            let offset = 12 + record * 16;
            if &bytes[offset..offset + 4] == b"name" {
                let start =
                    u32::from_be_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()) as usize;
                let length = u32::from_be_bytes(bytes[offset + 12..offset + 16].try_into().unwrap())
                    as usize;
                let old = [0, b'I', 0, b'n', 0, b't', 0, b'e', 0, b'r'];
                let new = [0, b'K', 0, b'a', 0, b'd', 0, b'r', 0, b'X'];
                for index in start..start + length - old.len() + 1 {
                    if bytes[index..index + old.len()] == old {
                        bytes[index..index + old.len()].copy_from_slice(&new)
                    }
                }
            }
        }
        let resources = Resources {
            fonts: vec![FontResource {
                family: "KadrX".into(),
                style: "Regular".into(),
                license: "OFL-1.1".into(),
                file: "test.ttf".into(),
                data: base64_encode(&bytes),
            }],
        };
        assert_eq!(base64_decode(&resources.fonts[0].data).unwrap(), bytes);
        assert_eq!(register(&resources), 1);
        assert_eq!(register(&resources), 0);
        let resolved = filmcraft_text::fonts::resolve("KadrX", "Regular");
        assert!(!resolved.missing);
        let face = filmcraft_text::fonts::face(resolved.face);
        assert!(face.font().is_some());
        assert_eq!(face.info.family, "KadrX");
        let mut embedded=Resources::default();
        embed(&mut embedded,[("KadrX".into(),"Regular".into()),("KadrX".into(),"Regular".into()),("Inter".into(),"Regular".into()),("Missing-export-font".into(),"Regular".into())],"OFL-test");
        assert_eq!(embedded.fonts.len(),1);
        assert_eq!(embedded.fonts[0].family,"KadrX");
        assert_eq!(embedded.fonts[0].license,"OFL-test");
        assert_eq!(base64_decode(&embedded.fonts[0].data).unwrap(),bytes);
        embed(&mut embedded,[("KadrX".into(),"Regular".into())],"Other-license");
        assert_eq!(embedded.fonts.len(),1);
        assert_eq!(embedded.fonts[0].license,"OFL-test");

        let mut template=crate::domain::graphic_templates::builtin_templates().unwrap().remove(0);
        let layer=template.graphic.layers.iter_mut().find(|layer|matches!(layer.content,crate::domain::graphic_clip::GraphicContent::Text{..})).unwrap();
        layer.original_parameters=Some(serde_json::json!({"font":{"value":{"Text":"Inter"},"keyframes":[{"time":0,"value":{"Text":"KadrX"},"interp":"Hold"},{"time":254016000000_i64,"value":{"Text":"Inter"},"interp":"Hold"}]}}).as_object().unwrap().clone());
        let original=template.clone();
        let exported=template.with_embedded_fonts("Animation-license").unwrap();
        assert_eq!(exported.resources.fonts.len(),1);
        assert_eq!(base64_decode(&exported.resources.fonts[0].data).unwrap(),bytes);
        assert_eq!(exported.resources.fonts[0].license,"Animation-license");
        assert_eq!(template,original);
        let layer=template.graphic.layers.iter_mut().find(|layer|matches!(layer.content,crate::domain::graphic_clip::GraphicContent::Text{..})).unwrap();
        layer.original_parameters=None;
        if let crate::domain::graphic_clip::GraphicContent::Text {parameters,..}=&mut layer.content {
            parameters.runs=serde_json::from_value(serde_json::json!([{"start":0,"end":1,"style":{"font":"KadrX","fontStyle":"Regular"}}])).unwrap();
        }
        let exported=template.with_embedded_fonts("Run-license").unwrap();
        assert_eq!(exported.resources.fonts.len(),1);
        assert_eq!(exported.resources.fonts[0].family,"KadrX");
        assert_eq!(base64_decode(&exported.resources.fonts[0].data).unwrap(),bytes);

    }
    #[test]
    fn malformed_resources_do_not_register_fonts() {
        assert_eq!(
            register(&Resources {
                fonts: vec![FontResource {
                    data: "@@@".into(),
                    ..Default::default()
                }]
            }),
            0
        );
        assert_eq!(register(&Resources::default()), 0);
    }
}
