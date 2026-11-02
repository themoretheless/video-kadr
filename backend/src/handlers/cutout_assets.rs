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
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    project_id: String,
}
pub(super) async fn authorize(state: &AppState, project: &str, actor: &str) -> AppResult<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("authorize cutout media project", e))?;
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accepted {
    #[serde(flatten)]
    pub media: MediaEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cutout: Option<AcceptedMask>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedMask {
    pub snapshot: crate::domain::cutout::Attachment,
    pub source: MediaEntry,
    pub mask: MediaEntry,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<MediaEntry>,
}
async fn accepted(
    state: &AppState,
    id: &str,
    project: &str,
    actor: &str,
    headers: &HeaderMap,
    metadata: &crate::services::local_cutout::Metadata,
    media: MediaEntry,
) -> AppResult<Accepted> {
    let Some(file_hash) = metadata.mask_file_hash.as_ref() else {
        return Ok(Accepted {
            media,
            cutout: None,
        });
    };
    let (_, source_id) = job_access(state, id)
        .await?
        .ok_or_else(|| AppError::not_found("Исходник маски не найден"))?;
    let mut source = state
        .library
        .get(&source_id)
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| AppError::conflict("Исходное изображение недоступно"))?;
    source.media_type = Some("image".into());
    source.width = Some(metadata.width);
    source.height = Some(metadata.height);
    let reserved: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM cutout_project_masks WHERE project_id=? AND mask_file_hash=?",
    )
    .bind(project)
    .bind(file_hash)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read project mask reservation", e))?;
    let mask_id = if let Some(id) = reserved {
        id
    } else {
        let mask = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO cutout_project_masks(project_id,mask_file_hash,source_id) VALUES(?,?,?)",
        )
        .bind(project)
        .bind(file_hash)
        .bind(&mask)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("reserve project mask", e))?;
        mask
    };
    let space = state
        .db
        .claim_project_generated_media(project, actor, &mask_id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    let mask = if let Some(entry) = state.library.get(&mask_id).await {
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::conflict("Сохранённая маска недоступна"))?;
        crate::services::local_cutout::read_verified_mask(&path, metadata)
            .await
            .map_err(|_| AppError::conflict("Сохранённая маска изменилась"))?;
        entry
    } else {
        let directory = crate::services::tts_model_download::directory(
            &state.sources_dir(),
            &std::path::Path::new("spaces").join(&space),
        )
        .await
        .map_err(|e| AppError::internal("create project mask directory", e))?;
        let filename = format!("{mask_id}.png");
        let destination = directory.join(&filename);
        if tokio::fs::symlink_metadata(&destination).await.is_ok() {
            crate::services::local_cutout::read_verified_mask(&destination, metadata)
                .await
                .map_err(|_| AppError::conflict("Сохранённая маска изменилась"))?;
        } else {
            let job_mask = job_mask_output(state, id)
                .await
                .map_err(|e| AppError::internal("resolve job mask", e))?;
            let bytes = crate::services::local_cutout::read_verified_mask(&job_mask, metadata)
                .await
                .map_err(|_| AppError::conflict("Исходная маска недоступна"))?;
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
                .map_err(|e| AppError::internal("stage project mask", e))?;
            file.write_all(&bytes)
                .await
                .map_err(|e| AppError::internal("write project mask", e))?;
            file.sync_all()
                .await
                .map_err(|e| AppError::internal("sync project mask", e))?;
            drop(file);
            authorize(state, project, actor).await?;
            authorize_job(state, id, headers, true).await?;
            tokio::fs::hard_link(&staged.0, &destination)
                .await
                .map_err(|e| AppError::internal("publish project mask", e))?;
        }
        let entry = MediaEntry::from_result(
            "source",
            &json!({"id":mask_id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":metadata.mask_width,"height":metadata.mask_height,"title":"Маска удаления фона"}),
        );
        if !state.library.add(entry.clone()).await {
            return Err(AppError::service_unavailable(
                "Не удалось сохранить маску в медиатеке",
            ));
        }
        state.index_media(&entry).await;
        entry
    };
    let settings: Option<String> =
        sqlx::query_scalar("SELECT settings_json FROM cutout_job_edits WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read accepted mask settings", e))?;
    let mut correction: crate::domain::cutout::Correction = if let Some(json) = settings {
        serde_json::from_str(&json)
            .map_err(|e| AppError::internal("decode accepted mask settings", e))?
    } else {
        crate::domain::cutout::Correction {
            custom: false,
            strokes: vec![],
            feather: 0.,
        }
    };
    correction
        .validate()
        .map_err(|_| AppError::conflict("Неверные сохранённые штрихи"))?;
    let mut regions = Vec::new();
    for attached in correction
        .strokes
        .iter_mut()
        .filter_map(|s| s.region.as_mut())
    {
        if attached.source_id != source_id || attached.source_hash != metadata.source_hash {
            return Err(AppError::conflict("Область относится к другому исходнику"));
        }
        let (replacement, entry) =
            project_region(state, project, actor, attached, metadata).await?;
        *attached = replacement;
        if !regions.iter().any(|e: &MediaEntry| e.id == entry.id) {
            regions.push(entry);
        }
    }
    let subject=job_subject(state,id).await?;
    let snapshot = crate::domain::cutout::Attachment {
        subject,
        source_id,
        mask_source_id: mask.id.clone(),
        source_hash: metadata.source_hash.clone(),
        mask_hash: metadata.mask_hash.clone(),
        mask_file_hash: file_hash.clone(),
        model_hash: metadata.model_hash.clone(),
        runtime_hash: metadata.runtime_hash.clone(),
        correction,
    };
    snapshot
        .validate()
        .map_err(|_| AppError::conflict("Неверные данные маски"))?;
    authorize(state, project, actor).await?;
    authorize_job(state, id, headers, true).await?;
    Ok(Accepted {
        media,
        cutout: Some(AcceptedMask {
            snapshot,
            source,
            mask,
            regions,
        }),
    })
}
async fn project_region(
    state: &AppState,
    project: &str,
    actor: &str,
    region: &crate::domain::cutout::RegionAttachment,
    original: &crate::services::local_cutout::Metadata,
) -> AppResult<(crate::domain::cutout::RegionAttachment, MediaEntry)> {
    let reserved: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM cutout_project_regions WHERE project_id=? AND mask_file_hash=?",
    )
    .bind(project)
    .bind(&region.mask_file_hash)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read project region", e))?;
    let id = if let Some(id) = reserved {
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO cutout_project_regions(project_id,mask_file_hash,source_id) VALUES(?,?,?)",
        )
        .bind(project)
        .bind(&region.mask_file_hash)
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("reserve project region", e))?;
        id
    };
    let space = state
        .db
        .claim_project_generated_media(project, actor, &id)
        .await
        .map_err(|_| AppError::forbidden("Доступ к проекту изменился"))?;
    let entry = if let Some(entry) = state.library.get(&id).await {
        if entry.kind != "source"
            || entry.media_type.as_deref() != Some("image")
            || entry.width != Some(256)
            || entry.height != Some(256)
        {
            return Err(AppError::conflict("Неверные данные области проекта"));
        }
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::conflict("Область проекта недоступна"))?;
        crate::services::local_mask_edit::verify_region_file(
            original,
            &crate::services::local_mask_edit::RegionInput {
                path: &path,
                attachment: region,
            },
        )
        .await
        .map_err(|_| AppError::conflict("Область проекта изменилась"))?;
        entry
    } else {
        let directory = crate::services::tts_model_download::directory(
            &state.sources_dir(),
            &std::path::Path::new("spaces").join(&space),
        )
        .await
        .map_err(|e| AppError::internal("create project region directory", e))?;
        let filename = format!("{id}.png");
        let destination = directory.join(&filename);
        if tokio::fs::symlink_metadata(&destination).await.is_ok() {
            crate::services::local_mask_edit::verify_region_file(
                original,
                &crate::services::local_mask_edit::RegionInput {
                    path: &destination,
                    attachment: region,
                },
            )
            .await
            .map_err(|_| AppError::conflict("Область проекта изменилась"))?;
        } else {
            let source = super::cutout_jobs::source(state, &region.mask_source_id, actor)
                .await
                .map_err(|_| AppError::forbidden("Исходная область недоступна"))?;
            crate::services::local_mask_edit::verify_region_file(
                original,
                &crate::services::local_mask_edit::RegionInput {
                    path: &source,
                    attachment: region,
                },
            )
            .await
            .map_err(|_| AppError::conflict("Исходная область изменилась"))?;
            let bytes = tokio::fs::read(&source)
                .await
                .map_err(|e| AppError::internal("read project region source", e))?;
            if bytes.len() > 1024 * 1024
                || format!("{:x}", Sha256::digest(&bytes)) != region.mask_file_hash
            {
                return Err(AppError::conflict("Исходная область изменилась"));
            }
            let staged = Staged(directory.join(format!(".{id}-{}.part", uuid::Uuid::new_v4())));
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let mut output = options
                .open(&staged.0)
                .await
                .map_err(|e| AppError::internal("stage project region", e))?;
            output
                .write_all(&bytes)
                .await
                .map_err(|e| AppError::internal("write project region", e))?;
            output
                .sync_all()
                .await
                .map_err(|e| AppError::internal("sync project region", e))?;
            drop(output);
            authorize(state, project, actor).await?;
            super::cutout_jobs::source(state, &region.mask_source_id, actor)
                .await
                .map_err(|_| AppError::forbidden("Доступ к области изменился"))?;
            tokio::fs::hard_link(&staged.0, &destination)
                .await
                .map_err(|e| AppError::internal("publish project region", e))?;
        }
        authorize(state, project, actor).await?;
        let entry = MediaEntry::from_result(
            "source",
            &json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":256,"height":256,"title":"Область маски проекта"}),
        );
        if !state.library.add(entry.clone()).await {
            return Err(AppError::service_unavailable(
                "Не удалось сохранить область проекта",
            ));
        }
        state.index_media(&entry).await;
        entry
    };
    authorize(state, project, actor).await?;
    let mut replacement = region.clone();
    replacement.mask_source_id = id;
    Ok((replacement, entry))
}
pub async fn store(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(target): ApiJson<Target>,
) -> AppResult<Json<Accepted>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    authorize(&state, &target.project_id, &actor).await?;
    let _operation = state.enhance_asset_ops.lock().await;
    let (metadata, bytes) = job_result(&state, &id, &headers).await?;
    let reserved: Option<String> = sqlx::query_scalar(
        "SELECT source_id FROM cutout_job_assets WHERE job_id=? AND project_id=?",
    )
    .bind(&id)
    .bind(&target.project_id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| AppError::internal("read cutout media reservation", e))?;
    let source_id = if let Some(id) = reserved {
        id
    } else {
        let source = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO cutout_job_assets(job_id,project_id,source_id) VALUES(?,?,?)")
            .bind(&id)
            .bind(&target.project_id)
            .bind(&source)
            .execute(state.db.pool())
            .await
            .map_err(|e| AppError::internal("reserve cutout media", e))?;
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
            .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?
            .file_type()
            .is_file()
        {
            return Err(AppError::conflict("Неверная сохранённая копия"));
        }
        let (_, hash) =
            crate::project_archive::fingerprint_file_with_limit(&path, 256 * 1024 * 1024)
                .await
                .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?;
        if hash != metadata.output_hash {
            return Err(AppError::conflict("Сохранённая копия изменилась"));
        }
        authorize(&state, &target.project_id, &actor).await?;
        authorize_job(&state, &id, &headers, true).await?;
        return Ok(Json(
            accepted(
                &state,
                &id,
                &target.project_id,
                &actor,
                &headers,
                &metadata,
                entry,
            )
            .await?,
        ));
    }
    let directory = crate::services::tts_model_download::directory(
        &state.sources_dir(),
        &std::path::Path::new("spaces").join(&space),
    )
    .await
    .map_err(|e| AppError::internal("create private cutout media directory", e))?;
    let filename = format!("{source_id}.png");
    let destination = directory.join(&filename);
    if destination
        .try_exists()
        .map_err(|e| AppError::internal("inspect cutout media", e))?
    {
        if !tokio::fs::symlink_metadata(&destination)
            .await
            .map_err(|_| AppError::conflict("Сохранённая копия недоступна"))?
            .file_type()
            .is_file()
        {
            return Err(AppError::conflict("Неверная сохранённая копия"));
        }
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
            .map_err(|e| AppError::internal("stage cutout media", e))?;
        file.write_all(&bytes)
            .await
            .map_err(|e| AppError::internal("write cutout media", e))?;
        file.sync_all()
            .await
            .map_err(|e| AppError::internal("sync cutout media", e))?;
        drop(file);
        tokio::fs::hard_link(&staged.0, &destination)
            .await
            .map_err(|e| AppError::internal("publish cutout media", e))?;
    }
    authorize(&state, &target.project_id, &actor).await?;
    authorize_job(&state, &id, &headers, true).await?;
    let entry = MediaEntry::from_result(
        "source",
        &json!({"id":source_id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":metadata.width,"height":metadata.height,"title":"Изображение без фона","sizeBytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}),
    );
    if !state.library.add(entry.clone()).await {
        return Err(AppError::service_unavailable(
            "Не удалось зарегистрировать копию в медиатеке",
        ));
    }
    state.index_media(&entry).await;
    Ok(Json(
        accepted(
            &state,
            &id,
            &target.project_id,
            &actor,
            &headers,
            &metadata,
            entry,
        )
        .await?,
    ))
}

pub(super) async fn job_access(state: &AppState, id: &str) -> AppResult<Option<(String, String)>> {
    if let Some(value) = super::selfie_jobs::access(state, id).await? {
        Ok(Some(value))
    } else {
        super::cutout_jobs::access(state, id).await
    }
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    source: bool,
) -> AppResult<()> {
    if super::selfie_jobs::access(state, id).await?.is_some() {
        super::selfie_jobs::authorize_job(state, id, headers, source).await
    } else {
        super::cutout_jobs::authorize_job(state, id, headers, source).await
    }
}
pub(super) async fn job_mask_output(
    state: &AppState,
    id: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if super::selfie_jobs::access(state, id)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .is_some()
    {
        super::selfie_jobs::mask_output(state, id)
    } else {
        super::cutout_jobs::mask_output(state, id)
    }
}
async fn job_result(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(crate::services::local_cutout::Metadata, Vec<u8>)> {
    if super::selfie_jobs::access(state, id).await?.is_some() {
        let (metadata, bytes) = super::selfie_jobs::checked_result(state, id, headers).await?;
        let metadata = serde_json::from_value(
            serde_json::to_value(metadata)
                .map_err(|e| AppError::internal("encode Selfie result", e))?,
        )
        .map_err(|e| AppError::internal("convert Selfie result", e))?;
        Ok((metadata, bytes))
    } else {
        super::cutout_jobs::checked_result(state, id, headers).await
    }
}

/// Provenance retained alongside correction recipes after work payload scrubbing.
pub(super) async fn job_subject(state:&AppState,id:&str)->AppResult<Option<crate::domain::concat_cutout_import::Subject>>{
 let subject:Option<String>=sqlx::query_scalar("SELECT subject_json FROM cutout_job_subjects WHERE job_id=?").bind(id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read cutout subject",e))?;
 subject.map(|json|serde_json::from_str(&json).map_err(|_|AppError::conflict("Неверный subject сохранённого анализа"))).transpose()
}
