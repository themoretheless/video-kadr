//! Durable owner-scoped offline speech jobs, sharing the existing outbox and cancellation registry.
use super::{
    jobs::{dispatch_job, JobLeaseHeartbeat},
    speech::Request,
};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::local_speech,
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
enum SpeechWorkError {
    AccessRevoked,
    SourceChanged,
    ModelChanged,
}
impl std::fmt::Display for SpeechWorkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AccessRevoked => "speech source access revoked",
            Self::SourceChanged => "speech source fingerprint changed",
            Self::ModelChanged => "speech model fingerprint changed",
        })
    }
}
impl std::error::Error for SpeechWorkError {}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpeechWork {
    pub schema_version: u32,
    request: Request,
    owner: String,
    source_hash: String,
    model_hash: String,
    #[serde(default)] model_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<ListQuery>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Размер страницы должен быть 1–50"));
    }
    let anchor = if let Some(cursor) = &query.cursor {
        let time: Option<i64> = sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN speech_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=?")
            .bind(cursor).bind(&owner).fetch_optional(state.db.pool()).await
            .map_err(|e|AppError::internal("load speech history cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница заданий"))?,
            cursor.clone(),
        ))
    } else {
        None
    };
    let rows: Vec<(String,String,i64)> = sqlx::query_as(
        "SELECT j.id,a.source_id,j.created_at FROM jobs j JOIN speech_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?")
        .bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0))
        .bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str()))
        .bind(i64::from(limit)+1).fetch_all(state.db.pool()).await
        .map_err(|e|AppError::internal("list speech history",e))?;
    let next_cursor = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|r| r.0.clone())
    } else {
        None
    };
    let mut jobs = Vec::new();
    for (id, source, created_at) in rows.into_iter().take(limit as usize) {
        if let Some(space) = state
            .db
            .source_space_id(&source)
            .await
            .map_err(|e| AppError::internal("resolve speech history source", e))?
        {
            if !state
                .db
                .is_space_member(&space, &owner)
                .await
                .map_err(|e| AppError::internal("authorize speech history source", e))?
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
            .map_err(|e| AppError::internal("read speech history job", e))?
        {
            jobs.push(json!({"jobId":id,"sourceId":source,"status":job.status,"progress":job.progress,"createdAt":created_at}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next_cursor})))
}

pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    require_source: bool,
) -> AppResult<()> {
    let access: Option<(String, String)> =
        sqlx::query_as("SELECT owner, source_id FROM speech_job_access WHERE job_id = ?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load speech job access", e))?;
    if let Some((owner, source)) = access {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        if actor != owner {
            return Err(AppError::forbidden("Это задание другого пользователя"));
        }
        if require_source {
            super::library::authorize_source_access(state, &source, headers).await?;
        }
    }
    Ok(())
}

async fn source_path(
    state: &AppState,
    source: &str,
    owner: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if let Some(space) = state.db.source_space_id(source).await? {
        if !state.db.is_space_member(&space, owner).await? {
            return Err(SpeechWorkError::AccessRevoked.into());
        }
    }
    let entry = state
        .library
        .get(source)
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| anyhow::anyhow!("speech source unavailable"))?;
    Ok(state.library.resolve_media_path(&entry).await?)
}

pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    let model_operation=state.speech_model_ops.lock().await;
    let (config,model_id)=super::speech_models::config_for_owner(&state,Some(&owner)).await?;
    if config.model.is_none() || !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Нужны локальные Whisper, модель и FFmpeg",
        ));
    }
    if request.start_us >= request.end_us || request.end_us - request.start_us > 600_000_000 {
        return Err(AppError::bad_request("Выберите диапазон речи до 10 минут"));
    }
    let path = source_path(&state, &request.source_id, &owner)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let model = config.model.as_ref().expect("checked model");
    let source_hash = crate::project_archive::fingerprint_file(&path)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
        .1;
    let model_hash = local_speech::fingerprint_model(model)
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?
        .1;
    let id = uuid::Uuid::new_v4().to_string();
    // Persist ACL before making work visible to the outbox dispatcher.
    sqlx::query("INSERT INTO speech_job_access(job_id, owner, source_id) VALUES(?, ?, ?)")
        .bind(&id)
        .bind(&owner)
        .bind(&request.source_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save speech job access", e))?;
    let work = SpeechWork {
        schema_version: 1,
        request,
        owner,
        source_hash,
        model_hash,
        model_id,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize speech job", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::Speech,
            &payload,
            &format!("speech:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue speech job", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    drop(model_operation);
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}

pub(super) fn spawn_speech_job(
    state: AppState,
    id: String,
    work: SpeechWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        let _model_use=super::speech_models::ModelUse::new(&st,work.model_id.clone(),false);
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
        if !JobService::mark_running(&st, &id, "speech", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (progress, receiver) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), receiver);
        let outcome: anyhow::Result<Value> = async {
            let path = source_path(&st, &work.request.source_id, &work.owner).await?;
            if crate::project_archive::fingerprint_file(&path).await?.1 != work.source_hash {
                return Err(SpeechWorkError::SourceChanged.into());
            }
            let config=super::speech_models::config_for_snapshot(&st,&work.owner,work.model_id.as_deref()).await.map_err(|_|SpeechWorkError::ModelChanged)?;
            let model = config
                .model
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("speech model unavailable"))?;
            if local_speech::fingerprint_model(model).await?.1 != work.model_hash {
                return Err(SpeechWorkError::ModelChanged.into());
            }
            let probe = crate::tools::probe_video(&st.process_runtime, &path).await?;
            anyhow::ensure!(
                probe.acodec.is_some()
                    && probe.duration.is_finite()
                    && probe.duration > 0.0
                    && work.request.end_us as f64 <= (probe.duration * 1_000_000.0).ceil(),
                "speech range outside audio source"
            );
            st.update_job_if_open(&id, |job| job.progress = Some(5.0))
                .await;
            let transcript = local_speech::transcribe_segmented(
                &st.process_runtime,
                &config,
                &path,
                work.request.start_us,
                work.request.end_us,
                &st.storage.join("speech-staging"),
                &work.request.language,
                &token,
                work.request.segmentation,
                &progress,
            )
            .await?;
            let current = source_path(&st, &work.request.source_id, &work.owner).await?;
            if crate::project_archive::fingerprint_file(&current).await?.1 != work.source_hash {
                return Err(SpeechWorkError::SourceChanged.into());
            }
            if transcript.model_hash != work.model_hash {
                return Err(SpeechWorkError::ModelChanged.into());
            }
            Ok(serde_json::to_value(transcript)?)
        }
        .await;
        drop(progress);
        let _ = drain.await;
        // Speech is data, not a MediaEntry: never insert transcript text in the shared library.
        if token.is_cancelled() {
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => JobEvent::Failed {
                    kind: match error.downcast_ref::<SpeechWorkError>() {
                        Some(SpeechWorkError::AccessRevoked) => crate::jobs::ErrorKind::Security,
                        Some(SpeechWorkError::SourceChanged | SpeechWorkError::ModelChanged) => {
                            crate::jobs::ErrorKind::Validation
                        }
                        None => crate::jobs::classify_job_error(&error),
                    },
                    message: crate::privacy::redact_text(&error.to_string()),
                },
            };
            JobService::transition(&st, &id, event).await;
            st.clear_cancel(&id).await;
        }
    });
}
