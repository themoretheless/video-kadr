mod handlers;
mod library;
mod model;
mod state;
mod tools;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use state::{AppState, ToolInfo};

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

    // Probe external tools once so /api/health and the logs reflect reality.
    let (ffmpeg, ffmpeg_version) = tools::check_tool("ffmpeg", "-version").await;
    let (ytdlp, ytdlp_version) = tools::check_tool("yt-dlp", "--version").await;
    if ffmpeg {
        tracing::info!("ffmpeg: {}", ffmpeg_version.clone().unwrap_or_default());
    } else {
        tracing::error!("ffmpeg not found on PATH — rendering will fail. Install with: brew install ffmpeg");
    }
    if ytdlp {
        tracing::info!("yt-dlp: {}", ytdlp_version.clone().unwrap_or_default());
    } else {
        tracing::error!("yt-dlp not found on PATH — imports will fail. Install with: brew install yt-dlp");
    }
    let tool_info = ToolInfo { ffmpeg, ytdlp, ffmpeg_version, ytdlp_version };

    let max_concurrent: usize = std::env::var("MAX_CONCURRENT_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    let lib = library::Library::load(storage.clone()).await;
    let state = AppState::new(storage.clone(), max_concurrent, tool_info, lib);

    // Optional TTL cleanup of generated/downloaded files.
    let ttl_hours: u64 = std::env::var("FILE_TTL_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if ttl_hours > 0 {
        spawn_cleanup(storage.clone(), ttl_hours);
        tracing::info!("file cleanup enabled: TTL {ttl_hours}h");
    }

    // Upload limit for local files (default 2 GiB), overridable via env.
    let max_upload: usize = std::env::var("MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2 * 1024 * 1024 * 1024);

    let app = Router::new()
        .route("/api/import", post(handlers::import_handler))
        .route(
            "/api/upload",
            post(handlers::upload_handler).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route("/api/edit", post(handlers::edit_handler))
        .route("/api/jobs/:id", get(handlers::job_status_handler))
        .route("/api/jobs/:id/cancel", post(handlers::cancel_handler))
        .route("/api/library", get(handlers::library_list_handler))
        .route("/api/library/:id", axum::routing::delete(handlers::library_delete_handler))
        .route("/api/health", get(handlers::health_handler))
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
    let host = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".into());
    let ip: std::net::IpAddr = host.parse().unwrap_or_else(|_| [127, 0, 0, 1].into());
    let addr = SocketAddr::from((ip, port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("backend listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Resolve when the process receives Ctrl-C (SIGINT).
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

/// Periodically delete files in sources/ and outputs/ older than `ttl_hours`.
fn spawn_cleanup(storage: PathBuf, ttl_hours: u64) {
    tokio::spawn(async move {
        let ttl = Duration::from_secs(ttl_hours * 3600);
        let mut tick = tokio::time::interval(Duration::from_secs(30 * 60));
        loop {
            tick.tick().await;
            for sub in ["sources", "outputs"] {
                let dir = storage.join(sub);
                let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
                    continue;
                };
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.file_name().and_then(|n| n.to_str()) == Some(".gitkeep") {
                        continue;
                    }
                    let Ok(meta) = entry.metadata().await else { continue };
                    let Ok(modified) = meta.modified() else { continue };
                    let age = SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default();
                    if age > ttl && tokio::fs::remove_file(&path).await.is_ok() {
                        tracing::info!("cleanup removed {}", path.display());
                    }
                }
            }
        }
    });
}
