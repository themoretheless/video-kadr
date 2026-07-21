use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use video_editor_backend::build_router_with_cors;
use video_editor_backend::config::AppConfig;
use video_editor_backend::db::Db;
use video_editor_backend::library::{Library, MediaEntry};
use video_editor_backend::process_control::ProcessRuntime;
use video_editor_backend::state::{AppState, ToolInfo};
use video_editor_backend::tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    let process_runtime = ProcessRuntime::new(config.process_runtime.clone())?;
    process_runtime.validate_deployment(config.bind_addr)?;
    tracing::info!(
        isolation.tier = %process_runtime.tier(),
        isolation.hard_memory = process_runtime.hard_memory_limit_enforced(),
        isolation.hard_pid_tree = process_runtime.hard_pid_limit_enforced(),
        "external process policy validated"
    );
    let storage = config.storage.clone();
    tokio::fs::create_dir_all(storage.join("sources")).await?;
    tokio::fs::create_dir_all(storage.join("outputs")).await?;
    tokio::fs::create_dir_all(storage.join("staging")).await?;
    tokio::fs::create_dir_all(storage.join("proxies")).await?;
    tokio::fs::create_dir_all(storage.join("artifacts")).await?;
    tokio::fs::create_dir_all(storage.join("luts")).await?;

    // Probe external tools once so /api/health and the logs reflect reality.
    let (ffmpeg, ffmpeg_version) = tools::check_tool(&process_runtime, "ffmpeg", "-version").await;
    let (ytdlp, ytdlp_version) = tools::check_tool(&process_runtime, "yt-dlp", "--version").await;
    if ffmpeg {
        tracing::info!("ffmpeg: {}", ffmpeg_version.clone().unwrap_or_default());
    } else {
        tracing::error!(
            "ffmpeg not found on PATH - rendering will fail. Install with: brew install ffmpeg"
        );
    }
    if ytdlp {
        tracing::info!("yt-dlp: {}", ytdlp_version.clone().unwrap_or_default());
    } else {
        tracing::error!(
            "yt-dlp not found on PATH - imports will fail. Install with: brew install yt-dlp"
        );
    }
    let (ffmpeg_encoders, ffmpeg_muxers, ffmpeg_filters) = if ffmpeg {
        tools::inspect_ffmpeg_support(&process_runtime).await
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let tool_info = ToolInfo {
        ffmpeg,
        ytdlp,
        ffmpeg_version,
        ytdlp_version,
        ffmpeg_encoders,
        ffmpeg_muxers,
        ffmpeg_filters,
    };

    let lib = Library::load(storage.clone()).await;
    let db = Db::open(&storage).await?;
    let state = AppState::new_with_process_runtime(
        storage.clone(),
        config.max_concurrent_jobs,
        tool_info,
        lib.clone(),
        db.clone(),
        config.encode_budget.clone(),
        config.cpu_queue_capacity,
        process_runtime.clone(),
    )?
    .with_workload_config(config.workload);
    // Reconcile durable jobs and rebuild derived state before workers can add
    // new media; incremental indexing owns every change after this boundary.
    state.recover_jobs().await;
    state.rebuild_media_search().await;
    video_editor_backend::handlers::start_job_dispatcher(&state);
    state.spawn_task(video_editor_backend::jobs::run_quarantine_cleanup(
        state.job_store.clone(),
        state.shutdown_token(),
    ));

    // Optional TTL cleanup of generated/downloaded files.
    let ttl_hours = config.file_ttl_hours;
    if ttl_hours > 0 {
        spawn_cleanup(&state, storage.clone(), lib, db, ttl_hours);
        tracing::info!("file cleanup enabled: TTL {ttl_hours}h");
    }

    // Upload limit for local files (default 2 GiB), overridable via env.
    let app = build_router_with_cors(state.clone(), config.max_upload_bytes, &config.cors_origins);

    let addr = SocketAddr::from((config.bind_addr, config.port));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(isolation.tier = %process_runtime.tier(), "backend listening on http://{addr}");
    let http_shutdown = CancellationToken::new();
    let http_shutdown_waiter = http_shutdown.clone();
    {
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                http_shutdown_waiter.cancelled().await;
            })
            .into_future();
        tokio::pin!(server);

        tokio::select! {
            result = &mut server => result?,
            _ = shutdown_signal() => {
                state.begin_shutdown();
                http_shutdown.cancel();
                match tokio::time::timeout(Duration::from_secs(30), &mut server).await {
                    Ok(result) => result?,
                    Err(_) => tracing::warn!("HTTP shutdown exceeded 30 seconds; continuing bounded shutdown"),
                }
            }
        }
    }
    state.begin_shutdown();
    if !state.wait_for_tasks(Duration::from_secs(30)).await {
        tracing::warn!("background task shutdown exceeded 30 seconds");
    }
    Ok(())
}

/// Resolve when the process receives Ctrl-C/SIGINT or SIGTERM.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown signal received");
}

/// Periodically delete files in sources/ and outputs/ older than `ttl_hours`.
fn spawn_cleanup(state: &AppState, storage: PathBuf, library: Library, db: Db, ttl_hours: u64) {
    let shutdown = state.shutdown_token();
    let media_index = state.media_index.clone();
    state.spawn_task(async move {
        let ttl = Duration::from_secs(ttl_hours * 3600);
        let mut tick = tokio::time::interval(Duration::from_secs(30 * 60));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tick.tick() => {}
            }
            let library_entries = library.list().await;
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
                    let Ok(meta) = entry.metadata().await else {
                        continue;
                    };
                    let Ok(modified) = meta.modified() else {
                        continue;
                    };
                    let age = SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default();
                    if age <= ttl {
                        continue;
                    }
                    let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    let entry = library_entries.iter().find(|entry| {
                        entry.kind_path_segment() == sub && entry.filename == filename
                    });
                    if let Some(entry) = entry {
                        let entry_id = entry.id.clone();
                        if library.remove(&entry_id).await {
                            if let Err(error) = media_index.remove(&entry_id).await {
                                tracing::warn!(media.id = %entry_id, %error, "cleanup search index");
                            }
                            if sub == "outputs" {
                                let _ = db.cache_delete_filename(filename).await;
                            }
                            tracing::info!("cleanup removed library entry {entry_id}");
                        }
                    } else if tokio::fs::remove_file(&path).await.is_ok() {
                        if sub == "outputs" {
                            let _ = db.cache_delete_filename(filename).await;
                        }
                        tracing::info!(subdirectory = sub, filename, "cleanup removed orphan");
                    }
                }
            }
        }
    });
}

trait CleanupEntryKind {
    fn kind_path_segment(&self) -> &'static str;
}

impl CleanupEntryKind for MediaEntry {
    fn kind_path_segment(&self) -> &'static str {
        if self.kind == "output" {
            "outputs"
        } else {
            "sources"
        }
    }
}
