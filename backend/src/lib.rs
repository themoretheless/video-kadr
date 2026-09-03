//! Library crate for the video-kadr backend.
//!
//! `main.rs` is a thin wrapper around this crate. Exposing the modules and the
//! router builder as a library lets the integration tests in `tests/` drive the
//! real HTTP API with `tower::ServiceExt::oneshot`, without binding a socket.

pub mod analysis;
pub mod artifacts;
pub mod backup;
pub mod capabilities;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod handlers;
pub mod http;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod luts;
pub mod model;
pub mod object_storage;
pub mod packaging;
pub mod ports;
pub mod privacy;
pub mod process_control;
pub mod project_archive;
pub mod reliability;
pub mod render;
pub mod runtime;
pub mod services;
pub mod state;
pub mod stock_catalog;
pub mod telemetry;
pub mod tools;
pub mod youtube;

use axum::extract::DefaultBodyLimit;
use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use config::CorsOrigins;
use http::ports::{
    RuntimeSystemPort, SqliteAuthPort, SqliteCompositionProjectPort, SqliteProjectPort,
    SqliteProjectReviewPort, SqliteSpacePort,
};
use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served only from `sources/`, `outputs/`, and the isolated
/// proxy media directory with HTTP range support. Proxy manifests and the
/// SQLite DB live outside these mounts and are intentionally unreachable.
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
    let auth_port = Arc::new(SqliteAuthPort::new(state.db.clone()));
    let project_port = Arc::new(SqliteProjectPort::new(state.db.clone()));
    let composition_project_port = Arc::new(SqliteCompositionProjectPort::new(
        state.db.clone(),
        state.library.clone(),
    ));
    let project_review_port = Arc::new(SqliteProjectReviewPort::new(state.db.clone()));
    let space_port = Arc::new(SqliteSpacePort::new(
        state.db.clone(),
        state.library.clone(),
    ));
    let source_files = Router::new()
        .route(
            "/files/sources/:filename",
            get(handlers::source_file_handler),
        )
        .with_state(state.clone());
    let metrics = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(state.clone());
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
        .route("/edit", post(handlers::edit_handler))
        .route(
            "/compositions/render",
            post(handlers::composition_render_handler),
        )
        .route("/jobs/failed", get(handlers::failed_jobs_handler))
        .route("/jobs/registry", get(handlers::job_registry_handler))
        .route("/jobs/:id", get(handlers::job_status_handler))
        .route("/jobs/:id/cancel", post(handlers::cancel_handler))
        .route("/jobs/:id/retry", post(handlers::retry_job_handler))
        .route("/jobs/:id/discard", post(handlers::discard_job_handler))
        .route("/library", get(handlers::library_list_handler))
        .route("/library/search", get(handlers::library_search_handler))
        .route("/stock/search", get(handlers::stock_search_handler))
        .route(
            "/publish/youtube/status",
            get(handlers::youtube_status_handler),
        )
        .route(
            "/publish/youtube/connect",
            post(handlers::youtube_connect_handler).delete(handlers::youtube_disconnect_handler),
        )
        .route(
            "/publish/youtube/callback",
            get(handlers::youtube_callback_handler),
        )
        .route("/publish/youtube", post(handlers::youtube_publish_handler))
        .route(
            "/library/:id/thumbnail",
            get(handlers::library_thumbnail_handler),
        )
        .route(
            "/library/:id/thumbnail/:key",
            get(handlers::library_thumbnail_version_handler),
        )
        .route(
            "/library/:id/filmstrip",
            get(handlers::library_filmstrip_handler),
        )
        .route(
            "/library/:id/filmstrip/:key",
            get(handlers::library_filmstrip_version_handler),
        )
        .route("/library/:id", delete(handlers::library_delete_handler))
        .route(
            "/library/:id/proxies",
            get(handlers::proxy_list_handler).post(handlers::proxy_create_handler),
        )
        .route(
            "/library/:id/proxies/:key",
            delete(handlers::proxy_delete_handler),
        )
        .route(
            "/library/:id/proxies/:key/content",
            get(handlers::proxy_content_handler),
        )
        .route(
            "/library/:id/metadata",
            patch(handlers::library_metadata_patch_handler)
                .put(handlers::library_metadata_put_handler),
        )
        .route(
            "/composition-projects/import",
            post(handlers::composition_project_archive_import_handler).layer(
                DefaultBodyLimit::max(project_archive::MAX_ARCHIVE_MULTIPART_BYTES),
            ),
        )
        .route(
            "/composition-projects/:id/archive",
            get(handlers::composition_project_archive_export_handler),
        )
        .with_state(state);
    let api = core_api
        .merge(http::system_router(system_port))
        .merge(http::auth_router(auth_port))
        .merge(http::project_router(project_port))
        .merge(http::composition_project_router(composition_project_port))
        .merge(http::project_review_router(project_review_port))
        .merge(http::space_router(space_port))
        .fallback(handlers::api_not_found_handler)
        .method_not_allowed_fallback(handlers::method_not_allowed_handler);

    let router = Router::new()
        .merge(metrics)
        .merge(source_files)
        .nest("/api", api)
        .nest_service("/files/outputs", ServeDir::new(storage.join("outputs")));
    http::policy::apply_public_layers(router, cors_layer(cors_origins))
}

fn cors_layer(origins: &CorsOrigins) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_header_values(origins)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            telemetry::REQUEST_ID_HEADER,
            header::HeaderName::from_static("x-space-id"),
        ])
        .expose_headers([telemetry::REQUEST_ID_HEADER, header::CONTENT_DISPOSITION])
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
