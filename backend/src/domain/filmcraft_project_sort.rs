//! Sort Icons keys from Filmcraft project_panel::cell (MIT, pinned 52318524).
use super::{
    filmcraft_project_index::ProjectIndex, filmcraft_project_inspect, filmcraft_source_inspect,
};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND};
use serde_json::{json, Value};

pub(crate) fn keys(index: &ProjectIndex<'_>, id: u64, item: &Value) -> Value {
    let subclip = item.pointer("/kind/Subclip");
    let parent = subclip.and_then(|value| value["parent"].as_u64());
    let media = item
        .pointer("/kind/Media")
        .or_else(|| index.item(parent?)?.pointer("/kind/Media"));
    let sequence = item.pointer("/kind/Sequence");
    let info = media.map_or(&Value::Null, |media| &media["info"]);
    let rate = if subclip.is_some() {
        parent
            .and_then(|id| index.item(id).map(|_| id))
            .and_then(|id| {
                if index.item(id)?.pointer("/kind/Subclip").is_some() {
                    Some(FrameRate::default())
                } else {
                    filmcraft_source_inspect::frame_rate(index, id).ok()
                }
            })
            .unwrap_or_default()
    } else {
        filmcraft_source_inspect::frame_rate(index, id).unwrap_or_default()
    }
    .sane();
    let frame_numerator = i128::from(TICKS_PER_SECOND) * i128::from(rate.den);
    let duration = filmcraft_project_inspect::duration(item).unwrap_or_default();
    let start = info["start_timecode"]
        .as_i64()
        .and_then(|frame| {
            i128::from(frame)
                .checked_mul(frame_numerator)
                .map(|ticks| ticks.div_euclid(i128::from(rate.num)))
        })
        .unwrap_or_default();
    let first = start.saturating_add(i128::from(
        subclip
            .and_then(|clip| clip["range"]["start"].as_i64())
            .unwrap_or_default(),
    ));
    let end = if duration > 0 {
        first
            .saturating_add(i128::from(duration))
            .saturating_sub(frame_numerator / i128::from(rate.num))
    } else {
        first
    };
    let video = &info["video"];
    let has_video = media.is_none() || subclip.is_some() || !video.is_null();
    let video_info = if let (Some(w), Some(h)) = (video["width"].as_u64(), video["height"].as_u64())
    {
        let par = video["par"][0].as_u64().unwrap_or(1) as f64
            / video["par"][1].as_u64().unwrap_or(1).max(1) as f64;
        format!("{w} x {h} ({par:.4})")
    } else if let Some(sequence) = sequence {
        format!(
            "{} x {} (1.0)",
            sequence["settings"]["width"], sequence["settings"]["height"]
        )
    } else {
        String::new()
    };
    let audio = &info["audio"];
    let audio_info = match (audio["sample_rate"].as_u64(), audio["channels"].as_u64()) {
        (Some(rate), Some(1)) => format!("{rate} Hz - Mono"),
        (Some(rate), Some(2)) => format!("{rate} Hz - Stereo"),
        (Some(rate), Some(channels)) => format!("{rate} Hz - {channels} ch"),
        _ => String::new(),
    };
    json!({
        "mediaType":filmcraft_project_inspect::type_label(item).unwrap_or_default(),
        "frameRate":if has_video { rate.as_f64() } else { 0.0 },
        "mediaStart":first as f64,"mediaEnd":end as f64,"mediaDuration":duration as f64,
        "videoInfo":video_info,"audioInfo":audio_info,"dateCreated":item["created"].as_f64().unwrap_or_default()
    })
}
