//! Owner model selection. A job resolves its captured ID, never the later selection.
use crate::{
    error::{ApiJson, AppError, AppResult},
    services::{cutout_model::Catalog as ModelCatalog, local_cutout::Config as CutoutConfig},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) async fn config_for_model(
    state: &AppState,
    owner: &str,
    id: Option<&str>,
) -> AppResult<CutoutConfig> {
    let mut config = state
        .cutout
        .as_ref()
        .clone()
        .ok_or_else(|| AppError::service_unavailable("Движок удаления фона не настроен"))?;
    if let Some(id) = id {
        let catalog = ModelCatalog::find(id)
            .map_err(|e| AppError::internal("read cutoutment catalog", e))?
            .ok_or_else(|| AppError::bad_request("Неизвестная модель удаления фона"))?;
        let owned: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cutout_models WHERE owner=? AND id=?)")
                .bind(owner)
                .bind(id)
                .fetch_one(state.db.pool())
                .await
                .map_err(|e| AppError::internal("read cutoutment model owner", e))?;
        if !owned {
            return Err(AppError::forbidden("Модель недоступна этому пользователю"));
        }
        let root = state.storage.join("models").join("cutout").join(id);
        let checked = root.clone();
        tokio::task::spawn_blocking(move || catalog.verify_directory(&checked))
            .await
            .map_err(|e| AppError::internal("join cutoutment verification", e))?
            .map_err(|_| {
                AppError::service_unavailable("Комплект модели отсутствует или повреждён")
            })?;
        config.model = root.join(crate::services::cutout_model::FILE);
    }
    Ok(config)
}
pub(crate) async fn config_for_owner(
    state: &AppState,
    owner: &str,
) -> AppResult<(CutoutConfig, Option<String>)> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM cutout_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read cutoutment selection", e))?;
    Ok((config_for_model(state, owner, id.as_deref()).await?, id))
}
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let catalogs =
        ModelCatalog::all().map_err(|e| AppError::internal("read cutoutment catalog", e))?;
    let _operation = state.cutout_model_ops.lock().await;
    for model in &catalogs {
        let _ = release_locked(&state, &model.id).await;
    }
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT m.id,m.name,m.bytes FROM cutout_models m WHERE m.owner=? AND NOT EXISTS(SELECT 1 FROM cutout_model_removals r WHERE r.owner=m.owner AND r.id=m.id) ORDER BY m.name,m.id")
            .bind(&owner)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| AppError::internal("list cutoutment models", e))?;
    let selected: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM cutout_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read cutoutment selection", e))?;
    Ok(Json(
        json!({"selectedId":selected.unwrap_or_else(||"configured".into()),"models":rows.into_iter().map(|(id,name,bytes)|json!({"id":id,"name":name,"bytes":bytes})).collect::<Vec<_>>(),"catalog":catalogs.into_iter().map(|m|json!({"id":m.id,"name":m.name,"bytes":m.bytes,"revision":m.revision,"licence":"Apache-2.0"})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    model_id: String,
}
pub async fn select(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Selection>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let _operation = state.cutout_model_ops.lock().await;
    if request.model_id == "configured" {
        sqlx::query("DELETE FROM cutout_model_selection WHERE owner=?")
            .bind(&owner)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reset cutoutment selection", e))?;
    } else {
        let removed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM cutout_model_removals WHERE owner=? AND id=?)",
        )
        .bind(&owner)
        .bind(&request.model_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read removed cutoutment model", e))?;
        if removed {
            return Err(AppError::not_found("Модель удалена из вашей библиотеки"));
        }
        config_for_model(&state, &owner, Some(&request.model_id)).await?;
        sqlx::query("INSERT INTO cutout_model_selection(owner,model_id) VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET model_id=excluded.model_id").bind(&owner).bind(&request.model_id).execute(state.db.pool()).await.map_err(|e| AppError::internal("select cutoutment model", e))?;
    }
    Ok(Json(json!({"selectedId":request.model_id})))
}

/// Caller holds cutout_model_ops: serialize admission, synchronous inference and publication.
async fn release_locked(state: &AppState, id: &str) -> anyhow::Result<bool> {
    let removed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cutout_model_removals WHERE id=?)")
            .bind(id)
            .fetch_one(state.db.pool())
            .await?;
    if !removed {
        return Ok(false);
    }
    let owners: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cutout_models m WHERE m.id=? AND NOT EXISTS(SELECT 1 FROM cutout_model_removals r WHERE r.id=m.id AND r.owner=m.owner))").bind(id).fetch_one(state.db.pool()).await?;
    // Retriable requests retain their payload. Malformed retained requests pin
    // conservatively instead of allowing collection through a failed JSON parse.
    let pinned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_requests WHERE kind='cutout' AND payload_json<>'null' AND CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.model_id')=? ELSE 1 END)").bind(id).fetch_one(state.db.pool()).await?;
    if owners || pinned {
        return Ok(false);
    }
    let parent = crate::services::tts_model_download::directory(
        &state.storage,
        std::path::Path::new("models/cutout"),
    )
    .await?;
    let root = parent.join(id);
    if let Some(configured) = state.cutout.as_ref().as_ref().map(|c| &c.model) {
        if let (Ok(default), Ok(target)) = (
            tokio::fs::canonicalize(configured).await,
            tokio::fs::canonicalize(&root).await,
        ) {
            if default.starts_with(&target) {
                return Ok(false);
            }
        }
    }
    match tokio::fs::symlink_metadata(&root).await {
        Ok(_) => {
            let checked = root.clone();
            let id_owned = id.to_owned();
            tokio::task::spawn_blocking(move || {
                ModelCatalog::find(&id_owned)?
                    .ok_or_else(|| anyhow::anyhow!("unknown cutoutment catalog"))?
                    .verify_directory(&checked)
            })
            .await??;
            let trash = crate::services::tts_model_download::directory(
                &state.storage,
                std::path::Path::new("cutout-model-trash"),
            )
            .await?
            .join(uuid::Uuid::new_v4().to_string());
            tokio::fs::create_dir(&trash).await?;
            tokio::fs::rename(&root, trash.join("bundle")).await?;
            tokio::fs::remove_dir_all(&trash).await?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    sqlx::query("DELETE FROM cutout_models WHERE id=? AND EXISTS(SELECT 1 FROM cutout_model_removals r WHERE r.id=cutout_models.id AND r.owner=cutout_models.owner)").bind(id).execute(state.db.pool()).await?;
    Ok(true)
}
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Selection>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if ModelCatalog::find(&request.model_id)
        .map_err(|e| AppError::internal("read cutoutment catalog", e))?
        .is_none()
    {
        return Err(AppError::bad_request("Неизвестная модель удаления фона"));
    }
    let _operation = state.cutout_model_ops.lock().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cutout_models WHERE owner=? AND id=?)")
            .bind(&owner)
            .bind(&request.model_id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read removed cutoutment model", e))?;
    if !exists {
        return Err(AppError::not_found("Модель не найдена"));
    }
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin cutoutment removal", e))?;
    sqlx::query("INSERT INTO cutout_model_epochs(owner,id,epoch) VALUES(?,?,1) ON CONFLICT(owner,id) DO UPDATE SET epoch=epoch+1").bind(&owner).bind(&request.model_id).execute(&mut *tx).await.map_err(|e|AppError::internal("advance cutoutment removal epoch",e))?;
    sqlx::query("INSERT OR IGNORE INTO cutout_model_removals(owner,id) VALUES(?,?)")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("remove cutoutment registration", e))?;
    sqlx::query("DELETE FROM cutout_model_selection WHERE owner=? AND model_id=?")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("reset cutoutment selection", e))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit cutoutment removal", e))?;
    let file_released = match release_locked(&state, &request.model_id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error,"removed cutoutment bundle retained");
            false
        }
    };
    Ok(Json(json!({"removed":true,"fileReleased":file_released})))
}
