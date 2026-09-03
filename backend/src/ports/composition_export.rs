//! Application port for compiling a validated multi-source composition into
//! an external-tool command. Source identifiers remain part of the immutable
//! composition while resolved filesystem paths are supplied only at execution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::domain::composition::{Composition, CompositionClipId, SourceId};

use super::media_export::CompiledExportCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "container", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompositionExportProfile {
    Mp4 { codec: CompositionMp4Codec },
    Webm { codec: CompositionWebmCodec },
    Mov { profile: CompositionProResProfile },
    Audio { codec: CompositionAudioCodec },
}

impl Default for CompositionExportProfile {
    fn default() -> Self {
        Self::Mp4 {
            codec: CompositionMp4Codec::H264,
        }
    }
}

impl CompositionExportProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::Mp4 {
                codec: CompositionMp4Codec::H264,
            } => "mp4/h264",
            Self::Mp4 {
                codec: CompositionMp4Codec::H265,
            } => "mp4/h265",
            Self::Webm {
                codec: CompositionWebmCodec::Vp9,
            } => "webm/vp9",
            Self::Webm {
                codec: CompositionWebmCodec::Av1,
            } => "webm/av1",
            Self::Mov {
                profile: CompositionProResProfile::Proxy,
            } => "mov/prores-proxy",
            Self::Mov {
                profile: CompositionProResProfile::Lt,
            } => "mov/prores-lt",
            Self::Mov {
                profile: CompositionProResProfile::Standard,
            } => "mov/prores-standard",
            Self::Mov {
                profile: CompositionProResProfile::Hq,
            } => "mov/prores-hq",
            Self::Audio {
                codec: CompositionAudioCodec::Mp3,
            } => "audio/mp3",
            Self::Audio {
                codec: CompositionAudioCodec::Wav,
            } => "audio/wav",
            Self::Audio {
                codec: CompositionAudioCodec::Aac,
            } => "audio/aac",
            Self::Audio {
                codec: CompositionAudioCodec::Flac,
            } => "audio/flac",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 { .. } => "mp4",
            Self::Webm { .. } => "webm",
            Self::Mov { .. } => "mov",
            Self::Audio {
                codec: CompositionAudioCodec::Mp3,
            } => "mp3",
            Self::Audio {
                codec: CompositionAudioCodec::Wav,
            } => "wav",
            Self::Audio {
                codec: CompositionAudioCodec::Aac,
            } => "aac",
            Self::Audio {
                codec: CompositionAudioCodec::Flac,
            } => "flac",
        }
    }

    pub fn muxer(self) -> &'static str {
        match self {
            Self::Audio {
                codec: CompositionAudioCodec::Aac,
            } => "adts",
            _ => self.extension(),
        }
    }

    pub fn video_codec(self) -> &'static str {
        match self {
            Self::Mp4 {
                codec: CompositionMp4Codec::H264,
            } => "h264",
            Self::Mp4 {
                codec: CompositionMp4Codec::H265,
            } => "hevc",
            Self::Webm {
                codec: CompositionWebmCodec::Vp9,
            } => "vp9",
            Self::Webm {
                codec: CompositionWebmCodec::Av1,
            } => "av1",
            Self::Mov { .. } => "prores",
            Self::Audio { .. } => "none",
        }
    }

    pub fn audio_codec(self) -> &'static str {
        match self {
            Self::Mp4 { .. } => "aac",
            Self::Webm { .. } => "opus",
            Self::Mov { .. } => "pcm_s16le",
            Self::Audio {
                codec: CompositionAudioCodec::Mp3,
            } => "mp3",
            Self::Audio {
                codec: CompositionAudioCodec::Wav,
            } => "pcm_s16le",
            Self::Audio {
                codec: CompositionAudioCodec::Aac,
            } => "aac",
            Self::Audio {
                codec: CompositionAudioCodec::Flac,
            } => "flac",
        }
    }

    pub fn is_audio_only(self) -> bool {
        matches!(self, Self::Audio { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionAudioCodec {
    Mp3,
    Wav,
    Aac,
    Flac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionMp4Codec {
    H264,
    H265,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionWebmCodec {
    Vp9,
    Av1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompositionProResProfile {
    Proxy,
    Lt,
    Standard,
    Hq,
}

impl CompositionProResProfile {
    pub fn ffmpeg_value(self) -> u8 {
        match self {
            Self::Proxy => 0,
            Self::Lt => 1,
            Self::Standard => 2,
            Self::Hq => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionAv1Encoder {
    LibSvtAv1,
    LibAomAv1,
}

impl CompositionAv1Encoder {
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::LibSvtAv1 => "libsvtav1",
            Self::LibAomAv1 => "libaom-av1",
        }
    }
}

/// Fully resolved process-level delivery settings. `video_quality` is CRF for
/// H.264/H.265/VP9/AV1 and qscale for ProRes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositionExportSpec {
    pub profile: CompositionExportProfile,
    pub video_quality: u32,
    pub video_bitrate_kbps: Option<u32>,
    pub av1_encoder: Option<CompositionAv1Encoder>,
    pub range: Option<CompositionExportRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionExportRange {
    pub start_ticks: u64,
    pub end_ticks: u64,
}

pub struct CompositionExportCompileRequest<'a> {
    pub inputs: &'a BTreeMap<SourceId, PathBuf>,
    /// Pre-materialized immutable UTF-8 text and a fail-closed resolved font
    /// for every active text clip. Text never enters an FFmpeg expression.
    pub text_resources: &'a BTreeMap<CompositionClipId, CompositionTextResource>,
    pub destination: &'a Path,
    pub parallel_jobs: usize,
    pub composition: &'a Composition,
    pub output: CompositionExportSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionTextResource {
    pub text_file: PathBuf,
    pub font_file: PathBuf,
}

pub trait CompositionExportCommandCompiler: Send + Sync {
    fn compile(
        &self,
        request: CompositionExportCompileRequest<'_>,
    ) -> Result<CompiledExportCommand>;
}
