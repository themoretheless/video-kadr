//! Durable private learned-video jobs. User input contains media IDs, never paths.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::{local_enhance, local_enhance_video},
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
    factor: u32,
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
    model_id: Option<String>,
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
        let created:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN enhance_video_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=? AND (? IS NULL OR a.source_id=?)").bind(&cursor).bind(&owner).bind(&page.source_id).bind(&page.source_id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read enhancement cursor",e))?;
        Some((
            created.ok_or_else(|| AppError::bad_request("Некорректная страница улучшений"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64,Option<String>,Option<i64>)>=sqlx::query_as("SELECT j.id,a.source_id,j.created_at,r.payload_json,s.factor FROM jobs j JOIN enhance_video_job_access a ON a.job_id=j.id LEFT JOIN job_requests r ON r.job_id=j.id LEFT JOIN enhance_video_job_settings s ON s.job_id=j.id WHERE a.owner=? AND (? IS NULL OR a.source_id=?) AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(&page.source_id).bind(&page.source_id).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list enhancements",e))?;
    let next = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|row| row.0.clone())
    } else {
        None
    };
    let mut jobs = vec![];
    for (id, source, created_at, payload, recorded_factor) in rows.into_iter().take(limit as usize)
    {
        if let Some(space) = state
            .db
            .source_space_id(&source)
            .await
            .map_err(|e| AppError::internal("read enhancement source space", e))?
        {
            if !state
                .db
                .is_space_member(&space, &owner)
                .await
                .map_err(|e| AppError::internal("authorize enhancement history", e))?
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
            .map_err(|e| AppError::internal("read enhancement history status", e))?
        {
            let factor = recorded_factor
                .and_then(|factor| u32::try_from(factor).ok())
                .filter(|factor| [1, 2, 4].contains(factor))
                .or_else(|| {
                    payload
                        .and_then(|json| serde_json::from_str::<Work>(&json).ok())
                        .filter(|work| {
                            work.owner == owner
                                && work.request.source_id == source
                                && work.validate().is_ok()
                        })
                        .map(|work| work.request.factor)
                        .or_else(|| {
                            job.result
                                .as_ref()
                                .and_then(|result| result["factor"].as_u64())
                                .and_then(|factor| u32::try_from(factor).ok())
                        })
                });
            {
                let factor = factor.filter(|factor| [1, 2, 4].contains(factor));
                jobs.push(json!({"jobId":id,"sourceId":source,"createdAt":created_at,"factor":factor,"status":job.status,"progress":job.progress}));
            }
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}
impl Work {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && !self.owner.is_empty()
                && self
                    .model_id
                    .as_deref()
                    .is_none_or(|id| id == crate::services::enhance_model::ID)
                && [1, 2, 4].contains(&self.request.factor)
                && [&self.source_hash, &self.engine_hash, &self.tools_hash]
                    .iter()
                    .all(|hash| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())),
            "invalid enhancement snapshot"
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
            Self::Access => "enhancement source access revoked",
            Self::Source => "enhancement source changed",
            Self::Engine => "enhancement engine changed",
        })
    }
}
impl std::error::Error for Invariant {}
async fn source(state: &AppState, id: &str, owner: &str) -> anyhow::Result<PathBuf> {
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
        .ok_or_else(|| anyhow::anyhow!("enhancement source missing"))?;
    Ok(state.library.resolve_media_path(&entry).await?)
}
async fn access(state: &AppState, id: &str) -> AppResult<Option<(String, String)>> {
    sqlx::query_as("SELECT owner,source_id FROM enhance_video_job_access WHERE job_id=?")
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read enhancement owner", e))
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
            return Err(AppError::forbidden("Это улучшение другого пользователя"));
        }
        if require_source {
            super::library::authorize_source_access(state, &source, headers).await?;
        }
    }
    Ok(())
}
fn output(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid enhancement ID"
    );
    Ok(state
        .storage
        .join("enhance-video-jobs")
        .join(format!("{id}.mp4")))
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    if ![1, 2, 4].contains(&request.factor) {
        return Err(AppError::bad_request("Выберите масштаб 1, 2 или 4"));
    }
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
    let _model_operation = state.enhance_model_ops.lock().await;
    let (config, model_id) = super::enhance_models::config_for_owner(&state, &owner).await?;
    let engine = local_enhance::identity(&config)
        .await
        .map_err(|_| AppError::service_unavailable("Модель улучшения недоступна"))?;
    let codecs = local_enhance_video::resolve_tools()
        .await
        .map_err(|_| AppError::service_unavailable("Видеокодеки не настроены"))?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO enhance_video_job_access(job_id,owner,source_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&request.source_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save enhancement access", e))?;
    sqlx::query("INSERT INTO enhance_video_job_settings(job_id,factor) VALUES(?,?)")
        .bind(&id)
        .bind(i64::from(request.factor))
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save enhancement settings", e))?;
    let work = Work {
        schema_version: 1,
        owner,
        request,
        source_hash,
        engine_hash: engine.hash,
        tools_hash: codecs.identity,
        model_id,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize enhancement", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::EnhanceVideo,
            &payload,
            &format!("enhance:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue enhancement", e))?;
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
) -> AppResult<(local_enhance_video::Metadata, PathBuf)> {
    if access(state, id).await?.is_none() {
        return Err(AppError::not_found("Улучшение видео не найдено"));
    }
    authorize_job(state, id, headers, true).await?;
    let job = state
        .get_job(id)
        .await
        .map_err(|e| AppError::internal("read video enhancement", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Улучшение видео ещё не готово"));
    }
    let metadata: local_enhance_video::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат видео"))?;
    let path = output(state, id).map_err(|_| AppError::not_found("Неверное задание"))?;
    let file = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::conflict("Файл улучшения недоступен"))?;
    if !file.file_type().is_file() {
        return Err(AppError::conflict("Файл улучшения недоступен"));
    }
    let (_, hash) =
        crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
            .await
            .map_err(|_| AppError::conflict("Файл улучшения недоступен"))?;
    if hash != metadata.output_hash {
        return Err(AppError::conflict("Файл улучшения изменился"));
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
        .map_err(|e| AppError::internal("serve enhanced video", e))?
        .map(Body::new);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("video/mp4"),
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
        if !JobService::mark_running(&st, &id, "enhance_video", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), rx);
        let outcome: anyhow::Result<Value> = async {
            let path = source(&st, &work.request.source_id, &work.owner).await?;
            anyhow::ensure!(
                crate::project_archive::fingerprint_file_with_limit(&path, 64 * 1024 * 1024 * 1024)
                    .await?
                    .1
                    == work.source_hash,
                Invariant::Source
            );
            let config =
                super::enhance_models::config_for_model(&st, &work.owner, work.model_id.as_deref())
                    .await
                    .map_err(|_| Invariant::Engine)?;
            anyhow::ensure!(
                local_enhance::identity(&config)
                    .await
                    .map_err(|_| Invariant::Engine)?
                    .hash
                    == work.engine_hash,
                Invariant::Engine
            );
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
            if target.try_exists()? {
                anyhow::ensure!(
                    tokio::fs::symlink_metadata(&target)
                        .await?
                        .file_type()
                        .is_file(),
                    "unexpected enhancement output"
                );
                tokio::fs::remove_file(&target).await?;
            }
            let result = local_enhance_video::enhance(
                &st.process_runtime,
                local_enhance_video::Tools {
                    ffmpeg: &codecs.ffmpeg,
                    ffprobe: &codecs.ffprobe,
                },
                &config,
                local_enhance_video::Input {
                    source: &path,
                    source_hash: &work.source_hash,
                    factor: work.request.factor,
                    staging_bytes: 64 * 1024 * 1024 * 1024,
                },
                &target,
                &token,
                std::time::Duration::from_secs(1800),
                &tx,
            )
            .await?;
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
            Ok(serde_json::to_value(result)?)
        }
        .await;
        drop(tx);
        let _ = drain.await;
        if token.is_cancelled() {
            if !st.is_shutting_down() {
                if let Ok(path) = output(&st, &id) {
                    let _ = tokio::fs::remove_file(path).await;
                }
            }
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => {
                    if let Ok(path) = output(&st, &id) {
                        let _ = tokio::fs::remove_file(path).await;
                    }
                    JobEvent::Failed {
                        kind: match error.downcast_ref::<Invariant>() {
                            Some(Invariant::Access) => crate::jobs::ErrorKind::Security,
                            Some(Invariant::Source | Invariant::Engine) => {
                                crate::jobs::ErrorKind::Validation
                            }
                            None => crate::jobs::classify_job_error(&error),
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
