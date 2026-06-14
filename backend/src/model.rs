use serde::{Deserialize, Serialize};

/// Status of an asynchronous job (import or edit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Error,
    Cancelled,
}

impl JobStatus {
    /// A job is terminal once it can no longer change state.
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Done | JobStatus::Error | JobStatus::Cancelled)
    }
}

/// A unit of background work tracked in memory and polled by the frontend.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub url: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
}

/// Request body for `POST /api/edit`. Field names arrive as camelCase from the
/// frontend (e.g. `videoId`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
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
#[derive(Debug, Deserialize)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

/// Crop rectangle in source pixels.
#[derive(Debug, Deserialize)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Target size. Use `-1` (or `-2`) for a dimension to keep aspect ratio.
#[derive(Debug, Deserialize)]
pub struct Scale {
    pub w: i32,
    pub h: i32,
}
