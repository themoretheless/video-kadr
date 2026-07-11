//! Media library endpoints: list and delete persisted sources/outputs.

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Json;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// `GET /api/library` — list persisted sources and outputs, newest first.
pub async fn library_list_handler(
    State(state): State<AppState>,
) -> Json<Vec<crate::library::MediaEntry>> {
    Json(state.library.list().await)
}

/// `DELETE /api/library/:id` — remove a library entry and delete its file.
pub async fn library_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    let entry = state.library.get(&id).await;
    if state.library.remove(&id).await {
        if let Some(entry) = entry.filter(|e| e.kind == "output") {
            let _ = state.db.cache_delete_filename(&entry.filename).await;
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Медиафайл не найден"))
    }
}
