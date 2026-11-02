//! ACL-bound source probing for source-document FillSlot. Does not save a project.
use crate::{
    domain::{composition::SourceId, media_probe::ProbeResult},
    error::{ApiJson, AppError, AppResult},
    services::filmcraft_video_preview::SourceVideoRevision,
    state::AppState,
};
use axum::{
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    document: Value,
    media_id: String,
    source_id: SourceId,
}
pub async fn fill_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Response> {
    super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, request.source_id.as_str(), &headers).await?;
    crate::domain::concat_media_slots::admit(&request.document)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if !request.document["media"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == request.media_id && item["placeholder"] == true)
    {
        return Err(AppError::bad_request(
            "FillSlot requires an existing placeholder",
        ));
    }
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable("Требуется локальный ffprobe"));
    }
    let _permit = SLOTS
        .acquire()
        .await
        .map_err(|_| AppError::service_unavailable("Проверка источников недоступна"))?;
    let entry = state
        .library
        .get(request.source_id.as_str())
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| AppError::not_found("Источник не найден"))?;
    let path = state
        .library
        .resolve_media_path(&entry)
        .await
        .map_err(|_| AppError::not_found("Источник не найден"))?;
    let revision = SourceVideoRevision::read(&path)
        .await
        .map_err(|_| AppError::not_found("Источник не найден"))?;
    let metadata = state
        .db
        .get_library_metadata(&entry.id)
        .await
        .map_err(|e| AppError::internal("read slot media metadata", e))?;
    let token = tokio_util::sync::CancellationToken::new();
    let _cancel = token.clone().drop_guard();
    let mut command = tokio::process::Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&path);
    let output = crate::process_control::capture_output_cancellable(
        &state.process_runtime,
        command,
        state.process_runtime.probe_policy(&path),
        Duration::from_secs(30),
        &token,
    )
    .await
    .map_err(|_| AppError::bad_request("Не удалось проверить источник"))?;
    if !output.status.success() {
        return Err(AppError::bad_request("Не удалось проверить источник"));
    }
    let probe = ProbeResult::from_ffprobe_json(
        &serde_json::from_slice(&output.stdout)
            .map_err(|_| AppError::bad_request("Некорректные данные ffprobe"))?,
    )
    .map_err(|_| AppError::bad_request("Некорректные данные ffprobe"))?;
    let kind = super::upload::media_type(&probe)
        .ok_or_else(|| AppError::bad_request("Неподдерживаемый источник"))?;
    let kind = serde_json::from_value(json!(kind))
        .map_err(|_| AppError::bad_request("Неподдерживаемый источник"))?;
    let name = metadata
        .as_ref()
        .and_then(|m| m.title.as_deref())
        .or(entry.title.as_deref())
        .or(entry.original_filename.as_deref())
        .unwrap_or(&entry.filename);
    let (replacement, source) = crate::domain::concat_registered_media::replacement(
        request.source_id.clone(),
        name,
        kind,
        &probe,
    )
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    let document =
        crate::domain::concat_media_slots::fill(&request.document, &request.media_id, &replacement)
            .map_err(|e| AppError::bad_request(e.to_string()))?;
    super::library::authorize_source_access(&state, request.source_id.as_str(), &headers).await?;
    let current = state
        .library
        .get(&entry.id)
        .await
        .ok_or_else(|| AppError::conflict("Источник изменился; повторите заполнение"))?;
    let current_path = state
        .library
        .resolve_media_path(&current)
        .await
        .map_err(|_| AppError::conflict("Источник изменился; повторите заполнение"))?;
    let current_metadata = state
        .db
        .get_library_metadata(&entry.id)
        .await
        .map_err(|e| AppError::internal("recheck slot media metadata", e))?;
    if current_path != path
        || serde_json::to_value(&current).ok() != serde_json::to_value(&entry).ok()
        || current_metadata != metadata
        || SourceVideoRevision::read(Path::new(&path))
            .await
            .map_err(|_| AppError::conflict("Источник изменился"))?
            != revision
    {
        return Err(AppError::conflict(
            "Источник изменился; повторите заполнение",
        ));
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"schemaVersion":1,"document":document,"applied":true,"source":source})),
    )
        .into_response())
}
