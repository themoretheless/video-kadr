//! Library crate for the video-editor backend.
//!
//! `main.rs` is a thin wrapper around this crate. Exposing the modules and the
//! router builder as a library lets the integration tests in `tests/` drive the
//! real HTTP API with `tower::ServiceExt::oneshot`, without binding a socket.

pub mod analysis;
pub mod artifacts;
pub mod assets;
pub mod backup;
pub mod capabilities;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod handlers;
pub mod http;
pub mod jobs;
pub mod library;
pub mod luts;
pub mod model;
pub mod packaging;
pub mod ports;
pub mod privacy;
pub mod process_control;
pub mod render;
pub mod runtime;
pub mod services;
pub mod state;
pub mod telemetry;
pub mod tools;

use axum::extract::DefaultBodyLimit;
use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use config::CorsOrigins;
use http::ports::{RuntimeSystemPort, SqliteProjectPort};
use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served only from `sources/` and `outputs/` with HTTP range
/// support (the browser needs it to seek videos). The SQLite DB lives next to
/// those directories but is intentionally not reachable via `/files`.
pub fn build_router(state: AppState, max_upload: usize) -> Router {
    build_router_with_cors(state, max_upload, &CorsOrigins::default())
}

pub fn build_router_with_cors(
    state: AppState,
    max_upload: usize,
    cors_origins: &CorsOrigins,
) -> Router {
    let storage = state.storage.clone();
    let system_port = Arc::new(RuntimeSystemPort::new(state.tools.clone()));
    let project_port = Arc::new(SqliteProjectPort::new(state.db.clone()));
    let core_api = Router::new()
        .route("/import", post(handlers::import_handler))
        .route(
            "/upload",
            post(handlers::upload_handler).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route(
            "/luts",
            get(handlers::lut_list_handler)
                .post(handlers::lut_upload_handler)
                .layer(DefaultBodyLimit::max(handlers::MAX_LUT_BODY_BYTES)),
        )
        .route("/luts/:id", get(handlers::lut_get_handler))
        .route(
            "/assets",
            get(handlers::asset_list_handler)
                .post(handlers::asset_upload_handler)
                .layer(DefaultBodyLimit::max(handlers::MAX_ASSET_BODY_BYTES)),
        )
        .route(
            "/assets/:id",
            get(handlers::asset_get_handler).delete(handlers::asset_delete_handler),
        )
        .route("/edit", post(handlers::edit_handler))
        .route("/jobs/failed", get(handlers::failed_jobs_handler))
        .route("/jobs/registry", get(handlers::job_registry_handler))
        .route("/jobs/:id", get(handlers::job_status_handler))
        .route("/jobs/:id/cancel", post(handlers::cancel_handler))
        .route("/jobs/:id/retry", post(handlers::retry_job_handler))
        .route("/jobs/:id/discard", post(handlers::discard_job_handler))
        .route("/library", get(handlers::library_list_handler))
        .route("/library/search", get(handlers::library_search_handler))
        .route("/library/:id", delete(handlers::library_delete_handler))
        .with_state(state);
    let api = core_api
        .merge(http::system_router(system_port))
        .merge(http::project_router(project_port))
        .fallback(handlers::api_not_found_handler)
        .method_not_allowed_fallback(handlers::method_not_allowed_handler);

    let router = Router::new()
        .nest("/api", api)
        .nest_service("/files/sources", ServeDir::new(storage.join("sources")))
        .nest_service("/files/outputs", ServeDir::new(storage.join("outputs")))
        .nest_service("/files/assets", ServeDir::new(storage.join("assets")));
    http::policy::apply_public_layers(router, cors_layer(cors_origins))
}

fn cors_layer(origins: &CorsOrigins) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_header_values(origins)))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, telemetry::REQUEST_ID_HEADER])
        .expose_headers([telemetry::REQUEST_ID_HEADER])
}

fn cors_header_values(origins: &CorsOrigins) -> Vec<HeaderValue> {
    origins
        .iter()
        .map(|origin| {
            origin
                .parse()
                .expect("CorsOrigins guarantees valid ASCII HTTP origins")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origins_use_local_dev_defaults() {
        let origins = cors_header_values(&CorsOrigins::default());
        assert!(origins.contains(&HeaderValue::from_static("http://localhost:5173")));
        assert!(origins.contains(&HeaderValue::from_static("http://127.0.0.1:8088")));
        assert!(!origins.contains(&HeaderValue::from_static("*")));
    }
}
