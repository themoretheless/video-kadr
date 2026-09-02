//! Typed audio-output policy, independent of encoder command construction.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::output::{AudioCodec, OutputFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Surround51,
}

impl ChannelLayout {
    pub const fn channel_count(self) -> u8 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Surround51 => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioOutputSpec {
    pub codec: AudioCodec,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate_hz: u32,
    pub channel_layout: ChannelLayout,
    /// Integrated loudness target in milli-LUFS; absent means no normalization.
    pub loudness_target_lufs_milli: Option<i32>,
}

impl AudioOutputSpec {
    pub fn default_for(format: OutputFormat, codec: AudioCodec) -> Result<Self, AudioOutputError> {
        let value = Self {
            codec,
            bitrate_kbps: match codec {
                AudioCodec::PcmS16Le => None,
                AudioCodec::Aac | AudioCodec::Opus | AudioCodec::Mp3 => Some(192),
            },
            sample_rate_hz: 48_000,
            channel_layout: ChannelLayout::Stereo,
            loudness_target_lufs_milli: Some(-14_000),
        };
        value.validate_for(format)?;
        Ok(value)
    }

    pub fn validate_for(&self, format: OutputFormat) -> Result<(), AudioOutputError> {
        let compatible = match format {
            OutputFormat::Mp4 | OutputFormat::Av1 => self.codec == AudioCodec::Aac,
            OutputFormat::Webm => self.codec == AudioCodec::Opus,
            OutputFormat::Prores => self.codec == AudioCodec::PcmS16Le,
            OutputFormat::Mp3 => self.codec == AudioCodec::Mp3,
            OutputFormat::Gif | OutputFormat::Png | OutputFormat::Jpg => false,
        };
        if !compatible {
            return Err(AudioOutputError::IncompatibleContainer);
        }
        if !matches!(self.sample_rate_hz, 44_100 | 48_000 | 96_000) {
            return Err(AudioOutputError::UnsupportedSampleRate);
        }
        match (self.codec, self.bitrate_kbps) {
            (AudioCodec::PcmS16Le, None) => {}
            (AudioCodec::Aac | AudioCodec::Opus | AudioCodec::Mp3, Some(32..=512)) => {}
            _ => return Err(AudioOutputError::InvalidBitrate),
        }
        if self.channel_layout == ChannelLayout::Surround51 && matches!(self.codec, AudioCodec::Mp3)
        {
            return Err(AudioOutputError::UnsupportedChannelLayout);
        }
        if self
            .loudness_target_lufs_milli
            .is_some_and(|value| !(-70_000..=0).contains(&value))
        {
            return Err(AudioOutputError::InvalidLoudnessTarget);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutputError {
    IncompatibleContainer,
    UnsupportedSampleRate,
    InvalidBitrate,
    UnsupportedChannelLayout,
    InvalidLoudnessTarget,
}

impl fmt::Display for AudioOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid audio output specification: {self:?}")
    }
}

impl std::error::Error for AudioOutputError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_container_compatible() {
        for (format, codec) in [
            (OutputFormat::Mp4, AudioCodec::Aac),
            (OutputFormat::Webm, AudioCodec::Opus),
            (OutputFormat::Prores, AudioCodec::PcmS16Le),
            (OutputFormat::Mp3, AudioCodec::Mp3),
        ] {
            AudioOutputSpec::default_for(format, codec).unwrap();
        }
    }

    #[test]
    fn lossy_and_pcm_bitrate_contracts_are_distinct() {
        let mut pcm =
            AudioOutputSpec::default_for(OutputFormat::Prores, AudioCodec::PcmS16Le).unwrap();
        pcm.bitrate_kbps = Some(192);
        assert_eq!(
            pcm.validate_for(OutputFormat::Prores),
            Err(AudioOutputError::InvalidBitrate)
        );
        let mut aac = AudioOutputSpec::default_for(OutputFormat::Mp4, AudioCodec::Aac).unwrap();
        aac.bitrate_kbps = None;
        assert_eq!(
            aac.validate_for(OutputFormat::Mp4),
            Err(AudioOutputError::InvalidBitrate)
        );
        aac.bitrate_kbps = Some(192);
        aac.loudness_target_lufs_milli = Some(1);
        assert_eq!(
            aac.validate_for(OutputFormat::Mp4),
            Err(AudioOutputError::InvalidLoudnessTarget)
        );
    }
}
