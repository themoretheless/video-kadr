//! Library crate for the video-editor backend.
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
pub mod jobs;
pub mod library;
pub mod model;
pub mod packaging;
pub mod ports;
pub mod privacy;
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

use http::ports::{RuntimeSystemPort, SqliteProjectPort};
use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served only from `sources/` and `outputs/` with HTTP range
/// support (the browser needs it to seek videos). The SQLite DB lives next to
/// those directories but is intentionally not reachable via `/files`.
pub fn build_router(state: AppState, max_upload: usize) -> Router {
    let storage = state.storage.clone();
    let system_port = Arc::new(RuntimeSystemPort::new(state.tools.clone()));
    let project_port = Arc::new(SqliteProjectPort::new(state.db.clone()));
    let core_api = Router::new()
        .route("/import", post(handlers::import_handler))
        .route(
            "/upload",
            post(handlers::upload_handler).layer(DefaultBodyLimit::max(max_upload)),
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
        .nest_service("/files/outputs", ServeDir::new(storage.join("outputs")));
    http::policy::apply_public_layers(router, cors_layer())
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins_from_env()))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, telemetry::REQUEST_ID_HEADER])
        .expose_headers([telemetry::REQUEST_ID_HEADER])
}

fn cors_origins_from_env() -> Vec<HeaderValue> {
    cors_origins(std::env::var("CORS_ALLOW_ORIGINS").ok().as_deref())
}

fn cors_origins(configured: Option<&str>) -> Vec<HeaderValue> {
    let raw_origins: Vec<&str> = configured
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                "http://localhost:5173",
                "http://127.0.0.1:5173",
                "http://localhost:8088",
                "http://127.0.0.1:8088",
            ]
        });

    raw_origins
        .into_iter()
        .filter_map(parse_cors_origin)
        .collect()
}

fn parse_cors_origin(origin: &str) -> Option<HeaderValue> {
    if origin == "*" {
        tracing::warn!("ignoring wildcard CORS origin; configure explicit origins");
        return None;
    }

    let Ok(url) = url::Url::parse(origin) else {
        tracing::warn!(origin = %privacy::RedactedUrl(origin), "ignoring invalid CORS origin: not a URL");
        return None;
    };
    let valid_scheme = matches!(url.scheme(), "http" | "https");
    let has_host = url.host_str().is_some();
    let plain_origin = url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none();
    if !valid_scheme || !has_host || !plain_origin {
        tracing::warn!(origin = %privacy::RedactedUrl(origin), "ignoring invalid CORS origin: expected http(s) origin");
        return None;
    }

    let normalized = origin.trim_end_matches('/');
    match HeaderValue::from_str(normalized) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!(origin = %privacy::RedactedUrl(origin), error = %e, "ignoring invalid CORS origin");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origins_use_local_dev_defaults() {
        let origins = cors_origins(None);
        assert!(origins.contains(&HeaderValue::from_static("http://localhost:5173")));
        assert!(origins.contains(&HeaderValue::from_static("http://127.0.0.1:8088")));
        assert!(!origins.contains(&HeaderValue::from_static("*")));
    }

    #[test]
    fn cors_origins_reject_wildcard_and_invalid_values() {
        let origins = cors_origins(Some(
            "https://app.example, *, bad header, https://app.example/path",
        ));
        assert_eq!(
            origins,
            vec![HeaderValue::from_static("https://app.example")]
        );
    }
}
