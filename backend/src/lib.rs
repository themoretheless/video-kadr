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
use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served only from `sources/` and `outputs/` with HTTP range
/// support (the browser needs it to seek videos). The SQLite DB lives next to
/// those directories but is intentionally not reachable via `/files`.
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
        .nest_service("/files/sources", ServeDir::new(storage.join("sources")))
        .nest_service("/files/outputs", ServeDir::new(storage.join("outputs")))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer())
        .with_state(state)
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins_from_env()))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE])
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
        tracing::warn!("ignoring invalid CORS origin {origin:?}: not a URL");
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
        tracing::warn!("ignoring invalid CORS origin {origin:?}: expected http(s) origin");
        return None;
    }

    let normalized = origin.trim_end_matches('/');
    match HeaderValue::from_str(normalized) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("ignoring invalid CORS origin {origin:?}: {e}");
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
