//! Application boundary for still-image encoders.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::still_container::StillContainerSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StillEncoderCapabilities {
    pub encoder_id: String,
    pub supported_codecs: Vec<crate::domain::still_container::StillCodec>,
    pub supports_alpha: bool,
    pub preserves_metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StillEncodeRequest<'a> {
    pub source: &'a Path,
    pub destination: &'a Path,
    pub spec: StillContainerSpec,
    pub quality: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StillEncodeResult {
    pub output_path: PathBuf,
    pub bytes_written: u64,
}

pub trait StillImageEncoder: Send + Sync {
    fn capabilities(&self) -> StillEncoderCapabilities;
    /// Validate capabilities and the typed container contract before spawning a tool.
    fn validate(&self, request: &StillEncodeRequest<'_>) -> Result<()>;
    fn encode(&self, request: StillEncodeRequest<'_>) -> Result<StillEncodeResult>;
}
