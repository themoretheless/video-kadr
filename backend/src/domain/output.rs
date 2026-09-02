//! Final output semantics shared by planning and encoder adapters.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::artifact_graph::Fingerprint;
use super::audio_output::{AudioOutputError, AudioOutputSpec};
use super::edit::OutputScale;

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
    pub fn parse(value: Option<&str>) -> Result<Self, OutputSpecError> {
        match value.unwrap_or("mp4") {
            "mp4" => Ok(Self::Mp4),
            "webm" => Ok(Self::Webm),
            "gif" => Ok(Self::Gif),
            "png" => Ok(Self::Png),
            "jpg" => Ok(Self::Jpg),
            "mp3" => Ok(Self::Mp3),
            "av1" => Ok(Self::Av1),
            "prores" => Ok(Self::Prores),
            _ => Err(OutputSpecError::InvalidFormat),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::Gif => "gif",
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Mp3 => "mp3",
            Self::Av1 => "av1",
            Self::Prores => "prores",
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

impl fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
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
    pub fn parse_mp4(value: Option<&str>) -> Result<Self, OutputSpecError> {
        match value.unwrap_or("h264") {
            "h264" => Ok(Self::H264),
            "h265" => Ok(Self::H265),
            _ => Err(OutputSpecError::InvalidCodec),
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutputSpecWire {
    format: OutputFormat,
    video_codec: Option<VideoCodec>,
    audio_codec: Option<AudioCodec>,
    crf: Option<u32>,
    fps_milli: Option<u32>,
    width: Option<i32>,
    height: Option<i32>,
}

impl<'de> Deserialize<'de> for OutputSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = OutputSpecWire::deserialize(deserializer)?;
        let value = Self {
            format: wire.format,
            video_codec: wire.video_codec,
            audio_codec: wire.audio_codec,
            crf: wire.crf,
            fps_milli: wire.fps_milli,
            width: wire.width,
            height: wire.height,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

impl OutputSpec {
    pub fn new(
        format: OutputFormat,
        mp4_codec: Option<VideoCodec>,
        muted: bool,
        quality: Option<u32>,
        fps: Option<f64>,
        scale: Option<OutputScale>,
    ) -> Result<Self, OutputSpecError> {
        let video_codec = match format {
            OutputFormat::Mp4 => Some(mp4_codec.unwrap_or(VideoCodec::H264)),
            OutputFormat::Webm => Some(VideoCodec::Vp9),
            OutputFormat::Av1 => Some(VideoCodec::Av1),
            OutputFormat::Prores => Some(VideoCodec::Prores),
            OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg | OutputFormat::Mp3 => None,
        };
        if format == OutputFormat::Mp4
            && !matches!(video_codec, Some(VideoCodec::H264 | VideoCodec::H265))
        {
            return Err(OutputSpecError::InvalidCodec);
        }
        let audio_codec = if muted
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
            VideoCodec::H264 => Some(quality.unwrap_or(23)),
            VideoCodec::H265 => Some(quality.unwrap_or(28)),
            VideoCodec::Vp9 | VideoCodec::Av1 => Some(quality.unwrap_or(32)),
            VideoCodec::Prores => None,
        });
        let fps_milli = fps
            .map(|value| {
                if !value.is_finite() || !(1.0..=240.0).contains(&value) {
                    return Err(OutputSpecError::InvalidFps);
                }
                Ok((value * 1000.0).round() as u32)
            })
            .transpose()?;
        let value = Self {
            format,
            video_codec,
            audio_codec,
            crf,
            fps_milli,
            width: scale.map(|value| value.width),
            height: scale.map(|value| value.height),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn fps(&self) -> Option<f64> {
        self.fps_milli.map(|value| f64::from(value) / 1000.0)
    }

    /// Expanded audio policy for encoder adapters without changing the stable wire contract.
    pub fn audio_output_spec(&self) -> Result<Option<AudioOutputSpec>, AudioOutputError> {
        self.audio_codec
            .map(|codec| AudioOutputSpec::default_for(self.format, codec))
            .transpose()
    }

    pub fn validate(&self) -> Result<(), OutputSpecError> {
        let expected_video = match self.format {
            OutputFormat::Mp4 => {
                matches!(self.video_codec, Some(VideoCodec::H264 | VideoCodec::H265))
            }
            OutputFormat::Webm => self.video_codec == Some(VideoCodec::Vp9),
            OutputFormat::Av1 => self.video_codec == Some(VideoCodec::Av1),
            OutputFormat::Prores => self.video_codec == Some(VideoCodec::Prores),
            OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg | OutputFormat::Mp3 => {
                self.video_codec.is_none()
            }
        };
        if !expected_video {
            return Err(OutputSpecError::InvalidCodec);
        }
        let expected_audio = match self.format {
            OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg => self.audio_codec.is_none(),
            OutputFormat::Webm => matches!(self.audio_codec, None | Some(AudioCodec::Opus)),
            OutputFormat::Prores => {
                matches!(self.audio_codec, None | Some(AudioCodec::PcmS16Le))
            }
            OutputFormat::Mp3 => self.audio_codec == Some(AudioCodec::Mp3),
            OutputFormat::Mp4 | OutputFormat::Av1 => {
                matches!(self.audio_codec, None | Some(AudioCodec::Aac))
            }
        };
        if !expected_audio {
            return Err(OutputSpecError::InvalidAudioCodec);
        }
        match (self.video_codec, self.crf) {
            (Some(VideoCodec::H264 | VideoCodec::H265), Some(value)) if value <= 51 => {}
            (Some(VideoCodec::Vp9 | VideoCodec::Av1), Some(value)) if value <= 63 => {}
            (Some(VideoCodec::Prores), None) | (None, None) => {}
            _ => return Err(OutputSpecError::InvalidQuality),
        }
        if self
            .fps_milli
            .is_some_and(|value| !(1_000..=240_000).contains(&value))
        {
            return Err(OutputSpecError::InvalidFps);
        }
        match (self.width, self.height) {
            (None, None) => {}
            (Some(width), Some(height)) => {
                let scale = OutputScale { width, height };
                let valid = |value| matches!(value, -2 | -1) || (2..=7680).contains(&value);
                if !valid(scale.width)
                    || !valid(scale.height)
                    || (scale.width < 0 && scale.height < 0)
                {
                    return Err(OutputSpecError::InvalidDimensions);
                }
            }
            _ => return Err(OutputSpecError::InvalidDimensions),
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::digest(
            &serde_json::to_vec(self).expect("OutputSpec serialization cannot fail"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputSpecError {
    InvalidFormat,
    InvalidCodec,
    InvalidAudioCodec,
    InvalidQuality,
    InvalidFps,
    InvalidDimensions,
}

impl fmt::Display for OutputSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid output specification: {self:?}")
    }
}

impl std::error::Error for OutputSpecError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(format: OutputFormat, codec: Option<VideoCodec>, quality: Option<u32>) -> OutputSpec {
        OutputSpec::new(format, codec, false, quality, None, None).unwrap()
    }

    #[test]
    fn quality_defaults_are_codec_specific() {
        assert_eq!(spec(OutputFormat::Mp4, None, None).crf, Some(23));
        assert_eq!(
            spec(OutputFormat::Mp4, Some(VideoCodec::H265), None).crf,
            Some(28)
        );
        assert_eq!(spec(OutputFormat::Webm, None, None).crf, Some(32));
        assert_eq!(spec(OutputFormat::Av1, None, None).crf, Some(32));
    }

    #[test]
    fn explicit_quality_and_output_shape_are_part_of_spec() {
        let value = OutputSpec::new(
            OutputFormat::Mp4,
            Some(VideoCodec::H265),
            false,
            Some(19),
            Some(29.97),
            Some(OutputScale {
                width: 1920,
                height: 1080,
            }),
        )
        .unwrap();
        assert_eq!(value.video_codec, Some(VideoCodec::H265));
        assert_eq!(value.crf, Some(19));
        assert_eq!(value.fps_milli, Some(29_970));
        assert_eq!((value.width, value.height), (Some(1920), Some(1080)));
    }

    #[test]
    fn serde_rejects_incompatible_codec_and_quality() {
        let value = spec(OutputFormat::Webm, None, None);
        let mut invalid = serde_json::to_value(value).unwrap();
        invalid["videoCodec"] = serde_json::json!("h264");
        assert!(serde_json::from_value::<OutputSpec>(invalid).is_err());

        let mut invalid = serde_json::to_value(spec(OutputFormat::Mp4, None, None)).unwrap();
        invalid["crf"] = serde_json::json!(99);
        assert!(serde_json::from_value::<OutputSpec>(invalid).is_err());
    }

    #[test]
    fn audio_policy_expands_from_stable_output_contract() {
        let output = spec(OutputFormat::Webm, None, None);
        let audio = output.audio_output_spec().unwrap().unwrap();
        assert_eq!(audio.codec, AudioCodec::Opus);
        assert_eq!(audio.sample_rate_hz, 48_000);
    }
}
