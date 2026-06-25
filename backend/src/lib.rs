//! Library crate for the video-editor backend.
//!
//! `main.rs` is a thin wrapper around this crate. Exposing the modules and the
//! router builder as a library lets the integration tests in `tests/` drive the
//! real HTTP API with `tower::ServiceExt::oneshot`, without binding a socket.

pub mod db;
pub mod handlers;
pub mod library;
pub mod model;
pub mod state;
pub mod tools;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served from `state.storage` with HTTP range support (the
/// browser needs it to seek videos), so the storage path is read before the
/// state is moved into the router.
pub fn build_router(state: AppState, max_upload: usize) -> Router {
    let storage = state.storage.clone();
    Router::new()
        .route("/api/import", post(handlers::import_handler))
        .route(
            "/api/upload",
            post(handlers::upload_handler).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route("/api/edit", post(handlers::edit_handler))
        .route("/api/jobs/:id", get(handlers::job_status_handler))
        .route("/api/jobs/:id/cancel", post(handlers::cancel_handler))
        .route("/api/library", get(handlers::library_list_handler))
        .route("/api/library/:id", delete(handlers::library_delete_handler))
        .route(
            "/api/projects",
            post(handlers::project_upsert_handler).get(handlers::project_list_handler),
        )
        .route(
            "/api/projects/by-video/:videoId",
            get(handlers::project_by_video_handler),
        )
        .route(
            "/api/projects/:id",
            get(handlers::project_get_handler).delete(handlers::project_delete_handler),
        )
        .route("/api/health", get(handlers::health_handler))
        .nest_service("/files", ServeDir::new(&storage))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
