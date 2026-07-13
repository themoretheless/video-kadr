use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: i64,
}

impl Rational {
    pub fn parse(value: &str) -> Option<Self> {
        let (numerator, denominator) = value.split_once('/')?;
        let numerator = numerator.parse().ok()?;
        let denominator = denominator.parse().ok()?;
        (denominator != 0).then_some(Self {
            numerator,
            denominator,
        })
    }

    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorMetadata {
    pub range: Option<String>,
    pub space: Option<String>,
    pub transfer: Option<String>,
    pub primaries: Option<String>,
    pub pixel_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamMetadata {
    pub index: u32,
    pub kind: StreamKind,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub profile: Option<String>,
    pub time_base: Option<Rational>,
    pub frame_rate: Option<Rational>,
    pub start_time_seconds: Option<f64>,
    pub duration_seconds: Option<f64>,
    pub frames: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub rotation_degrees: i32,
    pub color: ColorMetadata,
    pub dispositions: BTreeSet<String>,
    pub tags: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerMetadata {
    pub format_names: Vec<String>,
    pub format_long_name: Option<String>,
    pub start_time_seconds: Option<f64>,
    pub duration_seconds: f64,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
    pub tags: BTreeMap<String, String>,
}

/// Canonical metadata shared by every probing adapter. The top-level summary
/// fields keep call sites simple while `container` and `streams` preserve the
/// time-base, color, rotation, and disposition semantics needed by editors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeResult {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub format_name: Option<String>,
    pub container: ContainerMetadata,
    pub streams: Vec<StreamMetadata>,
}

impl ProbeResult {
    pub fn from_ffprobe_json(root: &Value) -> Result<Self, ProbeError> {
        if !root.is_object() {
            return Err(ProbeError::InvalidRoot);
        }
        let format = root.get("format").unwrap_or(&Value::Null);
        let format_name = string(format, "format_name");
        let format_names = format_name
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        let mut duration = number(format.get("duration")).unwrap_or(0.0).max(0.0);
        let container = ContainerMetadata {
            format_names,
            format_long_name: string(format, "format_long_name"),
            start_time_seconds: number(format.get("start_time")),
            duration_seconds: duration,
            size_bytes: unsigned(format.get("size")),
            bit_rate: unsigned(format.get("bit_rate")),
            tags: tags(format.get("tags")),
        };

        let streams: Vec<_> = root
            .get("streams")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(normalize_stream)
            .collect();
        if streams.is_empty() {
            return Err(ProbeError::NoStreams);
        }

        if duration == 0.0 {
            duration = streams
                .iter()
                .filter_map(|stream| stream.duration_seconds)
                .fold(0.0, f64::max);
        }
        let primary_video = streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Video);
        let primary_audio = streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Audio);
        let mut container = container;
        container.duration_seconds = duration;
        Ok(Self {
            duration,
            width: primary_video.and_then(|stream| stream.width).unwrap_or(0),
            height: primary_video.and_then(|stream| stream.height).unwrap_or(0),
            fps: primary_video
                .and_then(|stream| stream.frame_rate)
                .map(Rational::as_f64)
                .filter(|value| value.is_finite() && *value > 0.0),
            vcodec: primary_video.and_then(|stream| stream.codec_name.clone()),
            acodec: primary_audio.and_then(|stream| stream.codec_name.clone()),
            format_name,
            container,
            streams,
        })
    }
}

fn normalize_stream(stream: &Value) -> StreamMetadata {
    let time_base = stream
        .get("time_base")
        .and_then(Value::as_str)
        .and_then(Rational::parse);
    let duration_seconds = number(stream.get("duration")).or_else(|| {
        let ticks = number(stream.get("duration_ts"))?;
        Some(ticks * time_base?.as_f64())
    });
    let frame_rate = ["avg_frame_rate", "r_frame_rate"]
        .into_iter()
        .filter_map(|key| stream.get(key).and_then(Value::as_str))
        .filter_map(Rational::parse)
        .find(|rate| rate.numerator > 0 && rate.denominator > 0);
    StreamMetadata {
        index: unsigned32(stream.get("index")).unwrap_or(0),
        kind: match stream.get("codec_type").and_then(Value::as_str) {
            Some("video") => StreamKind::Video,
            Some("audio") => StreamKind::Audio,
            Some("subtitle") => StreamKind::Subtitle,
            Some("data") => StreamKind::Data,
            Some("attachment") => StreamKind::Attachment,
            _ => StreamKind::Unknown,
        },
        codec_name: string(stream, "codec_name"),
        codec_long_name: string(stream, "codec_long_name"),
        profile: string(stream, "profile"),
        time_base,
        frame_rate,
        start_time_seconds: number(stream.get("start_time")),
        duration_seconds,
        frames: unsigned(stream.get("nb_frames")),
        width: unsigned32(stream.get("width")),
        height: unsigned32(stream.get("height")),
        sample_rate: unsigned32(stream.get("sample_rate")),
        channels: unsigned32(stream.get("channels")),
        channel_layout: string(stream, "channel_layout"),
        rotation_degrees: rotation(stream),
        color: ColorMetadata {
            range: string(stream, "color_range"),
            space: string(stream, "color_space"),
            transfer: string(stream, "color_transfer"),
            primaries: string(stream, "color_primaries"),
            pixel_format: string(stream, "pix_fmt"),
        },
        dispositions: dispositions(stream.get("disposition")),
        tags: tags(stream.get("tags")),
    }
}

fn rotation(stream: &Value) -> i32 {
    let from_side_data = stream
        .get("side_data_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|item| number(item.get("rotation")));
    let from_tags = stream
        .get("tags")
        .and_then(|tags| number(tags.get("rotate")));
    (from_side_data.or(from_tags).unwrap_or(0.0).round() as i32).rem_euclid(360)
}

fn dispositions(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| (unsigned(Some(value)) == Some(1)).then_some(key.clone()))
        .collect()
}

fn tags(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .collect()
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    let parsed = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))?;
    parsed.is_finite().then_some(parsed)
}

fn unsigned(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn unsigned32(value: Option<&Value>) -> Option<u32> {
    unsigned(value).and_then(|value| u32::try_from(value).ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    InvalidRoot,
    NoStreams,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid media probe result: {self:?}")
    }
}

impl std::error::Error for ProbeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ffprobe_fixture_without_losing_semantics() {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../fixtures/media-probe/ffprobe-video.json"
        ))
        .unwrap();
        let probe = ProbeResult::from_ffprobe_json(&raw).unwrap();

        assert_eq!(probe.container.format_names, ["mov", "mp4"]);
        assert_eq!(probe.duration, 12.5);
        assert_eq!((probe.width, probe.height), (1920, 1080));
        assert!((probe.fps.unwrap() - 29.970_029_97).abs() < 1e-8);
        assert_eq!(probe.vcodec.as_deref(), Some("h264"));
        assert_eq!(probe.acodec.as_deref(), Some("aac"));
        assert_eq!(probe.streams[0].time_base.unwrap().as_f64(), 1.0 / 90_000.0);
        assert_eq!(probe.streams[0].rotation_degrees, 270);
        assert_eq!(probe.streams[0].color.primaries.as_deref(), Some("bt709"));
        assert!(probe.streams[0].dispositions.contains("default"));
    }

    #[test]
    fn derives_duration_from_stream_ticks_when_container_omits_it() {
        let raw = serde_json::json!({
            "streams": [{
                "index": 0,
                "codec_type": "audio",
                "codec_name": "opus",
                "time_base": "1/48000",
                "duration_ts": 96000
            }]
        });
        let probe = ProbeResult::from_ffprobe_json(&raw).unwrap();
        assert_eq!(probe.duration, 2.0);
    }
}
