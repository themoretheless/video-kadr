//! Render original .fcgt time metadata in original ticks, independently of editor time units.
use super::{graphic_template_file::GraphicTemplateFile, graphic_templates::GraphicTemplate};
use anyhow::{ensure, Result};
use serde_json::{json, Value};

pub fn snapshot(document: &Value) -> Result<GraphicTemplate> {
    GraphicTemplateFile::read(&serde_json::to_vec(document)?)?;
    let duration = document.get("duration").cloned().unwrap_or(json!(1_270_080_000_000_i64))
        .as_i64().ok_or_else(|| anyhow::anyhow!("invalid original template duration"))?;
    ensure!(duration > 0, "template duration must be positive");
    let mut timing = document.get("graphic").cloned().unwrap_or(json!({}));
    timing.as_object_mut().ok_or_else(|| anyhow::anyhow!("invalid graphic metadata"))?.remove("roll");
    let timing: super::graphic_responsive_time::ResponsiveTime = serde_json::from_value(timing)?;
    let frame = 254_016_000_000_i128 / 30;
    let duration = i128::from(duration);
    let frames = duration / frame + i128::from((duration % frame) * 2 > frame);
    let clip_duration = frames.max(1) * frame;
    let at = (duration / 2).min(254_016_000_000);
    let local = if timing.active() {
        i128::from(timing.remap(i64::try_from(at)?, i64::try_from(clip_duration)?)?)
    } else { at };
    // Only the transient render snapshot uses a neutral clock; the caller's document is intact.
    let mut normalized = document.clone();
    normalized["duration"] = json!(508_032_000_000_i64);
    normalized["graphic"] = json!({});
    let mut template = super::graphic_templates::from_original(&normalized)?;
    template.graphic = template.graphic.snapshot_original_ticks((template.canvas[0], template.canvas[1]), local)?;
    template.roll = None;
    template.responsive_time = None;
    Ok(template)
}

pub fn png(document: &Value, width: u32) -> Result<super::graphic_template_thumbnail::Thumbnail> {
    super::graphic_template_thumbnail::png(&snapshot(document)?, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_fractional_metadata_pixels_and_input_preservation() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-template-thumbnail-tick.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 128);
        for (index, case) in cases.iter().enumerate() {
            let before = case["document"].clone();
            let template = snapshot(&before).unwrap();
            let frame = super::super::graphic_template_thumbnail::render(&template, case["width"].as_u64().unwrap() as u32, &mut || Ok(())).unwrap();
            let actual: Vec<u32> = frame.pixels().iter().flatten().map(|value| value.to_bits()).collect();
            let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert!(actual == expected, "working pixels case{index}");
            let rgba = frame.to_surface(&mut || Ok(())).unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["rgba"].clone()).unwrap();
            assert!(rgba.rgba.iter().zip(expected).all(|(a,b)|a.abs_diff(b)<=1), "rgba case{index}");
            assert_eq!(before, case["document"]);
        }
    }
}
