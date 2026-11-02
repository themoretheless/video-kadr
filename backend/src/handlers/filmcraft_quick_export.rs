//! Quick Export: active client sequence, remembered owner/session preset and exact range.
use crate::{
    domain::{
        filmcraft_export_presets::DEFAULT_PRESET, filmcraft_export_range::SourceRange,
        filmcraft_project_file::SourceProjectDocument,
    },
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickRequest {
    document: Value,
    sequence_id: String,
    #[serde(default)]
    sources: BTreeMap<String, String>,
    preset: Option<String>,
    settings: Option<Value>,
    range: Option<String>,
    source_range: Option<SourceRange>,
    #[serde(default)]
    wait: bool,
}

pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<QuickRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let name = match request.preset {
        Some(name) => name,
        None => state
            .filmcraft_quick_presets
            .lock()
            .await
            .get(&owner)
            .cloned()
            .unwrap_or_else(|| DEFAULT_PRESET.into()),
    };
    let library = state
        .db
        .filmcraft_export_presets(&owner)
        .await
        .map_err(|error| AppError::internal("read quick export presets", error))?;
    let preset = library
        .find(&name)
        .ok_or_else(|| AppError::bad_request("unknown quick export preset"))?;
    let name = preset.name;
    let mut settings = preset.settings;
    if let Some(patch) = request.settings {
        if !patch.is_object() {
            return Err(AppError::bad_request("settings must be an object"));
        }
        super::filmcraft_export_presets::merge(&mut settings, patch);
    }
    let settings = crate::domain::filmcraft_export_preset_settings::normalize(settings, true)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut body = crate::domain::filmcraft_export_preset_resolve::request_patch(&settings)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let bytes = match request.document.as_str() {
        Some(text) => text.as_bytes().to_vec(),
        None => serde_json::to_vec(&request.document)
            .map_err(|error| AppError::bad_request(error.to_string()))?,
    };
    let document = SourceProjectDocument::read(&bytes)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let sequence = request
        .sequence_id
        .parse::<u64>()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let range = crate::domain::filmcraft_quick_export_range::resolve(
        &document,
        sequence,
        request.range.as_deref(),
        request.source_range.as_ref(),
    )
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    body["document"] = request.document;
    body["sequenceId"] = json!(request.sequence_id);
    body["sources"] = json!(request.sources);
    body["sourceRange"] = serde_json::to_value(range)
        .map_err(|error| AppError::internal("serialize quick range", error))?;
    enqueue(&state, headers, owner, name, body, request.wait).await
}

pub(super) async fn enqueue(
    state: &AppState,
    headers: HeaderMap,
    owner: String,
    name: String,
    body: Value,
    wait: bool,
) -> AppResult<(StatusCode, Json<Value>)> {
    let (status, Json(mut accepted)) =
        super::filmcraft_video_jobs::submit(State(state.clone()), headers, ApiJson(body)).await?;
    state
        .filmcraft_quick_presets
        .lock()
        .await
        .insert(owner, name.clone());
    accepted["presetName"] = json!(name);
    if wait {
        let id = accepted["jobId"]
            .as_str()
            .ok_or_else(|| {
                AppError::internal(
                    "missing quick export job",
                    anyhow::anyhow!("missing job ID"),
                )
            })?
            .to_owned();
        loop {
            let job = state
                .get_job(&id)
                .await
                .map_err(|error| AppError::internal("observe quick export", error))?
                .ok_or_else(|| AppError::not_found("quick export job disappeared"))?;
            if matches!(
                job.status,
                crate::model::JobStatus::Done
                    | crate::model::JobStatus::Error
                    | crate::model::JobStatus::Cancelled
                    | crate::model::JobStatus::Interrupted
            ) && !super::filmcraft_video_jobs::worker_active(state, &id)
            {
                accepted["status"] = serde_json::to_value(job.status)
                    .map_err(|error| AppError::internal("quick export status", error))?;
                accepted["result"] = job.result.unwrap_or(Value::Null);
                accepted["error"] = job.error.map(Value::String).unwrap_or(Value::Null);
                break;
            }
            if state.is_shutting_down() {
                return Err(AppError::service_unavailable(
                    "quick export completion unconfirmed during shutdown",
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    Ok((status, Json(accepted)))
}
