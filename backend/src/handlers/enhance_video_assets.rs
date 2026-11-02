//! Explicit project acceptance of a completed private learned-video result.
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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let (metadata, source_path) =
        super::enhance_video_jobs::checked_result(&state, &id, &headers).await?;
    let reserved: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM enhance_video_job_assets WHERE job_id=? AND project_id=?",
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
        sqlx::query(
            "INSERT INTO enhance_video_job_assets(job_id,project_id,source_id) VALUES(?,?,?)",
        )
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
        if !tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| AppError::conflict("Сохранённое видео недоступно"))?
            .file_type()
            .is_file()
        {
            return Err(AppError::conflict("Сохранённое видео повреждено"));
        }
        let (_, hash) =
            crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
                .await
                .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?;
        if hash != metadata.output_hash {
            return Err(AppError::conflict("Сохранённая копия изменилась"));
        }
        authorize(&state, &target.project_id, &actor).await?;
        super::enhance_video_jobs::authorize_job(&state, &id, &headers, true).await?;
        return Ok(Json(entry));
    }
    let directory = crate::services::tts_model_download::directory(
        &state.sources_dir(),
        &std::path::Path::new("spaces").join(&space),
    )
    .await
    .map_err(|e| AppError::internal("create private video media directory", e))?;
    let filename = format!("{source_id}.mp4");
    let destination = directory.join(&filename);
    if destination
        .try_exists()
        .map_err(|e| AppError::internal("inspect enhanced media", e))?
    {
        if !tokio::fs::symlink_metadata(&destination)
            .await
            .map_err(|_| AppError::conflict("Сохранённое видео недоступно"))?
            .file_type()
            .is_file()
        {
            return Err(AppError::conflict("Сохранённое видео повреждено"));
        }
        let (_, hash) = crate::project_archive::fingerprint_file_with_limit(
            &destination,
            64 * 1024 * 1024 * 1024,
        )
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
        let source_metadata = tokio::fs::symlink_metadata(&source_path)
            .await
            .map_err(|e| AppError::internal("inspect learned video", e))?;
        if !source_metadata.file_type().is_file() {
            return Err(AppError::conflict("Результат улучшения недоступен"));
        }
        let mut reader = tokio::fs::File::open(&source_path)
            .await
            .map_err(|e| AppError::internal("open learned video", e))?;
        let mut buffer = vec![0u8; 65536];
        let mut copied = 0u64;
        let mut hash = Sha256::new();
        loop {
            let count = reader
                .read(&mut buffer)
                .await
                .map_err(|e| AppError::internal("read learned video", e))?;
            if count == 0 {
                break;
            }
            copied = copied
                .checked_add(count as u64)
                .ok_or_else(|| AppError::conflict("Некорректный размер видеокопии"))?;
            if copied > 64 * 1024 * 1024 * 1024 {
                return Err(AppError::conflict("Видеокопия превышает допустимый размер"));
            }
            hash.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .await
                .map_err(|e| AppError::internal("write private video copy", e))?;
        }
        if format!("{:x}", hash.finalize()) != metadata.output_hash {
            return Err(AppError::conflict("Видеокопия изменилась при сохранении"));
        }
        file.sync_all()
            .await
            .map_err(|e| AppError::internal("sync enhanced media", e))?;
        drop(file);
        authorize(&state, &target.project_id, &actor).await?;
        super::enhance_video_jobs::authorize_job(&state, &id, &headers, true).await?;
        tokio::fs::hard_link(&staged.0, &destination)
            .await
            .map_err(|e| AppError::internal("publish enhanced media", e))?;
    }
    authorize(&state, &target.project_id, &actor).await?;
    super::enhance_video_jobs::authorize_job(&state, &id, &headers, true).await?;
    let file_metadata = tokio::fs::symlink_metadata(&destination)
        .await
        .map_err(|e| AppError::internal("inspect accepted video", e))?;
    if !file_metadata.file_type().is_file() {
        return Err(AppError::conflict("Сохранённое видео повреждено"));
    }
    let entry = MediaEntry::from_result(
        "source",
        &json!({"id":source_id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"video","width":metadata.width,"height":metadata.height,"duration":metadata.duration,"fps":f64::from(metadata.frame_rate.numerator)/f64::from(metadata.frame_rate.denominator),"vcodec":"h264","acodec":metadata.audio_codec,"title":"Улучшенное видео","sizeBytes":file_metadata.len(),"sha256":metadata.output_hash}),
    );
    if !state.library.add(entry.clone()).await {
        return Err(AppError::service_unavailable(
            "Не удалось зарегистрировать копию в медиатеке",
        ));
    }
    state.index_media(&entry).await;
    Ok(Json(entry))
}
