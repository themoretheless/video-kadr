use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const WIRE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WireSchemaVersion;

impl<'de> Deserialize<'de> for WireSchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u32::deserialize(deserializer)?;
        if version != WIRE_SCHEMA_VERSION {
            return Err(D::Error::custom(format!(
                "unsupported schemaVersion {version}; expected {WIRE_SCHEMA_VERSION}"
            )));
        }
        Ok(Self)
    }
}

impl Serialize for WireSchemaVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(WIRE_SCHEMA_VERSION)
    }
}

/// Status of an asynchronous job (import or edit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Error,
    Cancelled,
    /// The process restarted while this job was still in flight; it won't resume.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidJobStatus(String);

impl fmt::Display for InvalidJobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown job status token: {}", self.0)
    }
}

impl std::error::Error for InvalidJobStatus {}

impl JobStatus {
    /// A job is terminal once it can no longer change state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled | JobStatus::Interrupted
        )
    }

    /// Stable lowercase token used for the database and the JSON API.
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Error => "error",
            JobStatus::Cancelled => "cancelled",
            JobStatus::Interrupted => "interrupted",
        }
    }

    /// Parse a stable database token without hiding storage corruption.
    pub fn from_token(s: &str) -> Result<JobStatus, InvalidJobStatus> {
        match s {
            "pending" => Ok(JobStatus::Pending),
            "running" => Ok(JobStatus::Running),
            "done" => Ok(JobStatus::Done),
            "error" => Ok(JobStatus::Error),
            "cancelled" => Ok(JobStatus::Cancelled),
            "interrupted" => Ok(JobStatus::Interrupted),
            _ => Err(InvalidJobStatus(s.to_owned())),
        }
    }
}

/// A unit of background work tracked in memory and polled by the frontend.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Job {
    pub id: String,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 0..100. Omitted while unknown (e.g. before the child reports anything).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Coarse phase label: "queued" | "downloading" | "processing".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

impl Job {
    pub fn pending(id: String) -> Self {
        Job {
            id,
            status: JobStatus::Pending,
            result: None,
            error: None,
            progress: None,
            stage: None,
        }
    }
}

/// Request body for `POST /api/import`. `start`/`end` (seconds) optionally limit
/// the download to a section instead of fetching the whole (possibly very long)
/// video.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest {
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: WireSchemaVersion,
    pub url: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
}

/// Request body for `POST /api/edit`. Field names arrive as camelCase from the
/// frontend (e.g. `videoId`). It is also `Serialize` so a deserialized request
/// can be re-serialized canonically into the render-cache key.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditRequest {
    #[serde(default, skip_serializing)]
    pub schema_version: WireSchemaVersion,
    pub video_id: String,
    #[serde(default)]
    pub trim: Option<Trim>,
    /// Keep-segments to extract and concatenate (overrides `trim` when non-empty).
    /// Lets the user cut a piece out of the middle or stitch several ranges.
    #[serde(default)]
    pub segments: Option<Vec<Trim>>,
    #[serde(default)]
    pub crop: Option<Crop>,
    #[serde(default)]
    pub scale: Option<Scale>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default = "default_speed")]
    pub speed: f64,
    // --- round 2 effects ---
    /// Clockwise rotation in degrees: 0, 90, 180, 270.
    #[serde(default)]
    pub rotate: i32,
    #[serde(default)]
    pub flip_h: bool,
    #[serde(default)]
    pub flip_v: bool,
    /// Linear audio gain (1.0 = unchanged).
    #[serde(default = "default_one")]
    pub volume: f64,
    /// Fade in/out durations in seconds (0 = none), applied to video and audio.
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    /// Normalize loudness to a streaming target (EBU R128 via loudnorm).
    #[serde(default)]
    pub normalize_audio: bool,
    /// High-pass filter to cut low-frequency rumble/hum from the voice.
    #[serde(default)]
    pub highpass: bool,
    /// eq filter params: brightness -1..1, contrast/saturation around 1.0.
    #[serde(default)]
    pub brightness: f64,
    #[serde(default = "default_one")]
    pub contrast: f64,
    #[serde(default = "default_one")]
    pub saturation: f64,
    /// Named look: "grayscale" | "sepia" | "warm" | "cold".
    #[serde(default)]
    pub filter: Option<String>,
    /// Reverse the clip (buffers all frames; intended for short segments).
    #[serde(default)]
    pub reverse: bool,
    /// Output frame rate override.
    #[serde(default)]
    pub fps: Option<f64>,
    /// Hide a rectangular region with a filled box (privacy / censor).
    #[serde(default)]
    pub censor: Option<Crop>,
    #[serde(default)]
    pub censor_color: Option<String>,
    /// Darkened-edges vignette.
    #[serde(default)]
    pub vignette: bool,
    /// Spatial denoise (hqdn3d).
    #[serde(default)]
    pub denoise: bool,
    /// Sharpen amount (0 = off, ~0..3 luma_amount for unsharp).
    #[serde(default)]
    pub sharpen: f64,
    /// Film grain amount (0 = off, ~0..100 noise strength).
    #[serde(default)]
    pub grain: f64,
    /// Optional immutable LUT asset selected by id. The HTTP adapter resolves
    /// the id to a private filesystem path before the FFmpeg adapter runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lut: Option<LutSelection>,
    /// User-authored normalized tone curves. Missing channels are identities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves: Option<CustomCurves>,
    /// Letterbox/pillarbox to a target aspect like "9:16" (pad, no cropping).
    #[serde(default)]
    pub pad: Option<String>,
    // --- round 3 export options ---
    /// Output format: "mp4" (default) | "webm" | "gif" | "png" | "mp3".
    #[serde(default)]
    pub format: Option<String>,
    /// Video codec for mp4: "h264" (default) | "h265".
    #[serde(default)]
    pub codec: Option<String>,
    /// Quality as CRF (lower = better). Defaults per format/codec.
    #[serde(default)]
    pub quality: Option<u32>,
    // --- parity wave: every field below is optional and omitted when absent so
    // an untouched edit keeps today's exact canonical serialization ---
    /// Multi-source timeline. When non-empty it replaces `videoId` + `segments`
    /// as the source of the edit; `videoId` still names the primary source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clips: Option<Vec<Clip>>,
    /// Transition inserted between plain `segments` when `clips` is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_transition: Option<Transition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlays: Option<Vec<Overlay>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titles: Option<Vec<Title>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitles: Option<Subtitles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_tracks: Option<Vec<AudioTrack>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_dynamics: Option<AudioDynamics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    /// Keyframed speed multipliers over the output timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_ramps: Option<Vec<Keyframe>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reframe360: Option<Reframe360>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stabilize: Option<Stabilize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens_correction: Option<LensCorrection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_advanced: Option<ColorAdvanced>,
}

/// Interpolation between two keyframes, as sent on the wire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyframeInterpolation {
    Hold,
    #[default]
    Linear,
    Smooth,
}

impl KeyframeInterpolation {
    /// Stable wire token, matched by `domain::keyframes::Interpolation`.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Linear => "linear",
            Self::Smooth => "smooth",
        }
    }
}

/// One keyframe on the OUTPUT timeline: `t` seconds, `v` parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub t: f64,
    pub v: f64,
    #[serde(default)]
    pub interp: KeyframeInterpolation,
}

/// Colour wheel offsets/multipliers. Ranges are validated in the domain.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// A cross-fade between two neighbouring clips or segments.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transition {
    pub kind: String,
    pub duration: f64,
}

/// One clip on the multi-source timeline. `start`/`end` are in-points and
/// out-points inside the SOURCE, not on the output timeline.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clip {
    pub source_id: String,
    pub start: f64,
    pub end: f64,
    #[serde(default = "default_one")]
    pub speed: f64,
    #[serde(default = "default_one")]
    pub volume: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_in: Option<Transition>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaKey {
    pub color: String,
    pub similarity: f64,
    pub blend: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayAudio {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_one")]
    pub volume: f64,
}

/// Image or video overlay: watermark, logo, picture-in-picture, green screen.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Overlay {
    pub asset_id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    /// A null height keeps the source aspect ratio.
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default = "default_one")]
    pub opacity: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma_key: Option<ChromaKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<OverlayAudio>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleBox {
    pub color: String,
    pub opacity: f64,
    pub padding: f64,
}

/// Burned-in title or lower third.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Title {
    pub text: String,
    /// A null font asset selects the bundled default font.
    #[serde(default)]
    pub font_asset_id: Option<String>,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default = "default_white")]
    pub color: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_center")]
    pub align: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#box: Option<TitleBox>,
    #[serde(default)]
    pub border_width: f64,
    #[serde(default = "default_black")]
    pub border_color: String,
    #[serde(default)]
    pub shadow_x: f64,
    #[serde(default)]
    pub shadow_y: f64,
    #[serde(default = "default_black")]
    pub shadow_color: String,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default = "default_none_animation")]
    pub animation: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Subtitles {
    pub asset_id: String,
    #[serde(default)]
    pub burn_in: bool,
    #[serde(default = "default_subtitle_size")]
    pub font_size: f64,
    #[serde(default = "default_white")]
    pub color: String,
    #[serde(default)]
    pub outline_width: f64,
    #[serde(default = "default_bottom")]
    pub position: String,
    #[serde(default)]
    pub margin_v: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ducking {
    #[serde(default)]
    pub enabled: bool,
    pub threshold: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
}

/// One extra audio track laid onto the output timeline.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioTrack {
    pub asset_id: String,
    pub role: String,
    #[serde(default = "default_one")]
    pub gain: f64,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub source_start: f64,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub r#loop: bool,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ducking: Option<Ducking>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Compressor {
    pub threshold: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
    #[serde(default = "default_one")]
    pub makeup: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limiter {
    pub ceiling: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gate {
    pub threshold: f64,
    pub ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioDynamics {
    #[serde(default)]
    pub denoise: f64,
    #[serde(default)]
    pub dereverb: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressor: Option<Compressor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limiter: Option<Limiter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<Gate>,
    #[serde(default)]
    pub deesser: bool,
    /// Overrides the legacy `highpass` boolean when present.
    #[serde(default)]
    pub highpass_hz: Option<f64>,
    #[serde(default)]
    pub lowpass_hz: Option<f64>,
    #[serde(default = "default_audio_bitrate")]
    pub bitrate_kbps: u32,
    /// Multiplies `volume` over the output timeline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume_envelope: Vec<Keyframe>,
}

/// Animated transform: Ken Burns and animated reframing.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Motion {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zoom: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pan_x: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pan_y: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rotation: Vec<Keyframe>,
}

/// Insta360 / action-cam reframing of a spherical or fisheye source.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reframe360 {
    pub input_projection: String,
    pub output_projection: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fov: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub yaw: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pitch: Vec<Keyframe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roll: Vec<Keyframe>,
    #[serde(default = "default_reframe_width")]
    pub output_width: u32,
    #[serde(default = "default_reframe_height")]
    pub output_height: u32,
    #[serde(default)]
    pub horizon_lock: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stabilize {
    pub mode: String,
    #[serde(default = "default_smoothing")]
    pub smoothing: f64,
    #[serde(default)]
    pub zoom: f64,
    #[serde(default)]
    pub horizon_lock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LensCorrection {
    #[serde(default)]
    pub k1: f64,
    #[serde(default)]
    pub k2: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslBandAdjust {
    pub band: String,
    #[serde(default)]
    pub hue: f64,
    #[serde(default = "default_one")]
    pub saturation: f64,
    #[serde(default = "default_one")]
    pub luminance: f64,
}

/// Resolve-lite primary grade: white balance, exposure, wheels and HSL.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorAdvanced {
    #[serde(default)]
    pub temperature: f64,
    #[serde(default)]
    pub tint: f64,
    #[serde(default)]
    pub exposure: f64,
    #[serde(default)]
    pub highlights: f64,
    #[serde(default)]
    pub shadows: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lift: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gamma: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hsl: Vec<HslBandAdjust>,
}

fn default_font_size() -> f64 {
    48.0
}

fn default_subtitle_size() -> f64 {
    24.0
}

fn default_white() -> String {
    "#FFFFFF".into()
}

fn default_black() -> String {
    "#000000".into()
}

fn default_center() -> String {
    "center".into()
}

fn default_bottom() -> String {
    "bottom".into()
}

fn default_none_animation() -> String {
    "none".into()
}

fn default_audio_bitrate() -> u32 {
    128
}

fn default_reframe_width() -> u32 {
    1920
}

fn default_reframe_height() -> u32 {
    1080
}

fn default_smoothing() -> f64 {
    10.0
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LutSelection {
    pub id: String,
    /// 0 = bypass, 1 = full LUT.
    #[serde(default = "default_one")]
    pub intensity: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomCurves {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master: Option<Vec<CurvePoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub red: Option<Vec<CurvePoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub green: Option<Vec<CurvePoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blue: Option<Vec<CurvePoint>>,
}

fn default_speed() -> f64 {
    1.0
}

fn default_one() -> f64 {
    1.0
}

/// Trim the source to the region `[start, end]` (seconds).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

/// Crop rectangle in source pixels.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Target size. Use `-1` (or `-2`) for a dimension to keep aspect ratio.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scale {
    pub w: i32,
    pub h: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn job_status_terminal_only_for_finished() {
        assert!(JobStatus::Done.is_terminal());
        assert!(JobStatus::Error.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(JobStatus::Interrupted.is_terminal());
        assert!(!JobStatus::Pending.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }

    #[test]
    fn job_status_tokens_are_strict_and_round_trip() {
        let statuses = [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Done,
            JobStatus::Error,
            JobStatus::Cancelled,
            JobStatus::Interrupted,
        ];
        for status in statuses {
            assert_eq!(JobStatus::from_token(status.as_str()).unwrap(), status);
        }
        assert!(JobStatus::from_token("unknown").is_err());
        assert!(JobStatus::from_token("").is_err());
    }

    #[test]
    fn pending_job_serializes_minimally() {
        // None fields are skipped so the polling payload stays small.
        let v = serde_json::to_value(Job::pending("id1".into())).unwrap();
        assert_eq!(v["id"], "id1");
        assert_eq!(v["status"], "pending");
        assert!(v.get("result").is_none());
        assert!(v.get("error").is_none());
        assert!(v.get("progress").is_none());
        assert!(v.get("stage").is_none());
    }

    #[test]
    fn edit_request_applies_defaults() {
        let e: EditRequest = serde_json::from_value(json!({ "videoId": "abc" })).unwrap();
        assert_eq!(e.video_id, "abc");
        assert_eq!(e.speed, 1.0);
        assert_eq!(e.volume, 1.0);
        assert_eq!(e.contrast, 1.0);
        assert_eq!(e.saturation, 1.0);
        assert_eq!(e.brightness, 0.0);
        assert_eq!(e.rotate, 0);
        assert!(!e.mute);
        assert!(!e.flip_h);
        assert!(e.trim.is_none());
        assert!(e.segments.is_none());
        assert!(e.format.is_none());
    }

    #[test]
    fn edit_request_reads_camel_case() {
        let e: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "flipH": true,
            "fadeIn": 1.5,
            "censorColor": "white"
        }))
        .unwrap();
        assert!(e.flip_h);
        assert_eq!(e.fade_in, 1.5);
        assert_eq!(e.censor_color.as_deref(), Some("white"));
    }

    #[test]
    fn edit_request_reads_lut_and_custom_curves() {
        let e: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "lut": { "id": "lut-1", "intensity": 0.65 },
            "curves": {
                "master": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 0.9}],
                "red": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}]
            }
        }))
        .unwrap();

        assert_eq!(e.lut.as_ref().unwrap().id, "lut-1");
        assert_eq!(e.lut.as_ref().unwrap().intensity, 0.65);
        assert_eq!(e.curves.as_ref().unwrap().master.as_ref().unwrap().len(), 2);
        assert!(e.curves.as_ref().unwrap().green.is_none());
    }

    #[test]
    fn optional_color_grade_fields_do_not_change_default_serialization() {
        let e: EditRequest = serde_json::from_value(json!({ "videoId": "x" })).unwrap();
        let value = serde_json::to_value(e).unwrap();
        assert!(value.get("lut").is_none());
        assert!(value.get("curves").is_none());
    }

    /// Render-cache keys hash the canonical re-serialization of an
    /// `EditRequest`. Every field added for the parity wave is optional and
    /// skipped when absent, so an untouched edit must still produce exactly
    /// these bytes. Changing this string invalidates every cached render.
    const GOLDEN_DEFAULT_EDIT_REQUEST: &str = concat!(
        r#"{"videoId":"x","trim":null,"segments":null,"crop":null,"scale":null,"mute":false,"#,
        r#""speed":1.0,"rotate":0,"flipH":false,"flipV":false,"volume":1.0,"fadeIn":0.0,"#,
        r#""fadeOut":0.0,"normalizeAudio":false,"highpass":false,"brightness":0.0,"#,
        r#""contrast":1.0,"saturation":1.0,"filter":null,"reverse":false,"fps":null,"#,
        r#""censor":null,"censorColor":null,"vignette":false,"denoise":false,"sharpen":0.0,"#,
        r#""grain":0.0,"pad":null,"format":null,"codec":null,"quality":null}"#
    );

    #[test]
    fn untouched_edit_request_serializes_byte_identically() {
        let request: EditRequest = serde_json::from_value(json!({ "videoId": "x" })).unwrap();
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            GOLDEN_DEFAULT_EDIT_REQUEST
        );
    }

    #[test]
    fn import_request_optional_range() {
        let i: ImportRequest = serde_json::from_value(json!({ "url": "https://x/y" })).unwrap();
        assert_eq!(i.url, "https://x/y");
        assert!(i.start.is_none());
        assert!(i.end.is_none());

        let i2: ImportRequest =
            serde_json::from_value(json!({ "url": "u", "start": 1.0, "end": 2.0 })).unwrap();
        assert_eq!(i2.start, Some(1.0));
        assert_eq!(i2.end, Some(2.0));
    }

    #[test]
    fn wire_schema_version_is_optional_v1_and_rejects_future_versions() {
        assert!(serde_json::from_value::<ImportRequest>(json!({
            "url": "https://example.com/video"
        }))
        .is_ok());
        assert!(serde_json::from_value::<ImportRequest>(json!({
            "schemaVersion": 1,
            "url": "https://example.com/video"
        }))
        .is_ok());
        assert!(serde_json::from_value::<ImportRequest>(json!({
            "schemaVersion": 2,
            "url": "https://example.com/video"
        }))
        .is_err());
        assert!(serde_json::from_value::<EditRequest>(json!({
            "schemaVersion": 2,
            "videoId": "abc"
        }))
        .is_err());
    }
}
