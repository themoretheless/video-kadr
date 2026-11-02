//! Static Filmcraft template thumbnails: intrinsic aspect ratio, settled intro, roll at rest.
use super::{graphic_templates::GraphicTemplate,linear_color_frame::LinearColorFrame};
use anyhow::{ensure,Result};

pub fn render(template:&GraphicTemplate,width:u32,checkpoint:&mut dyn FnMut()->Result<()>) -> Result<LinearColorFrame> {
    ensure!(template.canvas[0]>0&&template.canvas[1]>0,"empty template canvas");
    let width=width.clamp(16,1920);
    let height=(width as f64*template.canvas[1] as f64/template.canvas[0] as f64).round().max(1.0) as u32;
    const TICKS: i128 = 254_016;
    let duration = i128::from(template.duration_us) * TICKS;
    let frame_ticks = 254_016_000_000 / 30;
    let whole_frames = duration / frame_ticks;
    let rounded_frames = whole_frames + i128::from((duration % frame_ticks) * 2 > frame_ticks);
    let clip_duration = rounded_frames.max(1) * frame_ticks;
    let at = (duration / 2).min(254_016_000_000);
    let local = if let Some(time) = template.responsive_time.filter(|time|time.active()) {
        let scale = |value: i64| -> Result<i64> { Ok(i64::try_from(i128::from(value)*TICKS)?) };
        let time = super::graphic_responsive_time::ResponsiveTime {
            intro: scale(time.intro)?, outro: scale(time.outro)?, design_in: scale(time.design_in)?, design_duration: scale(time.design_duration)?,
        };
        i128::from(time.remap(i64::try_from(at)?, i64::try_from(clip_duration)?)?)
    } else { at };
    super::graphic_template_fonts::register(&template.resources);
    let scale=width as f64/template.canvas[0] as f64;
    template.graphic.snapshot_original_ticks((template.canvas[0],template.canvas[1]),local)?.render_signed((template.canvas[0],template.canvas[1]),(width,height),0,[scale,0.0,0.0,scale,0.0,0.0],checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_builtin_and_animated_template_thumbnails_match_working_pixels() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-template-thumbnail.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),32);
        for (index,case) in cases.iter().enumerate() {
            let template=super::super::graphic_templates::from_original(&case["document"]).unwrap();
            let before=template.clone();
            let frame=render(&template,case["width"].as_u64().unwrap() as u32,&mut||Ok(())).unwrap();
            let actual:Vec<u32>=frame.pixels().iter().flatten().map(|value|value.to_bits()).collect();
            let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(actual,expected,"case{index}");
            let rgba=frame.to_surface(&mut||Ok(())).unwrap();
            assert_eq!(rgba.height,case["height"].as_u64().unwrap() as usize);
            let expected:Vec<u8>=serde_json::from_value(case["rgba"].clone()).unwrap();
            assert!(rgba.rgba.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=1),"rgba case{index}");
            assert_eq!(template,before);
        }
    }
    #[test]
    fn fractional_and_subframe_original_thumbnails_match_working_pixels() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-template-thumbnail-fractional.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),128);
        for (index,case) in cases.iter().enumerate() {
            let template=super::super::graphic_templates::from_original(&case["document"]).unwrap();
            let before=template.clone();
            let frame=render(&template,case["width"].as_u64().unwrap() as u32,&mut||Ok(())).unwrap();
            let actual:Vec<u32>=frame.pixels().iter().flatten().map(|value|value.to_bits()).collect();
            let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
            if actual != expected {
                let tick=case["sampleTick"].as_i64().unwrap();
                let snapshot=template.graphic.snapshot_original_ticks((template.canvas[0],template.canvas[1]),i128::from(tick)).unwrap();
                let opacity=match &snapshot.layers[0].content {super::super::graphic_clip::GraphicContent::Text{parameters,..}=>(parameters.opacity/100.).to_bits(),super::super::graphic_clip::GraphicContent::Shape{parameters,..}=>(parameters.opacity/100.).to_bits()};
                panic!("case{index}: original clock {tick}, snapshot opacity bits {opacity}, source opacity bits {}, first difference {:?}",case["opacityBits"],actual.iter().zip(&expected).position(|(a,b)|a!=b));
            }
            let rgba=frame.to_surface(&mut||Ok(())).unwrap();
            assert_eq!(rgba.height,case["height"].as_u64().unwrap() as usize);
            let expected:Vec<u8>=serde_json::from_value(case["rgba"].clone()).unwrap();
            assert!(rgba.rgba.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=1),"rgba case{index}");
            assert_eq!(template,before);
        }
    }
}

#[derive(Debug,Clone,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct Thumbnail {
    pub width:u32,
    pub height:u32,
    pub png_base64:String,
}
#[derive(Default)]
struct Cache {
    entries:std::collections::HashMap<Vec<u8>,Thumbnail>,
    bytes:usize,
}
/// Content-keyed cache, bounded by both total storage and entry count.
pub fn png(template:&GraphicTemplate,width:u32)->Result<Thumbnail> {
    use image::ImageEncoder;
    static CACHE:std::sync::OnceLock<std::sync::Mutex<Cache>>=std::sync::OnceLock::new();
    let width=width.clamp(16,1920);
    template.graphic.validate()?;
    let key=serde_json::to_vec(&(width,template))?;
    let cache=CACHE.get_or_init(Default::default);
    if let Some(hit)=cache.lock().unwrap_or_else(|error|error.into_inner()).entries.get(&key).cloned() {return Ok(hit)}
    let surface=render(template,width,&mut||Ok(()))?.to_surface(&mut||Ok(()))?;
    let mut bytes=Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(&surface.rgba,surface.width as u32,surface.height as u32,image::ExtendedColorType::Rgba8)?;
    let result=Thumbnail {width:surface.width as u32,height:surface.height as u32,png_base64:super::graphic_template_fonts::base64_encode(&bytes)};
    let size=key.len().saturating_add(result.png_base64.len());
    const LIMIT:usize=64<<20;
    if size<=LIMIT {
        let mut cache=cache.lock().unwrap_or_else(|error|error.into_inner());
        if !cache.entries.contains_key(&key) {
            if cache.entries.len()>=128||cache.bytes.saturating_add(size)>LIMIT {cache.entries.clear();cache.bytes=0;}
            cache.bytes+=size;cache.entries.insert(key,result.clone());
        }
    }
    Ok(result)
}
#[cfg(test)]
#[test]
fn thumbnail_png_preserves_rgba_and_content_cache_invalidates_on_edit() {
    let mut template=super::graphic_templates::builtin_templates().unwrap().remove(0);
    let first=png(&template,80).unwrap();
    let bytes=super::graphic_template_fonts::base64_decode(&first.png_base64).unwrap();
    let decoded=image::load_from_memory(&bytes).unwrap().to_rgba8();
    let expected=render(&template,80,&mut||Ok(())).unwrap().to_surface(&mut||Ok(())).unwrap();
    assert_eq!(decoded.as_raw(),&expected.rgba);
    assert_eq!(first.png_base64,png(&template,80).unwrap().png_base64);
    for layer in &mut template.graphic.layers {layer.enabled=false;}
    assert_ne!(first.png_base64,png(&template,80).unwrap().png_base64);
    assert_eq!(png(&template,0).unwrap().width,16);
}
