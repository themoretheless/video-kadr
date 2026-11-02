//! Project-owned automatic mask resources; immutable bytes independent of job history.
use crate::{
    error::{AppError, AppResult},
    library::MediaEntry,
    state::AppState,
};
use axum::http::HeaderMap;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const MAX: u64 = 64 * 1024 * 1024 * 1024;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    #[serde(flatten)]
    pub media: MediaEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_cutout: Option<Resources>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    pub snapshot: crate::domain::video_cutout::Attachment,
    pub source: MediaEntry,
    pub mask: MediaEntry,
}
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn valid_file(path: &Path, hash: &str) -> AppResult<u64> {
    let meta = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|_| AppError::conflict("Ресурс видеомаски недоступен"))?;
    if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > MAX {
        return Err(AppError::conflict("Неверный ресурс видеомаски"));
    }
    let (size, digest) = crate::project_archive::fingerprint_file_with_limit(path, MAX)
        .await
        .map_err(|_| AppError::conflict("Ресурс видеомаски недоступен"))?;
    if digest != hash {
        return Err(AppError::conflict("Ресурс видеомаски изменился"));
    }
    Ok(size)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn resource_for(
    owner: ResourceOwner,
    state: &AppState,
    job: &str,
    project: &str,
    actor: &str,
    headers: &HeaderMap,
    input: &Path,
    hash: &str,
    role: &str,
    mut entry: MediaEntry,
) -> AppResult<MediaEntry> {
    super::person_video_assets::authorize(state, project, actor).await?;
    owner.authorize(state, job, headers).await?;
    let source_size = valid_file(input, hash).await?;
    let reserved:Option<String>=sqlx::query_scalar("SELECT source_id FROM person_video_project_resources WHERE project_id=? AND role=? AND file_hash=?").bind(project).bind(role).bind(hash).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read project video resource reservation",e))?;
    let source_id = if let Some(id) = reserved {
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO person_video_project_resources(project_id,role,file_hash,source_id) VALUES(?,?,?,?)").bind(project).bind(role).bind(hash).bind(&id).execute(state.db.pool()).await.map_err(|e|AppError::internal("reserve project video resource",e))?;
        id
    };
    let space = state
        .db
        .claim_project_generated_media(project, actor, &source_id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    if let Some(existing) = state.library.get(&source_id).await {
        let path = state
            .library
            .resolve_media_path(&existing)
            .await
            .map_err(|_| AppError::conflict("Сохранённый ресурс недоступен"))?;
        valid_file(&path, hash).await?;
        super::person_video_assets::authorize(state, project, actor).await?;
        owner.authorize(state, job, headers).await?;
        return Ok(existing);
    }
    let directory = crate::services::tts_model_download::directory(
        &state.sources_dir(),
        &Path::new("spaces").join(&space),
    )
    .await
    .map_err(|e| AppError::internal("create video resource directory", e))?;
    let ext = Path::new(&entry.filename)
        .extension()
        .and_then(|v| v.to_str())
        .filter(|v| !v.is_empty() && v.len() <= 12 && v.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    let filename = format!("{source_id}.{ext}");
    let destination = directory.join(&filename);
    if destination
        .try_exists()
        .map_err(|e| AppError::internal("inspect saved video resource", e))?
    {
        valid_file(&destination, hash).await?;
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
            .map_err(|e| AppError::internal("stage video resource", e))?;
        let mut reader = tokio::fs::File::open(input)
            .await
            .map_err(|e| AppError::internal("open video resource", e))?;
        let mut buffer = vec![0u8; 65536];
        let mut digest = Sha256::new();
        let mut size = 0u64;
        loop {
            let count = reader
                .read(&mut buffer)
                .await
                .map_err(|e| AppError::internal("read video resource", e))?;
            if count == 0 {
                break;
            }
            size += count as u64;
            if size > source_size {
                return Err(AppError::conflict("Ресурс изменился при копировании"));
            }
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .await
                .map_err(|e| AppError::internal("copy video resource", e))?;
        }
        if size != source_size || format!("{:x}", digest.finalize()) != hash {
            return Err(AppError::conflict("Ресурс изменился при копировании"));
        }
        file.sync_all()
            .await
            .map_err(|e| AppError::internal("sync video resource", e))?;
        drop(file);
        super::person_video_assets::authorize(state, project, actor).await?;
        owner.authorize(state, job, headers).await?;
        valid_file(input, hash).await?;
        tokio::fs::hard_link(&staged.0, &destination)
            .await
            .map_err(|e| AppError::internal("publish video resource without replacement", e))?;
    }
    super::person_video_assets::authorize(state, project, actor).await?;
    owner.authorize(state, job, headers).await?;
    entry.id = source_id;
    entry.filename = filename.clone();
    entry.url = format!("/files/sources/{filename}");
    entry.storage_key = Some(format!("spaces/{space}/{filename}"));
    entry.size_bytes = Some(source_size);
    if !state.library.add(entry.clone()).await {
        return Err(AppError::service_unavailable(
            "Не удалось сохранить ресурс видеомаски",
        ));
    }
    state.index_media(&entry).await;
    Ok(entry)
}
pub(super) async fn accepted(
    state: &AppState,
    id: &str,
    project: &str,
    actor: &str,
    headers: &HeaderMap,
    media: MediaEntry,
) -> AppResult<Accepted> {
    let (movie, _) = super::person_video_jobs::checked_result(state, id, headers).await?;
    if movie.automatic_mask_video.is_none() {
        return Ok(Accepted {
            media,
            video_cutout: None,
        });
    }
    let (automatic, path) =
        super::person_video_jobs::checked_automatic_video(state, id, headers).await?;
    let source_id: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM person_video_job_access WHERE job_id=? AND owner=?",
    )
    .bind(id)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read original video resource", e))?;
    let source_id = source_id.ok_or_else(|| AppError::not_found("Исходник видео не найден"))?;
    let mut source = state
        .library
        .get(&source_id)
        .await
        .filter(|e| e.kind == "source" && e.media_type.as_deref() == Some("video"))
        .ok_or_else(|| AppError::conflict("Исходное видео недоступно"))?;
    source.width.get_or_insert(movie.width);
    source.height.get_or_insert(movie.height);
    source.duration.get_or_insert(movie.duration);
    source.fps.get_or_insert(
        f64::from(movie.frame_rate.numerator) / f64::from(movie.frame_rate.denominator),
    );
    if source.acodec.is_none() {
        source.acodec = movie.audio_codec.clone();
    }
    let original = state
        .library
        .resolve_media_path(&source)
        .await
        .map_err(|_| AppError::conflict("Исходное видео недоступно"))?;
    let source = resource(
        state,
        id,
        project,
        actor,
        headers,
        &original,
        &movie.source_hash,
        "original",
        source,
    )
    .await?;
    let settings = super::person_video_jobs::correction_recipe(
        axum::extract::State(state.clone()),
        axum::extract::Path(id.to_owned()),
        headers.clone(),
    )
    .await?
    .0;
    let corrections: Vec<crate::domain::video_cutout::FrameCorrection> =
        serde_json::from_value(settings["corrections"].clone())
            .map_err(|_| AppError::conflict("Рецепт видеомаски недоступен"))?;
    let entry = MediaEntry::from_result(
        "source",
        &serde_json::json!({"id":"pending","filename":"automatic-masks.mkv","url":"/pending","mediaType":"video","width":automatic.width,"height":automatic.height,"duration":automatic.frames as f64*f64::from(automatic.frame_rate.denominator)/f64::from(automatic.frame_rate.numerator),"fps":f64::from(automatic.frame_rate.numerator)/f64::from(automatic.frame_rate.denominator),"vcodec":"ffv1","title":"Автоматические видеомаски"}),
    );
    let mask = resource(
        state,
        id,
        project,
        actor,
        headers,
        &path,
        &automatic.file_hash,
        "automatic-masks",
        entry,
    )
    .await?;
    let snapshot = crate::domain::video_cutout::Attachment {
        source_id: source.id.clone(),
        mask_source_id: mask.id.clone(),
        source_hash: movie.source_hash,
        mask_hash: automatic.mask_hash,
        mask_file_hash: automatic.file_hash,
        width: movie.width,
        height: movie.height,
        mask_width: automatic.width,
        mask_height: automatic.height,
        frames: movie.frames,
        frame_rate: movie.frame_rate,
        corrections,
        tracked_regions: match settings.pointer("/projectOrigin/snapshot/trackedRegions") {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|_| AppError::conflict("Неверные сохранённые видеорегионы"))?,
            None => Vec::new(),
        },
    };
    snapshot
        .validate()
        .map_err(|_| AppError::conflict("Неверный снимок видеомаски"))?;
    Ok(Accepted {
        media,
        video_cutout: Some(Resources {
            snapshot,
            source,
            mask,
        }),
    })
}

#[derive(Clone, Copy)]
pub(super) enum ResourceOwner {
    PersonVideo,
    Sam,
}
impl ResourceOwner {
    async fn authorize(self, state: &AppState, job: &str, headers: &HeaderMap) -> AppResult<()> {
        match self {
            Self::PersonVideo => {
                super::person_video_jobs::authorize_job(state, job, headers, true).await
            }
            Self::Sam => super::sam_jobs::authorize_job(state, job, headers, true).await,
        }
    }
}
#[allow(clippy::too_many_arguments)]
async fn resource(
    state: &AppState,
    job: &str,
    project: &str,
    actor: &str,
    headers: &HeaderMap,
    input: &Path,
    hash: &str,
    role: &str,
    entry: MediaEntry,
) -> AppResult<MediaEntry> {
    resource_for(
        ResourceOwner::PersonVideo,
        state,
        job,
        project,
        actor,
        headers,
        input,
        hash,
        role,
        entry,
    )
    .await
}
