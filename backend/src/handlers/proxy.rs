//! Durable production proxy jobs and safe source-scoped proxy management.

use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, ensure, Context};
use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::analysis::proxy::{
    proxy_key, proxy_key_for_fingerprint, proxy_media_filename, ProxyArtifact, ProxyProfile,
    SourceIdentity, SourceMedia, MAX_PROXY_ARTIFACTS_PER_SOURCE,
};
use crate::config::resource_classes::ResourceClass;
use crate::db::valid_composition_source_id;
use crate::domain::artifact_graph::Fingerprint;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, EnqueueOutcome, JobEvent, JobKind};
use crate::library::MediaEntry;
use crate::model::JobStatus;
use crate::state::{AppState, CancelJobOutcome};
use crate::tools;

use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use super::{
    acquire_job_permit_or_cancelled, acquire_render_permit_or_cancelled, apply_job_event,
    classify_job_error, mark_cancelled, mark_queued, mark_running, spawn_progress_drain,
};

const PROXY_WORK_SCHEMA_VERSION: u32 = 1;
const MAX_ACTIVE_PROXY_JOBS: u32 = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProxyWork {
    schema_version: u32,
    source_id: String,
    source_fingerprint: Fingerprint,
    profile: ProxyProfile,
    #[serde(default)]
    trace_context: crate::telemetry::context::TraceContext,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProxyDedupeIdentity<'a> {
    pipeline_version: &'static str,
    source_id: &'a str,
    key: &'a Fingerprint,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadyProxyResponse {
    key: Fingerprint,
    profile: ProxyProfile,
    status: &'static str,
    url: String,
    size_bytes: u64,
    sha256: Fingerprint,
}

impl ReadyProxyResponse {
    fn from_artifact(artifact: ProxyArtifact) -> Self {
        let filename = proxy_media_filename(&artifact.key, &artifact.profile);
        Self {
            key: artifact.key,
            profile: artifact.profile,
            status: "ready",
            url: format!("/files/proxies/{filename}"),
            size_bytes: artifact.file.size,
            sha256: artifact.file.sha256,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActiveProxyJobResponse {
    job_id: String,
    key: Fingerprint,
    profile: ProxyProfile,
    status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyListResponse {
    source_id: String,
    source_fingerprint: Fingerprint,
    status: &'static str,
    proxies: Vec<ReadyProxyResponse>,
    jobs: Vec<ActiveProxyJobResponse>,
}

/// `POST /api/library/:id/proxies` — validate and fingerprint an original,
/// then enqueue a path-free durable proxy job.
pub async fn proxy_create_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(profile): ApiJson<ProxyProfile>,
) -> AppResult<(StatusCode, Json<Value>)> {
    validate_profile(&profile)?;
    ensure_video_library_entry(&state, &id).await?;
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Создание proxy требует локальные ffmpeg и ffprobe",
        ));
    }
    let source = current_source_identity(&state, &id, CancellationToken::new())
        .await
        .map_err(map_source_inspection_error)?;
    let key = proxy_key(&source, &profile);
    let work = ProxyWork {
        schema_version: PROXY_WORK_SCHEMA_VERSION,
        source_id: id,
        source_fingerprint: source.fingerprint.clone(),
        profile,
        trace_context: trace.map(|Extension(value)| value).unwrap_or_default(),
    };
    let dedupe = proxy_job_dedupe_key(&work.source_id, &key)?;
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize proxy job", error))?;
    let mut resolved_id = None;
    for attempt in 0..2 {
        let job_id = Uuid::new_v4().to_string();
        match state
            .enqueue_job(job_id.clone(), JobKind::Proxy, &payload, &dedupe)
            .await
            .map_err(|error| AppError::internal("enqueue proxy job", error))?
        {
            EnqueueOutcome::Created(_) => {
                resolved_id = Some(job_id);
                break;
            }
            EnqueueOutcome::Existing(existing) => {
                let job = state
                    .get_job(&existing)
                    .await
                    .map_err(|error| AppError::internal("load deduplicated proxy job", error))?;
                let restart = match job.as_ref().map(|job| job.status) {
                    Some(JobStatus::Cancelled) => true,
                    Some(JobStatus::Done) => !state
                        .proxy_service
                        .list_ready(&source, CancellationToken::new())
                        .await
                        .map_err(|error| AppError::internal("verify deduplicated proxy", error))?
                        .iter()
                        .any(|artifact| artifact.key == key),
                    _ => false,
                };
                if restart && attempt == 0 {
                    state
                        .job_store
                        .forget_dedupe(&dedupe)
                        .await
                        .map_err(|error| AppError::internal("refresh proxy dedupe", error))?;
                    continue;
                }
                resolved_id = Some(existing);
                break;
            }
            EnqueueOutcome::RateLimited => {
                return Err(AppError::too_many_requests(
                    "Слишком много новых задач; повторите позже",
                ));
            }
        }
    }
    let resolved_id = resolved_id.ok_or_else(|| {
        AppError::internal(
            "enqueue proxy job",
            anyhow!("proxy dedupe could not be refreshed"),
        )
    })?;
    dispatch_job(&state, &resolved_id).await;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"jobId": resolved_id, "key": key})),
    ))
}

/// `GET /api/library/:id/proxies` — verified current artifacts plus durable
/// pending/running work. Stale source fingerprints are intentionally omitted.
pub async fn proxy_list_handler(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
) -> AppResult<Json<ProxyListResponse>> {
    ensure_video_library_entry(&state, &id).await?;
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Проверка proxy требует локальный ffprobe",
        ));
    }
    let source = current_source_identity(&state, &id, CancellationToken::new())
        .await
        .map_err(map_source_inspection_error)?;
    let proxies = state
        .proxy_service
        .list_ready(&source, CancellationToken::new())
        .await
        .map_err(|error| AppError::internal("list source proxies", error))?
        .into_iter()
        .map(ReadyProxyResponse::from_artifact)
        .collect::<Vec<_>>();
    let mut jobs = Vec::new();
    for request in state
        .job_store
        .active_requests(JobKind::Proxy, MAX_ACTIVE_PROXY_JOBS)
        .await
        .map_err(|error| AppError::internal("list active proxy jobs", error))?
    {
        let Ok(work) = serde_json::from_value::<ProxyWork>(request.payload) else {
            tracing::warn!(job.id = %request.job.id, "ignored incompatible active proxy payload");
            continue;
        };
        if work.schema_version != PROXY_WORK_SCHEMA_VERSION
            || work.source_id != id
            || work.source_fingerprint != source.fingerprint
            || work.profile.validate().is_err()
        {
            continue;
        }
        jobs.push(ActiveProxyJobResponse {
            job_id: request.job.id,
            key: proxy_key_for_fingerprint(
                &work.source_id,
                &work.source_fingerprint,
                &work.profile,
            ),
            profile: work.profile,
            status: request.job.status,
            progress: request.job.progress,
            stage: request.job.stage,
        });
        if jobs.len() > MAX_PROXY_ARTIFACTS_PER_SOURCE {
            return Err(AppError::conflict(
                "Список активных proxy-задач превышает лимит",
            ));
        }
    }
    let status = if !jobs.is_empty() {
        "processing"
    } else if !proxies.is_empty() {
        "ready"
    } else {
        "none"
    };
    Ok(Json(ProxyListResponse {
        source_id: id,
        source_fingerprint: source.fingerprint,
        status,
        proxies,
        jobs,
    }))
}

/// `DELETE /api/library/:id/proxies/:key` — cancel matching durable work and
/// remove only the source-owned content-addressed derivative.
pub async fn proxy_delete_handler(
    State(state): State<AppState>,
    AxPath((id, key)): AxPath<(String, String)>,
) -> AppResult<StatusCode> {
    ensure_video_library_entry(&state, &id).await?;
    let parsed_key = parse_proxy_key(&key)?;
    let cancelled = !cancel_proxy_jobs(&state, &id, Some(&parsed_key))
        .await?
        .is_empty();
    let removed = state
        .proxy_service
        .remove_key_for_source(&id, &parsed_key)
        .await
        .map_err(|error| AppError::internal("delete source proxy", error))?;
    let dedupe = proxy_job_dedupe_key(&id, &parsed_key)?;
    state
        .job_store
        .forget_dedupe(&dedupe)
        .await
        .map_err(|error| AppError::internal("forget deleted proxy job", error))?;
    if removed || cancelled {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Proxy не найден"))
    }
}

/// Called by source deletion before the authoritative original is removed.
pub(super) async fn cleanup_source_proxies(state: &AppState, source_id: &str) -> AppResult<()> {
    for key in cancel_proxy_jobs(state, source_id, None).await? {
        state
            .proxy_service
            .remove_key_for_source(source_id, &key)
            .await
            .map_err(|error| AppError::internal("wait for active source proxy cleanup", error))?;
    }
    state
        .proxy_service
        .remove_source(source_id)
        .await
        .map_err(|error| AppError::internal("delete proxies for source", error))?;
    Ok(())
}

pub(super) fn spawn_proxy_job(
    state: AppState,
    job_id: String,
    work: ProxyWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let span = work.trace_context.job_span(&job_id, "proxy");
    let task = async move {
        let _lease = lease;
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let _render_permit = match acquire_render_permit_or_cancelled(&st, &jid, &token).await {
            Some(permit) => permit,
            None => return,
        };
        let _permit =
            match acquire_job_permit_or_cancelled(&st, &jid, &token, ResourceClass::Analysis).await
            {
                Some(permit) => permit,
                None => return,
            };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }
        if !mark_running(&st, &jid, "proxying", attempt).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let (progress, receiver) = mpsc::unbounded_channel();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), receiver);
        let outcome = run_proxy_work(&st, &work, &progress, &token).await;
        drop(progress);
        let _ = drain.await;
        finish_proxy_job(&st, &jid, outcome).await;
    }
    .instrument(span);
    state.spawn_task(task);
}

async fn run_proxy_work(
    state: &AppState,
    work: &ProxyWork,
    progress: &mpsc::UnboundedSender<f64>,
    token: &CancellationToken,
) -> anyhow::Result<Option<ProxyArtifact>> {
    ensure!(
        work.schema_version == PROXY_WORK_SCHEMA_VERSION,
        "unsupported proxy work version"
    );
    work.profile.validate()?;
    ensure!(
        state.tools.ffmpeg,
        "ffmpeg is unavailable for proxy generation"
    );
    let source = current_source_identity(state, &work.source_id, token.child_token()).await?;
    ensure!(
        source.fingerprint == work.source_fingerprint,
        "proxy source fingerprint is stale"
    );
    let artifact = state
        .proxy_service
        .ensure_with_progress(
            source.clone(),
            work.profile.clone(),
            progress.clone(),
            token.child_token(),
        )
        .await?;
    if token.is_cancelled() {
        state.proxy_service.remove(&artifact).await?;
        return Ok(None);
    }
    let current = current_source_identity(state, &work.source_id, token.child_token()).await;
    if !matches!(current, Ok(ref current) if current.fingerprint == work.source_fingerprint) {
        state.proxy_service.remove(&artifact).await?;
        return Err(anyhow!(
            "proxy source changed or was deleted during generation"
        ));
    }
    Ok(Some(artifact))
}

async fn finish_proxy_job(
    state: &AppState,
    job_id: &str,
    outcome: anyhow::Result<Option<ProxyArtifact>>,
) {
    match outcome {
        Ok(Some(artifact)) => {
            let result = serde_json::to_value(ReadyProxyResponse::from_artifact(artifact.clone()))
                .expect("verified proxy response serialization cannot fail");
            if !apply_job_event(state, job_id, JobEvent::Succeeded { result }).await {
                if let Err(error) = state
                    .proxy_service
                    .remove_key_for_source(&artifact.source_id, &artifact.key)
                    .await
                {
                    tracing::error!(job.id = job_id, %error, "remove unclaimed proxy artifact");
                }
            }
        }
        Ok(None) if state.is_shutting_down() => {}
        Ok(None) => {
            apply_job_event(state, job_id, JobEvent::Cancelled).await;
        }
        Err(error) => {
            let kind = classify_job_error(&error);
            let message = crate::privacy::redact_text(&error.to_string());
            tracing::error!(job.id = job_id, error.kind = kind.as_str(), error = %message, "proxy job failed");
            let updated = apply_job_event(state, job_id, JobEvent::Failed { kind, message }).await;
            if updated && kind.retryable() {
                match state.job_store.schedule_retry(job_id).await {
                    Ok(Some((job, delay))) => {
                        state.replace_job(job).await;
                        tracing::info!(
                            job.id = job_id,
                            retry.delay_ms = delay.as_millis(),
                            "proxy retry scheduled"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::error!(job.id = job_id, %error, "schedule proxy retry");
                    }
                }
            }
        }
    }
    state.clear_cancel(job_id).await;
}

async fn cancel_proxy_jobs(
    state: &AppState,
    source_id: &str,
    key: Option<&Fingerprint>,
) -> AppResult<Vec<Fingerprint>> {
    let requests = state
        .job_store
        .active_requests(JobKind::Proxy, MAX_ACTIVE_PROXY_JOBS)
        .await
        .map_err(|error| AppError::internal("list proxy jobs before cancellation", error))?;
    let mut matching_keys = Vec::new();
    for request in requests {
        let Ok(work) = serde_json::from_value::<ProxyWork>(request.payload) else {
            continue;
        };
        let work_key =
            proxy_key_for_fingerprint(&work.source_id, &work.source_fingerprint, &work.profile);
        if work.source_id != source_id || key.is_some_and(|key| work_key != *key) {
            continue;
        }
        matching_keys.push(work_key);
        match state
            .cancel_open_job(&request.job.id)
            .await
            .map_err(|error| AppError::internal("cancel proxy job", error))?
        {
            CancelJobOutcome::Cancelled
            | CancelJobOutcome::AlreadyFinished
            | CancelJobOutcome::NotFound => {}
        }
    }
    matching_keys.sort();
    matching_keys.dedup();
    Ok(matching_keys)
}

async fn ensure_video_library_entry(state: &AppState, id: &str) -> AppResult<MediaEntry> {
    if !valid_composition_source_id(id) {
        return Err(AppError::bad_request("Некорректный id источника"));
    }
    let entry = state
        .library
        .get(id)
        .await
        .ok_or_else(|| AppError::not_found("Источник не найден"))?;
    if entry.kind != "source" || entry.media_type.as_deref() != Some("video") {
        return Err(AppError::bad_request(
            "Proxy поддерживаются только для исходного видео",
        ));
    }
    Ok(entry)
}

async fn current_source_identity(
    state: &AppState,
    source_id: &str,
    cancellation: CancellationToken,
) -> anyhow::Result<SourceIdentity> {
    ensure!(
        valid_composition_source_id(source_id),
        "invalid proxy source id"
    );
    let entry = state
        .library
        .get(source_id)
        .await
        .ok_or_else(|| anyhow!("proxy source not found"))?;
    ensure!(
        entry.kind == "source" && entry.media_type.as_deref() == Some("video"),
        "proxy source is not a video"
    );
    let original_path = validated_original_path(state, &entry).await?;
    let probe = tools::probe_video(&state.process_runtime, &original_path)
        .await
        .context("ffprobe proxy source")?;
    ensure!(
        probe.vcodec.is_some() && probe.width > 0 && probe.height > 0 && probe.duration > 0.0,
        "proxy source probe is not video"
    );
    state
        .proxy_service
        .inspect_source(
            SourceMedia {
                id: source_id.to_owned(),
                original_path,
                duration_seconds: probe.duration,
            },
            cancellation,
        )
        .await
}

async fn validated_original_path(state: &AppState, entry: &MediaEntry) -> anyhow::Result<PathBuf> {
    ensure!(
        safe_filename(&entry.filename),
        "unsafe proxy source filename"
    );
    let sources = tokio::fs::canonicalize(state.sources_dir())
        .await
        .context("canonicalize proxy source directory")?;
    let candidate = state.sources_dir().join(&entry.filename);
    let metadata = tokio::fs::symlink_metadata(&candidate)
        .await
        .context("inspect proxy source")?;
    ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "proxy source is not a regular file"
    );
    let canonical = tokio::fs::canonicalize(&candidate)
        .await
        .context("canonicalize proxy source")?;
    ensure!(
        canonical.parent() == Some(sources.as_path()),
        "proxy source is outside media storage"
    );
    Ok(canonical)
}

fn safe_filename(filename: &str) -> bool {
    !filename.is_empty()
        && filename.len() <= 255
        && !filename.contains(['/', '\\'])
        && matches!(
            Path::new(filename)
                .components()
                .collect::<Vec<_>>()
                .as_slice(),
            [Component::Normal(_)]
        )
}

fn validate_profile(profile: &ProxyProfile) -> AppResult<()> {
    profile
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))
}

fn proxy_job_dedupe_key(source_id: &str, key: &Fingerprint) -> AppResult<String> {
    dedupe_key(
        "proxy",
        &ProxyDedupeIdentity {
            pipeline_version: "proxy-v1",
            source_id,
            key,
        },
    )
    .map_err(|error| AppError::internal("build proxy dedupe key", error))
}

fn parse_proxy_key(value: &str) -> AppResult<Fingerprint> {
    let key =
        Fingerprint::parse(value).map_err(|_| AppError::bad_request("Некорректный proxy key"))?;
    if key.as_str() != value {
        return Err(AppError::bad_request("Некорректный proxy key"));
    }
    Ok(key)
}

fn map_source_inspection_error(error: anyhow::Error) -> AppError {
    if tools::is_tool_timeout(&error) {
        AppError::gateway_timeout("Проверка исходного видео превысила лимит времени")
    } else if error.to_string().contains("ffprobe") || error.to_string().contains("not video") {
        AppError::unsupported_media_type("Не удалось проверить исходное видео")
    } else {
        AppError::internal("inspect proxy source", error)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::state::ToolInfo;

    #[test]
    fn durable_proxy_payload_contains_identity_but_no_storage_location() {
        let work = ProxyWork {
            schema_version: PROXY_WORK_SCHEMA_VERSION,
            source_id: "source-id".into(),
            source_fingerprint: Fingerprint::digest(b"source"),
            profile: ProxyProfile::default(),
            trace_context: Default::default(),
        };
        let value = serde_json::to_value(work).unwrap();
        assert_eq!(value["sourceId"], json!("source-id"));
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("path"));
        assert!(!encoded.contains("url"));
        assert!(!encoded.contains("storage"));
    }

    #[test]
    fn proxy_keys_and_source_filenames_are_strict() {
        let key = Fingerprint::digest(b"proxy").to_string();
        assert!(parse_proxy_key(&key).is_ok());
        assert!(parse_proxy_key(&key.to_uppercase()).is_err());
        assert!(safe_filename("source.mp4"));
        assert!(!safe_filename("../source.mp4"));
        assert!(!safe_filename("nested/source.mp4"));
    }

    #[tokio::test]
    async fn durable_proxy_outbox_recovers_without_a_path_payload() {
        let directory = tempfile::tempdir().unwrap();
        for name in ["sources", "outputs", "staging", "luts"] {
            tokio::fs::create_dir_all(directory.path().join(name))
                .await
                .unwrap();
        }
        let first_db = Db::open(directory.path()).await.unwrap();
        let first_library = Library::load(directory.path().to_path_buf()).await;
        let first = AppState::new(
            directory.path().to_path_buf(),
            1,
            ToolInfo {
                ffmpeg: true,
                ..ToolInfo::default()
            },
            first_library,
            first_db,
        );
        let work = ProxyWork {
            schema_version: PROXY_WORK_SCHEMA_VERSION,
            source_id: "missing-source".into(),
            source_fingerprint: Fingerprint::digest(b"missing"),
            profile: ProxyProfile::default(),
            trace_context: Default::default(),
        };
        first
            .enqueue_job(
                "recovered-proxy".into(),
                JobKind::Proxy,
                &serde_json::to_value(work).unwrap(),
                "recovered-proxy-dedupe",
            )
            .await
            .unwrap();
        drop(first);

        let reopened_db = Db::open(directory.path()).await.unwrap();
        let reopened_library = Library::load(directory.path().to_path_buf()).await;
        let reopened = AppState::new(
            directory.path().to_path_buf(),
            1,
            ToolInfo {
                ffmpeg: true,
                ..ToolInfo::default()
            },
            reopened_library,
            reopened_db,
        );
        crate::handlers::resume_pending_jobs(&reopened).await;
        for _ in 0..100 {
            let job = reopened.get_job("recovered-proxy").await.unwrap().unwrap();
            if job.status.is_terminal() {
                assert_eq!(job.status, JobStatus::Error);
                assert!(job.error.as_deref().is_some_and(
                    |error| !error.contains(directory.path().to_string_lossy().as_ref())
                ));
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("recovered proxy job did not become terminal");
    }
}
