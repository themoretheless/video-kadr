//! Shared source-authorized endpoints for shot review and MCP compute tools.
use super::library::authorize_source_access;
use crate::{
    analysis::shots::{self, FrameComparison, ShotMeasurement, ShotScan, Span},
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Path as AxPath, Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasurementRequest {
    spans: Vec<Span>,
    expected_source_hash: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompareRequest {
    left_tick: u64,
    right_tick: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameQuery {
    tick: u64,
}

async fn source(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(std::path::PathBuf, crate::tools::ProbeInfo)> {
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Анализ сцен требует локальный FFmpeg",
        ));
    }
    authorize_source_access(state, id, headers).await?;
    let entry = state
        .library
        .get(id)
        .await
        .filter(|entry| entry.kind == "source")
        .ok_or_else(|| AppError::not_found("Источник не найден"))?;
    let path = state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|_| AppError::not_found("Медиафайл не найден"))?;
    let probe = crate::tools::probe_video(&state.process_runtime, &path)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if probe.width == 0 || probe.height == 0 || !probe.duration.is_finite() || probe.duration <= 0.0
    {
        return Err(AppError::bad_request(
            "Нужен видеоисточник с известной длительностью",
        ));
    }
    Ok((path, probe))
}
async fn cache_location(
    state: &AppState,
    path: &std::path::Path,
) -> AppResult<(String, std::path::PathBuf)> {
    let (_, hash) = crate::project_archive::fingerprint_file(path)
        .await
        .map_err(|error| AppError::internal("fingerprint shot source", error))?;
    let key = format!(
        "{:x}",
        Sha256::digest(format!(
            "shots-v1-rgbpng:{hash}:{}",
            state.tools.ffmpeg_version.as_deref().unwrap_or_default()
        ))
    );
    Ok((hash, state.storage.join("shot-analysis").join(key)))
}

async fn verify_source_hash(path: &std::path::Path, expected: &str) -> AppResult<()> {
    let (_, actual) = crate::project_archive::fingerprint_file(path)
        .await
        .map_err(|error| AppError::internal("verify shot source", error))?;
    if actual != expected {
        return Err(AppError::bad_request(
            "Источник изменился во время анализа; повторите анализ",
        ));
    }
    Ok(())
}

pub async fn library_shot_scan_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
) -> AppResult<Json<ShotScan>> {
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (path, probe) = source(&state, &id, &headers).await?;
    let duration = (probe.duration * 1_000_000.0).round() as u64;
    let (hash, dir) = cache_location(&state, &path).await?;
    let file = dir.join("scan.json");
    let lock = state
        .render_lock(&format!("shot-scan:{}", dir.display()))
        .await;
    let _guard = lock.lock().await;
    if let Some(scan) = shots::read_cache::<ShotScan>(&file).await {
        if shots::valid_cached_scan(&scan, &hash, duration) {
            return Ok(Json(scan));
        }
    }
    let scan = shots::scan_source(&state.process_runtime, &path, hash.clone(), duration)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    verify_source_hash(&path, &hash).await?;
    shots::write_cache(&file, &scan)
        .await
        .map_err(|error| AppError::internal("write shot scan", error))?;
    Ok(Json(scan))
}

pub async fn library_shot_measure_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<MeasurementRequest>,
) -> AppResult<Json<Vec<ShotMeasurement>>> {
    if request.spans.is_empty() || request.spans.len() > shots::MAX_MEASURE_SPANS {
        return Err(AppError::bad_request("Измерение принимает 1–64 сцены"));
    }
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (path, probe) = source(&state, &id, &headers).await?;
    let duration = (probe.duration * 1_000_000.0).round() as u64;
    for span in &request.spans {
        shots::validate_span(span, duration)
            .map_err(|error| AppError::bad_request(error.to_string()))?;
    }
    let (width, height) = shots::measurement_raster(probe.width, probe.height)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let (hash, dir) = cache_location(&state, &path).await?;
    if request
        .expected_source_hash
        .as_ref()
        .is_some_and(|expected| expected != &hash)
    {
        return Err(AppError::bad_request(
            "Источник изменился; повторите анализ сцен перед измерением",
        ));
    }
    let mut measurements = Vec::new();
    let mut pending_cache = Vec::new();
    for span in request.spans {
        let file = dir.join(format!(
            "stats-{}-{}.json",
            span.start_ticks, span.end_ticks
        ));
        let lock = state
            .render_lock(&format!("shot-stats:{}", file.display()))
            .await;
        let _guard = lock.lock().await;
        if let Some(stats) = shots::read_cache::<ShotMeasurement>(&file).await {
            if stats.span.start_ticks == span.start_ticks
                && stats.span.end_ticks == span.end_ticks
                && stats.brightness.is_finite()
                && (0.0..=1.0).contains(&stats.brightness)
                && stats.motion.is_finite()
                && (0.0..=1.0).contains(&stats.motion)
                && stats.sharpness.is_finite()
                && stats.sharpness >= 0.0
                && stats
                    .flags
                    .iter()
                    .all(|flag| matches!(flag.as_str(), "black" | "freeze" | "fade"))
            {
                measurements.push(stats);
                continue;
            }
        }
        let stats = shots::measure_span(&state.process_runtime, &path, span, width, height)
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        pending_cache.push((file, stats.clone()));
        measurements.push(stats);
    }
    verify_source_hash(&path, &hash).await?;
    for (file, stats) in pending_cache {
        shots::write_cache(&file, &stats)
            .await
            .map_err(|error| AppError::internal("write shot stats", error))?;
    }
    Ok(Json(measurements))
}

pub async fn library_compare_frames_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<CompareRequest>,
) -> AppResult<Json<FrameComparison>> {
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (path, probe) = source(&state, &id, &headers).await?;
    let duration = (probe.duration * 1_000_000.0).round() as u64;
    if request.left_tick >= duration || request.right_tick >= duration {
        return Err(AppError::bad_request("Кадры вне длительности источника"));
    }
    let (width, height) = shots::measurement_raster(probe.width, probe.height)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let left = shots::extract_rgb(
        &state.process_runtime,
        &path,
        request.left_tick,
        width,
        height,
    )
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    let right = shots::extract_rgb(
        &state.process_runtime,
        &path,
        request.right_tick,
        width,
        height,
    )
    .await
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    let similarity = shots::similarity(&left, &right);
    Ok(Json(FrameComparison {
        left_tick: request.left_tick,
        right_tick: request.right_tick,
        similarity,
        motion: 1.0 - similarity,
    }))
}

pub async fn library_analysis_frame_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<FrameQuery>,
) -> AppResult<Response> {
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (path, probe) = source(&state, &id, &headers).await?;
    if query.tick >= (probe.duration * 1_000_000.0).round() as u64 {
        return Err(AppError::bad_request("Кадр вне длительности источника"));
    }
    let (width, height) = shots::measurement_raster(probe.width, probe.height)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let image = shots::extract_rgb(&state.process_runtime, &path, query.tick, width, height)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| AppError::internal("encode analysis frame", error))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        output.into_inner(),
    )
        .into_response())
}
