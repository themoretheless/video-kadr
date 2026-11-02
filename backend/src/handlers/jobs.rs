use std::time::Duration;

use axum::extract::{Path as AxPath, State};
use axum::Json;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, AppResult};
use crate::jobs::{ErrorKind, JobEvent, JobKind};
use crate::model::Job;
use crate::state::{AppState, CancelJobOutcome};

use super::apply_job_event;
use super::composition::{spawn_composition_job, CompositionWork};
use super::edit::{spawn_edit_job, EditWork};
use super::import::{spawn_import_job, ImportWork};
use super::proxy::{spawn_proxy_job, ProxyWork};
use super::publish::{spawn_publish_job, PublishWork};

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
                work.validate_schema()?;
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
            JobKind::Composition => {
                let work = serde_json::from_value::<CompositionWork>(envelope.payload)?;
                anyhow::ensure!(
                    work.schema_version
                        == crate::services::composition::COMPOSITION_RENDER_SCHEMA_VERSION,
                    "unsupported composition work version"
                );
                let lease =
                    JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                spawn_composition_job(
                    state.clone(),
                    job_id.into(),
                    work,
                    envelope.attempt,
                    token,
                    lease,
                );
            }
            JobKind::Stabilization => {
                let work=serde_json::from_value::<super::stabilization_jobs::StabilizationWork>(envelope.payload)?;
                anyhow::ensure!(work.schema_version==1,"unsupported stabilization work version");
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::stabilization_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::CutoutModelDownload => {
                let work=serde_json::from_value::<super::cutout_model_downloads::DownloadWork>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::cutout_model_downloads::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::SamModelDownload => {
                let work=serde_json::from_value::<super::sam_model_downloads::DownloadWork>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::sam_model_downloads::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::EnhanceModelDownload => {
                let work=serde_json::from_value::<super::enhance_model_downloads::DownloadWork>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::enhance_model_downloads::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::TtsModelDownload => {
                let work=serde_json::from_value::<super::tts_model_downloads::DownloadWork>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::tts_model_downloads::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Tts => {
                let work=serde_json::from_value::<super::tts_jobs::TtsWork>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::tts_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::SpeechModelDownload => {
                let work=serde_json::from_value::<super::speech_model_downloads::DownloadWork>(envelope.payload)?;
                work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::speech_model_downloads::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::PersonVideo => {
                let work=serde_json::from_value::<super::person_video_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::person_video_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::EnhanceVideo => {
                let work=serde_json::from_value::<super::enhance_video_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::enhance_video_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::SamRegion => {
                let work=serde_json::from_value::<super::sam_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::sam_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::CutoutEdit => {
                let work=serde_json::from_value::<super::cutout_edit_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::cutout_edit_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Cutout => {
                let work=serde_json::from_value::<super::cutout_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::cutout_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Selfie => {
                let work=serde_json::from_value::<super::selfie_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::selfie_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Enhance => {
                let work=serde_json::from_value::<super::enhance_jobs::Work>(envelope.payload)?;work.validate()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::enhance_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Speech => {
                let work = serde_json::from_value::<super::speech_jobs::SpeechWork>(envelope.payload)?;
                anyhow::ensure!(work.schema_version == 1, "unsupported speech work version");
                let lease = JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                super::speech_jobs::spawn_speech_job(state.clone(), job_id.into(), work, envelope.attempt, token, lease);
            }
            JobKind::FilmcraftVideo => {
                let work=serde_json::from_value::<super::filmcraft_video_jobs::Work>(envelope.payload)?.upgraded()?;
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::filmcraft_video_jobs::spawn(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::NestedExport => {
                let work=serde_json::from_value::<super::nested_export::NestedExportWork>(envelope.payload)?;
                anyhow::ensure!(work.schema_version==1,"unsupported native export work version");
                let lease=JobLeaseHeartbeat::start(state,job_id,envelope.attempt,lease_seconds);
                super::nested_export::spawn_nested_export_job(state.clone(),job_id.into(),work,envelope.attempt,token,lease);
            }
            JobKind::Proxy => {
                let work = serde_json::from_value::<ProxyWork>(envelope.payload)?;
                let lease =
                    JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                spawn_proxy_job(
                    state.clone(),
                    job_id.into(),
                    work,
                    envelope.attempt,
                    token,
                    lease,
                );
            }
            JobKind::Publish => {
                let work = serde_json::from_value::<PublishWork>(envelope.payload)?;
                anyhow::ensure!(work.schema_version == 1, "unsupported publish work version");
                let lease =
                    JobLeaseHeartbeat::start(state, job_id, envelope.attempt, lease_seconds);
                spawn_publish_job(
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
                message: crate::messages::JOB_PAYLOAD_INCOMPATIBLE.into(),
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
    pub(super) fn start(state: &AppState, job_id: &str, attempt: u32, lease_seconds: i64) -> Self {
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
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Job>> {
    super::speech_jobs::authorize_job(&state, &id, &headers, true).await?;
    super::enhance_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::cutout_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::selfie_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::sam_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::enhance_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::person_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::tts_jobs::authorize_job(&state,&id,&headers).await?;
    super::speech_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::tts_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::enhance_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::cutout_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::sam_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::stabilization_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::filmcraft_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    match state
        .get_job(&id)
        .await
        .map_err(|error| AppError::internal("load job", error))?
    {
        Some(job) => Ok(Json(job)),
        None => Err(AppError::not_found(crate::messages::JOB_NOT_FOUND)),
    }
}

/// `POST /api/jobs/:id/cancel` - request cancellation of a running/pending job.
pub async fn cancel_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    super::speech_jobs::authorize_job(&state, &id, &headers, false).await?;
    super::enhance_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::cutout_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::selfie_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::sam_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::enhance_video_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::person_video_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::tts_jobs::authorize_job(&state,&id,&headers).await?;
    super::speech_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::tts_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::enhance_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::cutout_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::sam_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::stabilization_jobs::authorize_job(&state,&id,&headers,false).await?;
    super::filmcraft_video_jobs::authorize_job(&state,&id,&headers,false).await?;
    match state
        .cancel_open_job(&id)
        .await
        .map_err(|error| AppError::internal("load job before cancellation", error))?
    {
        CancelJobOutcome::NotFound => Err(AppError::not_found(crate::messages::JOB_NOT_FOUND)),
        CancelJobOutcome::AlreadyFinished => {
            Err(AppError::conflict(crate::messages::JOB_ALREADY_FINISHED))
        }
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
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    super::speech_jobs::authorize_job(&state, &id, &headers, true).await?;
    super::enhance_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::cutout_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::selfie_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::sam_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::enhance_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::person_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::tts_jobs::authorize_job(&state,&id,&headers).await?;
    super::speech_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::tts_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::enhance_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::cutout_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::sam_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::stabilization_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::filmcraft_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::filmcraft_export_queue::authorize_external_retry(&state,&id,&headers).await?;
    if state
        .get_job(&id)
        .await
        .map_err(|error| AppError::internal("load job before retry", error))?
        .is_none()
    {
        return Err(AppError::not_found(crate::messages::JOB_NOT_FOUND));
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
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    super::speech_jobs::authorize_job(&state, &id, &headers, true).await?;
    super::enhance_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::cutout_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::selfie_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::sam_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::enhance_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::person_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::tts_jobs::authorize_job(&state,&id,&headers).await?;
    super::speech_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::tts_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::enhance_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::cutout_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::sam_model_downloads::authorize_job(&state,&id,&headers).await?;
    super::stabilization_jobs::authorize_job(&state,&id,&headers,true).await?;
    super::filmcraft_video_jobs::authorize_job(&state,&id,&headers,true).await?;
    let discarded = state
        .job_store
        .discard_failed(&id, "local-operator", None)
        .await
        .map_err(|error| AppError::internal("discard failed job", error))?;
    let Some(job) = discarded else {
        return Err(AppError::not_found(crate::messages::FAILED_JOB_NOT_FOUND));
    };
    state.replace_job(job).await;
    Ok(Json(json!({ "jobId": id, "status": "discarded" })))
}
