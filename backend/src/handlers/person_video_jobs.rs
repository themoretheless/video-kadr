//! Durable private learned-video jobs. User input contains media IDs, never paths.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::{local_enhance_video, local_person, local_person_video},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    source_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Work {
    schema_version: u32,
    owner: String,
    request: Request,
    source_hash: String,
    engine_hash: String,
    tools_hash: String,
    #[serde(default)]
    correction: Option<CorrectionWork>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CorrectionWork {
    pub(super) base_job_id: String,
    pub(super) mask_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) project_origin: Option<super::person_video_project::Origin>,
    pub(super) corrections: Vec<crate::services::local_person_correct::FrameCorrection>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorrectionRequest {
    corrections: Vec<crate::services::local_person_correct::FrameCorrection>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    cursor: Option<String>,
    limit: Option<u32>,
    source_id: Option<String>,
}
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Размер страницы должен быть 1–50"));
    }
    let anchor = if let Some(cursor) = page.cursor {
        let created:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN person_video_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=? AND (? IS NULL OR a.source_id=?)").bind(&cursor).bind(&owner).bind(&page.source_id).bind(&page.source_id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read person video cursor",e))?;
        Some((
            created.ok_or_else(|| AppError::bad_request("Некорректная страница обработок"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.source_id,j.created_at FROM jobs j JOIN person_video_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR a.source_id=?) AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(&page.source_id).bind(&page.source_id).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list person videos",e))?;
    let next = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|row| row.0.clone())
    } else {
        None
    };
    let mut jobs = vec![];
    for (id, source, created_at) in rows.into_iter().take(limit as usize) {
        if let Some(space) = state
            .db
            .source_space_id(&source)
            .await
            .map_err(|e| AppError::internal("read person video source space", e))?
        {
            if !state
                .db
                .is_space_member(&space, &owner)
                .await
                .map_err(|e| AppError::internal("authorize person video history", e))?
            {
                continue;
            }
        }
        if !state
            .library
            .get(&source)
            .await
            .is_some_and(|entry| entry.kind == "source")
        {
            continue;
        }
        if let Some(job) = state
            .get_job(&id)
            .await
            .map_err(|e| AppError::internal("read person video history status", e))?
        {
            jobs.push(json!({"jobId":id,"sourceId":source,"createdAt":created_at,"status":job.status,"progress":job.progress}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}
impl Work {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        if let Some(edit) = &self.correction {
            anyhow::ensure!(
                uuid::Uuid::parse_str(&edit.base_job_id)?.to_string() == edit.base_job_id
                    && edit.mask_hash.len() == 64
                    && edit.mask_hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid correction reference"
            );
            if let Some(origin) = &edit.project_origin {
                origin.validate()?;
                anyhow::ensure!(
                    origin.snapshot.source_id == self.request.source_id
                        && origin.snapshot.source_hash == self.source_hash
                        && origin.snapshot.mask_hash == edit.mask_hash,
                    "invalid project video origin"
                );
            }
            if let Some(origin) = &edit.project_origin {
                crate::domain::video_cutout::validate_corrections_with_regions(
                    &edit.corrections,
                    origin.snapshot.frames,
                    &origin.snapshot.tracked_regions,
                )?;
            } else {
                crate::services::local_person_correct::validate_edits(&edit.corrections, 360000)?;
            }
        }

        anyhow::ensure!(
            self.schema_version == 1
                && !self.owner.is_empty()
                && [&self.source_hash, &self.engine_hash, &self.tools_hash]
                    .iter()
                    .all(|hash| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())),
            "invalid person video snapshot"
        );
        Ok(())
    }
}
#[derive(Debug)]
enum Invariant {
    Access,
    Source,
    Engine,
}
impl std::fmt::Display for Invariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Access => "person video source access revoked",
            Self::Source => "person video source changed",
            Self::Engine => "person video engine changed",
        })
    }
}
impl std::error::Error for Invariant {}
pub(super) async fn source(state: &AppState, id: &str, owner: &str) -> anyhow::Result<PathBuf> {
    if let Some(space) = state.db.source_space_id(id).await? {
        if !state.db.is_space_member(&space, owner).await? {
            return Err(Invariant::Access.into());
        }
    }
    let entry = state
        .library
        .get(id)
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| anyhow::anyhow!("person video source missing"))?;
    Ok(state.library.resolve_media_path(&entry).await?)
}
async fn access(state: &AppState, id: &str) -> AppResult<Option<(String, String)>> {
    sqlx::query_as("SELECT owner,source_id FROM person_video_job_access WHERE job_id=?")
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read person video owner", e))
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    require_source: bool,
) -> AppResult<()> {
    if let Some((owner, source)) = access(state, id).await? {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        if actor != owner {
            return Err(AppError::forbidden(
                "Это удаление фона другого пользователя",
            ));
        }
        if require_source {
            super::library::authorize_source_access(state, &source, headers).await?;
            if let Some(origin) = project_origin(state, id).await? {
                super::person_video_project::authorize_resources(state, &actor, &origin)
                    .await
                    .map_err(|_| {
                        AppError::forbidden("Нет доступа к ресурсам проектной видеомаски")
                    })?;
            }
        }
    }
    Ok(())
}
fn output(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid person video ID"
    );
    Ok(state
        .storage
        .join("person-video-jobs")
        .join(format!("{id}.mov")))
}
fn masks(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    Ok(output(state, id)?.with_extension("masks"))
}
async fn remove_outputs(state: &AppState, id: &str) {
    for path in [
        output(state, id),
        masks(state, id),
        output(state, id).map(|p| p.with_extension("automatic.mkv")),
        output(state, id).map(|p| p.with_extension("original.masks")),
    ]
    .into_iter()
    .flatten()
    {
        let _ = tokio::fs::remove_file(path).await;
    }
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    let path = source(&state, &request.source_id, &owner)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if !state
        .library
        .get(&request.source_id)
        .await
        .is_some_and(|entry| entry.media_type.as_deref() == Some("video"))
    {
        return Err(AppError::bad_request("Выберите видео из медиатеки"));
    }
    let source_hash =
        crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
            .await
            .map_err(|_| AppError::bad_request("Исходник недоступен или слишком велик"))?
            .1;
    let config = state
        .person
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("RVM не настроен"))?;
    let engine = local_person::identity(&config)
        .await
        .map_err(|_| AppError::service_unavailable("Модель удаления фона недоступна"))?;
    let codecs = local_enhance_video::resolve_tools()
        .await
        .map_err(|_| AppError::service_unavailable("Видеокодеки не настроены"))?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO person_video_job_access(job_id,owner,source_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&request.source_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save person video access", e))?;
    let work = Work {
        schema_version: 1,
        owner,
        request,
        source_hash,
        engine_hash: engine.hash,
        tools_hash: codecs.identity,
        correction: None,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize person video", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::PersonVideo,
            &payload,
            &format!("person-video:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue person video", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
pub(super) async fn checked_result(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(local_person_video::Metadata, PathBuf)> {
    if access(state, id).await?.is_none() {
        return Err(AppError::not_found("Удаление фона видео не найдено"));
    }
    authorize_job(state, id, headers, true).await?;
    let job = state
        .get_job(id)
        .await
        .map_err(|e| AppError::internal("read video person video", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Удаление фона видео ещё не готово"));
    }
    let metadata: local_person_video::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат видео"))?;
    let path = output(state, id).map_err(|_| AppError::not_found("Неверное задание"))?;
    let file = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::conflict("Файл удаления фона недоступен"))?;
    if !file.file_type().is_file() {
        return Err(AppError::conflict("Файл удаления фона недоступен"));
    }
    let (_, hash) =
        crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
            .await
            .map_err(|_| AppError::conflict("Файл удаления фона недоступен"))?;
    if hash != metadata.output_hash {
        return Err(AppError::conflict("Файл удаления фона изменился"));
    }
    if let Some(origin) = project_origin(state, id).await? {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        super::person_video_project::resources(state, &actor, &origin)
            .await
            .map_err(|_| AppError::conflict("Ресурсы проектной видеомаски изменились"))?;
    }
    authorize_job(state, id, headers, true).await?;
    Ok((metadata, path))
}
pub async fn video(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::http::Request<Body>,
) -> AppResult<Response> {
    use tower::ServiceExt;
    let (_, path) = checked_result(&state, &id, request.headers()).await?;
    let mut response = tower_http::services::ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| AppError::internal("serve person video", e))?
        .map(Body::new);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("video/quicktime"),
    );
    Ok(response)
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: Work,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        if !JobService::mark_queued(&st, &id).await {
            st.clear_cancel(&id).await;
            return;
        }
        let Some(_guards) = JobService::acquire_execution(
            &st,
            &id,
            &token,
            None,
            AcquirePlan {
                resource_class: ResourceClass::Analysis,
                acquire_render_permit: false,
            },
        )
        .await
        else {
            return;
        };
        if !JobService::mark_running(&st, &id, "person_video", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), rx);
        let outcome: anyhow::Result<Value> = async {
            let operation_deadline =
                tokio::time::Instant::now() + std::time::Duration::from_secs(1800);
            let path = source(&st, &work.request.source_id, &work.owner).await?;
            anyhow::ensure!(
                crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
                    .await?
                    .1
                    == work.source_hash,
                Invariant::Source
            );
            let config = st.person.as_ref().as_ref().ok_or(Invariant::Engine)?;
            let engine = if work.correction.is_some() {
                crate::services::local_mask_edit::identity(&config.executable)
                    .await
                    .map_err(|_| Invariant::Engine)?
            } else {
                local_person::identity(config)
                    .await
                    .map_err(|_| Invariant::Engine)?
                    .hash
            };
            anyhow::ensure!(engine == work.engine_hash, Invariant::Engine);
            let codecs = local_enhance_video::resolve_tools()
                .await
                .map_err(|_| Invariant::Engine)?;
            anyhow::ensure!(codecs.identity == work.tools_hash, Invariant::Engine);
            let target = output(&st, &id)?;
            let parent = target.parent().unwrap();
            tokio::fs::create_dir_all(parent).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
            }
            let mask = masks(&st, &id)?;
            let automatic = target.with_extension("automatic.mkv");
            for target in [&target, &mask, &automatic] {
                if target.try_exists()? {
                    anyhow::ensure!(
                        tokio::fs::symlink_metadata(&target)
                            .await?
                            .file_type()
                            .is_file(),
                        "unexpected person video output"
                    );
                    tokio::fs::remove_file(&target).await?;
                }
            }
            if let Some(edit) = &work.correction {
                if let Some(origin) = &edit.project_origin {
                    let (_, movie) =
                        super::person_video_project::inputs(&st, &work.owner, origin).await?;
                    let raw = target.with_extension("original.masks");
                    if raw.try_exists()? {
                        anyhow::ensure!(
                            tokio::fs::symlink_metadata(&raw)
                                .await?
                                .file_type()
                                .is_file(),
                            "invalid restored mask output"
                        );
                        tokio::fs::remove_file(&raw).await?;
                    }
                    let (updates, mut receiver) = tokio::sync::mpsc::unbounded_channel();
                    let sender = tx.clone();
                    let drain = tokio::spawn(async move {
                        while let Some(value) = receiver.recv().await {
                            let _ = sender.send(value * 0.05);
                        }
                    });
                    let snapshot = &origin.snapshot;
                    let decoded = crate::services::local_person_mask_video::decode(
                        &st.process_runtime,
                        crate::services::local_person_mask_video::Tools {
                            ffmpeg: &codecs.ffmpeg,
                            ffprobe: &codecs.ffprobe,
                        },
                        &movie,
                        &crate::services::local_person_mask_video::Metadata {
                            width: snapshot.mask_width,
                            height: snapshot.mask_height,
                            frames: snapshot.frames,
                            frame_rate: snapshot.frame_rate,
                            mask_hash: snapshot.mask_hash.clone(),
                            file_hash: snapshot.mask_file_hash.clone(),
                            size_bytes: tokio::fs::metadata(&movie).await?.len(),
                        },
                        &raw,
                        64 * 1024 * 1024 * 1024,
                        &token,
                        operation_deadline.saturating_duration_since(tokio::time::Instant::now()),
                        &updates,
                    )
                    .await;
                    drop(updates);
                    let _ = drain.await;
                    decoded?;
                }
            }
            let (movie_updates, mut movie_progress) = tokio::sync::mpsc::unbounded_channel();
            let updates = tx.clone();
            let movie_drain = tokio::spawn(async move {
                while let Some(value) = movie_progress.recv().await {
                    let _ = updates.send(5. + value * 0.80);
                }
            });
            let frame_result: anyhow::Result<local_person_video::Metadata> = async {
                Ok(if let Some(edit) = &work.correction {
                    let original = original_masks(
                        &st,
                        &work.owner,
                        &work.request.source_id,
                        &work.source_hash,
                        edit,
                    )
                    .await?;
                    let input = local_person_video::CorrectionInput {
                        video: local_person_video::Input {
                            source: &path,
                            source_hash: &work.source_hash,
                            mask_destination: Some(&mask),
                            staging_bytes: 64 * 1024 * 1024 * 1024,
                        },
                        masks: &original,
                        mask_hash: &edit.mask_hash,
                        corrections: &edit.corrections,
                    };
                    let resolved = if let Some(origin) = &edit.project_origin {
                        super::person_video_project::used_regions(
                            &st,
                            &work.owner,
                            origin,
                            &edit.corrections,
                        )
                        .await?
                    } else {
                        Vec::new()
                    };
                    let regions = resolved
                        .iter()
                        .map(|(path, attachment)| {
                            crate::services::local_person_video_regions::EncodedRegion {
                                video: path,
                                attachment,
                            }
                        })
                        .collect::<Vec<_>>();
                    if regions.is_empty() {
                        local_person_video::correct(
                            &st.process_runtime,
                            local_person_video::Tools {
                                ffmpeg: &codecs.ffmpeg,
                                ffprobe: &codecs.ffprobe,
                            },
                            &config.executable,
                            input,
                            &target,
                            &token,
                            operation_deadline
                                .saturating_duration_since(tokio::time::Instant::now()),
                            &movie_updates,
                        )
                        .await?
                    } else {
                        crate::services::local_person_video_regions::correct(
                            &st.process_runtime,
                            local_person_video::Tools {
                                ffmpeg: &codecs.ffmpeg,
                                ffprobe: &codecs.ffprobe,
                            },
                            &config.executable,
                            input,
                            &regions,
                            &target,
                            &token,
                            operation_deadline
                                .saturating_duration_since(tokio::time::Instant::now()),
                            &movie_updates,
                        )
                        .await?
                    }
                } else {
                    local_person_video::segment(
                        &st.process_runtime,
                        local_person_video::Tools {
                            ffmpeg: &codecs.ffmpeg,
                            ffprobe: &codecs.ffprobe,
                        },
                        &config,
                        local_person_video::Input {
                            source: &path,
                            source_hash: &work.source_hash,
                            mask_destination: Some(&mask),
                            staging_bytes: 64 * 1024 * 1024 * 1024,
                        },
                        &target,
                        &token,
                        operation_deadline.saturating_duration_since(tokio::time::Instant::now()),
                        &movie_updates,
                    )
                    .await?
                })
            }
            .await;
            drop(movie_updates);
            let _ = movie_drain.await;
            let mut result = frame_result?;
            let (original, original_hash) = if let Some(edit) = &work.correction {
                (
                    original_masks(
                        &st,
                        &work.owner,
                        &work.request.source_id,
                        &work.source_hash,
                        edit,
                    )
                    .await?,
                    edit.mask_hash.clone(),
                )
            } else {
                (mask.clone(), result.mask_hash.clone())
            };
            let (mask_updates, mut mask_progress) = tokio::sync::mpsc::unbounded_channel();
            let updates = tx.clone();
            let mask_drain = tokio::spawn(async move {
                while let Some(value) = mask_progress.recv().await {
                    let _ = updates.send(85. + value * 0.14);
                }
            });
            let encoded = crate::services::local_person_mask_video::encode(
                &st.process_runtime,
                crate::services::local_person_mask_video::Tools {
                    ffmpeg: &codecs.ffmpeg,
                    ffprobe: &codecs.ffprobe,
                },
                crate::services::local_person_mask_video::Input {
                    masks: &original,
                    mask_hash: &original_hash,
                    width: result.mask_width,
                    height: result.mask_height,
                    frames: result.frames,
                    frame_rate: result.frame_rate,
                    staging_bytes: 64 * 1024 * 1024 * 1024,
                },
                &automatic,
                &token,
                operation_deadline.saturating_duration_since(tokio::time::Instant::now()),
                &mask_updates,
            )
            .await;
            drop(mask_updates);
            let _ = mask_drain.await;
            result.automatic_mask_video = Some(encoded?);

            let current = source(&st, &work.request.source_id, &work.owner).await?;
            anyhow::ensure!(
                crate::project_archive::fingerprint_file_with_limit(
                    &current,
                    64 * 1024 * 1024 * 1024
                )
                .await?
                .1 == work.source_hash,
                Invariant::Source
            );
            let engine = if let Some(edit) = &work.correction {
                original_masks(
                    &st,
                    &work.owner,
                    &work.request.source_id,
                    &work.source_hash,
                    edit,
                )
                .await?;
                crate::services::local_mask_edit::identity(&config.executable).await?
            } else {
                local_person::identity(config).await?.hash
            };
            anyhow::ensure!(engine == work.engine_hash, Invariant::Engine);
            anyhow::ensure!(
                local_enhance_video::resolve_tools().await?.identity == work.tools_hash,
                Invariant::Engine
            );
            Ok(serde_json::to_value(result)?)
        }
        .await;
        drop(tx);
        let _ = drain.await;
        if token.is_cancelled() {
            if !st.is_shutting_down() {
                remove_outputs(&st, &id).await;
            }
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => {
                    remove_outputs(&st, &id).await;
                    JobEvent::Failed {
                        kind: match error.downcast_ref::<Invariant>() {
                            Some(Invariant::Access) => crate::jobs::ErrorKind::Security,
                            Some(Invariant::Source | Invariant::Engine) => {
                                crate::jobs::ErrorKind::Validation
                            }
                            None => match error
                                .downcast_ref::<super::person_video_project::Invariant>()
                            {
                                Some(super::person_video_project::Invariant::Access) => {
                                    crate::jobs::ErrorKind::Security
                                }
                                Some(super::person_video_project::Invariant::Snapshot) => {
                                    crate::jobs::ErrorKind::Validation
                                }
                                None => crate::jobs::classify_job_error(&error),
                            },
                        },
                        message: crate::privacy::redact_text(&error.to_string()),
                    }
                }
            };
            JobService::transition(&st, &id, event).await;
            st.clear_cancel(&id).await;
        }
    });
}

/// Host capability only; do not expose configured filesystem paths or digests.
pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    super::publish::authenticated_actor(&state, &headers).await?;
    let available = if let Some(config) = state.person.as_ref().as_ref() {
        local_person::identity(config).await.is_ok()
            && local_enhance_video::resolve_tools().await.is_ok()
    } else {
        false
    };
    let correction_available = if let Some(config) = state.person.as_ref().as_ref() {
        crate::services::local_mask_edit::identity(&config.executable)
            .await
            .is_ok()
            && local_enhance_video::resolve_tools().await.is_ok()
    } else {
        false
    };
    Ok(Json(
        json!({"available":available,"correctionAvailable":correction_available,"model":"rvm-mobilenetv3","video":true,"alpha":true,"timing":"cfr","color":"sdr","maxComponentBits":8}),
    ))
}

async fn checked_masks(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(local_person_video::Metadata, PathBuf)> {
    let (metadata, _) = checked_result(&state, &id, headers).await?;
    let path = verify_mask_file(state, id, &metadata).await?;
    authorize_job(&state, &id, headers, true).await?;
    Ok((metadata, path))
}

async fn verify_mask_file(
    state: &AppState,
    id: &str,
    metadata: &local_person_video::Metadata,
) -> AppResult<PathBuf> {
    let path = masks(&state, &id).map_err(|_| AppError::not_found("Неверное задание"))?;
    if ![metadata.mask_width, metadata.mask_height]
        .iter()
        .all(|n| (64..=512).contains(n) && n % 16 == 0)
    {
        return Err(AppError::conflict("Неверные размеры видеомасок"));
    }
    let expected = u64::from(metadata.mask_width)
        .checked_mul(u64::from(metadata.mask_height))
        .and_then(|n| n.checked_mul(metadata.frames))
        .filter(|n| *n > 0 && *n <= 64 * 1024 * 1024 * 1024)
        .ok_or_else(|| AppError::conflict("Неверный размер видеомасок"))?;
    let info = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::conflict("Видеомаски недоступны"))?;
    if !info.file_type().is_file() || info.len() != expected {
        return Err(AppError::conflict("Видеомаски повреждены"));
    }
    let (_, hash) = crate::project_archive::fingerprint_file_with_limit(&path, expected)
        .await
        .map_err(|_| AppError::conflict("Видеомаски недоступны"))?;
    if hash != metadata.mask_hash {
        return Err(AppError::conflict("Видеомаски изменились"));
    }
    Ok(path)
}
async fn original_masks(
    state: &AppState,
    owner: &str,
    source_id: &str,
    source_hash: &str,
    edit: &CorrectionWork,
) -> anyhow::Result<PathBuf> {
    if let Some(origin) = &edit.project_origin {
        super::person_video_project::inputs(state, owner, origin).await?;
        anyhow::ensure!(
            origin.snapshot.source_id == source_id
                && origin.snapshot.source_hash == source_hash
                && origin.snapshot.mask_hash == edit.mask_hash,
            Invariant::Source
        );
        let path = output(state, &edit.base_job_id)?.with_extension("original.masks");
        let expected = u64::from(origin.snapshot.mask_width)
            * u64::from(origin.snapshot.mask_height)
            * origin.snapshot.frames;
        let info = tokio::fs::symlink_metadata(&path).await?;
        anyhow::ensure!(
            info.file_type().is_file() && info.len() == expected,
            Invariant::Source
        );
        anyhow::ensure!(
            crate::project_archive::fingerprint_file_with_limit(&path, expected)
                .await?
                .1
                == edit.mask_hash,
            Invariant::Source
        );
        return Ok(path);
    }
    let access: Option<(String, String)> =
        sqlx::query_as("SELECT owner,source_id FROM person_video_job_access WHERE job_id=?")
            .bind(&edit.base_job_id)
            .fetch_optional(state.db.pool())
            .await?;
    let access = access.ok_or(Invariant::Access)?;
    anyhow::ensure!(
        access.0 == owner && access.1 == source_id,
        Invariant::Access
    );
    let job = state
        .get_job(&edit.base_job_id)
        .await?
        .ok_or(Invariant::Source)?;
    anyhow::ensure!(
        job.status == crate::model::JobStatus::Done,
        Invariant::Source
    );
    let metadata: local_person_video::Metadata =
        serde_json::from_value(job.result.ok_or(Invariant::Source)?)?;
    anyhow::ensure!(
        metadata.correction.is_none()
            && metadata.source_hash == source_hash
            && metadata.mask_hash == edit.mask_hash,
        Invariant::Source
    );
    verify_mask_file(state, &edit.base_job_id, &metadata)
        .await
        .map_err(|_| Invariant::Source.into())
}
pub async fn mask_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::http::Request<Body>,
) -> AppResult<Response> {
    use tower::ServiceExt;
    let (_, path) = checked_masks(&state, &id, request.headers()).await?;
    let mut response = tower_http::services::ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| AppError::internal("serve person video masks", e))?
        .map(Body::new);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/octet-stream"),
    );
    Ok(response)
}

pub async fn mask_frame(
    State(state): State<AppState>,
    Path((id, frame)): Path<(String, u64)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let (metadata, mut path) = checked_masks(&state, &id, &headers).await?;
    if let Some(origin) = project_origin(&state, &id).await? {
        let actor = super::publish::authenticated_actor(&state, &headers).await?;
        super::person_video_project::resources(&state, &actor, &origin)
            .await
            .map_err(|_| AppError::conflict("Ресурсы видеомаски изменились"))?;
        path = output(&state, &id)
            .map_err(|_| AppError::not_found("Неверное задание"))?
            .with_extension("original.masks");
        let expected = u64::from(origin.snapshot.mask_width)
            * u64::from(origin.snapshot.mask_height)
            * origin.snapshot.frames;
        let info = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|_| AppError::conflict("Исходные видеомаски недоступны"))?;
        if !info.file_type().is_file()
            || info.len() != expected
            || crate::project_archive::fingerprint_file_with_limit(&path, expected)
                .await
                .map_err(|_| AppError::conflict("Исходные видеомаски недоступны"))?
                .1
                != origin.snapshot.mask_hash
        {
            return Err(AppError::conflict("Исходные видеомаски изменились"));
        }
    }
    if frame >= metadata.frames {
        return Err(AppError::bad_request("Кадр вне видеомаски"));
    }
    let size = u64::from(metadata.mask_width) * u64::from(metadata.mask_height);
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::internal("open video mask frame", e))?;
    file.seek(std::io::SeekFrom::Start(frame * size))
        .await
        .map_err(|e| AppError::internal("seek video mask frame", e))?;
    let mut bytes = vec![0u8; size as usize];
    file.read_exact(&mut bytes)
        .await
        .map_err(|e| AppError::internal("read video mask frame", e))?;
    let encoded = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let image = image::GrayImage::from_raw(metadata.mask_width, metadata.mask_height, bytes)
            .ok_or_else(|| anyhow::anyhow!("invalid frame shape"))?;
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image).write_to(&mut encoded, image::ImageFormat::Png)?;
        Ok(encoded.into_inner())
    })
    .await
    .map_err(|e| AppError::internal("join video mask PNG", e))?
    .map_err(|e| AppError::internal("encode video mask PNG", e))?;
    authorize_job(&state, &id, &headers, true).await?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(encoded))
        .map_err(|e| AppError::internal("video mask PNG response", e))
}

pub async fn correction_recipe(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let (metadata, _) = checked_result(&state, &id, &headers).await?;
    let row:Option<(String,String,String)>=sqlx::query_as("SELECT base_job_id,mask_hash,corrections_json FROM person_video_job_corrections WHERE job_id=?").bind(&id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read video correction recipe",e))?;
    if let Some((base, hash, recipe)) = row {
        let corrections: Vec<crate::services::local_person_correct::FrameCorrection> =
            serde_json::from_str(&recipe)
                .map_err(|_| AppError::conflict("Неверная правка маски"))?;
        let origin = project_origin(&state, &id).await?;
        let digest = if let Some(origin) = &origin {
            crate::domain::video_cutout::validate_corrections_with_regions(
                &corrections,
                metadata.frames,
                &origin.snapshot.tracked_regions,
            )
        } else {
            crate::services::local_person_correct::validate_edits(&corrections, metadata.frames)
        }
        .map_err(|_| AppError::conflict("Неверная правка маски"))?;
        if !metadata
            .correction
            .as_ref()
            .is_some_and(|p| p.source_mask_hash == hash && p.correction_hash == digest)
        {
            return Err(AppError::conflict("Правка маски изменилась"));
        }
        if let Some(origin) = origin {
            let actor = super::publish::authenticated_actor(&state, &headers).await?;
            super::person_video_project::resources(&state, &actor, &origin)
                .await
                .map_err(|_| AppError::conflict("Проектная маска изменилась"))?;
            let used = corrections
                .iter()
                .flat_map(|e| {
                    e.correction
                        .strokes
                        .iter()
                        .filter_map(|s| s.region.as_ref().map(|r| r.mask_source_id.as_str()))
                })
                .collect::<std::collections::BTreeSet<_>>();
            let expected = origin
                .snapshot
                .tracked_regions
                .iter()
                .filter(|r| used.contains(r.mask_source_id.as_str()))
                .map(|r| (r.mask_source_id.clone(), r.mask_hash.clone()))
                .collect::<std::collections::BTreeMap<_, _>>();
            if metadata
                .correction
                .as_ref()
                .is_none_or(|p| p.region_hashes != expected)
            {
                return Err(AppError::conflict("Привязки видеорегионов изменились"));
            }
            if origin.snapshot.mask_hash != hash
                || origin.snapshot.source_hash != metadata.source_hash
            {
                return Err(AppError::conflict("Проектная правка изменилась"));
            }
            return Ok(Json(
                json!({"baseJobId":base,"maskHash":hash,"corrections":corrections,"projectOrigin":origin}),
            ));
        }
        let (original, _) = checked_masks(&state, &base, &headers).await?;
        if original.correction.is_some()
            || original.mask_hash != hash
            || original.source_hash != metadata.source_hash
        {
            return Err(AppError::conflict("Исходная маска изменилась"));
        }
        Ok(Json(
            json!({"baseJobId":base,"maskHash":hash,"corrections":corrections}),
        ))
    } else {
        if metadata.correction.is_some() {
            return Err(AppError::conflict("Правка маски недоступна"));
        }
        Ok(Json(
            json!({"baseJobId":id,"maskHash":metadata.mask_hash,"corrections":[]}),
        ))
    }
}
pub async fn submit_correction(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<CorrectionRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let (prior, _) = checked_result(&state, &id, &headers).await?;
    if let Some(origin) = project_origin(&state, &id).await? {
        return super::person_video_project::continue_job(
            &state,
            owner,
            origin,
            request.corrections,
        )
        .await;
    }

    let base: Option<String> =
        sqlx::query_scalar("SELECT base_job_id FROM person_video_job_corrections WHERE job_id=?")
            .bind(&id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("resolve original video masks", e))?;
    if prior.correction.is_some() != base.is_some() {
        return Err(AppError::conflict("Неверная ссылка на исходную маску"));
    }
    let base = base.unwrap_or(id);
    let (metadata, _) = checked_masks(&state, &base, &headers).await?;
    if prior.source_hash != metadata.source_hash
        || prior
            .correction
            .as_ref()
            .is_some_and(|p| p.source_mask_hash != metadata.mask_hash)
    {
        return Err(AppError::conflict("Ссылка на исходную маску изменилась"));
    }
    if metadata.correction.is_some() {
        return Err(AppError::conflict("Нужна исходная автоматическая маска"));
    }
    crate::services::local_person_correct::validate_edits(&request.corrections, metadata.frames)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let (_, source_id) = access(&state, &base)
        .await?
        .ok_or_else(|| AppError::not_found("Исходное задание не найдено"))?;
    let source = source(&state, &source_id, &owner)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let source_hash =
        crate::project_archive::fingerprint_file_with_limit(&source, 64 * 1024 * 1024 * 1024)
            .await
            .map_err(|_| AppError::conflict("Исходник недоступен"))?
            .1;
    if source_hash != metadata.source_hash {
        return Err(AppError::conflict("Исходник изменился"));
    }
    let edit = CorrectionWork {
        base_job_id: base,
        mask_hash: metadata.mask_hash,
        project_origin: None,
        corrections: request.corrections,
    };
    let id = uuid::Uuid::new_v4().to_string();
    enqueue_correction(&state, id, owner, source_id, source_hash, edit).await
}
pub(super) async fn enqueue_correction(
    state: &AppState,
    id: String,
    owner: String,
    source_id: String,
    source_hash: String,
    edit: CorrectionWork,
) -> AppResult<(StatusCode, Json<Value>)> {
    let config = state
        .person
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Нативный обработчик не настроен"))?;
    let engine_hash = crate::services::local_mask_edit::identity(&config.executable)
        .await
        .map_err(|_| AppError::service_unavailable("Нативный обработчик недоступен"))?;
    let codecs = local_enhance_video::resolve_tools()
        .await
        .map_err(|_| AppError::service_unavailable("Видеокодеки не настроены"))?;
    let mut transaction = state
        .db
        .pool()
        .begin()
        .await
        .map_err(|e| AppError::internal("begin video correction", e))?;
    sqlx::query("INSERT INTO person_video_job_access(job_id,owner,source_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&source_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| AppError::internal("record video correction access", e))?;
    sqlx::query("INSERT INTO person_video_job_corrections(job_id,base_job_id,mask_hash,corrections_json) VALUES(?,?,?,?)").bind(&id).bind(&edit.base_job_id).bind(&edit.mask_hash).bind(serde_json::to_string(&edit.corrections).map_err(|e|AppError::internal("serialize video correction",e))?).execute(&mut *transaction).await.map_err(|e|AppError::internal("record video correction recipe",e))?;
    if let Some(origin) = &edit.project_origin {
        sqlx::query("INSERT INTO person_video_project_origins(job_id,origin_json) VALUES(?,?)")
            .bind(&id)
            .bind(
                serde_json::to_string(origin)
                    .map_err(|e| AppError::internal("serialize project video origin", e))?,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|e| AppError::internal("record project video origin", e))?;
    }
    transaction
        .commit()
        .await
        .map_err(|e| AppError::internal("commit video correction", e))?;
    let work = Work {
        schema_version: 1,
        owner,
        request: Request { source_id },
        source_hash,
        engine_hash,
        tools_hash: codecs.identity,
        correction: Some(edit),
    };
    let payload = serde_json::to_value(work)
        .map_err(|e| AppError::internal("serialize video correction work", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::PersonVideo,
            &payload,
            &format!("person-video:{id}"),
        )
        .await;
    if !matches!(
        &outcome,
        Ok(EnqueueOutcome::Created(_) | EnqueueOutcome::Existing(_))
    ) {
        let mut cleanup = state
            .db
            .pool()
            .begin()
            .await
            .map_err(|e| AppError::internal("begin unqueued correction cleanup", e))?;
        for table in [
            "person_video_project_origins",
            "person_video_job_corrections",
            "person_video_job_access",
        ] {
            let sql = format!(
                "DELETE FROM {table} WHERE job_id=? AND NOT EXISTS(SELECT 1 FROM jobs WHERE id=?)"
            );
            sqlx::query(&sql)
                .bind(&id)
                .bind(&id)
                .execute(&mut *cleanup)
                .await
                .map_err(|e| AppError::internal("remove unqueued correction", e))?;
        }
        cleanup
            .commit()
            .await
            .map_err(|e| AppError::internal("commit unqueued correction cleanup", e))?;
    }
    let outcome = outcome.map_err(|e| AppError::internal("enqueue video correction", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}

pub(super) async fn checked_automatic_video(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(crate::services::local_person_mask_video::Metadata, PathBuf)> {
    let (movie, _) = checked_result(state, id, headers).await?;
    let metadata = movie
        .automatic_mask_video
        .ok_or_else(|| AppError::conflict("Видео автоматических масок ещё не подготовлено"))?;
    let expected = movie
        .correction
        .as_ref()
        .map(|p| p.source_mask_hash.as_str())
        .unwrap_or(&movie.mask_hash);
    if metadata.mask_hash != expected
        || metadata.width != movie.mask_width
        || metadata.height != movie.mask_height
        || metadata.frames != movie.frames
        || metadata.frame_rate != movie.frame_rate
        || metadata.size_bytes == 0
        || metadata.size_bytes > 64 * 1024 * 1024 * 1024
    {
        return Err(AppError::conflict("Неверные метаданные видео масок"));
    }
    let path = output(state, id)
        .map_err(|_| AppError::not_found("Неверное задание"))?
        .with_extension("automatic.mkv");
    let info = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::conflict("Видео масок недоступно"))?;
    if !info.file_type().is_file() || info.len() != metadata.size_bytes {
        return Err(AppError::conflict("Видео масок повреждено"));
    }
    let (_, hash) = crate::project_archive::fingerprint_file_with_limit(&path, metadata.size_bytes)
        .await
        .map_err(|_| AppError::conflict("Видео масок недоступно"))?;
    if hash != metadata.file_hash {
        return Err(AppError::conflict("Видео масок изменилось"));
    }
    authorize_job(state, id, headers, true).await?;
    Ok((metadata, path))
}
pub async fn automatic_video(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::http::Request<Body>,
) -> AppResult<Response> {
    use tower::ServiceExt;
    let (_, path) = checked_automatic_video(&state, &id, request.headers()).await?;
    let mut response = tower_http::services::ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| AppError::internal("serve automatic video masks", e))?
        .map(Body::new);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("video/x-matroska"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

pub(super) async fn project_origin(
    state: &AppState,
    id: &str,
) -> AppResult<Option<super::person_video_project::Origin>> {
    let origin: Option<String> =
        sqlx::query_scalar("SELECT origin_json FROM person_video_project_origins WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read video project origin", e))?;
    origin
        .map(|value| {
            let origin: super::person_video_project::Origin = serde_json::from_str(&value)
                .map_err(|_| AppError::conflict("Неверный источник проектной правки"))?;
            origin
                .validate()
                .map_err(|_| AppError::conflict("Неверный источник проектной правки"))?;
            Ok(origin)
        })
        .transpose()
}

/// Original oriented source RGBA at an exact decoder ordinal; no model/ORT required.
pub async fn source_frame(
    State(state): State<AppState>,
    Path((id, frame)): Path<(String, u64)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (metadata, _) = checked_result(&state, &id, &headers).await?;
    if frame >= metadata.frames {
        return Err(AppError::bad_request("Кадр вне исходного видео"));
    }
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let (_, source_id) = access(&state, &id)
        .await?
        .ok_or_else(|| AppError::not_found("Исходник задания не найден"))?;
    struct Cancel(CancellationToken);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.cancel();
        }
    }
    let cancel = state.shutdown_token();
    let _cancel = Cancel(cancel.clone());
    let st = state.clone();
    let png=state.spawn_task(async move{
        let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(15);
        let _permit=tokio::select!{_=cancel.cancelled()=>return Err(AppError::service_unavailable("Просмотр кадра остановлен")),permit=tokio::time::timeout_at(deadline,st.acquire_job_slot(ResourceClass::Analysis))=>permit.map_err(|_|AppError::service_unavailable("Просмотр кадра ожидает свободный обработчик"))?.map_err(|_|AppError::service_unavailable("Просмотр кадра останавливается"))?};
        authorize_job(&st,&id,&headers,true).await?;
        let path=source(&st,&source_id,&actor).await.map_err(|_|AppError::forbidden("Исходник недоступен"))?;
        let codecs=local_enhance_video::resolve_tools().await.map_err(|_|AppError::service_unavailable("Видеокодеки недоступны"))?;
        let png=crate::services::local_person_source_frame::png(&st.process_runtime,crate::services::local_person_source_frame::Tools{ffmpeg:&codecs.ffmpeg,ffprobe:&codecs.ffprobe},crate::services::local_person_source_frame::Input{source:&path,source_hash:&metadata.source_hash,width:metadata.width,height:metadata.height,frames:metadata.frames,frame},&cancel,deadline.saturating_duration_since(tokio::time::Instant::now())).await.map_err(|_|AppError::conflict("Не удалось прочитать неизменённый исходный кадр"))?;
        authorize_job(&st,&id,&headers,true).await?;
        if cancel.is_cancelled(){return Err(AppError::service_unavailable("Просмотр кадра остановлен"));}
        Ok::<_,AppError>(png)
    }).await.map_err(|_|AppError::service_unavailable("Просмотр кадра остановлен"))??;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(png))
        .map_err(|e| AppError::internal("source video frame response", e))
}
