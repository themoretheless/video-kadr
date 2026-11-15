//! Project panel sort keys and list timecodes from Filmcraft (MIT, pinned 52318524).
use super::{
    filmcraft_project_index::ProjectIndex, filmcraft_project_inspect, filmcraft_source_inspect,
};
use filmcraft_color::{ColorInfo, ColorSpace, WorkingSpace};
use filmcraft_time::{format_time, FrameRate, Tick, TimeDisplay, TICKS_PER_SECOND};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub(crate) fn usage(index: &ProjectIndex<'_>) -> BTreeMap<u64, [u64; 2]> {
    let mut counts = BTreeMap::<u64, [u64; 2]>::new();
    for (_, sequence) in index.sequences() {
        for (channel, tracks) in ["video_tracks", "audio_tracks"].into_iter().enumerate() {
            for track in sequence[tracks].as_array().into_iter().flatten() {
                for clip in track["items"].as_array().into_iter().flatten() {
                    if let Some(id) = clip["item"].as_u64() {
                        counts.entry(id).or_default()[channel] += 1;
                    }
                }
            }
        }
    }
    counts
}

pub(crate) fn keys(index: &ProjectIndex<'_>, id: u64, item: &Value, usage: [u64; 2]) -> Value {
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
    let has_audio = item.pointer("/kind/Media").is_some() && !audio.is_null()
        || sequence.is_some_and(|sequence| {
            sequence["audio_tracks"]
                .as_array()
                .is_some_and(|tracks| !tracks.is_empty())
        });
    let audio_info = match (audio["sample_rate"].as_u64(), audio["channels"].as_u64()) {
        (Some(rate), Some(1)) => format!("{rate} Hz - Mono"),
        (Some(rate), Some(2)) => format!("{rate} Hz - Stereo"),
        (Some(rate), Some(channels)) => format!("{rate} Hz - {channels} ch"),
        _ => String::new(),
    };
    let color_space = if !video.is_null() {
        media
            .and_then(|media| {
                serde_json::from_value::<ColorSpace>(media["interpret"]["color_space"].clone()).ok()
            })
            .or_else(|| {
                serde_json::from_value::<ColorInfo>(video["color"].clone())
                    .ok()
                    .map(|color| ColorSpace::from_info(&color))
            })
            .map_or("", ColorSpace::label)
    } else {
        sequence
            .map(|sequence| {
                serde_json::from_value::<WorkingSpace>(
                    sequence["settings"]["color"]["working"].clone(),
                )
                .unwrap_or_default()
            })
            .map_or("", WorkingSpace::label)
    };
    let metadata: Map<_, _> = item["metadata"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, value)| {
            value.is_string()
                && !matches!(
                    key.as_str(),
                    "Freeform Position" | "Freeform Stack" | "Poster Frame"
                )
                && !key.starts_with("Freeform Arrangement: ")
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut keys = json!({
        "mediaType":filmcraft_project_inspect::type_label(item).unwrap_or_default(),
        "frameRate":if has_video { rate.as_f64() } else { 0.0 },
        "mediaStart":first as f64,"mediaEnd":end as f64,"mediaDuration":duration as f64,
        "videoInfo":video_info,"audioInfo":audio_info,"dateCreated":item["created"].as_f64().unwrap_or_default(),
        "videoCodec":video["codec"].as_str().unwrap_or_default(),
        "audioCodec":audio["codec"].as_str().unwrap_or_default(),
        "status":media.map_or("", |m| if m["offline"] == true { "Offline" } else { "Online" }),
        "proxy":media.map_or("", |m| if m["proxy"].is_null() { "None" } else { "Attached" }),
        "colorSpace":color_space,
        "videoUsage":usage[0],
        "audioUsage":usage[1],
        "filePath":media.and_then(|m| m.pointer("/media/File/path")).and_then(Value::as_str).unwrap_or_default(),
        "fileSize":info["file_size"].as_u64(),
        "metadata":metadata
    });
    let fd = frame_numerator / i128::from(rate.num);
    let duration = i128::from(duration);
    let marked_item = item
        .pointer("/kind/Media")
        .or(sequence)
        .unwrap_or(&Value::Null);
    let mark_in = i128::from(marked_item["mark_in"].as_i64().unwrap_or_default());
    let mark_out = marked_item["mark_out"]
        .as_i64()
        .map_or(duration, |tick| i128::from(tick) + fd)
        .min(duration.max(fd));
    let mut timecodes = Map::new();
    for (column, ticks) in [
        ("mediaStart", Some(first)),
        ("mediaEnd", Some(end)),
        ("mediaDuration", Some(duration)),
        ("videoIn", has_video.then_some(first + mark_in)),
        ("videoOut", has_video.then_some(first + mark_out - fd)),
        ("videoDuration", has_video.then_some(mark_out - mark_in)),
        ("audioIn", has_audio.then_some(first + mark_in)),
        ("audioOut", has_audio.then_some(first + mark_out - fd)),
        ("audioDuration", has_audio.then_some(mark_out - mark_in)),
        ("subclipStart", subclip.map(|_| first)),
        ("subclipEnd", subclip.map(|_| first + duration - fd)),
    ] {
        let Some(ticks) = ticks else { continue };
        let tick = Tick(ticks.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64);
        timecodes.insert(
            column.into(),
            json!(format_time(
                tick,
                rate,
                false,
                TimeDisplay::Timecode,
                48_000
            )),
        );
        keys[column] = json!(ticks as f64);
    }
    keys["timecodes"] = Value::Object(timecodes);
    keys
}
