//! Media library endpoints: list and delete persisted sources/outputs.

use axum::extract::{Path as AxPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::domain::artifact_graph::Fingerprint;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// `GET /api/library` — list persisted sources and outputs, newest first.
pub async fn library_list_handler(
    State(state): State<AppState>,
) -> Json<Vec<crate::library::MediaEntry>> {
    Json(state.library.list().await)
}

#[derive(Debug, Deserialize)]
pub struct LibrarySearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: u32,
}

pub async fn library_search_handler(
    State(state): State<AppState>,
    Query(query): Query<LibrarySearchQuery>,
) -> AppResult<Json<Vec<crate::ports::SearchHit>>> {
    let hits = state
        .media_search
        .search(&query.q, query.limit)
        .await
        .map_err(|error| AppError::internal("search media library", error))?;
    Ok(Json(hits))
}

/// `DELETE /api/library/:id` — remove a library entry and delete its file.
pub async fn library_delete_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<StatusCode> {
    let entry = state.library.get(&id).await;
    if state.library.remove(&id).await {
        if entry.as_ref().is_some_and(|entry| entry.kind == "source") {
            match state.derived_job_store.detach_media_project(&id).await {
                Ok(proxy_keys) => {
                    for key in proxy_keys {
                        if let Ok(key) = Fingerprint::parse(key) {
                            if let Err(error) =
                                super::derived::gc_proxy_if_unreferenced(&state, &key).await
                            {
                                tracing::warn!(proxy.key = %key, %error, "garbage collect detached proxy");
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(media.id = %id, %error, "detach deleted source media jobs")
                }
            }
        }
        if let Err(error) = state.media_index.remove(&id).await {
            tracing::warn!(media.id = %id, %error, "remove media from search index");
        }
        if let Some(entry) = entry.filter(|e| e.kind == "output") {
            let _ = state.db.cache_delete_filename(&entry.filename).await;
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Медиафайл не найден"))
    }
}

fn default_search_limit() -> u32 {
    20
}
