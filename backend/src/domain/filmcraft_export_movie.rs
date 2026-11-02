//! Filmcraft export settings: H.264 multiplexer and audio codec/precision.
//! MIT, pinned 5231852443363f001c3f6b396dd9b1e6461ae2be settings.rs / aac_factory.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Multiplexer {
    #[default]
    Mp4,
    Mov,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioCodec {
    #[default]
    Auto,
    Aac,
    Pcm,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct H264AudioSettings {
    pub codec: AudioCodec,
    pub bitrate_kbps: u32,
    pub bits: u16,
    pub sample_rate: Option<u32>,
    pub channels: u32,
}
impl Default for H264AudioSettings {
    fn default() -> Self {
        Self {
            codec: AudioCodec::Auto,
            bitrate_kbps: 320,
            bits: 16,
            sample_rate: None,
            channels: 2,
        }
    }
}
impl H264AudioSettings {
    pub fn codec(self, multiplexer: Multiplexer) -> AudioCodec {
        if multiplexer == Multiplexer::Mp4 || self.codec == AudioCodec::Auto {
            AudioCodec::Aac
        } else {
            self.codec
        }
    }
    pub fn pcm_bits(self) -> u16 {
        if self.bits >= 24 {
            24
        } else {
            16
        }
    }
    pub fn sample_rate(self) -> u32 {
        self.resolve_sample_rate(48000)
    }
    pub fn resolve_sample_rate(self, sequence_rate: u32) -> u32 {
        self.sample_rate.filter(|rate| (8000..=192000).contains(rate)).unwrap_or(sequence_rate)
    }
    pub fn channels(self) -> u32 { match self.channels { 0 | 1 => 1, 6.. => 6, _ => 2 } }
    pub fn validate_rate(self, multiplexer: Multiplexer) -> anyhow::Result<()> {
        anyhow::ensure!(self.codec(multiplexer) != AudioCodec::Aac || [8000,11025,12000,16000,22050,24000,32000,44100,48000,64000,88200,96000].contains(&self.sample_rate()), "unsupported AAC sample rate");
        Ok(())
    }
    pub fn aac_bps(self) -> u32 {
        (self.bitrate_kbps.clamp(32, 512) * 1000).min(6 * self.sample_rate() * self.channels())
    }
}

/// Shared export audio settings; legacy H.264 type name remains source-compatible.
pub type AudioSettings = H264AudioSettings;

/// Explicit request overrides; omitted fields retain the selected preset policy.
#[derive(Clone,Copy,Debug,Default,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct AudioOverrides {
    pub codec:Option<AudioCodec>,
    pub sample_rate:Option<u32>,
    pub channels:Option<u32>,
    pub bits:Option<u16>,
    pub bitrate_kbps:Option<u32>,
}
impl AudioOverrides {
    pub fn apply(self,mut base:AudioSettings)->AudioSettings {
        if let Some(value)=self.codec{base.codec=value;}
        if let Some(value)=self.sample_rate{base.sample_rate=Some(value);}
        if let Some(value)=self.channels{base.channels=value;}
        if let Some(value)=self.bits{base.bits=value;}
        if let Some(value)=self.bitrate_kbps{base.bitrate_kbps=value;}
        base
    }
}

/// Peak PCM staging per sample frame. OP-Atom closes each mono file after copying
/// it into the ZIP, so only the current channel is duplicated during packing.
pub fn audio_staging_frame_bytes(channels:u32, encoded_sample_bytes:u8, op_atom:bool, normalization:bool)->u64 {
    let channels=u64::from(channels);let encoded=u64::from(encoded_sample_bytes);
    let packing=if op_atom{channels*4+(channels+1)*encoded}else{channels*(4+encoded)};
    if normalization{packing.max(channels*8)}else{packing}
}
