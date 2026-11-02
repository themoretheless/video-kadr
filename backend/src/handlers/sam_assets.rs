//! Explicit project acceptance of an independent learned region and portable provenance.
use crate::{
    domain::cutout::RegionAttachment,
    error::{ApiJson, AppError, AppResult},
    library::MediaEntry,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    project_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    #[serde(flatten)]
    pub media: MediaEntry,
    pub region: RegionAttachment,
    pub source: MediaEntry,
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
) -> AppResult<Json<Accepted>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &target.project_id, &actor).await?;
    let _operation = state.enhance_asset_ops.lock().await;
    let (metadata, source_id) = super::sam_jobs::result_metadata(&state, &id, &headers).await?;
    let points: String =
        sqlx::query_scalar("SELECT points_json FROM sam_job_access WHERE job_id=?")
            .bind(&id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read accepted SAM prompts", e))?;
    let points = serde_json::from_str(&points)
        .map_err(|e| AppError::internal("decode accepted SAM prompts", e))?;
    let reserved: Option<String> =
        sqlx::query_scalar("SELECT source_id FROM sam_job_assets WHERE job_id=? AND project_id=?")
            .bind(&id)
            .bind(&target.project_id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read SAM media reservation", e))?;
    let mask_id = if let Some(id) = reserved {
        id
    } else {
        let source = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO sam_job_assets(job_id,project_id,source_id) VALUES(?,?,?)")
            .bind(&id)
            .bind(&target.project_id)
            .bind(&source)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reserve SAM media", e))?;
        source
    };
    let region = RegionAttachment {
        source_id: source_id.clone(),
        mask_source_id: mask_id.clone(),
        source_hash: metadata.source_hash.clone(),
        mask_hash: metadata.mask_hash.clone(),
        mask_file_hash: metadata.output_hash.clone(),
        encoder_hash: metadata.encoder_hash.clone(),
        decoder_hash: metadata.decoder_hash.clone(),
        runtime_hash: metadata.runtime_hash.clone(),
        prompt_hash: metadata.prompt_hash.clone(),
        points,
    };
    region
        .validate()
        .map_err(|_| AppError::conflict("Сохранённые точки выделения изменились"))?;
    let space = state
        .db
        .claim_project_generated_media(&target.project_id, &actor, &mask_id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    let media = if let Some(entry) = state.library.get(&mask_id).await {
        if entry.kind != "source"
            || entry.media_type.as_deref() != Some("image")
            || entry.width != Some(256)
            || entry.height != Some(256)
        {
            return Err(AppError::conflict("Неверные данные сохранённой области"));
        }
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::conflict("Сохранённая область недоступна"))?;
        crate::services::local_sam::verify_png(path, metadata.clone())
            .await
            .map_err(|_| AppError::conflict("Сохранённая область изменилась"))?;
        entry
    } else {
        let directory = crate::services::tts_model_download::directory(
            &state.sources_dir(),
            &std::path::Path::new("spaces").join(&space),
        )
        .await
        .map_err(|e| AppError::internal("create SAM media directory", e))?;
        let filename = format!("{mask_id}.png");
        let destination = directory.join(&filename);
        if tokio::fs::symlink_metadata(&destination).await.is_ok() {
            crate::services::local_sam::verify_png(destination.clone(), metadata.clone())
                .await
                .map_err(|_| AppError::conflict("Сохранённая область изменилась"))?;
        } else {
            let path = super::sam_jobs::output(&state, &id)
                .map_err(|_| AppError::not_found("Выделение не найдено"))?;
            crate::services::local_sam::verify_png(path.clone(), metadata.clone())
                .await
                .map_err(|_| AppError::conflict("Исходная область недоступна"))?;
            let bytes = tokio::fs::read(path)
                .await
                .map_err(|e| AppError::internal("read SAM source PNG", e))?;
            use sha2::Digest;
            if bytes.len() > 1024 * 1024
                || format!("{:x}", sha2::Sha256::digest(&bytes)) != metadata.output_hash
            {
                return Err(AppError::conflict("Исходная область изменилась"));
            }
            let staged =
                Staged(directory.join(format!(".{mask_id}-{}.part", uuid::Uuid::new_v4())));
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let mut file = options
                .open(&staged.0)
                .await
                .map_err(|e| AppError::internal("stage SAM media", e))?;
            file.write_all(&bytes)
                .await
                .map_err(|e| AppError::internal("write SAM media", e))?;
            file.sync_all()
                .await
                .map_err(|e| AppError::internal("sync SAM media", e))?;
            drop(file);
            super::cutout_assets::authorize(&state, &target.project_id, &actor).await?;
            super::sam_jobs::result_metadata(&state, &id, &headers).await?;
            tokio::fs::hard_link(&staged.0, &destination)
                .await
                .map_err(|e| AppError::internal("publish SAM media", e))?;
        }
        super::cutout_assets::authorize(&state, &target.project_id, &actor).await?;
        super::sam_jobs::result_metadata(&state, &id, &headers).await?;
        let entry = MediaEntry::from_result(
            "source",
            &json!({"id":mask_id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":256,"height":256,"title":"Область выделения SAM"}),
        );
        if !state.library.add(entry.clone()).await {
            return Err(AppError::service_unavailable(
                "Не удалось сохранить область в медиатеке",
            ));
        }
        state.index_media(&entry).await;
        entry
    };
    let mut source = state
        .library
        .get(&source_id)
        .await
        .filter(|entry| entry.kind == "source")
        .ok_or_else(|| AppError::conflict("Исходное изображение недоступно"))?;
    source.media_type = Some("image".into());
    source.width = Some(metadata.width);
    source.height = Some(metadata.height);
    super::cutout_assets::authorize(&state, &target.project_id, &actor).await?;
    super::sam_jobs::result_metadata(&state, &id, &headers).await?;
    Ok(Json(Accepted {
        media,
        region,
        source,
    }))
}
