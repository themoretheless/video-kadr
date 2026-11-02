//! Trusted native source-project video bridge. HTTP/job admission and
//! publication belong to the caller; output is never a client filesystem path.
use super::filmcraft_project::{render_export_frame, PreviewFrameRequest};
use crate::{
    domain::{
        composition::FrameRateRatio, filmcraft_project_file::SourceProjectDocument,
        filmcraft_project_index::ProjectIndex,
    },
    state::AppState,
    tools::native_rgba_export::{
        encode_frames_with_source_range, RasterAudio, RasterEncoding, RasterVideoSpec,
    },
};
use anyhow::{ensure, Context, Result};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportPreset {
    #[default]
    Lossless,
    AdaptiveHigh,
    AdaptiveMedium,
    AdaptiveLow,
    HighQuality1080,
    HighQuality2160,
    Youtube1080,
    Youtube2160,
    Vimeo1080,
    Vimeo2160,
    SocialVertical,
    Mjpeg,
    Hevc,
    ProresProxy,
    ProresLt,
    ProresStandard,
    ProresHq,
    DnxhrLb,
    DnxhrSq,
    DnxhrHq,
    Apv42210,
    Apv42212,
    MxfDnxhrHq,
    MxfProresHq,
    MxfAtomDnxhrHq,
    MxfH264,
    PngSequence,
    TiffSequence,
    BmpSequence,
    Gif,
    Wav16,
    Wav24,
    Aiff16,
}
impl ExportPreset {
    pub fn h264(self) -> bool {
        matches!(
            self,
            Self::AdaptiveHigh
                | Self::AdaptiveMedium
                | Self::AdaptiveLow
                | Self::HighQuality1080
                | Self::HighQuality2160
                | Self::Youtube1080
                | Self::Youtube2160
                | Self::Vimeo1080
                | Self::Vimeo2160
                | Self::SocialVertical
                | Self::MxfH264
        )
    }
    pub fn audio_only(self) -> bool {
        matches!(self, Self::Wav16 | Self::Wav24 | Self::Aiff16)
    }
    pub fn pcm_preset(self) -> crate::tools::native_pcm_export::PcmPreset {
        match self {
            Self::Wav24 => crate::tools::native_pcm_export::PcmPreset::Wav24,
            Self::Aiff16 => crate::tools::native_pcm_export::PcmPreset::Aiff16,
            _ => crate::tools::native_pcm_export::PcmPreset::Wav16,
        }
    }
    pub fn image_sequence(self) -> bool {
        matches!(
            self,
            Self::PngSequence | Self::TiffSequence | Self::BmpSequence
        )
    }
    pub fn silent_visual(self) -> bool {
        self.image_sequence() || matches!(self, Self::Gif)
    }
    pub fn preserve_alpha(self) -> bool {
        matches!(self, Self::Lossless) || self.silent_visual()
    }
    pub fn native_output(self) -> bool {
        self.audio_only() || self.silent_visual() || self.native_intra() || self.h264() || matches!(self,Self::Hevc)
    }
    pub fn native_mxf(self) -> bool {
        matches!(
            self,
            Self::MxfDnxhrHq | Self::MxfProresHq | Self::MxfAtomDnxhrHq | Self::MxfH264
        )
    }
    pub fn native_apv(self) -> bool {
        matches!(self, Self::Apv42210 | Self::Apv42212)
    }
    pub fn native_dnx(self) -> bool {
        matches!(self, Self::DnxhrLb | Self::DnxhrSq | Self::DnxhrHq)
    }
    pub fn native_intra(self) -> bool {
        self.native_mxf()
            || self.native_apv()
            || self.native_dnx()
            || matches!(self, Self::Mjpeg)
            || self.prores_profile().is_some()
    }
    fn prores_profile(self) -> Option<u8> {
        match self {
            Self::ProresProxy => Some(0),
            Self::ProresLt => Some(1),
            Self::ProresStandard => Some(2),
            Self::ProresHq => Some(3),
            _ => None,
        }
    }
    pub fn extension(self) -> &'static str {
        if self.audio_only() {
            if matches!(self, Self::Aiff16) {
                "aiff"
            } else {
                "wav"
            }
        } else if matches!(self, Self::Gif) {
            "gif"
        } else if self.image_sequence() || matches!(self, Self::MxfAtomDnxhrHq) {
            "zip"
        } else if self.native_mxf() {
            "mxf"
        } else if matches!(self, Self::Lossless) {
            "mkv"
        } else if self.native_intra() {
            "mov"
        } else {
            "mp4"
        }
    }
    pub fn encoder(self) -> &'static str {
        if matches!(self,Self::Hevc) {"native_hevc"} else if matches!(self, Self::Mjpeg) {
            "native_mjpeg"
        } else if self.audio_only() {
            "native_pcm"
        } else if self.silent_visual() {
            "native_image_sequence"
        } else if self.native_mxf() {
            "native_mxf"
        } else if self.native_apv() {
            "native_apv"
        } else if self.native_dnx() {
            "native_dnxhr"
        } else if matches!(self, Self::Lossless) {
            "ffv1"
        } else if self.prores_profile().is_some() {
            "native_prores"
        } else {
            "native_h264"
        }
    }
    pub fn video_codec(self) -> &'static str {
        if matches!(self,Self::Hevc) {return "hevc";}
        if matches!(self, Self::Mjpeg) {
            return "mjpeg";
        }
        if self.audio_only() {
            "none"
        } else if matches!(self, Self::Gif) {
            "gif"
        } else if self.image_sequence() {
            match self {
                Self::PngSequence => "png",
                Self::TiffSequence => "tiff",
                _ => "bmp",
            }
        } else if matches!(self, Self::MxfH264) {
            "h264"
        } else if matches!(self, Self::MxfDnxhrHq | Self::MxfAtomDnxhrHq) {
            "dnxhd"
        } else if matches!(self, Self::MxfProresHq) {
            "prores"
        } else if self.native_apv() {
            "apv"
        } else if self.native_dnx() {
            "dnxhd"
        } else if matches!(self, Self::Lossless) {
            "ffv1"
        } else if self.prores_profile().is_some() {
            "prores"
        } else {
            "h264"
        }
    }
    pub fn audio_codec(self) -> &'static str {
        if self.audio_only() {
            match self {
                Self::Wav24 => "pcm_s24le",
                Self::Aiff16 => "pcm_s16be",
                _ => "pcm_s16le",
            }
        } else if matches!(self, Self::Lossless) {
            "pcm_f32le"
        } else if self.native_intra() {
            "pcm_s24le"
        } else {
            "aac"
        }
    }
    pub fn muxer(self) -> &'static str {
        if self.audio_only() {
            self.extension()
        } else if matches!(self, Self::Gif) {
            "gif"
        } else if self.image_sequence() {
            "zip"
        } else if self.native_mxf() {
            "mxf"
        } else if matches!(self, Self::Lossless) {
            "matroska"
        } else if self.native_intra() {
            "mov"
        } else {
            "mp4"
        }
    }
    pub fn frame_size(self) -> Option<(usize, usize)> {
        match self {
            Self::Gif => Some((640, 360)),
            Self::HighQuality1080 | Self::Youtube1080 | Self::Vimeo1080 => Some((1920, 1080)),
            Self::HighQuality2160 | Self::Youtube2160 | Self::Vimeo2160 => Some((3840, 2160)),
            Self::SocialVertical => Some((1080, 1920)),
            _ => None,
        }
    }
    pub fn scaling(self) -> crate::domain::filmcraft_export_geometry::Scaling {
        if matches!(self, Self::SocialVertical) {
            crate::domain::filmcraft_export_geometry::Scaling::ScaleToFill
        } else {
            crate::domain::filmcraft_export_geometry::Scaling::ScaleToFit
        }
    }
    pub fn encoding(self, spec: RasterVideoSpec) -> RasterEncoding {
        if matches!(self, Self::Mjpeg) {
            return RasterEncoding::Mjpeg { quality: 90 };
        }
        if matches!(self, Self::Gif) {
            return RasterEncoding::Gif;
        }
        if self.image_sequence() {
            return RasterEncoding::ImageSequence {
                format: match self {
                    Self::PngSequence => 0,
                    Self::TiffSequence => 1,
                    _ => 2,
                },
            };
        }
        if matches!(self, Self::MxfH264) {
            return RasterEncoding::MxfH264 {
                start: 0,
                drop_frame: false,
                keyint: None,
                profile: Default::default(),
                bitrate: None,
            };
        }
        if matches!(self, Self::MxfAtomDnxhrHq) {
            return RasterEncoding::MxfAtom {
                start: 0,
                drop_frame: false,
            };
        }
        if self.native_mxf() {
            return RasterEncoding::Mxf {
                prores: matches!(self, Self::MxfProresHq),
                start: 0,
                drop_frame: false,
            };
        }
        if self.native_apv() {
            return RasterEncoding::Apv {
                profile: if matches!(self, Self::Apv42210) { 0 } else { 1 },
            };
        }
        if self.native_dnx() {
            return RasterEncoding::DnxHr {
                profile: match self {
                    Self::DnxhrLb => 0,
                    Self::DnxhrSq => 1,
                    _ => 2,
                },
            };
        }
        if matches!(self, Self::Lossless) {
            return RasterEncoding::Ffv1;
        }
        if let Some(profile) = self.prores_profile() {
            return RasterEncoding::ProRes { profile };
        }
        let fps = spec.frame_rate.numerator as f64 / spec.frame_rate.denominator as f64;
        let fixed = match self {
            Self::HighQuality1080 => Some((20_000, 24_000)),
            Self::HighQuality2160 => Some((60_000, 72_000)),
            Self::Youtube1080 => Some((16_000, 20_000)),
            Self::Youtube2160 => Some((45_000, 54_000)),
            Self::Vimeo1080 => Some((18_000, 24_000)),
            Self::Vimeo2160 => Some((50_000, 65_000)),
            Self::SocialVertical => Some((12_000, 16_000)),
            _ => None,
        };
        let (target_kbps, max_kbps) = fixed.unwrap_or_else(|| {
            let bpp = match self {
                Self::AdaptiveHigh => 0.2f32,
                Self::AdaptiveMedium => 0.1f32,
                _ => 0.05f32,
            };
            let target = ((spec.width as f64 * spec.height as f64 * fps * f64::from(bpp) / 1000.)
                .round() as u32)
                .max(100);
            (
                target,
                (u64::from(target) * 3 / 2).min(u64::from(u32::MAX)) as u32,
            )
        });
        let keyint = (fps * 2.).round().max(1.) as u32;
        if matches!(self, Self::Vimeo1080 | Self::Vimeo2160) {
            RasterEncoding::H264TwoPass {
                target_kbps,
                max_kbps,
                keyint,
                profile: Default::default(),
                mode: crate::domain::filmcraft_export_h264::BitrateMode::Vbr2Pass,
            }
        } else {
            RasterEncoding::H264 {
                target_kbps,
                max_kbps,
                keyint,
                profile: Default::default(),
                mode: Default::default(),
            }
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoRequest {
    pub document: Value,
    pub sequence_id: String,
    #[serde(default)]
    pub start_ticks: Option<String>,
    #[serde(default)]
    pub start_frames: Option<String>,
    #[serde(default)]
    pub source_range: Option<crate::domain::filmcraft_export_range::SourceRange>,
    #[serde(default)]
    pub frames: u64,
    #[serde(default)]
    pub with_audio: bool,
    #[serde(default)]
    pub preset: ExportPreset,
    #[serde(default = "default_mjpeg_quality")]
    pub mjpeg_quality: u8,
    #[serde(default)]
    pub frame_size: Option<(usize, usize)>,
    #[serde(default)]
    pub scale: Option<f32>,
    #[serde(default)]
    pub burn_captions: Option<bool>,
    #[serde(default)]
    pub caption_sidecar: Option<String>,
    #[serde(default)]
    pub frame_rate: Option<FrameRateRatio>,
    #[serde(default)]
    pub keyframe_distance: Option<u32>,
    #[serde(default)]
    pub h264_profile: crate::domain::filmcraft_export_h264::H264Profile,
    #[serde(default)]
    pub h264_level: Option<u8>,
    #[serde(default)]
    pub multiplexer: crate::domain::filmcraft_export_movie::Multiplexer,
    #[serde(default)]
    pub h264_audio: Option<crate::domain::filmcraft_export_movie::H264AudioSettings>,
    #[serde(default)]
    pub audio: Option<crate::domain::filmcraft_export_movie::AudioOverrides>,
    #[serde(default)]
    pub bitrate: Option<crate::domain::filmcraft_export_h264::BitrateSettings>,
    #[serde(default)]
    pub scaling: Option<crate::domain::filmcraft_export_geometry::Scaling>,
    #[serde(default)]
    pub pixel_aspect: Option<(u32, u32)>,
    #[serde(default)]
    pub max_render_quality: bool,
    #[serde(default)]
    pub metadata: crate::domain::filmcraft_export_metadata::ExportMetadata,
    #[serde(default)]
    pub video_limiter: crate::domain::filmcraft_export_limiter::VideoLimiter,
    #[serde(default)]
    pub loudness: crate::tools::native_loudness_export::LoudnessSettings,
    #[serde(default)]
    pub name_overlay: crate::domain::filmcraft_export_text::TextOverlay,
    #[serde(default)]
    pub image_overlay: crate::domain::filmcraft_export_image::ImageOverlay,
    #[serde(default)]
    pub timecode_overlay: crate::domain::filmcraft_export_text::TextOverlay,
    #[serde(default)]
    pub sources: BTreeMap<String, String>,
}
fn default_mjpeg_quality() -> u8 {
    90
}
impl VideoRequest {
    pub(super) fn document(&self) -> Result<SourceProjectDocument> {
        let bytes = match &self.document {
            Value::String(s) => s.as_bytes().to_vec(),
            v => serde_json::to_vec(v)?,
        };
        SourceProjectDocument::read(&bytes)
    }
    fn export_labels(
        &self,
        time: i64,
    ) -> Result<Vec<(crate::domain::filmcraft_export_text::TextOverlay, String)>> {
        let mut labels = Vec::new();
        if self.name_overlay.enabled && !self.name_overlay.text.trim().is_empty() {
            labels.push((
                self.name_overlay.clone(),
                self.name_overlay.text.trim().to_owned(),
            ));
        }
        if self.timecode_overlay.enabled {
            let document = self.document()?;
            let index = ProjectIndex::new(&document)?;
            let sequence = index
                .item(self.sequence_id.parse()?)
                .and_then(|v| v.pointer("/kind/Sequence"))
                .context("export sequence missing")?;
            let rate: filmcraft_time::FrameRate =
                serde_json::from_value(sequence["settings"]["frame_rate"].clone())?;
            let frame = rate
                .frame_at(filmcraft_time::Tick(time))
                .checked_add(
                    sequence["start_timecode"]
                        .as_i64()
                        .context("missing sequence start timecode")?,
                )
                .context("export timecode overflow")?;
            let tc = filmcraft_time::format_timecode_frames(
                frame,
                rate,
                sequence["settings"]["drop_frame"]
                    .as_bool()
                    .context("missing drop-frame setting")?,
            );
            let prefix = self.timecode_overlay.text.trim();
            labels.push((
                self.timecode_overlay.clone(),
                if prefix.is_empty() {
                    tc
                } else {
                    format!("{prefix} {tc}")
                },
            ));
        }
        Ok(labels)
    }
    fn encoding(&self, spec: RasterVideoSpec) -> Result<RasterEncoding> {
        if matches!(self.preset,ExportPreset::Hevc) {
            let bitrate=self.bitrate.unwrap_or_default().resolve(spec.width,spec.height,spec.frame_rate)?;
            ensure!(bitrate.mode!=crate::domain::filmcraft_export_h264::BitrateMode::Vbr2Pass,"HEVC has no two-pass mode");
            return Ok(RasterEncoding::NativeHevc {target_kbps:bitrate.target_kbps,max_kbps:bitrate.max_kbps,keyint:self.keyframe_distance.filter(|v|*v>0).unwrap_or_else(||(2.*spec.frame_rate.numerator as f64/spec.frame_rate.denominator as f64).round().max(1.)as u32),cbr:bitrate.mode==crate::domain::filmcraft_export_h264::BitrateMode::Cbr,multiplexer:self.multiplexer,audio:self.resolved_audio()?});
        }

        let mut encoding = self.preset.encoding(spec);
        if matches!(self.preset, ExportPreset::Mjpeg) {
            encoding = RasterEncoding::Mjpeg {
                quality: self.mjpeg_quality.clamp(1, 100),
            };
        }
        match &mut encoding {
            RasterEncoding::H264 { profile, .. }
            | RasterEncoding::H264TwoPass { profile, .. }
            | RasterEncoding::MxfH264 { profile, .. } => *profile = self.h264_profile,
            _ => {}
        }
        if let Some(distance) = self.keyframe_distance.filter(|v| *v > 0) {
            match &mut encoding {
                RasterEncoding::H264 { keyint, .. }
                | RasterEncoding::H264TwoPass { keyint, .. } => *keyint = distance,
                RasterEncoding::MxfH264 { keyint, .. } => *keyint = Some(distance),
                _ => {}
            }
        }
        if let Some(settings) = self.bitrate.filter(|_| self.preset.h264() || matches!(self.preset,ExportPreset::Hevc)) {
            let resolved = settings.resolve(spec.width, spec.height, spec.frame_rate)?;
            encoding = match encoding {
                RasterEncoding::H264 {
                    keyint, profile, ..
                }
                | RasterEncoding::H264TwoPass {
                    keyint, profile, ..
                } => RasterEncoding::H264 {
                    target_kbps: resolved.target_kbps,
                    max_kbps: resolved.max_kbps,
                    keyint,
                    profile,
                    mode: resolved.mode,
                },
                RasterEncoding::MxfH264 {
                    start,
                    drop_frame,
                    keyint,
                    profile,
                    ..
                } => RasterEncoding::MxfH264 {
                    start,
                    drop_frame,
                    keyint,
                    profile,
                    bitrate: Some(resolved),
                },
                other => other,
            };
        }
        if self.preset.h264() && !self.preset.native_mxf() {
            encoding = match encoding {
                RasterEncoding::H264 {
                    target_kbps,
                    max_kbps,
                    keyint,
                    profile,
                    mode,
                }
                | RasterEncoding::H264TwoPass {
                    target_kbps,
                    max_kbps,
                    keyint,
                    profile,
                    mode,
                } => RasterEncoding::NativeH264 {
                    target_kbps,
                    max_kbps,
                    keyint,
                    profile,
                    mode,
                    multiplexer: self.multiplexer,
                    audio: if self.has_audio() {
                        self.resolved_audio()?
                    } else {
                        self.h264_audio.unwrap_or_default()
                    },
                },
                other => other,
            };
        }
        Ok(encoding)
    }
    pub fn intra_movie(&self) -> bool {
        self.preset.prores_profile().is_some()
            || self.preset.native_dnx()
            || self.preset.native_apv()
            || matches!(self.preset, ExportPreset::Mjpeg)
    }
    pub fn h264_movie(&self) -> bool {
        (self.preset.h264() || matches!(self.preset,ExportPreset::Hevc)) && !self.preset.native_mxf()
    }
    pub fn extension(&self) -> &'static str {
        if self.h264_movie()
            && self.multiplexer == crate::domain::filmcraft_export_movie::Multiplexer::Mov
        {
            "mov"
        } else {
            self.preset.extension()
        }
    }
    fn configured_audio(&self) -> crate::domain::filmcraft_export_movie::AudioSettings {
        let base = if self.preset.audio_only() {
            crate::domain::filmcraft_export_movie::AudioSettings {
                bits: self.preset.pcm_preset().bits(),
                sample_rate: Some(48000),
                ..Default::default()
            }
        } else if self.preset.native_intra() {
            crate::domain::filmcraft_export_movie::AudioSettings {
                codec: crate::domain::filmcraft_export_movie::AudioCodec::Pcm,
                bits: 24,
                sample_rate: Some(48000),
                ..Default::default()
            }
        } else {
            self.h264_audio
                .unwrap_or(crate::domain::filmcraft_export_movie::AudioSettings {
                    sample_rate: Some(48000),
                    ..Default::default()
                })
        };
        let mut settings = self.audio.map_or(base, |settings| settings.apply(base));
        if self.preset.native_mxf()
            || (self.intra_movie()
                && settings.codec == crate::domain::filmcraft_export_movie::AudioCodec::Auto)
        {
            settings.codec = crate::domain::filmcraft_export_movie::AudioCodec::Pcm;
        }
        settings
    }
    pub fn audio_bits(&self) -> u16 {
        self.configured_audio().pcm_bits()
    }
    pub fn pcm_format(&self) -> crate::domain::filmcraft_pcm_export::PcmFormat {
        if matches!(self.preset, ExportPreset::Aiff16) {
            crate::domain::filmcraft_pcm_export::PcmFormat::Aiff
        } else {
            crate::domain::filmcraft_pcm_export::PcmFormat::Wav
        }
    }
    pub fn audio_codec(&self) -> &'static str {
        if self.preset.audio_only() {
            if matches!(self.preset, ExportPreset::Aiff16) {
                if self.audio_bits() == 24 {
                    "pcm_s24be"
                } else {
                    "pcm_s16be"
                }
            } else if self.audio_bits() == 24 {
                "pcm_s24le"
            } else {
                "pcm_s16le"
            }
        } else if self.h264_movie() || self.preset.native_intra() {
            if self
                .configured_audio()
                .codec(if self.preset.native_intra() {
                    crate::domain::filmcraft_export_movie::Multiplexer::Mov
                } else {
                    self.multiplexer
                })
                == crate::domain::filmcraft_export_movie::AudioCodec::Pcm
            {
                if self.audio_bits() == 24 {
                    "pcm_s24le"
                } else {
                    "pcm_s16le"
                }
            } else {
                "aac"
            }
        } else {
            self.preset.audio_codec()
        }
    }
    pub fn native_output(&self) -> bool {
        self.preset.native_output()
    }
    pub fn has_audio(&self) -> bool {
        self.with_audio || self.preset.audio_only()
    }
    fn resolved_audio(&self) -> Result<crate::domain::filmcraft_export_movie::H264AudioSettings> {
        let document = self.document()?;
        let index = ProjectIndex::new(&document)?;
        let sequence = index
            .item(self.sequence_id.parse()?)
            .and_then(|v| v.pointer("/kind/Sequence"))
            .context("export sequence missing")?;
        let sequence_rate = u32::try_from(
            sequence["settings"]["sample_rate"]
                .as_u64()
                .context("export sequence sample rate missing")?,
        )?;
        let mut settings = self.configured_audio();
        settings.sample_rate = Some(settings.resolve_sample_rate(sequence_rate));
        ensure!(
            (8000..=192000).contains(&settings.sample_rate.unwrap()),
            "unsupported export sample rate"
        );
        if self.preset.audio_only() || self.preset.native_mxf() {
            settings.codec = crate::domain::filmcraft_export_movie::AudioCodec::Pcm;
        } else if self.intra_movie() {
            if settings.codec == crate::domain::filmcraft_export_movie::AudioCodec::Auto {
                settings.codec = crate::domain::filmcraft_export_movie::AudioCodec::Pcm;
            }
            settings.validate_rate(crate::domain::filmcraft_export_movie::Multiplexer::Mov)?;
        } else {
            settings.validate_rate(self.multiplexer)?;
        }
        Ok(settings)
    }
    pub fn audio_channels(&self) -> Result<u32> {
        if self.h264_movie() || self.preset.native_intra() || self.preset.audio_only() {
            Ok(self.resolved_audio()?.channels())
        } else {
            Ok(2)
        }
    }
    pub fn audio_sample_rate(&self) -> Result<u32> {
        if self.h264_movie() || self.preset.native_intra() || self.preset.audio_only() {
            Ok(self.resolved_audio()?.sample_rate())
        } else {
            Ok(48000)
        }
    }
    pub(super) fn audio_range(&self, spec: RasterVideoSpec, start: i64) -> Result<(i64, u64)> {
        if let Some(range) = &self.source_range {
            return range.audio_span(self.audio_sample_rate()?);
        }
        let end = if self.frame_rate.is_some() && !self.preset.audio_only() {
            self.render_tick(spec, start, spec.frames)?
        } else {
            self.tick(spec, start, self.frames)?
        };
        let rate = i128::from(self.audio_sample_rate()?);
        let first = (i128::from(start) * rate).div_euclid(254_016_000_000);
        let last = (i128::from(end) * rate).div_euclid(254_016_000_000);
        let frames = u64::try_from(last - first)?;
        ensure!(frames > 0, "empty audio export range");
        Ok((
            i64::try_from(-((-first * 254_016_000_000).div_euclid(rate)))?,
            frames,
        ))
    }
    pub(super) fn geometry(
        &self,
        spec: RasterVideoSpec,
    ) -> Result<crate::domain::filmcraft_export_geometry::Geometry> {
        let document = self.document()?;
        let index = ProjectIndex::new(&document)?;
        let settings = &index
            .item(self.sequence_id.parse()?)
            .and_then(|v| v.pointer("/kind/Sequence"))
            .context("export sequence missing")?["settings"];
        let source = (
            usize::try_from(settings["width"].as_u64().context("export width missing")?)?,
            usize::try_from(
                settings["height"]
                    .as_u64()
                    .context("export height missing")?,
            )?,
        );
        crate::domain::filmcraft_export_geometry::Geometry::new(
            source,
            (spec.width, spec.height),
            self.frame_size
                .or(if self.scale.is_none() {
                    self.preset.frame_size()
                } else {
                    None
                })
                .is_some(),
            self.scaling.unwrap_or_else(|| self.preset.scaling()),
            self.pixel_aspect,
            self.max_render_quality,
        )
    }
    pub(super) fn spec(&self) -> Result<(RasterVideoSpec, i64)> {
        let exact_range = self
            .source_range
            .as_ref()
            .map(|range| range.bounds())
            .transpose()?;
        ensure!(
            exact_range.is_none()
                || (self.frames == 0 && self.start_ticks.is_none() && self.start_frames.is_none()),
            "sourceRange cannot be combined with legacy start/frames"
        );
        ensure!(
            self.scale
                .is_none_or(|scale| scale.is_finite() && scale > 0.),
            "export scale must be finite and positive"
        );
        ensure!(
            !self.preset.silent_visual() || !self.with_audio,
            "image sequences and GIF do not contain audio"
        );
        self.loudness.validate()?;
        self.name_overlay.validate()?;
        self.timecode_overlay.validate()?;
        self.image_overlay.validate()?;
        self.metadata.validate()?;
        self.video_limiter.validate()?;
        ensure!(self.sources.len() <= 64, "too many video sources");
        let document = self.document()?;
        let index = ProjectIndex::new(&document)?;
        let sequence = index
            .item(self.sequence_id.parse()?)
            .and_then(|v| v.pointer("/kind/Sequence"))
            .context("export sequence missing")?;
        let settings = &sequence["settings"];
        if let Some(kind) = self.caption_sidecar.as_deref() {
            if sequence["caption_tracks"]
                .as_array()
                .is_some_and(|tracks| !tracks.is_empty())
            {
                ensure!(
                    filmcraft_captions::Format::from_name(kind).is_some(),
                    "unknown caption sidecar format"
                );
            }
        }
        let rate: filmcraft_time::FrameRate =
            serde_json::from_value(settings["frame_rate"].clone())?;
        if self.preset.audio_only() {
            ensure!(
                rate.num > 0 && rate.den > 0,
                "invalid audio source frame rate"
            );
            let source_rate = FrameRateRatio {
                numerator: u32::try_from(rate.num)?,
                denominator: u32::try_from(rate.den)?,
            };
            let count = if let Some(range) = &self.source_range {
                range.video_span(source_rate)?.0
            } else {
                self.frames
            };
            ensure!((1..=360000).contains(&count), "invalid audio range");
            let spec = RasterVideoSpec {
                width: 1,
                height: 1,
                frames: count,
                frame_rate: source_rate,
                staging_bytes: 8 * 1024 * 1024 * 1024,
            };
            let start = if let Some((start, _)) = exact_range {
                start
            } else {
                ensure!(
                    self.start_ticks.is_some() != self.start_frames.is_some(),
                    "provide exactly one export start: ticks or frames"
                );
                match &self.start_ticks {
                    Some(value) => value.parse()?,
                    None => self.tick(spec, 0, 0)?,
                }
            };
            let (_, frames) = self.audio_range(spec, start)?;
            let channels = u128::from(self.audio_channels()?);
            let raw = u128::from(frames) * channels * 4;
            let payload = u128::from(frames) * channels * u128::from(self.audio_bits() / 8);
            let overhead = if matches!(self.preset, ExportPreset::Aiff16) {
                54 + (payload & 1)
            } else if channels == 6 {
                68
            } else {
                44
            };
            let staged =
                (raw + payload + overhead).max(if self.loudness.enabled { raw * 2 } else { 0 });
            ensure!(
                staged <= u128::from(spec.staging_bytes),
                "audio staging budget exceeded"
            );
            crate::domain::filmcraft_pcm_export::header(
                frames,
                self.audio_sample_rate()?,
                self.audio_channels()? as u16,
                self.audio_bits(),
                self.pcm_format(),
            )?;
            return Ok((spec, start));
        }
        let width = usize::try_from(settings["width"].as_u64().context("export width missing")?)?;
        let height = usize::try_from(
            settings["height"]
                .as_u64()
                .context("export height missing")?,
        )?;
        ensure!(
            width > 0 && height > 0 && width <= 3840 && height <= 2160,
            "export dimensions exceed 3840x2160"
        );
        let (width, height) = match self.frame_size.or(if self.scale.is_none() {
            self.preset.frame_size()
        } else {
            None
        }) {
            Some(size) => size,
            None => match self.scale {
                Some(scale) => crate::domain::filmcraft_export_geometry::scaled_source_size(
                    (width, height),
                    scale,
                )?,
                None => (width, height),
            },
        };
        ensure!(
            width > 0 && height > 0 && width <= 3840 && height <= 2160,
            "invalid export frame size"
        );
        let (width, height) = if self.preset.preserve_alpha() {
            (width, height)
        } else {
            crate::domain::filmcraft_export_geometry::video_dimensions(width, height)
        };
        let mut spec = RasterVideoSpec {
            width,
            height,
            frames: if exact_range.is_some() {
                1
            } else {
                self.frames
            },
            frame_rate: FrameRateRatio {
                numerator: u32::try_from(rate.num)?,
                denominator: u32::try_from(rate.den)?,
            },
            staging_bytes: 8 * 1024 * 1024 * 1024,
        };
        ensure!(
            exact_range.is_some() || (1..=360000).contains(&self.frames),
            "invalid raster frame count"
        );
        if self.frame_rate.is_none() {
            if matches!(self.preset, ExportPreset::Gif) {
                RasterVideoSpec { frames: 1, ..spec }.validate()?;
            } else {
                spec.validate()?;
            }
        } else {
            ensure!(rate.num > 0 && rate.den > 0, "invalid source frame rate");
        }
        self.geometry(spec)?;
        if let Some((n, d)) = self.pixel_aspect.filter(|_| !self.preset.native_intra()) {
            ensure!(
                n <= i32::MAX as u32 && d <= i32::MAX as u32,
                "pixel aspect exceeds encoder rational range"
            );
        }

        if let Some(range) = &self.source_range {
            let output_rate =
                self.frame_rate
                    .unwrap_or(if matches!(self.preset, ExportPreset::Gif) {
                        FrameRateRatio {
                            numerator: 15,
                            denominator: 1,
                        }
                    } else {
                        spec.frame_rate
                    });
            let (frames, aligned) = range.video_span(output_rate)?;
            spec.frames = frames;
            spec.frame_rate = output_rate;
            spec.validate()?;
            self.validate_av_staging(spec)?;
            return Ok((spec, aligned));
        }

        ensure!(
            self.start_ticks.is_some() != self.start_frames.is_some(),
            "provide exactly one video start: ticks or frames"
        );
        let start = match &self.start_ticks {
            Some(value) => value.parse()?,
            None => self.tick(spec, 0, 0)?,
        };
        self.tick(spec, start, self.frames - 1)?;
        if self.frame_rate.is_some() || matches!(self.preset, ExportPreset::Gif) {
            let end = self.tick(spec, start, self.frames)?;
            let output_rate = self.frame_rate.unwrap_or(FrameRateRatio {
                numerator: 15,
                denominator: 1,
            });
            let (frames, aligned) =
                crate::domain::filmcraft_export_clock::span(start, end, output_rate)?;
            spec.frames = frames;
            spec.frame_rate = output_rate;
            spec.validate()?;
            self.validate_av_staging(spec)?;
            return Ok((spec, aligned));
        }
        self.validate_av_staging(spec)?;
        Ok((spec, start))
    }
    fn validate_av_staging(&self, spec: RasterVideoSpec) -> Result<()> {
        if matches!(self.preset,ExportPreset::Hevc) {self.encoding(spec)?;}

        if let Some(settings) = self.bitrate.filter(|_| self.preset.h264() || matches!(self.preset,ExportPreset::Hevc)) {
            let bitrate = settings.resolve(spec.width, spec.height, spec.frame_rate)?;
            let estimate = (u128::from(bitrate.max_kbps)
                * 1000
                * u128::from(spec.frames)
                * u128::from(spec.frame_rate.denominator))
            .div_ceil(u128::from(spec.frame_rate.numerator) * 8);
            ensure!(
                estimate <= u128::from(spec.staging_bytes),
                "encoded H264 staging budget exceeded"
            );
        }
        if self.with_audio {
            let encoded_sample_bytes = match self.audio_codec() {
                "pcm_s24le" => 3,
                "pcm_s16le" | "pcm_s16be" => 2,
                "pcm_f32le" => 4,
                "aac" => 1,
                _ => 0,
            };
            let frame_bytes = crate::domain::filmcraft_export_movie::audio_staging_frame_bytes(
                self.audio_channels()?,
                encoded_sample_bytes,
                matches!(self.preset, ExportPreset::MxfAtomDnxhrHq),
                self.loudness.enabled,
            );
            let audio = spec
                .audio_frames_at_rate(self.audio_sample_rate()?)?
                .checked_mul(frame_bytes)
                .context("audio staging size overflow")?;
            let video = spec.width as u64 * spec.height as u64 * 4 * spec.frames;
            ensure!(
                audio
                    .checked_add(video)
                    .is_some_and(|n| n <= spec.staging_bytes),
                "audiovisual export exceeds staging budget"
            );
        }
        Ok(())
    }
    fn render_tick(&self, spec: RasterVideoSpec, start: i64, index: u64) -> Result<i64> {
        if self.source_range.is_some()
            || self.frame_rate.is_some()
            || matches!(self.preset, ExportPreset::Gif)
        {
            return crate::domain::filmcraft_export_clock::sample(spec.frame_rate, start, index);
        }
        self.tick(spec, start, index)
    }
    /// Source clocks use project ticks directly, never rounded microseconds.
    fn tick(&self, spec: RasterVideoSpec, start: i64, index: u64) -> Result<i64> {
        let frame = if let Some(value) = &self.start_frames {
            i128::from(value.parse::<i64>()?) + i128::from(index)
        } else {
            i128::from(index)
        };
        let delta = frame
            .checked_mul(254_016_000_000)
            .and_then(|v| v.checked_mul(i128::from(spec.frame_rate.denominator)))
            .context("video frame clock overflow")?
            .div_euclid(i128::from(spec.frame_rate.numerator));
        Ok(i64::try_from(if self.start_frames.is_some() {
            delta
        } else {
            i128::from(start) + delta
        })?)
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Snapshot {
    identities: BTreeMap<String, ([u8; 32], Option<[u8; 32]>)>,
    pub sources: std::collections::BTreeSet<String>,
}
pub(super) async fn snapshot(
    state: &AppState,
    headers: &HeaderMap,
    request: &VideoRequest,
    owner: Option<&str>,
) -> Result<Snapshot> {
    use sha2::Digest;
    let document = request.document()?;
    let index = ProjectIndex::new(&document)?;
    let mut result = BTreeMap::new();
    let mut sources = std::collections::BTreeSet::new();
    for (item, id) in &request.sources {
        if let Some(owner) = owner {
            super::stabilization_jobs::authorize_source_owner(state, id, owner).await
        } else {
            super::library::authorize_source_access(state, id, headers).await
        }
        .map_err(|e| anyhow::anyhow!("export access: {e:?}"))?;
        sources.insert(id.clone());
        let entry = state
            .library
            .get(id)
            .await
            .context("export source missing")?;
        let path = state.library.resolve_media_path(&entry).await?;
        let revision =
            crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path).await?;
        let fingerprint = crate::project_archive::fingerprint_file(&path).await?.1;
        ensure!(
            revision
                == crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path)
                    .await?,
            "export source changed while fingerprinting"
        );
        let mut hash = sha2::Sha256::new();
        hash.update(fingerprint.as_bytes());
        hash.update(revision.fingerprint());
        let root: [u8; 32] = hash.finalize().into();
        let media = index
            .resolve_media(item.parse()?)
            .filter(|m| m.item_id.to_string() == *item)
            .context("export binding must identify root media")?;
        let sequence = if media.media["info"]["kind"] == "ImageSequence" {
            let mut reader = super::filmcraft_image_sequence::catalog(
                state,
                headers,
                owner,
                &entry,
                CancellationToken::new(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("export sequence: {e:?}"))?;
            sources.extend(reader.member_ids().map(str::to_owned));
            Some(
                reader
                    .snapshot()
                    .await
                    .map_err(|e| anyhow::anyhow!("export snapshot: {e:?}"))?
                    .0,
            )
        } else {
            None
        };
        result.insert(item.clone(), (root, sequence));
    }
    if request.image_overlay.enabled && !request.preset.audio_only() {
        let id = &request.image_overlay.source_id;
        let path = image_overlay_path(state, headers, request, owner).await?;
        let revision =
            crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path).await?;
        let fingerprint = crate::project_archive::fingerprint_file(&path).await?.1;
        ensure!(
            revision
                == crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path)
                    .await?,
            "image overlay changed while fingerprinting"
        );
        let mut hash = sha2::Sha256::new();
        hash.update(fingerprint.as_bytes());
        hash.update(revision.fingerprint());
        result.insert("@image-overlay".into(), (hash.finalize().into(), None));
        sources.insert(id.clone());
    }
    Ok(Snapshot {
        identities: result,
        sources,
    })
}
async fn image_overlay_path(
    state: &AppState,
    headers: &HeaderMap,
    request: &VideoRequest,
    owner: Option<&str>,
) -> Result<std::path::PathBuf> {
    let id = &request.image_overlay.source_id;
    if let Some(owner) = owner {
        super::stabilization_jobs::authorize_source_owner(state, id, owner).await
    } else {
        super::library::authorize_source_access(state, id, headers).await
    }
    .map_err(|e| anyhow::anyhow!("image overlay access: {e:?}"))?;
    let entry = state
        .library
        .get(id)
        .await
        .filter(|e| e.kind == "source")
        .context("image overlay source missing")?;
    ensure!(
        entry.media_type.as_deref() != Some("video")
            && entry.media_type.as_deref() != Some("audio"),
        "image overlay requires an image"
    );
    let path = state.library.resolve_media_path(&entry).await?;
    ensure!(
        tokio::fs::metadata(&path).await?.len() <= 64 * 1024 * 1024,
        "image overlay file budget exceeded"
    );
    Ok(path)
}
/// Encodes the selected video preset and optional native stereo audio. Preserves output on refusal, checks
/// all source identities across the complete render, and publishes without overwrite.
pub(super) async fn encode_video_inner(
    state: &AppState,
    headers: &HeaderMap,
    request: &VideoRequest,
    output: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
    background: Option<super::filmcraft_project::BackgroundPreview>,
) -> Result<Option<crate::tools::native_loudness_export::LoudnessReport>> {
    ensure!(!cancel.is_cancelled(), "video export cancelled");
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("export deadline overflow")?;
    let (spec, start) = request.spec()?;
    let geometry = if request.preset.audio_only() {
        None
    } else {
        Some(request.geometry(spec)?)
    };
    let metadata = request.metadata.ffmpeg_tags();
    ensure!(!output.try_exists()?, "export output already exists");
    let owner = background.as_ref().map(|b| b.owner.as_str());
    let before = snapshot(state, headers, request, owner).await?;
    let image_overlay = if request.image_overlay.enabled && !request.preset.audio_only() {
        let path = image_overlay_path(state, headers, request, owner).await?;
        let settings = request.image_overlay.clone();
        let surface = state
            .cpu_pool
            .execute(cancel.child_token(), move |_| {
                crate::domain::group_media::decode_image_surface(&path)
            })
            .await
            .map_err(|e| anyhow::anyhow!("image overlay decode: {e:?}"))?;
        Some(std::sync::Arc::new((settings, surface)))
    } else {
        None
    };
    let parent = output.parent().context("export output parent missing")?;
    let staging = parent.join(format!(
        ".filmcraft-video-{}.{}",
        uuid::Uuid::new_v4(),
        request.extension()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(staging.clone());
    if request.preset.audio_only() {
        return Box::pin(async {
            let (audio_start, frames) = request.audio_range(spec, start)?;
            let phase = super::nested_export::ProgressLane::new(progress, 0., 0.8);
            let mixed = super::filmcraft_export_audio::prepare(
                state,
                request,
                audio_start,
                frames,
                parent,
                deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .context("audio export timeout")?,
                cancel,
                &phase.sender,
            )
            .await?;
            let phase = super::nested_export::ProgressLane::new(progress, 80., 0.2);
            crate::tools::native_pcm_export::encode_layout(
                &mixed.path,
                frames,
                request.preset.pcm_preset(),
                crate::tools::native_pcm_export::PcmLayout {
                    sample_rate: request.audio_sample_rate()?,
                    channels: request.audio_channels()? as u16,
                    bits: request.audio_bits(),
                },
                &staging,
                deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .context("audio export timeout")?,
                cancel,
                &phase.sender,
                state.cpu_pool.clone(),
            )
            .await?;
            ensure!(
                before == snapshot(state, headers, request, owner).await?,
                "audio sources changed during rendering"
            );
            ensure!(
                !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
                "audio export cancelled/timeout before publication"
            );
            tokio::fs::hard_link(&staging, output)
                .await
                .context("publish Filmcraft audio without overwrite")?;
            Ok(mixed.loudness)
        })
        .await;
    }
    let geometry = geometry.context("video geometry missing")?;
    let mixed = if request.with_audio {
        let (audio_start, audio_frames) = request.audio_range(spec, start)?;
        let phase = super::nested_export::ProgressLane::new(progress, 0., 0.3);
        Some(
            super::filmcraft_export_audio::prepare(
                state,
                request,
                audio_start,
                audio_frames,
                parent,
                deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .context("audio export timeout")?,
                cancel,
                &phase.sender,
            )
            .await?,
        )
    } else {
        None
    };
    let phase = super::nested_export::ProgressLane::new(
        progress,
        if request.with_audio { 30. } else { 0. },
        if request.with_audio { 0.7 } else { 1. },
    );
    let encoding = if request.preset.native_mxf() {
        let document = request.document()?;
        let index = ProjectIndex::new(&document)?;
        let sequence = index
            .item(request.sequence_id.parse()?)
            .and_then(|v| v.pointer("/kind/Sequence"))
            .context("export sequence missing")?;
        let rate = filmcraft_time::FrameRate::new(
            spec.frame_rate.numerator as i64,
            spec.frame_rate.denominator as i64,
        );
        let base = sequence["start_timecode"]
            .as_i64()
            .context("missing sequence start timecode")?;
        let (base, first) = if request.source_range.is_some() || request.frame_rate.is_some() {
            let source_rate: filmcraft_time::FrameRate =
                serde_json::from_value(sequence["settings"]["frame_rate"].clone())?;
            let ticks = i64::try_from(
                (i128::from(base) * 254_016_000_000 * i128::from(source_rate.den))
                    .div_euclid(i128::from(source_rate.num)),
            )?;
            (
                rate.frame_at(filmcraft_time::Tick(ticks)),
                crate::domain::filmcraft_export_clock::first_frame(spec.frame_rate, start)?,
            )
        } else {
            (base, rate.frame_at(filmcraft_time::Tick(start)))
        };
        let start = base
            .checked_add(first)
            .context("MXF start timecode overflow")?;
        let drop_frame = sequence["settings"]["drop_frame"]
            .as_bool()
            .context("missing drop frame setting")?
            && rate.supports_drop_frame();
        if matches!(request.preset, ExportPreset::MxfH264) {
            RasterEncoding::MxfH264 {
                start,
                drop_frame,
                keyint: request.keyframe_distance.filter(|v| *v > 0),
                profile: request.h264_profile,
                bitrate: request
                    .bitrate
                    .map(|s| s.resolve(spec.width, spec.height, spec.frame_rate))
                    .transpose()?,
            }
        } else if matches!(request.preset, ExportPreset::MxfAtomDnxhrHq) {
            RasterEncoding::MxfAtom { start, drop_frame }
        } else {
            RasterEncoding::Mxf {
                prores: matches!(request.preset, ExportPreset::MxfProresHq),
                start,
                drop_frame,
            }
        }
    } else {
        request.encoding(spec)?
    };
    encode_frames_with_source_range(&state.process_runtime,Path::new("ffmpeg"),&staging,spec,deadline.checked_duration_since(tokio::time::Instant::now()).context("export timed out")?,cancel,&phase.sender,mixed.as_ref().map(|audio|RasterAudio{path:&audio.path,frames:audio.frames}),encoding,request.pixel_aspect,&metadata,Some(state.cpu_pool.clone()),if request.preset.native_intra()&&request.has_audio(){Some(request.resolved_audio()?)}else{None},if request.preset.h264(){request.h264_level}else{None},request.source_range.clone(),|index,_|{
        let state=state.clone();let headers=headers.clone();let request=request.clone();let background=background.clone();let effect_pool=state.cpu_pool.clone();let effect_cancel=cancel.child_token();let image_overlay=image_overlay.clone();
        async move {
            let time=request.render_tick(spec,start,index)?;
            let labels=request.export_labels(time)?;
            let frame:PreviewFrameRequest=serde_json::from_value(serde_json::json!({"document":request.document,"sequenceId":request.sequence_id,"sources":request.sources,"timeTicks":time.to_string(),"captions":request.burn_captions.unwrap_or(true)}))?;
            let result=if !matches!(request.preset,ExportPreset::Lossless)||request.frame_size.is_some()||request.scale.is_some()||request.pixel_aspect.is_some()||request.max_render_quality||!labels.is_empty()||image_overlay.is_some(){super::filmcraft_project::render_video_frame(state,headers,frame,background,(geometry,!request.preset.preserve_alpha()),labels,image_overlay).await}else if let Some(background)=background {super::filmcraft_project::render_export_frame_owned(state,frame,background).await}else{render_export_frame(state,headers,frame).await};
            let mut surface=result.map_err(|e|anyhow::anyhow!("export frame: {e:?}"))?;
            if request.video_limiter.enabled {
                let _guard=effect_cancel.clone().drop_guard();let limiter=request.video_limiter;
                effect_pool.execute(effect_cancel.clone(),move |_|{
                    limiter.apply(&mut surface,||{ensure!(!effect_cancel.is_cancelled(),"video limiter cancelled");Ok(())})?;
                    Ok(surface)
                }).await.map_err(|e|anyhow::anyhow!("export video limiter: {e:?}"))
            }else{Ok(surface)}
        }
    }).await?;
    ensure!(
        !cancel.is_cancelled(),
        "export cancelled before publication"
    );
    ensure!(
        before == snapshot(state, headers, request, owner).await?,
        "export sources changed during rendering"
    );
    ensure!(
        tokio::time::Instant::now() < deadline && !cancel.is_cancelled(),
        "export timed out or cancelled before publication"
    );
    tokio::fs::hard_link(&staging, output)
        .await
        .context("publish Filmcraft video without overwrite")?;
    Ok(mixed.as_ref().and_then(|audio| audio.loudness))
}

/// Trusted foreground audiovisual entry; durable workers supply their persisted owner separately.
pub async fn encode_video(
    state: &AppState,
    headers: &HeaderMap,
    request: &VideoRequest,
    output: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<()> {
    Box::pin(encode_video_inner(
        state, headers, request, output, timeout, cancel, progress, None,
    ))
    .await
    .map(|_| ())
}
