use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::Json;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};
use crate::jobs::{ErrorKind, JobEvent, JobKind};
use crate::model::Job;
use crate::state::{AppState, CancelJobOutcome};

use super::{apply_job_event, spawn_edit_job, spawn_import_job, EditWork, ImportWork};

pub(super) async fn dispatch_job(state: &AppState, job_id: &str) {
    // The hot job map is bounded at startup. Hydrate older durable work before
    // claiming its outbox lease, otherwise a worker would have no JobCell.
    match state.get_job(job_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::error!(job.id = job_id, "outbox references a missing durable job");
            return;
        }
        Err(error) => {
            tracing::error!(job.id = job_id, %error, "load durable job before dispatch");
            return;
        }
    }
    let lease_seconds = i64::try_from(state.job_timeout().as_secs())
        .unwrap_or(i64::MAX)
        .saturating_add(60);
    let envelope = match state.job_store.claim(job_id, lease_seconds).await {
        Ok(Some(envelope)) => envelope,
        Ok(None) => return,
        Err(error) => {
            tracing::error!(job.id = job_id, %error, "claim durable job");
            return;
        }
    };
    let snapshot = match state.db.load_job(job_id).await {
        Ok(Some(job)) => job,
        Ok(None) => {
            tracing::error!(
                job.id = job_id,
                "claimed outbox job has no durable snapshot"
            );
            return;
        }
        Err(error) => {
            tracing::error!(job.id = job_id, %error, "reload claimed job snapshot");
            return;
        }
    };
    state.replace_job(snapshot).await;
    let token = state.register_cancel(job_id).await;
    let result = (|| -> anyhow::Result<()> {
        match envelope.kind {
            JobKind::Import => {
                let work = serde_json::from_value::<ImportWork>(envelope.payload)?;
                anyhow::ensure!(work.schema_version == 1, "unsupported import work version");
                let lease =
                    JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                spawn_import_job(
                    state.clone(),
                    job_id.into(),
                    work,
                    envelope.attempt,
                    token,
                    lease,
                );
            }
            JobKind::Edit => {
                let work = serde_json::from_value::<EditWork>(envelope.payload)?;
                work.timeline_semantics()?;
                let lease =
                    JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                spawn_edit_job(
                    state.clone(),
                    job_id.into(),
                    work,
                    envelope.attempt,
                    token,
                    lease,
                );
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        apply_job_event(
            state,
            job_id,
            JobEvent::Failed {
                kind: ErrorKind::Internal,
                message: "Сохранённая задача имеет несовместимый формат".into(),
            },
        )
        .await;
        state.clear_cancel(job_id).await;
        tracing::error!(job.id = job_id, %error, "decode durable job payload");
    }
}

pub(super) struct JobLeaseHeartbeat {
    stop: CancellationToken,
}

impl JobLeaseHeartbeat {
    fn start(state: &AppState, job_id: &str, attempt: u32, lease_seconds: i64) -> Self {
        let stop = CancellationToken::new();
        let stop_waiter = stop.clone();
        let shutdown = state.shutdown_token();
        let store = state.job_store.clone();
        let job_id = job_id.to_string();
        let cadence = Duration::from_secs(
            u64::try_from(lease_seconds.max(3) / 3)
                .unwrap_or(60)
                .clamp(1, 60),
        );
        state.spawn_task(async move {
            loop {
                tokio::select! {
                    _ = stop_waiter.cancelled() => break,
                    _ = shutdown.cancelled() => break,
                    _ = tokio::time::sleep(cadence) => {}
                }
                match store.renew_lease(&job_id, attempt, lease_seconds).await {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(error) => {
                        tracing::warn!(job.id = %job_id, job.attempt = attempt, %error, "renew job lease");
                    }
                }
            }
        });
        Self { stop }
    }
}

impl Drop for JobLeaseHeartbeat {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub async fn resume_pending_jobs(state: &AppState) {
    match state.job_store.schedule_due_retries().await {
        Ok(jobs) => {
            for job in jobs {
                state.replace_job(job).await;
            }
        }
        Err(error) => tracing::error!(%error, "recover due job retries"),
    }
    match state.job_store.deliverable_ids().await {
        Ok(ids) => {
            for id in ids {
                dispatch_job(state, &id).await;
            }
        }
        Err(error) => tracing::error!(%error, "load durable job outbox"),
    }
}

pub fn start_job_dispatcher(state: &AppState) {
    let worker = state.clone();
    let shutdown = state.shutdown_token();
    state.spawn_task(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => resume_pending_jobs(&worker).await,
            }
        }
    });
}

/// `GET /api/jobs/:id` - poll the status of an import or edit job.
pub async fn job_status_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Job>> {
    match state
        .get_job(&id)
        .await
        .map_err(|error| AppError::internal("load job", error))?
    {
        Some(job) => Ok(Json(job)),
        None => Err(AppError::not_found("Задача не найдена")),
    }
}

/// `POST /api/jobs/:id/cancel` - request cancellation of a running/pending job.
pub async fn cancel_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Value>> {
    match state
        .cancel_open_job(&id)
        .await
        .map_err(|error| AppError::internal("load job before cancellation", error))?
    {
        CancelJobOutcome::NotFound => Err(AppError::not_found("Задача не найдена")),
        CancelJobOutcome::AlreadyFinished => Err(AppError::conflict("Задача уже завершена")),
        CancelJobOutcome::Cancelled => Ok(Json(json!({ "status": "cancelled" }))),
    }
}

pub async fn failed_jobs_handler(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let jobs = state
        .job_store
        .list_failed()
        .await
        .map_err(|error| AppError::internal("list failed jobs", error))?;
    Ok(Json(json!({ "jobs": jobs })))
}

pub async fn job_registry_handler(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let counts = state
        .job_store
        .lifecycle_counts()
        .await
        .map_err(|error| AppError::internal("read job registries", error))?;
    Ok(Json(json!({ "counts": counts })))
}

pub async fn retry_job_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Value>> {
    if state
        .get_job(&id)
        .await
        .map_err(|error| AppError::internal("load job before retry", error))?
        .is_none()
    {
        return Err(AppError::not_found("Задача не найдена"));
    }
    let job = state
        .job_store
        .retry_failed(&id, "local-operator")
        .await
        .map_err(|error| AppError::conflict(error.to_string()))?;
    state.replace_job(job).await;
    dispatch_job(&state, &id).await;
    Ok(Json(json!({ "jobId": id, "status": "pending" })))
}

pub async fn discard_job_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<Value>> {
    let discarded = state
        .job_store
        .discard_failed(&id, "local-operator", None)
        .await
        .map_err(|error| AppError::internal("discard failed job", error))?;
    let Some(job) = discarded else {
        return Err(AppError::not_found("Неудачная задача не найдена"));
    };
    state.replace_job(job).await;
    Ok(Json(json!({ "jobId": id, "status": "discarded" })))
}
