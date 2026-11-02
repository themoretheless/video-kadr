//! Typed portable settings adapted from Filmcraft (MIT).
//! Pinned 5231852443363f001c3f6b396dd9b1e6461ae2be export/{lib,settings}.rs.
//! License: docs/licenses/filmcraft-MIT.txt. Runtime encoder state is omitted.
use anyhow::{ensure, Result};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Import uses serde normalization; save additionally invokes source validation.
pub fn normalize(value: Value, validate: bool) -> Result<Value> {
    let settings: ExportSettings = serde_json::from_value(value)?;
    if validate {
        settings.validate()?;
    }
    Ok(serde_json::to_value(settings)?)
}

impl ExportSettings {
    pub fn validate(&self) -> Result<()> {
        if let Some(range) = self.range {
            ensure!(
                range.start.0 >= 0
                    && range.duration.0 > 0
                    && range
                        .start
                        .0
                        .checked_add(range.duration.0)
                        .is_some_and(|end| end <= Tick::MAX.0),
                "invalid export range"
            );
        }
        ensure!(
            self.scale.is_finite() && self.scale > 0.,
            "output scale must be finite and positive"
        );
        if let Some((w, h)) = self.frame_size {
            ensure!(
                w > 0
                    && h > 0
                    && w <= 32768
                    && h <= 32768
                    && u64::from(w) * u64::from(h) <= 268435456,
                "invalid output frame size"
            );
        }
        if let Some(rate) = self.frame_rate {
            ensure!(
                rate.num > 0
                    && rate.den > 0
                    && rate.as_f64() <= 1000.
                    && rate.num <= i64::from(u32::MAX)
                    && rate.den <= i64::from(u32::MAX)
                    && i128::from(filmcraft_time::TICKS_PER_SECOND) * i128::from(rate.den)
                        / i128::from(rate.num.max(1))
                        <= i128::from(Tick::MAX.0),
                "invalid output frame rate"
            );
        }
        ensure!(
            !self
                .audio
                .sample_rate
                .is_some_and(|rate| rate == 0 || rate > 384000),
            "output sample rate must be between 1 and 384000 Hz"
        );
        ensure!(
            self.field_order == FieldOrder::Progressive
                || matches!(self.format, Format::Wav | Format::Aiff),
            "Filmcraft encoders write progressive frames"
        );
        ensure!(
            !self.effects.image_overlay.enabled
                || !self.effects.image_overlay.path.trim().is_empty(),
            "image overlay: no image file chosen"
        );
        ensure!(
            self.format != Format::Hevc || self.bitrate_mode != BitrateMode::Vbr2Pass,
            "HEVC has no two-pass mode"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    #[default]
    #[serde(rename = "h264", alias = "H264")]
    H264,
    #[serde(rename = "hevc", alias = "Hevc")]
    Hevc,
    #[serde(rename = "prores", alias = "ProRes")]
    ProRes,
    #[serde(rename = "dnxhr", alias = "DnxHr")]
    DnxHr,
    #[serde(rename = "apv", alias = "Apv")]
    Apv,
    #[serde(rename = "mjpeg", alias = "Mjpeg")]
    Mjpeg,
    #[serde(rename = "png", alias = "PngSequence")]
    PngSequence,
    #[serde(rename = "tiff", alias = "TiffSequence")]
    TiffSequence,
    #[serde(rename = "bmp", alias = "BmpSequence")]
    BmpSequence,
    #[serde(rename = "gif", alias = "Gif")]
    Gif,
    #[serde(rename = "wav", alias = "Wav")]
    Wav,
    #[serde(rename = "aiff", alias = "Aiff")]
    Aiff,
    #[serde(rename = "mxf-op1a", alias = "MxfOp1a")]
    MxfOp1a,
    #[serde(rename = "mxf-opatom", alias = "MxfOpAtom")]
    MxfOpAtom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scaling {
    #[default]
    ScaleToFit,
    ScaleToFill,
    StretchToFill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldOrder {
    #[default]
    Progressive,
    UpperFirst,
    LowerFirst,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum H264Profile {
    Baseline,
    Main,
    #[default]
    High,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HardwareEncoding {
    #[default]
    Off,
    Auto,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BitrateMode {
    Cbr,
    #[default]
    Vbr1Pass,
    Vbr2Pass,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Multiplexer {
    #[default]
    Mp4,
    Mov,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MxfVideoCodec {
    #[default]
    Dnxhr,
    ProRes,
    H264,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioCodec {
    #[default]
    Auto,
    Aac,
    Pcm,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub codec: AudioCodec,
    pub sample_rate: Option<u32>,
    pub channels: u32,
    pub bitrate_kbps: u32,
    pub bits: u16,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            codec: AudioCodec::Auto,
            sample_rate: None,
            channels: 2,
            bitrate_kbps: 320,
            bits: 16,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageOverlay {
    pub enabled: bool,
    pub path: String,
    pub placement: Placement,
    pub offset: (f32, f32),
    pub size_percent: f32,
    pub opacity: f32,
}

impl Default for ImageOverlay {
    fn default() -> Self {
        ImageOverlay {
            enabled: false,
            path: String::new(),
            placement: Placement::TopRight,
            offset: (0.0, 0.0),
            size_percent: 15.0,
            opacity: 100.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextOverlay {
    pub enabled: bool,
    pub text: String,
    pub placement: Placement,
    pub offset: (f32, f32),
    pub size_percent: f32,
    pub opacity: f32,
}

impl Default for TextOverlay {
    fn default() -> Self {
        TextOverlay {
            enabled: false,
            text: String::new(),
            placement: Placement::BottomCenter,
            offset: (0.0, 0.0),
            size_percent: 5.0,
            opacity: 100.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoLimiter {
    pub enabled: bool,
    pub min_percent: f32,
    pub max_percent: f32,
}

impl Default for VideoLimiter {
    fn default() -> Self {
        VideoLimiter {
            enabled: false,
            min_percent: 0.0,
            max_percent: 100.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessNormalization {
    pub enabled: bool,
    pub target_lufs: f64,
    pub true_peak_dbtp: f64,
}

impl Default for LoudnessNormalization {
    fn default() -> Self {
        LoudnessNormalization {
            enabled: false,
            target_lufs: -23.0,
            true_peak_dbtp: -2.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportEffects {
    pub image_overlay: ImageOverlay,
    pub name_overlay: TextOverlay,
    pub timecode_overlay: TextOverlay,
    pub video_limiter: VideoLimiter,
    pub loudness: LoudnessNormalization,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportMetadata {
    pub title: String,
    pub creator: String,
    pub copyright: String,
    pub description: String,
    pub comment: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportSettings {
    pub format: Format,
    pub path: String,
    pub range: Option<TimeRange>,
    pub scale: f32,
    pub include_audio: bool,
    pub quality: u8,
    pub bitrate_kbps: u32,
    pub burn_captions: bool,
    #[serde(default)]
    pub part_of_batch: bool,
    #[serde(default)]
    pub prores_profile: String,
    #[serde(default)]
    pub dnx_profile: String,
    #[serde(default)]
    pub apv_profile: String,
    #[serde(default)]
    pub mxf_video_codec: MxfVideoCodec,
    #[serde(default)]
    pub sdr: bool,
    pub frame_size: Option<(u32, u32)>,
    pub frame_rate: Option<FrameRate>,
    pub scaling: Scaling,
    pub pixel_aspect: Option<(u32, u32)>,
    pub field_order: FieldOrder,
    pub h264_profile: H264Profile,
    pub h264_level: Option<u8>,
    pub bitrate_mode: BitrateMode,
    #[serde(default)]
    pub hardware_encoding: HardwareEncoding,
    pub max_bitrate_kbps: Option<u32>,
    pub adaptive_bitrate: Option<f32>,
    pub keyframe_distance: Option<u32>,
    pub render_at_max_depth: bool,
    pub max_render_quality: bool,
    pub audio: AudioSettings,
    pub multiplexer: Multiplexer,
    pub caption_sidecar: Option<String>,
    pub effects: ExportEffects,
    pub metadata: ExportMetadata,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: Format::H264,
            path: String::new(),
            range: None,
            scale: 1.0,
            include_audio: true,
            quality: 90,
            bitrate_kbps: 20_000,
            burn_captions: false,
            part_of_batch: false,
            prores_profile: String::new(),
            dnx_profile: String::new(),
            apv_profile: String::new(),
            mxf_video_codec: MxfVideoCodec::default(),
            sdr: false,
            frame_size: None,
            frame_rate: None,
            scaling: Scaling::default(),
            pixel_aspect: None,
            field_order: FieldOrder::Progressive,
            h264_profile: H264Profile::High,
            h264_level: None,
            bitrate_mode: BitrateMode::default(),
            hardware_encoding: HardwareEncoding::default(),
            max_bitrate_kbps: None,
            adaptive_bitrate: None,
            keyframe_distance: None,
            render_at_max_depth: false,
            max_render_quality: false,
            audio: AudioSettings::default(),
            multiplexer: Multiplexer::Mp4,
            caption_sidecar: None,
            effects: ExportEffects::default(),
            metadata: ExportMetadata::default(),
        }
    }
}
