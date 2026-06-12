use serde::{Deserialize, Serialize};

/// Status of an asynchronous job (import or edit).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Error,
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
}

impl Job {
    pub fn pending(id: String) -> Self {
        Job { id, status: JobStatus::Pending, result: None, error: None }
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
    #[serde(default)]
    pub crop: Option<Crop>,
    #[serde(default)]
    pub scale: Option<Scale>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default = "default_speed")]
    pub speed: f64,
}

fn default_speed() -> f64 {
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
