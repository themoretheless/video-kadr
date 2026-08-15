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
    /// Ordered keep-segments to extract and concatenate (overrides `trim` when
    /// non-empty). Client order is timeline order; duplicate and overlapping
    /// ranges intentionally repeat source material. Supported for MP4, WebM,
    /// AV1, and ProRes outputs; other formats reject the combination.
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
    /// Stereo balance, -1 = left and +1 = right.
    #[serde(default)]
    pub pan: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_eq: Option<AudioEqSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressor: Option<AudioCompressorSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limiter: Option<AudioLimiterSelection>,
    /// eq filter params: brightness -1..1, contrast/saturation around 1.0.
    #[serde(default)]
    pub brightness: f64,
    #[serde(default = "default_one")]
    pub contrast: f64,
    #[serde(default = "default_one")]
    pub saturation: f64,
    /// Selective HSL adjustments for the six deterministic FFmpeg colour
    /// ranges. Omitted/zero-valued bands are identity operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hsl: Option<HslAdjustmentsSelection>,
    /// Shadows/midtones/highlights RGB balance, matching FFmpeg colorbalance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_wheels: Option<ColorWheelsSelection>,
    /// Optional deterministic chroma key. The colour is a strict six-digit
    /// RGB hex value; similarity, edge blend and spill suppression are 0..=1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma_key: Option<ChromaKeySelection>,
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
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LutSelection {
    pub id: String,
    /// 0 = bypass, 1 = full LUT.
    #[serde(default = "default_one")]
    pub intensity: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaKeySelection {
    pub key_color: String,
    #[serde(default = "default_chroma_similarity")]
    pub similarity: f64,
    #[serde(default = "default_chroma_blend")]
    pub blend: f64,
    #[serde(default)]
    pub spill_suppression: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslBandSelection {
    #[serde(default)]
    pub hue: f64,
    #[serde(default)]
    pub saturation: f64,
    #[serde(default)]
    pub lightness: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslAdjustmentsSelection {
    #[serde(default)]
    pub red: HslBandSelection,
    #[serde(default)]
    pub yellow: HslBandSelection,
    #[serde(default)]
    pub green: HslBandSelection,
    #[serde(default)]
    pub cyan: HslBandSelection,
    #[serde(default)]
    pub blue: HslBandSelection,
    #[serde(default)]
    pub magenta: HslBandSelection,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorWheelSelection {
    #[serde(default)]
    pub red: f64,
    #[serde(default)]
    pub green: f64,
    #[serde(default)]
    pub blue: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorWheelsSelection {
    #[serde(default)]
    pub shadows: ColorWheelSelection,
    #[serde(default)]
    pub midtones: ColorWheelSelection,
    #[serde(default)]
    pub highlights: ColorWheelSelection,
    #[serde(default = "default_true")]
    pub preserve_luminosity: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioEqSelection {
    #[serde(default)]
    pub low_gain_db: f64,
    #[serde(default)]
    pub mid_gain_db: f64,
    #[serde(default)]
    pub high_gain_db: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioCompressorSelection {
    #[serde(default = "default_compressor_threshold_db")]
    pub threshold_db: f64,
    #[serde(default = "default_compressor_ratio")]
    pub ratio: f64,
    #[serde(default = "default_compressor_attack_ms")]
    pub attack_ms: f64,
    #[serde(default = "default_compressor_release_ms")]
    pub release_ms: f64,
    #[serde(default)]
    pub makeup_gain_db: f64,
}

impl Default for AudioCompressorSelection {
    fn default() -> Self {
        Self {
            threshold_db: default_compressor_threshold_db(),
            ratio: default_compressor_ratio(),
            attack_ms: default_compressor_attack_ms(),
            release_ms: default_compressor_release_ms(),
            makeup_gain_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioLimiterSelection {
    #[serde(default = "default_limiter_ceiling_db")]
    pub ceiling_db: f64,
    #[serde(default = "default_limiter_release_ms")]
    pub release_ms: f64,
}

impl Default for AudioLimiterSelection {
    fn default() -> Self {
        Self {
            ceiling_db: default_limiter_ceiling_db(),
            release_ms: default_limiter_release_ms(),
        }
    }
}

impl Default for ColorWheelsSelection {
    fn default() -> Self {
        Self {
            shadows: ColorWheelSelection::default(),
            midtones: ColorWheelSelection::default(),
            highlights: ColorWheelSelection::default(),
            preserve_luminosity: true,
        }
    }
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

fn default_true() -> bool {
    true
}

fn default_compressor_threshold_db() -> f64 {
    -18.0
}

fn default_compressor_ratio() -> f64 {
    3.0
}

fn default_compressor_attack_ms() -> f64 {
    20.0
}

fn default_compressor_release_ms() -> f64 {
    250.0
}

fn default_limiter_ceiling_db() -> f64 {
    -1.0
}

fn default_limiter_release_ms() -> f64 {
    50.0
}

fn default_chroma_similarity() -> f64 {
    0.1
}

fn default_chroma_blend() -> f64 {
    0.05
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
    fn edit_request_reads_selective_hsl_and_color_wheels() {
        let edit: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "hsl": {
                "red": {"hue": 12.0, "saturation": -0.25, "lightness": 0.1},
                "blue": {"hue": -8.0}
            },
            "colorWheels": {
                "shadows": {"red": 0.2, "blue": -0.1},
                "highlights": {"green": 0.15}
            }
        }))
        .unwrap();

        let hsl = edit.hsl.unwrap();
        assert_eq!(hsl.red.hue, 12.0);
        assert_eq!(hsl.red.saturation, -0.25);
        assert_eq!(hsl.blue.hue, -8.0);
        assert_eq!(hsl.green, HslBandSelection::default());
        let wheels = edit.color_wheels.unwrap();
        assert_eq!(wheels.shadows.red, 0.2);
        assert_eq!(wheels.shadows.blue, -0.1);
        assert_eq!(wheels.highlights.green, 0.15);
        assert!(wheels.preserve_luminosity);
    }

    #[test]
    fn edit_request_reads_deterministic_audio_dsp_defaults() {
        let edit: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "pan": -0.4,
            "audioEq": {"lowGainDb": 2.0},
            "compressor": {},
            "limiter": {}
        }))
        .unwrap();

        assert_eq!(edit.pan, -0.4);
        assert_eq!(edit.audio_eq.unwrap().low_gain_db, 2.0);
        assert_eq!(
            edit.compressor.unwrap(),
            AudioCompressorSelection::default()
        );
        assert_eq!(edit.limiter.unwrap(), AudioLimiterSelection::default());
    }

    #[test]
    fn edit_request_reads_chroma_key_and_applies_control_defaults() {
        let explicit: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "chromaKey": {
                "keyColor": "#12ab34",
                "similarity": 0.2,
                "blend": 0.08,
                "spillSuppression": 0.6
            }
        }))
        .unwrap();
        let chroma = explicit.chroma_key.unwrap();
        assert_eq!(chroma.key_color, "#12ab34");
        assert_eq!(chroma.similarity, 0.2);
        assert_eq!(chroma.blend, 0.08);
        assert_eq!(chroma.spill_suppression, 0.6);

        let defaults: EditRequest = serde_json::from_value(json!({
            "videoId": "x",
            "chromaKey": { "keyColor": "#00ff00" }
        }))
        .unwrap();
        let chroma = defaults.chroma_key.unwrap();
        assert_eq!(chroma.similarity, 0.1);
        assert_eq!(chroma.blend, 0.05);
        assert_eq!(chroma.spill_suppression, 0.0);
    }

    #[test]
    fn optional_color_grade_fields_do_not_change_default_serialization() {
        let e: EditRequest = serde_json::from_value(json!({ "videoId": "x" })).unwrap();
        let value = serde_json::to_value(e).unwrap();
        assert!(value.get("lut").is_none());
        assert!(value.get("curves").is_none());
        assert!(value.get("chromaKey").is_none());
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
