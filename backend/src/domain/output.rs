//! Final output semantics shared by planning and encoder adapters.

use serde::{Deserialize, Serialize};

use crate::model::EditRequest;

use super::artifact_graph::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Mp4,
    Webm,
    Gif,
    Png,
    Jpg,
    Mp3,
    Av1,
    Prores,
}

impl OutputFormat {
    pub fn from_request(value: Option<&str>) -> Self {
        match value.unwrap_or("mp4") {
            "webm" => Self::Webm,
            "gif" => Self::Gif,
            "png" => Self::Png,
            "jpg" => Self::Jpg,
            "mp3" => Self::Mp3,
            "av1" => Self::Av1,
            "prores" => Self::Prores,
            _ => Self::Mp4,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Webm => "webm",
            Self::Gif => "gif",
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Mp3 => "mp3",
            Self::Prores => "mov",
            Self::Mp4 | Self::Av1 => "mp4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
    Prores,
}

impl VideoCodec {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::H264 => "libx264",
            Self::H265 => "libx265",
            Self::Vp9 => "libvpx-vp9",
            Self::Av1 => "libsvtav1",
            Self::Prores => "prores_ks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodec {
    Aac,
    Opus,
    PcmS16Le,
    Mp3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputSpec {
    pub format: OutputFormat,
    pub video_codec: Option<VideoCodec>,
    pub audio_codec: Option<AudioCodec>,
    pub crf: Option<u32>,
    /// Milliframes per second avoids floats in artifact identity.
    pub fps_milli: Option<u32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

impl OutputSpec {
    pub fn from_edit(edit: &EditRequest) -> Self {
        let format = OutputFormat::from_request(edit.format.as_deref());
        let video_codec = match format {
            OutputFormat::Mp4 => Some(if edit.codec.as_deref() == Some("h265") {
                VideoCodec::H265
            } else {
                VideoCodec::H264
            }),
            OutputFormat::Webm => Some(VideoCodec::Vp9),
            OutputFormat::Av1 => Some(VideoCodec::Av1),
            OutputFormat::Prores => Some(VideoCodec::Prores),
            OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg | OutputFormat::Mp3 => None,
        };
        let audio_codec = if edit.mute
            || matches!(
                format,
                OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg
            ) {
            None
        } else {
            match format {
                OutputFormat::Webm => Some(AudioCodec::Opus),
                OutputFormat::Prores => Some(AudioCodec::PcmS16Le),
                OutputFormat::Mp3 => Some(AudioCodec::Mp3),
                _ => Some(AudioCodec::Aac),
            }
        };
        let crf = video_codec.and_then(|codec| match codec {
            VideoCodec::H264 => Some(edit.quality.unwrap_or(23)),
            VideoCodec::H265 => Some(edit.quality.unwrap_or(28)),
            VideoCodec::Vp9 | VideoCodec::Av1 => Some(edit.quality.unwrap_or(32)),
            VideoCodec::Prores => None,
        });
        Self {
            format,
            video_codec,
            audio_codec,
            crf,
            fps_milli: edit
                .fps
                .filter(|fps| fps.is_finite() && *fps > 0.0)
                .map(|fps| (fps * 1000.0).round().clamp(1.0, u32::MAX as f64) as u32),
            width: edit.scale.as_ref().map(|scale| scale.w),
            height: edit.scale.as_ref().map(|scale| scale.h),
        }
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::digest(
            &serde_json::to_vec(self).expect("OutputSpec serialization cannot fail"),
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn spec(value: serde_json::Value) -> OutputSpec {
        OutputSpec::from_edit(&serde_json::from_value(value).unwrap())
    }

    #[test]
    fn quality_defaults_are_codec_specific() {
        assert_eq!(spec(json!({"videoId": "x"})).crf, Some(23));
        assert_eq!(spec(json!({"videoId": "x", "codec": "h265"})).crf, Some(28));
        assert_eq!(
            spec(json!({"videoId": "x", "format": "webm"})).crf,
            Some(32)
        );
        assert_eq!(spec(json!({"videoId": "x", "format": "av1"})).crf, Some(32));
    }

    #[test]
    fn explicit_quality_and_output_shape_are_part_of_spec() {
        let value = spec(json!({
            "videoId": "x",
            "codec": "h265",
            "quality": 19,
            "fps": 29.97,
            "scale": {"w": 1920, "h": 1080}
        }));
        assert_eq!(value.video_codec, Some(VideoCodec::H265));
        assert_eq!(value.crf, Some(19));
        assert_eq!(value.fps_milli, Some(29_970));
        assert_eq!((value.width, value.height), (Some(1920), Some(1080)));
    }
}
