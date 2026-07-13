//! Project endpoints: autosave/restore of a clip's edit recipe (SQLite-backed).

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::db::Project;
use crate::error::{ApiJson, AppError, AppResult};
use crate::model::WireSchemaVersion;
use crate::state::AppState;

const MAX_PROJECT_JSON_BYTES: usize = 64 * 1024;

struct ParsedProject {
    video_id: String,
    name: String,
    video: Value,
    edit: Value,
}

/// The wire envelope is strict so misspelled top-level fields fail fast. The
/// persisted `video` and `edit` documents remain intentionally open JSON: old
/// project versions must survive until the migration layer understands them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUpsertRequest {
    #[serde(rename = "schemaVersion", default)]
    _schema_version: WireSchemaVersion,
    #[serde(default)]
    video_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    video: Option<Value>,
    #[serde(default)]
    edit: Option<Value>,
}

/// `POST /api/projects` - create or update the saved project for a clip.
pub async fn project_upsert_handler(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ProjectUpsertRequest>,
) -> AppResult<Json<Project>> {
    let parsed = parse_project_body(body)?;
    let project = state
        .db
        .upsert_project(&parsed.video_id, &parsed.name, &parsed.video, &parsed.edit)
        .await
        .map_err(|error| AppError::internal("upsert project", error))?;
    Ok(Json(project))
}

fn parse_project_body(body: ProjectUpsertRequest) -> AppResult<ParsedProject> {
    let ProjectUpsertRequest {
        _schema_version: _,
        video_id,
        name,
        video,
        edit,
    } = body;
    let video_id = video_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("videoId обязателен"))?;
    let video = video.ok_or_else(|| AppError::bad_request("video обязателен"))?;
    if !video.is_object() {
        return Err(AppError::bad_request("video обязателен"));
    }
    let edit = edit.ok_or_else(|| AppError::bad_request("edit обязателен"))?;
    if !edit.is_object() {
        return Err(AppError::bad_request("edit обязателен"));
    }
    ensure_project_json_size("video", &video)?;
    ensure_project_json_size("edit", &edit)?;

    Ok(ParsedProject {
        video_id,
        name: resolve_project_name(name.as_deref(), &video),
        video,
        edit,
    })
}

fn resolve_project_name(name: Option<&str>, video: &Value) -> String {
    name.filter(|value| !value.trim().is_empty())
        .or_else(|| video.get("title").and_then(Value::as_str))
        .or_else(|| video.get("filename").and_then(Value::as_str))
        .unwrap_or("Без названия")
        .to_owned()
}

fn ensure_project_json_size(field: &str, value: &Value) -> AppResult<()> {
    let size = serde_json::to_vec(value)
        .map_err(|error| AppError::internal("serialize project field", error))?
        .len();
    if size > MAX_PROJECT_JSON_BYTES {
        return Err(AppError::payload_too_large(format!(
            "{field} больше {MAX_PROJECT_JSON_BYTES} байт"
        )));
    }
    Ok(())
}

/// `GET /api/projects` - list saved projects, most recently updated first.
pub async fn project_list_handler(State(state): State<AppState>) -> AppResult<Json<Vec<Project>>> {
    state
        .db
        .list_projects()
        .await
        .map(Json)
        .map_err(|error| AppError::internal("list projects", error))
}

/// `GET /api/projects/:id` - fetch one project by its id.
pub async fn project_get_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Project>> {
    match state.db.get_project(&id).await {
        Ok(Some(project)) => Ok(Json(project)),
        Ok(None) => Err(AppError::not_found("Проект не найден")),
        Err(error) => Err(AppError::internal("get project", error)),
    }
}

/// `GET /api/projects/by-video/:videoId` - fetch the saved project for a clip.
pub async fn project_by_video_handler(
    State(state): State<AppState>,
    AxPath(video_id): AxPath<String>,
) -> AppResult<Json<Project>> {
    match state.db.get_project_by_video(&video_id).await {
        Ok(Some(project)) => Ok(Json(project)),
        Ok(None) => Err(AppError::not_found("Проект не найден")),
        Err(error) => Err(AppError::internal("get project by video", error)),
    }
}

/// `DELETE /api/projects/:id` - remove a saved project.
pub async fn project_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    match state.db.delete_project(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(AppError::not_found("Проект не найден")),
        Err(error) => Err(AppError::internal("delete project", error)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn project_payload_parsing_is_separate_from_persistence() {
        let body = serde_json::from_value(json!({
            "videoId": "video-1",
            "video": { "filename": "clip.mp4" },
            "edit": { "mute": true }
        }))
        .unwrap();
        let parsed = parse_project_body(body).unwrap();

        assert_eq!(parsed.video_id, "video-1");
        assert_eq!(parsed.name, "clip.mp4");
        assert_eq!(parsed.edit["mute"], true);
    }
}
