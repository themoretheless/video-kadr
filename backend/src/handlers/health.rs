//! Health endpoint: readiness plus external tool availability/versions.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

/// `GET /api/health` — readiness plus external tool availability/versions.
pub async fn health_handler(State(state): State<AppState>) -> Json<Value> {
    let t = &state.tools;
    Json(json!({
        "status": if t.ffmpeg && t.ytdlp { "ok" } else { "degraded" },
        "ffmpeg": t.ffmpeg,
        "ytdlp": t.ytdlp,
        "ffmpegVersion": t.ffmpeg_version,
        "ytdlpVersion": t.ytdlp_version,
    }))
}
