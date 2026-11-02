//! Durable browser-operation receipts; job IDs and recipes remain server-owned.
use crate::{
    error::{AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde_json::{json, Value};
#[derive(sqlx::FromRow)]
pub(super) struct Receipt {
    pub request_hash: Option<String>,
    pub job_id: String,
    pub source_id: Option<String>,
    pub source_hash: Option<String>,
    pub accepted_json: Option<String>,
    pub cancelled: i64,
}
pub(super) fn validate_id(value: &str) -> AppResult<()> {
    if uuid::Uuid::parse_str(value).is_err()
        || uuid::Uuid::parse_str(value).unwrap().to_string() != value
    {
        return Err(AppError::bad_request("Неверный идентификатор операции SAM"));
    }
    Ok(())
}
async fn read(
    state: &AppState,
    owner: &str,
    project: &str,
    operation: &str,
) -> AppResult<Option<Receipt>> {
    sqlx::query_as("SELECT request_hash,job_id,source_id,source_hash,accepted_json,cancelled FROM concat_video_smart_operations WHERE owner=? AND project_id=? AND operation_id=?").bind(owner).bind(project).bind(operation).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read SAM operation",e))
}
pub(super) async fn peek(
    state: &AppState,
    owner: &str,
    project: &str,
    operation: &str,
    hash: &str,
    headers: &HeaderMap,
) -> AppResult<Option<Receipt>> {
    let row = read(state, owner, project, operation).await?;
    if let Some(row) = &row {
        if row.cancelled != 0 {
            return Err(AppError::conflict("Операция SAM отменена"));
        }
        if row.request_hash.as_deref() != Some(hash) {
            return Err(AppError::conflict(
                "Идентификатор операции SAM уже связан с другим запросом",
            ));
        }
        if let Some(source) = &row.source_id {
            super::library::authorize_source_access(state, source, headers).await?;
        }
    }
    Ok(row)
}
pub(super) fn accepted(row: &Receipt) -> AppResult<Value> {
    let value: Value = serde_json::from_str(
        row.accepted_json
            .as_deref()
            .ok_or_else(|| AppError::conflict("Нет сохранённого запуска SAM"))?,
    )
    .map_err(|_| AppError::conflict("Повреждена операция SAM"))?;
    if value["jobId"].as_str() != Some(&row.job_id) {
        return Err(AppError::conflict("Изменилась задача операции SAM"));
    }
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn reserve(
    state: &AppState,
    owner: &str,
    project: &str,
    operation: &str,
    hash: &str,
    source: &str,
    source_hash: &str,
    mut started: Value,
    headers: &HeaderMap,
) -> AppResult<Receipt> {
    let id = uuid::Uuid::new_v4().to_string();
    started["jobId"] = json!(id);
    sqlx::query("INSERT OR IGNORE INTO concat_video_smart_operations(owner,project_id,operation_id,request_hash,job_id,source_id,source_hash,accepted_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(owner).bind(project).bind(operation).bind(hash).bind(&id).bind(source).bind(source_hash).bind(started.to_string()).bind(crate::library::now_secs() as i64).execute(state.db.pool()).await.map_err(|e|AppError::internal("reserve SAM operation",e))?;
    let row = peek(state, owner, project, operation, hash, headers)
        .await?
        .ok_or_else(|| AppError::conflict("Резерв SAM недоступен"))?;
    if row.source_id.as_deref() != Some(source) || row.source_hash.as_deref() != Some(source_hash) {
        return Err(AppError::conflict("Исходник операции SAM изменился"));
    }
    let mut expected = started;
    expected["jobId"] = json!(row.job_id);
    if accepted(&row)? != expected {
        return Err(AppError::conflict(
            "Временная привязка операции SAM изменилась",
        ));
    }
    Ok(row)
}
pub(super) async fn is_cancelled(state: &AppState, job: &str) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concat_video_smart_operations WHERE job_id=? AND cancelled<>0)").bind(job).fetch_one(state.db.pool()).await?)
}
pub(super) async fn stop_if_cancelled(state: &AppState, job: &str) -> AppResult<bool> {
    if !is_cancelled(state, job)
        .await
        .map_err(|e| AppError::internal("read SAM cancellation", e))?
    {
        return Ok(false);
    }
    state
        .cancel_open_job(job)
        .await
        .map_err(|e| AppError::internal("cancel SAM operation job", e))?;
    Ok(true)
}
pub async fn lookup(
    State(state): State<AppState>,
    Path((project, operation)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    validate_id(&operation)?;
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project, &actor).await?;
    let row = read(&state, &actor, &project, &operation)
        .await?
        .ok_or_else(|| AppError::not_found("Операция SAM не найдена"))?;
    if let Some(source) = &row.source_id {
        super::library::authorize_source_access(&state, source, &headers).await?;
    }
    let queued = state
        .get_job(&row.job_id)
        .await
        .map_err(|e| AppError::internal("read SAM operation job", e))?
        .is_some();
    let started = if row.accepted_json.is_some() {
        accepted(&row)?
    } else {
        Value::Null
    };
    Ok(Json(
        json!({"operationId":operation,"jobId":row.job_id,"started":started,"cancelled":row.cancelled!=0,"enqueued":queued}),
    ))
}
pub async fn cancel(
    State(state): State<AppState>,
    Path((project, operation)): Path<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    validate_id(&operation)?;
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project, &actor).await?;
    // A cancellation before the POST arrives is a tombstone, not a missing-job no-op.
    sqlx::query("INSERT INTO concat_video_smart_operations(owner,project_id,operation_id,job_id,cancelled,created_at) VALUES(?,?,?,?,1,?) ON CONFLICT(owner,project_id,operation_id) DO UPDATE SET cancelled=1").bind(&actor).bind(&project).bind(&operation).bind(uuid::Uuid::new_v4().to_string()).bind(crate::library::now_secs() as i64).execute(state.db.pool()).await.map_err(|e|AppError::internal("cancel SAM operation",e))?;
    let row = read(&state, &actor, &project, &operation)
        .await?
        .ok_or_else(|| AppError::conflict("Нет отменённой операции SAM"))?;
    stop_if_cancelled(&state, &row.job_id).await?;
    Ok(Json(
        json!({"operationId":operation,"jobId":row.job_id,"cancelled":true}),
    ))
}
