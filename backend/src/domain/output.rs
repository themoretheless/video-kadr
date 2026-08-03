//! Final output semantics shared by planning and encoder adapters.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::artifact_graph::Fingerprint;
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
    pub target_size_bytes: Option<u64>,
    pub video_bitrate_bps: Option<u64>,
    pub audio_bitrate_bps: Option<u32>,
    pub estimator_version: Option<String>,
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
    target_size_bytes: Option<u64>,
    video_bitrate_bps: Option<u64>,
    audio_bitrate_bps: Option<u32>,
    estimator_version: Option<String>,
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
            target_size_bytes: wire.target_size_bytes,
            video_bitrate_bps: wire.video_bitrate_bps,
            audio_bitrate_bps: wire.audio_bitrate_bps,
            estimator_version: wire.estimator_version,
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
            target_size_bytes: None,
            video_bitrate_bps: None,
            audio_bitrate_bps: match audio_codec {
                Some(AudioCodec::Opus) => Some(96_000),
                Some(AudioCodec::Aac) => Some(128_000),
                _ => None,
            },
            estimator_version: None,
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

    pub fn apply_target_size(
        &mut self,
        target_bytes: u64,
        audio_bitrate_bps: Option<u32>,
        duration_seconds: f64,
        width: u32,
        height: u32,
        fps: f64,
    ) -> Result<(), OutputSpecError> {
        if !matches!(
            self.format,
            OutputFormat::Mp4 | OutputFormat::Webm | OutputFormat::Av1
        ) || !duration_seconds.is_finite()
            || duration_seconds <= 0.0
            || target_bytes < 128 * 1024
        {
            return Err(OutputSpecError::InvalidTargetSize);
        }
        let audio_bitrate = if self.audio_codec.is_some() {
            audio_bitrate_bps.unwrap_or(128_000)
        } else {
            0
        };
        if self.audio_codec.is_some() && !(32_000..=512_000).contains(&audio_bitrate) {
            return Err(OutputSpecError::InvalidTargetSize);
        }
        let raw_bitrate =
            target_bytes as f64 * 8.0 * 0.95 / duration_seconds - f64::from(audio_bitrate);
        let pixel_floor = (f64::from(width) * f64::from(height) * fps * 0.005).max(100_000.0);
        if !raw_bitrate.is_finite() || raw_bitrate < pixel_floor || raw_bitrate > 100_000_000.0 {
            return Err(OutputSpecError::InvalidTargetSize);
        }
        // size-v1 wire values are normalized to whole kbit/s in both TS and Rust.
        let video_bitrate = (raw_bitrate / 1000.0).round() as u64 * 1000;
        self.crf = None;
        self.target_size_bytes = Some(target_bytes);
        self.video_bitrate_bps = Some(video_bitrate);
        self.audio_bitrate_bps = self.audio_codec.map(|_| audio_bitrate);
        self.estimator_version = Some("size-v1".into());
        self.validate()
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
        match (self.video_codec, self.crf, self.video_bitrate_bps) {
            (Some(VideoCodec::H264 | VideoCodec::H265), Some(value), None) if value <= 51 => {}
            (Some(VideoCodec::Vp9 | VideoCodec::Av1), Some(value), None) if value <= 63 => {}
            (
                Some(VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Vp9 | VideoCodec::Av1),
                None,
                Some(value),
            ) if (64_000..=100_000_000).contains(&value) => {}
            (Some(VideoCodec::Prores), None, None) | (None, None, None) => {}
            _ => return Err(OutputSpecError::InvalidQuality),
        }
        let target_mode = self.target_size_bytes.is_some()
            || self.video_bitrate_bps.is_some()
            || self.estimator_version.is_some();
        if target_mode
            && (self.target_size_bytes.is_none()
                || self.video_bitrate_bps.is_none()
                || self.estimator_version.as_deref() != Some("size-v1"))
        {
            return Err(OutputSpecError::InvalidTargetSize);
        }
        if target_mode && !matches!(self.target_size_bytes, Some(1_000_000..=100_000_000_000)) {
            return Err(OutputSpecError::InvalidTargetSize);
        }
        match self.audio_codec {
            Some(AudioCodec::Aac | AudioCodec::Opus)
                if self
                    .audio_bitrate_bps
                    .is_some_and(|value| (32_000..=512_000).contains(&value)) => {}
            Some(AudioCodec::PcmS16Le | AudioCodec::Mp3) | None
                if self.audio_bitrate_bps.is_none() => {}
            _ => return Err(OutputSpecError::InvalidAudioCodec),
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
    InvalidTargetSize,
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
    fn size_v1_matches_typescript_golden_and_replaces_crf() {
        let mut value = spec(OutputFormat::Mp4, None, None);
        value
            .apply_target_size(10_000_000, Some(128_000), 10.0, 1920, 1080, 30.0)
            .unwrap();
        assert_eq!(value.video_bitrate_bps, Some(7_472_000));
        assert_eq!(value.audio_bitrate_bps, Some(128_000));
        assert_eq!(value.estimator_version.as_deref(), Some("size-v1"));
        assert_eq!(value.crf, None);
        let mut below_pixel_floor = spec(OutputFormat::Mp4, None, None);
        assert_eq!(
            below_pixel_floor.apply_target_size(1_000_000, Some(128_000), 60.0, 3840, 2160, 60.0),
            Err(OutputSpecError::InvalidTargetSize)
        );
    }

    #[test]
    fn serde_rejects_tampered_rate_control_and_audio_fields() {
        let mut quality = serde_json::to_value(spec(OutputFormat::Mp4, None, None)).unwrap();
        quality["audioBitrateBps"] = serde_json::json!(9_999_999);
        assert!(serde_json::from_value::<OutputSpec>(quality).is_err());

        let mut target = spec(OutputFormat::Mp4, None, None);
        target
            .apply_target_size(10_000_000, Some(128_000), 10.0, 1920, 1080, 30.0)
            .unwrap();
        let mut wire = serde_json::to_value(target).unwrap();
        wire["targetSizeBytes"] = serde_json::json!(0);
        assert!(serde_json::from_value::<OutputSpec>(wire).is_err());
    }
}
