#![allow(dead_code)]

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use video_kadr_backend::build_router;
use video_kadr_backend::db::Db;
use video_kadr_backend::library::Library;
use video_kadr_backend::state::{AppState, ToolInfo};

pub const UPLOAD_LIMIT: usize = 64 * 1024 * 1024;

pub async fn make_state(ffmpeg: bool, ytdlp: bool) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let storage = dir.path().to_path_buf();
    for directory in ["sources", "outputs", "staging", "luts"] {
        tokio::fs::create_dir_all(storage.join(directory))
            .await
            .unwrap();
    }
    let library = Library::load(storage.clone()).await;
    let db = Db::open(&storage).await.unwrap();
    let tools = ToolInfo {
        ffmpeg,
        ytdlp,
        ffmpeg_version: None,
        ytdlp_version: None,
        ffmpeg_encoders: if ffmpeg {
            [
                "libx264",
                "libx265",
                "aac",
                "libvpx-vp9",
                "libopus",
                "libsvtav1",
                "prores_ks",
                "gif",
                "png",
                "mjpeg",
                "libmp3lame",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        } else {
            Vec::new()
        },
        ffmpeg_muxers: if ffmpeg {
            ["mp4", "webm", "mov", "gif", "image2", "mp3"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        },
        ffmpeg_filters: if ffmpeg {
            [
                "palettegen",
                "paletteuse",
                "hue",
                "colorchannelmixer",
                "colorbalance",
                "curves",
                "lut3d",
                "blend",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        } else {
            Vec::new()
        },
    };
    (AppState::new(storage, 2, tools, library, db), dir)
}

pub fn router(state: AppState) -> Router {
    build_router(state, UPLOAD_LIMIT)
}

pub fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

pub fn assert_api_error(body: &Value, code: &str) {
    assert_eq!(body["code"], code);
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "expected a non-empty API error, got: {body}"
    );
}

pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value, String) {
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, text)
}
