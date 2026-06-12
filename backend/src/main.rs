mod handlers;
mod model;
mod state;
mod tools;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let storage = std::env::var("STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("storage"));
    tokio::fs::create_dir_all(storage.join("sources")).await?;
    tokio::fs::create_dir_all(storage.join("outputs")).await?;

    let state = AppState::new(storage.clone());

    let app = Router::new()
        .route("/api/import", post(handlers::import_handler))
        .route("/api/edit", post(handlers::edit_handler))
        .route("/api/jobs/:id", get(handlers::job_status_handler))
        .route("/api/health", get(|| async { "ok" }))
        // Static file serving for both source and rendered videos. ServeDir
        // honours HTTP range requests, which the browser needs to seek videos.
        .nest_service("/files", ServeDir::new(&storage))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("backend listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
