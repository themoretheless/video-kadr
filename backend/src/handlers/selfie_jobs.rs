//! Durable private learned-image jobs. User input contains media IDs, never paths.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::local_selfie,
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
use sha2::{Digest, Sha256};
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
        let created:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN selfie_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=? AND (? IS NULL OR a.source_id=?)").bind(&cursor).bind(&owner).bind(&page.source_id).bind(&page.source_id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read selfie cursor",e))?;
        Some((
            created
                .ok_or_else(|| AppError::bad_request("Некорректная страница выделения человека"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.source_id,j.created_at FROM jobs j JOIN selfie_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR a.source_id=?) AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(&page.source_id).bind(&page.source_id).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list selfies",e))?;
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
            .map_err(|e| AppError::internal("read selfie source space", e))?
        {
            if !state
                .db
                .is_space_member(&space, &owner)
                .await
                .map_err(|e| AppError::internal("authorize selfie history", e))?
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
            .map_err(|e| AppError::internal("read selfie history status", e))?
        {
            jobs.push(json!({"jobId":id,"sourceId":source,"createdAt":created_at,"status":job.status,"progress":job.progress}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}
impl Work {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && !self.owner.is_empty()
                && self.model_id.is_none()
                && [&self.source_hash, &self.engine_hash]
                    .iter()
                    .all(|hash| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit())),
            "invalid selfie snapshot"
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
            Self::Access => "selfie source access revoked",
            Self::Source => "selfie source changed",
            Self::Engine => "selfie engine changed",
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
        .ok_or_else(|| anyhow::anyhow!("selfie source missing"))?;
    Ok(state.library.resolve_media_path(&entry).await?)
}
pub(super) async fn access(state: &AppState, id: &str) -> AppResult<Option<(String, String)>> {
    sqlx::query_as("SELECT owner,source_id FROM selfie_job_access WHERE job_id=?")
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read selfie owner", e))
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
                "Это выделение человека другого пользователя",
            ));
        }
        if require_source {
            super::library::authorize_source_access(state, &source, headers).await?;
        }
    }
    Ok(())
}
pub(super) fn output(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid selfie ID"
    );
    Ok(state.storage.join("selfie-jobs").join(format!("{id}.png")))
}
pub(super) fn mask_output(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    let path = output(state, id)?;
    Ok(path.with_file_name(format!("{id}.mask.png")))
}
pub async fn mask(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if access(&state, &id).await?.is_none() {
        return Err(AppError::not_found("Маска не найдена"));
    }
    authorize_job(&state, &id, &headers, true).await?;
    let job = state
        .get_job(&id)
        .await
        .map_err(|e| AppError::internal("read mask job", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Маска ещё не готова"));
    }
    let metadata: local_selfie::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат"))?;
    if metadata.mask_file_hash.is_none() {
        return Err(AppError::not_found("В этом задании маска не сохранена"));
    }
    let path = mask_output(&state, &id).map_err(|_| AppError::not_found("Неверное задание"))?;
    let bytes = local_selfie::read_verified_mask(&path, &metadata)
        .await
        .map_err(|_| AppError::conflict("Маска отсутствует или изменилась"))?;
    authorize_job(&state, &id, &headers, true).await?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal("mask response", e))
}
/// Bounded, oriented source PNG for source-UV brush authoring.
pub async fn original(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (owner, source_id) = access(&state, &id)
        .await?
        .ok_or_else(|| AppError::not_found("Исходник маски не найден"))?;
    authorize_job(&state, &id, &headers, true).await?;
    let job = state
        .get_job(&id)
        .await
        .map_err(|e| AppError::internal("read selfie original", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Исходная маска ещё не готова"));
    }
    let metadata: local_selfie::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат"))?;
    if metadata.mask_file_hash.is_none() {
        return Err(AppError::conflict("В этом задании маска не сохранена"));
    }
    let path = source(&state, &source_id, &owner)
        .await
        .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let png = original_preview(
        &state,
        path,
        &metadata.source_hash,
        (metadata.width, metadata.height),
    )
    .await?;
    authorize_job(&state, &id, &headers, true).await?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(png))
        .map_err(|e| AppError::internal("source preview response", e))
}
pub(super) async fn original_preview(
    state: &AppState,
    path: PathBuf,
    hash: &str,
    size: (u32, u32),
) -> AppResult<Vec<u8>> {
    super::cutout_jobs::original_preview(state, path, hash, size).await
}
fn config(state: &AppState) -> AppResult<local_selfie::Config> {
    state
        .selfie
        .as_ref()
        .clone()
        .ok_or_else(|| AppError::service_unavailable("Движок Selfie не настроен"))
}

pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    super::publish::authenticated_actor(&state, &headers).await?;
    let available = match config(&state) {
        Ok(config) => local_selfie::identity(&config).await.is_ok(),
        Err(_) => false,
    };
    Ok(Json(
        json!({"available":available,"engine":"mediapipe-selfie-256","stillImages":true,"video":false}),
    ))
}

pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    let (config, model_id) = (config(&state)?, None::<String>);
    let engine = local_selfie::identity(&config)
        .await
        .map_err(|_| AppError::service_unavailable("Модель выделения человека недоступна"))?;
    let path = source(&state, &request.source_id, &owner)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let (w, h) = crate::services::local_enhance::dimensions(path.clone(), true)
        .await
        .map_err(|_| AppError::bad_request("Нужно поддерживаемое неподвижное изображение"))?;
    if u64::from(w) * u64::from(h) > 64 * 1024 * 1024 {
        return Err(AppError::bad_request(
            "Результат превышает предел размера изображения",
        ));
    }
    let source_hash = crate::project_archive::fingerprint_file_with_limit(&path, 256 * 1024 * 1024)
        .await
        .map_err(|_| AppError::bad_request("Исходник недоступен или слишком велик"))?
        .1;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO selfie_job_access(job_id,owner,source_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&request.source_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save selfie access", e))?;
    let work = Work {
        schema_version: 1,
        owner,
        request,
        source_hash,
        engine_hash: engine.hash,
        model_id,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize selfie", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::Selfie,
            &payload,
            &format!("selfie:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue selfie", e))?;
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
) -> AppResult<(local_selfie::Metadata, Vec<u8>)> {
    if access(&state, &id).await?.is_none() {
        return Err(AppError::not_found("Удаление фона не найдено"));
    }
    authorize_job(&state, &id, &headers, true).await?;
    let job = state
        .get_job(&id)
        .await
        .map_err(|e| AppError::internal("read selfie", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Удаление фона ещё не готово"));
    }
    let metadata: local_selfie::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат"))?;
    let path = output(&state, &id).map_err(|_| AppError::not_found("Неверное задание"))?;
    let (_, hash) = crate::project_archive::fingerprint_file_with_limit(&path, 256 * 1024 * 1024)
        .await
        .map_err(|_| AppError::conflict("Файл выделения человека недоступен"))?;
    if hash != metadata.output_hash {
        return Err(AppError::conflict("Файл выделения человека изменился"));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::internal("read selfie image", e))?;
    if bytes.len() > 256 * 1024 * 1024
        || format!("{:x}", Sha256::digest(&bytes)) != metadata.output_hash
    {
        return Err(AppError::conflict("Файл выделения человека изменился"));
    }
    authorize_job(&state, &id, &headers, true).await?;
    Ok((metadata, bytes))
}
pub async fn image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (_, bytes) = checked_result(&state, &id, &headers).await?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal("selfie response", e))
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
        if !JobService::mark_running(&st, &id, "selfie", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), rx);
        let outcome: anyhow::Result<Value> = async {
            let path = source(&st, &work.request.source_id, &work.owner).await?;
            anyhow::ensure!(
                crate::project_archive::fingerprint_file_with_limit(&path, 256 * 1024 * 1024)
                    .await?
                    .1
                    == work.source_hash,
                Invariant::Source
            );
            let config = config(&st).map_err(|_| Invariant::Engine)?;
            anyhow::ensure!(
                local_selfie::identity(&config)
                    .await
                    .map_err(|_| Invariant::Engine)?
                    .hash
                    == work.engine_hash,
                Invariant::Engine
            );
            let target = output(&st, &id)?;
            let mask_target = mask_output(&st, &id)?;
            let parent = target.parent().unwrap();
            tokio::fs::create_dir_all(parent).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
            }
            for target in [&target, &mask_target] {
                if tokio::fs::symlink_metadata(target).await.is_ok() {
                    anyhow::ensure!(
                        tokio::fs::symlink_metadata(target)
                            .await?
                            .file_type()
                            .is_file(),
                        "unexpected selfie output"
                    );
                    tokio::fs::remove_file(target).await?;
                }
            }
            let result = local_selfie::cutout(
                &st.process_runtime,
                &config,
                local_selfie::Input {
                    source: &path,
                    source_hash: &work.source_hash,
                    mask_destination: Some(&mask_target),
                },
                &target,
                &token,
                std::time::Duration::from_secs(1800),
                &tx,
            )
            .await?;
            let current = source(&st, &work.request.source_id, &work.owner).await?;
            anyhow::ensure!(
                crate::project_archive::fingerprint_file_with_limit(&current, 256 * 1024 * 1024)
                    .await?
                    .1
                    == work.source_hash,
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
                    if let Ok(mask) = mask_output(&st, &id) {
                        let _ = tokio::fs::remove_file(mask).await;
                    }
                }
            }
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => {
                    if let Ok(path) = output(&st, &id) {
                        let _ = tokio::fs::remove_file(path).await;
                        if let Ok(mask) = mask_output(&st, &id) {
                            let _ = tokio::fs::remove_file(mask).await;
                        }
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
