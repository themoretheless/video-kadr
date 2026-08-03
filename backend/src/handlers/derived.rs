use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::analysis::proxy::{ProxyProfile, ProxyService, SourceIdentity};
use crate::artifacts::fingerprint_file;
use crate::domain::artifact_graph::Fingerprint;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::derived::{
    ClaimedDerivedTask, DerivedGraphSpec, DerivedTaskKind, DerivedTaskRecord, DerivedTaskSpec,
};
use crate::library::MediaEntry;
use crate::state::AppState;
use crate::tools::proxy::FfmpegProxyEncoder;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DerivedListQuery {
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PriorityRequest {
    priority: i64,
    expected_revision: u64,
}

pub async fn enqueue_derived_graph(
    State(state): State<AppState>,
    ApiJson(spec): ApiJson<DerivedGraphSpec>,
) -> AppResult<Json<Value>> {
    state
        .derived_job_store
        .enqueue_graph(&spec)
        .await
        .map(|graph| Json(json!(graph)))
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn list_derived_jobs(
    State(state): State<AppState>,
    Query(query): Query<DerivedListQuery>,
) -> AppResult<Json<Vec<DerivedTaskRecord>>> {
    state
        .derived_job_store
        .list(query.project_id.as_deref())
        .await
        .map(Json)
        .map_err(|error| AppError::internal("list derived jobs", error))
}

pub async fn derived_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<DerivedTaskRecord>>> {
    let tasks = state
        .derived_job_store
        .graph_tasks(&id)
        .await
        .map_err(|error| AppError::internal("read derived graph", error))?;
    if tasks.is_empty() {
        return Err(AppError::not_found("Граф фоновых задач не найден"));
    }
    Ok(Json(tasks))
}

pub async fn reprioritize_derived_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PriorityRequest>,
) -> AppResult<Json<Value>> {
    match state
        .derived_job_store
        .set_priority(&id, body.priority, body.expected_revision)
        .await
        .map_err(|error| AppError::internal("reprioritize derived job", error))?
    {
        Ok(revision) => Ok(Json(json!({ "taskId": id, "priorityRevision": revision }))),
        Err(conflict) => Err(AppError::conflict(format!(
            "priority revision conflict; actual={}",
            conflict.actual_revision
        ))),
    }
}

pub async fn cancel_derived_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DerivedListQuery>,
) -> AppResult<Json<Value>> {
    let outcome = if let Some(project_id) = query.project_id.as_deref() {
        state
            .derived_job_store
            .cancel_for_project(&id, project_id)
            .await
    } else {
        state.derived_job_store.cancel(&id).await
    };
    let changed = match outcome {
        Ok(changed) => changed,
        Err(error) if error.to_string().contains("shared derived task") => {
            return Err(AppError::conflict(error.to_string()))
        }
        Err(error) if error.to_string().contains("membership not found") => {
            return Err(AppError::not_found(error.to_string()))
        }
        Err(error) => return Err(AppError::internal("cancel derived job", error)),
    };
    if changed == 0 && query.project_id.is_none() {
        return Err(AppError::not_found(
            "Фоновая задача не найдена или уже завершена",
        ));
    }
    Ok(Json(
        json!({ "taskId": id, "cancelled": changed, "projectId": query.project_id }),
    ))
}

pub async fn cancel_derived_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let cancelled = state
        .derived_job_store
        .cancel_graph(&id)
        .await
        .map_err(|error| {
            if error.to_string().contains("not found") {
                AppError::not_found("Граф фоновых задач не найден")
            } else {
                AppError::internal("cancel derived graph", error)
            }
        })?;
    Ok(Json(json!({ "graphId": id, "cancelled": cancelled })))
}

pub async fn retry_derived_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let changed = state
        .derived_job_store
        .retry(&id)
        .await
        .map_err(|error| AppError::internal("retry derived job", error))?;
    if !changed {
        return Err(AppError::conflict(
            "Повтор доступен только для failed-задачи",
        ));
    }
    Ok(Json(json!({ "taskId": id, "status": "pending" })))
}

pub fn schedule_source_graph(state: &AppState, entry: MediaEntry) {
    let worker = state.clone();
    state.spawn_task(async move {
        let project_id = format!("media:{}", entry.id);
        match worker.derived_job_store.project_cancelled(&project_id).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => { tracing::warn!(media.id = %entry.id, %error, "read derived cancellation intent"); return; }
        }
        let Some(path) = worker.library.source_path(&entry) else { return };
        let cancellation = worker.shutdown_token().child_token();
        let identity = match fingerprint_file(&worker.cpu_pool, path.clone(), cancellation).await {
            Ok(value) => value,
            Err(error) => { tracing::warn!(media.id = %entry.id, %error, "fingerprint source for derived graph"); return; }
        };
        let fingerprint = identity.sha256.to_string();
        let profile = ProxyProfile::default();
        let probe_key = Fingerprint::combine([b"probe-v1".as_slice(), fingerprint.as_bytes()]).to_string();
        let mut tasks = vec![DerivedTaskSpec {
            key: "probe".into(), artifact_key: probe_key, kind: DerivedTaskKind::Probe,
            payload: json!({ "sourceId": entry.id, "sourceFingerprint": fingerprint }),
            dependencies: vec![], priority: 10, max_attempts: 3,
        }];
        if entry.width.unwrap_or(0) > 0 {
            let profile_json = serde_json::to_value(&profile).expect("proxy profile serializes");
            let source = SourceIdentity { id: entry.id.clone(), original_path: path.clone(), duration_seconds: entry.duration.unwrap_or(0.0), fingerprint: identity.sha256.clone() };
            let proxy_key = crate::analysis::proxy::proxy_key(&source, &profile).to_string();
            tasks.push(DerivedTaskSpec {
                key: "proxy".into(), artifact_key: proxy_key, kind: DerivedTaskKind::Proxy,
                payload: json!({ "sourceId": entry.id, "sourceFingerprint": fingerprint, "profile": profile_json }),
                dependencies: vec!["probe".into()], priority: 0, max_attempts: 3,
            });
        }
        if let Err(error) = worker.derived_job_store.enqueue_graph(&DerivedGraphSpec { project_id, tasks }).await {
            tracing::warn!(media.id = %entry.id, %error, "enqueue source derived graph");
        }
    });
}

pub fn start_derived_dispatcher(state: &AppState) {
    let worker = state.clone();
    state.spawn_task(async move {
        let worker_id = format!("derived-{}", Uuid::new_v4());
        let shutdown = worker.shutdown_token();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! { _ = shutdown.cancelled() => break, _ = tick.tick() => {} }
            if let Err(error) = worker.derived_job_store.recover_expired().await {
                tracing::warn!(%error, "recover derived leases");
            }
            loop {
                let claim = match worker.derived_job_store.claim_next(&worker_id, 30).await {
                    Ok(Some(claim)) => claim,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::warn!(%error, "claim derived task");
                        break;
                    }
                };
                run_claim(&worker, &worker_id, claim).await;
            }
        }
    });
}

async fn run_claim(state: &AppState, worker_id: &str, claim: ClaimedDerivedTask) {
    let cancellation = state.shutdown_token().child_token();
    let execution = execute_claim(state, &claim, cancellation.clone());
    tokio::pin!(execution);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
    let outcome = loop {
        tokio::select! {
            result = &mut execution => break result,
            _ = heartbeat.tick() => match state.derived_job_store.renew_lease(&claim.task_id, claim.generation, worker_id, 30).await {
                Ok(true) => {}, _ => { cancellation.cancel(); break Err(anyhow::anyhow!("derived task cancelled or lease lost")); }
            }
        }
    };
    match outcome {
        Ok(result) => {
            let _ = state
                .derived_job_store
                .complete(&claim.task_id, claim.generation, worker_id, &result)
                .await;
        }
        Err(error) => {
            let message = crate::privacy::redact_text(&error.to_string());
            let unsupported = matches!(
                claim.kind,
                DerivedTaskKind::Waveform | DerivedTaskKind::Thumbnails | DerivedTaskKind::Conform
            );
            if unsupported {
                let _ = state
                    .derived_job_store
                    .fail_permanent(&claim.task_id, claim.generation, worker_id, &message)
                    .await;
            } else {
                let _ = state
                    .derived_job_store
                    .fail(&claim.task_id, claim.generation, worker_id, &message, 5)
                    .await;
            }
        }
    }
}

async fn execute_claim(
    state: &AppState,
    claim: &ClaimedDerivedTask,
    cancellation: CancellationToken,
) -> anyhow::Result<Value> {
    let source_id = claim
        .payload
        .get("sourceId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("sourceId is required"))?;
    let entry = state
        .library
        .get(source_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("source is offline"))?;
    let path = state
        .library
        .source_path(&entry)
        .ok_or_else(|| anyhow::anyhow!("derived source is not source media"))?;
    match claim.kind {
        DerivedTaskKind::Probe => {
            let info = crate::tools::probe_video(&state.process_runtime, &path).await?;
            Ok(
                json!({ "duration": info.duration, "width": info.width, "height": info.height, "fps": info.fps, "vcodec": info.vcodec, "acodec": info.acodec }),
            )
        }
        DerivedTaskKind::Proxy => {
            let fingerprint = Fingerprint::parse(
                claim
                    .payload
                    .get("sourceFingerprint")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )?;
            let profile: ProxyProfile = serde_json::from_value(
                claim
                    .payload
                    .get("profile")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            )?;
            let source = SourceIdentity {
                id: entry.id,
                original_path: path,
                duration_seconds: entry.duration.unwrap_or(0.0),
                fingerprint,
            };
            let service = ProxyService::new(
                state.storage.clone(),
                state.cpu_pool.clone(),
                Arc::new(FfmpegProxyEncoder::new(state.process_runtime.clone())),
            );
            Ok(serde_json::to_value(
                service.ensure(source, profile, cancellation).await?,
            )?)
        }
        _ => anyhow::bail!(
            "derived executor {} is unavailable",
            serde_json::to_string(&claim.kind)?
        ),
    }
}
