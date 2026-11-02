//! Owner model selection. A job resolves its captured ID, never the later selection.
use crate::{
    error::{ApiJson, AppError, AppResult},
    services::{local_sam::Config as SamConfig, sam_model::Catalog as ModelCatalog},
    state::AppState,
};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) async fn config_for_model(
    state: &AppState,
    owner: &str,
    id: Option<&str>,
) -> AppResult<SamConfig> {
    let mut config = state
        .sam
        .as_ref()
        .clone()
        .ok_or_else(|| AppError::service_unavailable("Движок выделения объекта не настроен"))?;
    if let Some(id) = id {
        let catalog = ModelCatalog::find(id)
            .map_err(|e| AppError::internal("read SAM catalog", e))?
            .ok_or_else(|| AppError::bad_request("Неизвестная модель выделения объекта"))?;
        let owned: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sam_models WHERE owner=? AND id=?)")
                .bind(owner)
                .bind(id)
                .fetch_one(state.db.pool())
                .await
                .map_err(|e| AppError::internal("read SAM model owner", e))?;
        if !owned {
            return Err(AppError::forbidden("Модель недоступна этому пользователю"));
        }
        let root = state.storage.join("models").join("sam").join(id);
        let checked = root.clone();
        tokio::task::spawn_blocking(move || catalog.verify_directory(&checked))
            .await
            .map_err(|e| AppError::internal("join SAM verification", e))?
            .map_err(|_| {
                AppError::service_unavailable("Комплект модели отсутствует или повреждён")
            })?;
        config.encoder = root.join(crate::services::sam_model::ENCODER);
        config.decoder = root.join(crate::services::sam_model::DECODER);
    }
    Ok(config)
}
pub(crate) async fn config_for_owner(
    state: &AppState,
    owner: &str,
) -> AppResult<(SamConfig, Option<String>)> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM sam_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read SAM selection", e))?;
    Ok((config_for_model(state, owner, id.as_deref()).await?, id))
}
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let catalogs = ModelCatalog::all().map_err(|e| AppError::internal("read SAM catalog", e))?;
    let _operation = state.sam_model_ops.lock().await;
    for model in &catalogs {
        let _ = release_locked(&state, &model.id).await;
    }
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT m.id,m.name,m.bytes FROM sam_models m WHERE m.owner=? AND NOT EXISTS(SELECT 1 FROM sam_model_removals r WHERE r.owner=m.owner AND r.id=m.id) ORDER BY m.name,m.id")
            .bind(&owner)
            .fetch_all(state.db.pool())
            .await
            .map_err(|e| AppError::internal("list SAM models", e))?;
    let selected: Option<String> =
        sqlx::query_scalar("SELECT model_id FROM sam_model_selection WHERE owner=?")
            .bind(owner)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read SAM selection", e))?;
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
    let _operation = state.sam_model_ops.lock().await;
    if request.model_id == "configured" {
        sqlx::query("DELETE FROM sam_model_selection WHERE owner=?")
            .bind(&owner)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reset SAM selection", e))?;
    } else {
        let removed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sam_model_removals WHERE owner=? AND id=?)",
        )
        .bind(&owner)
        .bind(&request.model_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read removed SAM model", e))?;
        if removed {
            return Err(AppError::not_found("Модель удалена из вашей библиотеки"));
        }
        config_for_model(&state, &owner, Some(&request.model_id)).await?;
        sqlx::query("INSERT INTO sam_model_selection(owner,model_id) VALUES(?,?) ON CONFLICT(owner) DO UPDATE SET model_id=excluded.model_id").bind(&owner).bind(&request.model_id).execute(state.db.pool()).await.map_err(|e| AppError::internal("select SAM model", e))?;
    }
    Ok(Json(json!({"selectedId":request.model_id})))
}

/// Caller holds sam_model_ops: serialize admission, synchronous inference and publication.
async fn release_locked(state: &AppState, id: &str) -> anyhow::Result<bool> {
    let removed: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sam_model_removals WHERE id=?)")
            .bind(id)
            .fetch_one(state.db.pool())
            .await?;
    if !removed {
        return Ok(false);
    }
    let owners: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sam_models m WHERE m.id=? AND NOT EXISTS(SELECT 1 FROM sam_model_removals r WHERE r.id=m.id AND r.owner=m.owner))").bind(id).fetch_one(state.db.pool()).await?;
    // Retriable requests retain their payload. Malformed retained requests pin
    // conservatively instead of allowing collection through a failed JSON parse.
    let pinned: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM job_requests WHERE kind='sam_region' AND payload_json<>'null' AND CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.model_id')=? ELSE 1 END)").bind(id).fetch_one(state.db.pool()).await?;
    if owners || pinned {
        return Ok(false);
    }
    let parent = crate::services::tts_model_download::directory(
        &state.storage,
        std::path::Path::new("models/sam"),
    )
    .await?;
    let root = parent.join(id);
    if let Some(configured) = state.sam.as_ref().as_ref().map(|c| &c.encoder) {
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
                    .ok_or_else(|| anyhow::anyhow!("unknown SAM catalog"))?
                    .verify_directory(&checked)
            })
            .await??;
            let trash = crate::services::tts_model_download::directory(
                &state.storage,
                std::path::Path::new("sam-model-trash"),
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
    sqlx::query("DELETE FROM sam_models WHERE id=? AND EXISTS(SELECT 1 FROM sam_model_removals r WHERE r.id=sam_models.id AND r.owner=sam_models.owner)").bind(id).execute(state.db.pool()).await?;
    Ok(true)
}
pub async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Selection>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if ModelCatalog::find(&request.model_id)
        .map_err(|e| AppError::internal("read SAM catalog", e))?
        .is_none()
    {
        return Err(AppError::bad_request(
            "Неизвестная модель выделения объекта",
        ));
    }
    let _operation = state.sam_model_ops.lock().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sam_models WHERE owner=? AND id=?)")
            .bind(&owner)
            .bind(&request.model_id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read removed SAM model", e))?;
    if !exists {
        return Err(AppError::not_found("Модель не найдена"));
    }
    let mut tx = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin SAM removal", e))?;
    sqlx::query("INSERT INTO sam_model_epochs(owner,id,epoch) VALUES(?,?,1) ON CONFLICT(owner,id) DO UPDATE SET epoch=epoch+1").bind(&owner).bind(&request.model_id).execute(&mut *tx).await.map_err(|e|AppError::internal("advance SAM removal epoch",e))?;
    sqlx::query("INSERT OR IGNORE INTO sam_model_removals(owner,id) VALUES(?,?)")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("remove SAM registration", e))?;
    sqlx::query("DELETE FROM sam_model_selection WHERE owner=? AND model_id=?")
        .bind(&owner)
        .bind(&request.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::internal("reset SAM selection", e))?;
    tx.commit()
        .await
        .map_err(|e| AppError::internal("commit SAM removal", e))?;
    let file_released = match release_locked(&state, &request.model_id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error,"removed SAM bundle retained");
            false
        }
    };
    Ok(Json(json!({"removed":true,"fileReleased":file_released})))
}
