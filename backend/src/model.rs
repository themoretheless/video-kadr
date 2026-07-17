use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const WIRE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default)]
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

    /// Parse a token read back from the database (unknown -> Pending).
    pub fn from_token(s: &str) -> JobStatus {
        match s {
            "running" => JobStatus::Running,
            "done" => JobStatus::Done,
            "error" => JobStatus::Error,
            "cancelled" => JobStatus::Cancelled,
            "interrupted" => JobStatus::Interrupted,
            _ => JobStatus::Pending,
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
#[derive(Debug, Deserialize, Serialize)]
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

fn default_speed() -> f64 {
    1.0
}

fn default_one() -> f64 {
    1.0
}

/// Trim the source to the region `[start, end]` (seconds).
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

/// Crop rectangle in source pixels.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Target size. Use `-1` (or `-2`) for a dimension to keep aspect ratio.
#[derive(Debug, Deserialize, Serialize)]
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
        assert!(!JobStatus::Pending.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
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
