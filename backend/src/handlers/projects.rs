//! Project endpoints: autosave/restore of a clip's edit recipe (SQLite-backed).

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::db::Project;
use crate::state::AppState;

const MAX_PROJECT_JSON_BYTES: usize = 64 * 1024;

/// `POST /api/projects` — create or update (keyed by `videoId`) the saved
/// project for a clip: its `video` metadata plus the `edit` recipe. Acts as the
/// autosave endpoint so reopening a clip restores the work.
pub async fn project_upsert_handler(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Project>, (StatusCode, String)> {
    let video_id = body["videoId"]
        .as_str()
        .ok_or((StatusCode::BAD_REQUEST, "videoId обязателен".to_string()))?;
    let video = body
        .get("video")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or((StatusCode::BAD_REQUEST, "video обязателен".to_string()))?;
    let edit = body
        .get("edit")
        .filter(|v| v.is_object())
        .cloned()
        .ok_or((StatusCode::BAD_REQUEST, "edit обязателен".to_string()))?;
    ensure_project_json_size("video", &video)?;
    ensure_project_json_size("edit", &edit)?;
    let name = body["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| video["title"].as_str())
        .or_else(|| video["filename"].as_str())
        .unwrap_or("Без названия")
        .to_string();
    let project = state
        .db
        .upsert_project(video_id, &name, &video, &edit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(project))
}

fn ensure_project_json_size(field: &str, value: &Value) -> Result<(), (StatusCode, String)> {
    let size = serde_json::to_vec(value)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("{field}: {e}")))?
        .len();
    if size > MAX_PROJECT_JSON_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("{field} больше {MAX_PROJECT_JSON_BYTES} байт"),
        ));
    }
    Ok(())
}

/// `GET /api/projects` — list saved projects, most recently updated first.
pub async fn project_list_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<Project>>, (StatusCode, String)> {
    state
        .db
        .list_projects()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// `GET /api/projects/:id` — fetch one project by its id.
pub async fn project_get_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> Result<Json<Project>, StatusCode> {
    match state.db.get_project(&id).await {
        Ok(Some(p)) => Ok(Json(p)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// `GET /api/projects/by-video/:videoId` — fetch the saved project for a clip.
pub async fn project_by_video_handler(
    State(state): State<AppState>,
    AxPath(video_id): AxPath<String>,
) -> Result<Json<Project>, StatusCode> {
    match state.db.get_project_by_video(&video_id).await {
        Ok(Some(p)) => Ok(Json(p)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// `DELETE /api/projects/:id` — remove a saved project.
pub async fn project_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> StatusCode {
    match state.db.delete_project(&id).await {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
