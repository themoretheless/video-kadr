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
use serde::{Deserialize, Serialize};
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
    let path = source_path(state, id, headers).await?;
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

async fn source_path(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<std::path::PathBuf> {
    authorize_source_access(state, id, headers).await?;
    let entry = state
        .library
        .get(id)
        .await
        .filter(|entry| entry.kind == "source")
        .ok_or_else(|| AppError::not_found("Источник не найден"))?;
    state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|_| AppError::not_found("Медиафайл не найден"))
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DescribeShotRequest {
    span: Span,
    expected_source_hash: String,
    #[serde(default)]
    focus: crate::analysis::scene_description::Focus,
    #[serde(default)]
    language: crate::analysis::scene_description::Language,
}

pub async fn library_shot_describe_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<DescribeShotRequest>,
) -> AppResult<Json<crate::analysis::scene_description::Description>> {
    use crate::analysis::scene_description::sample_ticks;

    super::publish::authenticated_actor(&state, &headers).await?;
    let config = state
        .scene_description
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Модель описания сцен не настроена"))?;
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (path, probe) = source(&state, &id, &headers).await?;
    let duration = (probe.duration * 1_000_000.0).round() as u64;
    shots::validate_span(&request.span, duration)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let (hash, directory) = cache_location(&state, &path).await?;
    if request.expected_source_hash != hash {
        return Err(AppError::bad_request(
            "Источник изменился; повторите анализ сцен",
        ));
    }

    let Span {
        start_ticks,
        end_ticks,
    } = request.span;
    let model_digest = scene_description_digest(config).await?;
    let key = config.cache_key(
        start_ticks,
        end_ticks,
        request.focus,
        request.language,
        model_digest.as_deref(),
    );
    let file = directory.join(format!("description-{key}.json"));
    let lock = state.render_lock(&file.to_string_lossy()).await;
    let _guard = lock.lock().await;
    authorize_source_access(&state, &id, &headers).await?;
    if let Some(result) = read_scene_description(&file).await {
        verify_source_hash(&path, &hash).await?;
        return Ok(Json(result));
    }

    let (width, height) = shots::measurement_raster(probe.width, probe.height)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut frames = Vec::new();
    for tick in sample_ticks(start_ticks, end_ticks) {
        let image = shots::extract_rgb(&state.process_runtime, &path, tick, width, height)
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| AppError::internal("encode scene sample", error))?;
        frames.push((tick - start_ticks, png.into_inner()));
    }
    verify_source_hash(&path, &hash).await?;
    authorize_source_access(&state, &id, &headers).await?;
    let result = config
        .describe(frames, request.focus, request.language)
        .await
        .map_err(|_| {
            AppError::service_unavailable("Модель описания сцен не вернула корректный ответ")
        })?;
    verify_source_hash(&path, &hash).await?;
    authorize_source_access(&state, &id, &headers).await?;
    if let Some(managed) = &config.managed {
        let installed = managed
            .installed_digest(&config.client)
            .await
            .map_err(|_| AppError::service_unavailable("Локальная служба Ollama недоступна"))?;
        if installed != model_digest {
            return Err(AppError::service_unavailable(
                "Локальная модель изменилась во время описания",
            ));
        }
    }
    let entry = serde_json::json!({
        "span": {"startTicks": start_ticks, "endTicks": end_ticks},
        "text": result.text,
        "tags": result.tags
    });
    shots::write_cache(&file, &entry)
        .await
        .map_err(|error| AppError::internal("write scene description", error))?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadSceneDescriptions {
    spans: Vec<Span>,
    expected_source_hash: String,
    #[serde(default)]
    focus: crate::analysis::scene_description::Focus,
    #[serde(default)]
    language: crate::analysis::scene_description::Language,
}

#[derive(Serialize)]
pub struct CachedSceneDescription {
    span: Span,
    description: crate::analysis::scene_description::Description,
}

pub async fn library_shot_descriptions_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<ReadSceneDescriptions>,
) -> AppResult<Json<Vec<CachedSceneDescription>>> {
    super::publish::authenticated_actor(&state, &headers).await?;
    if request.spans.is_empty()
        || request.spans.len() > 128
        || request
            .spans
            .iter()
            .any(|span| span.start_ticks >= span.end_ticks)
    {
        return Err(AppError::bad_request(
            "Чтение описаний принимает 1–128 непустых интервалов",
        ));
    }
    let path = source_path(&state, &id, &headers).await?;
    let Some(config) = state.scene_description.as_ref().as_ref() else {
        return Ok(Json(Vec::new()));
    };
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (hash, directory) = cache_location(&state, &path).await?;
    if request.expected_source_hash != hash {
        return Err(AppError::bad_request(
            "Источник изменился; повторите анализ сцен",
        ));
    }
    let digest = scene_description_digest(config).await?;
    let mut results = Vec::new();
    for span in request.spans {
        let key = config.cache_key(
            span.start_ticks,
            span.end_ticks,
            request.focus,
            request.language,
            digest.as_deref(),
        );
        if let Some(description) =
            read_scene_description(&directory.join(format!("description-{key}.json"))).await
        {
            results.push(CachedSceneDescription { span, description });
        }
    }
    authorize_source_access(&state, &id, &headers).await?;
    verify_source_hash(&path, &hash).await?;
    Ok(Json(results))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDescriptionCatalogQuery {
    #[serde(default)]
    focus: crate::analysis::scene_description::Focus,
    #[serde(default)]
    language: crate::analysis::scene_description::Language,
    start_ticks: Option<u64>,
    end_ticks: Option<u64>,
    cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDescriptionCatalog {
    source_hash: String,
    descriptions: Vec<CachedSceneDescription>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct CatalogEntry {
    span: Span,
    #[serde(flatten)]
    description: crate::analysis::scene_description::Description,
}

pub async fn library_scene_description_catalog(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    Query(query): Query<SceneDescriptionCatalogQuery>,
) -> AppResult<Json<SceneDescriptionCatalog>> {
    super::publish::authenticated_actor(&state, &headers).await?;
    let exact = match (query.start_ticks, query.end_ticks) {
        (None, None) => None,
        (Some(start_ticks), Some(end_ticks))
            if start_ticks < end_ticks && query.cursor.is_none() =>
        {
            Some(Span {
                start_ticks,
                end_ticks,
            })
        }
        _ => {
            return Err(AppError::bad_request(
                "Нужен непустой интервал исходника без курсора",
            ))
        }
    };
    let cursor = query
        .cursor
        .as_deref()
        .map(|cursor| {
            let invalid = || AppError::bad_request("Некорректный курсор описаний");
            let (start, end) = cursor.split_once('-').ok_or_else(invalid)?;
            let start = start.parse::<u64>().map_err(|_| invalid())?;
            let end = end.parse::<u64>().map_err(|_| invalid())?;
            if start >= end {
                return Err(invalid());
            }
            Ok((start, end))
        })
        .transpose()?;

    let path = source_path(&state, &id, &headers).await?;
    let _permit = state
        .acquire_job_slot(ResourceClass::Analysis)
        .await
        .map_err(|_| AppError::service_unavailable("Анализ останавливается"))?;
    let (hash, directory) = cache_location(&state, &path).await?;
    let mut page = SceneDescriptionCatalog {
        source_hash: hash,
        descriptions: Vec::new(),
        next_cursor: None,
    };
    let Some(config) = state.scene_description.as_ref().as_ref() else {
        return Ok(Json(page));
    };
    let digest = scene_description_digest(config).await?;
    let file_name = |span: &Span| {
        let key = config.cache_key(
            span.start_ticks,
            span.end_ticks,
            query.focus,
            query.language,
            digest.as_deref(),
        );
        format!("description-{key}.json")
    };
    if let Some(span) = exact {
        if let Some(description) = read_scene_description(&directory.join(file_name(&span))).await {
            page.descriptions
                .push(CachedSceneDescription { span, description });
        }
    } else {
        const PAGE_SIZE: usize = 128;
        let mut listing = match tokio::fs::read_dir(&directory).await {
            Ok(listing) => listing,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Json(page)),
            Err(error) => return Err(AppError::internal("list scene descriptions", error)),
        };
        let mut matches = std::collections::BTreeMap::new();
        while let Some(file) = listing
            .next_entry()
            .await
            .map_err(|error| AppError::internal("read scene description catalog", error))?
        {
            let name = file.file_name();
            if !name.to_string_lossy().starts_with("description-") {
                continue;
            }
            let Some(entry) = shots::read_cache::<CatalogEntry>(&file.path()).await else {
                continue;
            };
            let key = (entry.span.start_ticks, entry.span.end_ticks);
            if key.0 >= key.1
                || cursor.is_some_and(|cursor| key <= cursor)
                || name != file_name(&entry.span).as_str()
                || entry.description.validate().is_err()
            {
                continue;
            }
            matches.insert(
                key,
                CachedSceneDescription {
                    span: entry.span,
                    description: entry.description,
                },
            );
            if matches.len() > PAGE_SIZE + 1 {
                matches.pop_last();
            }
        }
        if matches.len() > PAGE_SIZE {
            matches.pop_last();
            if let Some(((start, end), _)) = matches.last_key_value() {
                page.next_cursor = Some(format!("{start}-{end}"));
            }
        }
        page.descriptions = matches.into_values().collect();
    }
    authorize_source_access(&state, &id, &headers).await?;
    verify_source_hash(&path, &page.source_hash).await?;
    Ok(Json(page))
}

async fn scene_description_digest(
    config: &crate::analysis::scene_description::Config,
) -> AppResult<Option<String>> {
    let Some(managed) = &config.managed else {
        return Ok(None);
    };
    managed
        .installed_digest(&config.client)
        .await
        .map_err(|_| AppError::service_unavailable("Локальная служба Ollama недоступна"))?
        .map(Some)
        .ok_or_else(|| AppError::service_unavailable("Сначала установите модель описания сцен"))
}

async fn read_scene_description(
    file: &std::path::Path,
) -> Option<crate::analysis::scene_description::Description> {
    let result: crate::analysis::scene_description::Description = shots::read_cache(file).await?;
    result.validate().ok()?;
    Some(result)
}

pub async fn scene_model_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Option<crate::analysis::scene_model::Status>>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let Some(config) = state.scene_description.as_ref().as_ref() else {
        return Ok(Json(None));
    };
    let result = match &config.managed {
        Some(managed) => Some(managed.status(&config.client, &actor).await),
        None => None,
    };
    Ok(Json(result))
}

pub async fn install_scene_model(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<crate::analysis::scene_model::Status>> {
    use crate::analysis::scene_model::{Status, MODEL};

    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let config = state
        .scene_description
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Локальная модель не настроена"))?;
    let managed = config
        .managed
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Локальная модель не настроена"))?;
    if actor != managed.installer {
        return Err(AppError::forbidden(
            "Этой учётной записи недоступна установка модели",
        ));
    }
    let phase = managed
        .install(&config.client)
        .await
        .map_err(|_| AppError::service_unavailable("Локальная служба Ollama недоступна"))?;
    Ok(Json(Status {
        model: MODEL,
        phase,
        can_install: true,
    }))
}
