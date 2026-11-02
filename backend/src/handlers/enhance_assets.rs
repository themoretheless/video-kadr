//! Explicit project acceptance of a completed private learned-image result.
use crate::{
    error::{ApiJson, AppError, AppResult},
    library::MediaEntry,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    project_id: String,
}
async fn authorize(state: &AppState, project: &str, actor: &str) -> AppResult<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("authorize enhanced media project", e))?;
    if !matches!(role.as_deref(), Some("owner" | "editor")) {
        return Err(AppError::forbidden(
            "Для сохранения копии нужен доступ редактора к проекту",
        ));
    }
    Ok(())
}
struct Staged(std::path::PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub async fn store(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(target): ApiJson<Target>,
) -> AppResult<Json<MediaEntry>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    authorize(&state, &target.project_id, &actor).await?;
    let _operation = state.enhance_asset_ops.lock().await;
    let (metadata, bytes) = super::enhance_jobs::checked_result(&state, &id, &headers).await?;
    let reserved: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM enhance_job_assets WHERE job_id=? AND project_id=?",
    )
    .bind(&id)
    .bind(&target.project_id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read enhanced media reservation", e))?;
    let source_id = if let Some(id) = reserved {
        id
    } else {
        let source = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO enhance_job_assets(job_id,project_id,source_id) VALUES(?,?,?)")
            .bind(&id)
            .bind(&target.project_id)
            .bind(&source)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reserve enhanced media", e))?;
        source
    };
    let space = state
        .db
        .claim_project_generated_media(&target.project_id, &actor, &source_id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    if let Some(entry) = state.library.get(&source_id).await {
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?;
        let (_, hash) =
            crate::project_archive::fingerprint_file_with_limit(&path, 256 * 1024 * 1024)
                .await
                .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?;
        if hash != metadata.output_hash {
            return Err(AppError::conflict("Сохранённая копия изменилась"));
        }
        authorize(&state, &target.project_id, &actor).await?;
        return Ok(Json(entry));
    }
    let directory = state.sources_dir().join("spaces").join(&space);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| AppError::internal("create enhanced media directory", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|e| AppError::internal("protect enhanced media directory", e))?;
    }
    let filename = format!("{source_id}.png");
    let destination = directory.join(&filename);
    if destination
        .try_exists()
        .map_err(|e| AppError::internal("inspect enhanced media", e))?
    {
        let (_, hash) =
            crate::project_archive::fingerprint_file_with_limit(&destination, 256 * 1024 * 1024)
                .await
                .map_err(|_| AppError::conflict("Неверная сохранённая копия"))?;
        if hash != metadata.output_hash {
            return Err(AppError::conflict("Сохранённая копия изменилась"));
        }
    } else {
        let staged = Staged(directory.join(format!(".{source_id}-{}.part", uuid::Uuid::new_v4())));
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options
            .open(&staged.0)
            .await
            .map_err(|e| AppError::internal("stage enhanced media", e))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| AppError::internal("write enhanced media", e))?;
        file.sync_all()
            .await
            .map_err(|e| AppError::internal("sync enhanced media", e))?;
        drop(file);
        tokio::fs::hard_link(&staged.0, &destination)
            .await
            .map_err(|e| AppError::internal("publish enhanced media", e))?;
    }
    authorize(&state, &target.project_id, &actor).await?;
    super::enhance_jobs::authorize_job(&state, &id, &headers, true).await?;
    let entry = MediaEntry::from_result(
        "source",
        &json!({"id":source_id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":metadata.width,"height":metadata.height,"title":"Улучшенное изображение","sizeBytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}),
    );
    if !state.library.add(entry.clone()).await {
        return Err(AppError::service_unavailable(
            "Не удалось зарегистрировать копию в медиатеке",
        ));
    }
    state.index_media(&entry).await;
    Ok(Json(entry))
}
