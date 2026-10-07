use std::future::IntoFuture;
use std::net::SocketAddr;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use video_kadr_backend::build_router_with_cors;
use video_kadr_backend::config::AppConfig;
use video_kadr_backend::db::Db;
use video_kadr_backend::library::Library;
use video_kadr_backend::process_control::ProcessRuntime;
use video_kadr_backend::state::{AppState, ToolInfo};
use video_kadr_backend::tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    video_kadr_backend::telemetry::console::init(&config.console)?;
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

    let mut lib = Library::load(storage.clone()).await;
    if let Some(url) = config.object_store_url.as_deref() {
        let object_storage = video_kadr_backend::object_storage::SourceObjectStore::from_url(url)?;
        lib = lib.with_object_storage(object_storage);
        tracing::info!("S3-compatible source backup enabled");
    }
    let db = Db::open(&storage).await?;
    let mut state = AppState::new_with_resource_limits(
        storage.clone(),
        config.max_concurrent_jobs,
        tool_info,
        lib.clone(),
        db.clone(),
        config.encode_budget.clone(),
        config.cpu_queue_capacity,
        process_runtime.clone(),
        config.resource_classes,
    )?
    .with_workload_config(config.workload);
    if let Some(api_key) = config.pexels_api_key {
        state = state.with_stock_catalog(video_kadr_backend::stock_catalog::PexelsClient::new(
            api_key,
        )?);
        tracing::info!("Pexels stock catalog enabled");
    }
    if let (Some(config), Some(token_key)) = (config.youtube_oauth, config.youtube_token_key) {
        state = state.with_youtube_oauth(
            video_kadr_backend::youtube::YouTubeOAuthClient::new(config)?,
            video_kadr_backend::youtube::TokenCipher::new(token_key)?,
        );
        tracing::info!("YouTube OAuth publishing enabled");
    }
    // Reconcile durable jobs and rebuild derived state before workers can add
    // new media; incremental indexing owns every change after this boundary.
    state.recover_jobs().await;
    state.sync_media_search().await;
    state.cleanup_thumbnail_cache().await;
    if state.library.has_object_storage() {
        let backup_library = state.library.clone();
        let backup_shutdown = state.shutdown_token();
        state.spawn_task(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = backup_shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        let report = backup_library.sync_source_backups().await;
                        tracing::info!(
                            uploaded = report.uploaded,
                            unchanged = report.unchanged,
                            failed = report.failed,
                            "source backup reconciliation finished"
                        );
                    }
                }
            }
        });
    }
    video_kadr_backend::handlers::start_job_dispatcher(&state);
    state.spawn_task(video_kadr_backend::jobs::run_quarantine_cleanup(
        state.job_store.clone(),
        state.shutdown_token(),
    ));

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
