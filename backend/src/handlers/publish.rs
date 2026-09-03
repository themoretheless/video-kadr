use anyhow::Context;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::resource_classes::ResourceClass;
use crate::error::{AppError, AppResult};
use crate::jobs::{EnqueueOutcome, ErrorKind, JobEvent, JobKind};
use crate::state::AppState;
use crate::youtube::{
    YouTubeConnectionStatus, YouTubeUploadCheckpoint, YouTubeUploadMetadata, YouTubeUploadRequest,
};

use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use super::{acquire_job_permit_or_cancelled, apply_job_event, mark_running, spawn_progress_drain};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConnectResponse {
    authorization_url: String,
}

#[derive(Debug, Deserialize)]
pub struct YouTubeCallbackQuery {
    state: String,
    code: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct YouTubePublishRequest {
    output_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default = "private_privacy")]
    privacy_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PublishWork {
    pub schema_version: u32,
    pub output_id: String,
    pub actor: String,
    pub metadata: YouTubeUploadMetadata,
}

fn private_privacy() -> String {
    "private".into()
}

pub async fn youtube_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<YouTubeConnectionStatus>> {
    let actor = authenticated_actor(&state, &headers).await?;
    let connected = state
        .db
        .has_youtube_connection(&actor)
        .await
        .map_err(|error| AppError::internal("read YouTube connection", error))?;
    Ok(Json(YouTubeConnectionStatus {
        configured: state.youtube_oauth.is_some(),
        connected,
    }))
}

pub async fn youtube_callback_handler(
    State(state): State<AppState>,
    Query(query): Query<YouTubeCallbackQuery>,
) -> AppResult<Json<YouTubeConnectionStatus>> {
    let client = state
        .youtube_oauth
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("YouTube OAuth не настроен на сервере"))?;
    let cipher = state
        .youtube_token_cipher
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Хранилище токенов YouTube не настроено"))?;
    let actor = state
        .db
        .consume_publish_oauth_state("youtube", &query.state)
        .await
        .map_err(|error| AppError::internal("consume YouTube OAuth state", error))?
        .ok_or_else(|| {
            AppError::bad_request("Состояние подключения YouTube недействительно или истекло")
        })?;
    if let Some(error) = query.error.as_deref() {
        return Err(AppError::bad_request(format!(
            "YouTube отклонил подключение: {}",
            safe_provider_error(error)
        )));
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("YouTube не вернул код авторизации"))?;
    let tokens = client.exchange_code(code).await.map_err(|error| {
        tracing::warn!(
            error = %crate::privacy::redact_text(&error.to_string()),
            "YouTube OAuth exchange failed"
        );
        AppError::service_unavailable("Не удалось завершить подключение YouTube")
    })?;
    state
        .db
        .save_youtube_tokens(cipher, &actor, tokens)
        .await
        .map_err(|error| AppError::internal("save encrypted YouTube tokens", error))?;
    Ok(Json(YouTubeConnectionStatus {
        configured: true,
        connected: true,
    }))
}

pub async fn youtube_disconnect_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let actor = authenticated_actor(&state, &headers).await?;
    let client = state
        .youtube_oauth
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("YouTube OAuth не настроен на сервере"))?;
    let cipher = state
        .youtube_token_cipher
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Хранилище токенов YouTube не настроено"))?;
    if let Some(tokens) = state
        .db
        .load_youtube_tokens(cipher, &actor)
        .await
        .map_err(|error| AppError::internal("load YouTube tokens for revocation", error))?
    {
        let token = tokens
            .refresh_token
            .as_deref()
            .unwrap_or(&tokens.access_token);
        client.revoke_token(token).await.map_err(|error| {
            tracing::warn!(error = %crate::privacy::redact_text(&error.to_string()), "YouTube OAuth revocation failed");
            AppError::service_unavailable("Не удалось отозвать доступ YouTube; подключение сохранено для повторной попытки")
        })?;
    }
    state
        .db
        .delete_youtube_connection(&actor)
        .await
        .map_err(|error| AppError::internal("delete YouTube connection", error))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn youtube_publish_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    crate::error::ApiJson(request): crate::error::ApiJson<YouTubePublishRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let actor = authenticated_actor(&state, &headers).await?;
    let output_id = uuid::Uuid::parse_str(&request.output_id)
        .map_err(|_| AppError::bad_request("Некорректный идентификатор результата"))?
        .to_string();
    if !state
        .db
        .can_access_output(&output_id, &actor)
        .await
        .map_err(|error| AppError::internal("authorize YouTube output", error))?
    {
        return Err(AppError::not_found("Результат экспорта не найден"));
    }
    if state.youtube_oauth.is_none() || state.youtube_token_cipher.is_none() {
        return Err(AppError::service_unavailable(
            "YouTube OAuth не настроен на сервере",
        ));
    }
    if !state
        .db
        .has_youtube_connection(&actor)
        .await
        .map_err(|error| AppError::internal("read YouTube connection", error))?
    {
        return Err(AppError::conflict("Сначала подключите YouTube"));
    }
    let metadata = YouTubeUploadMetadata {
        title: request.title,
        description: request.description,
        privacy_status: request.privacy_status,
    };
    metadata
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    // Validate before enqueueing by serializing the exact durable identity; the
    // transport repeats provider limits before any network request.
    let identity = serde_json::to_vec(&(&actor, &output_id, &metadata))
        .map_err(|error| AppError::internal("serialize YouTube publish identity", error))?;
    let key = format!("{:x}", Sha256::digest(identity));
    let work = PublishWork {
        schema_version: 1,
        output_id,
        actor,
        metadata,
    };
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize YouTube publish job", error))?;
    let job_id = Uuid::new_v4().to_string();
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Publish, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue YouTube publish job", error))?
    {
        EnqueueOutcome::Created(_) => job_id,
        EnqueueOutcome::Existing(id) => id,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests(
                "Слишком много новых задач; повторите позже",
            ))
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobId": resolved_id }))))
}

pub(super) fn spawn_publish_job(
    state: AppState,
    job_id: String,
    work: PublishWork,
    attempt: u32,
    cancellation: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let owner = state.clone();
    owner.spawn_task(async move {
        let _lease = lease;
        if !mark_running(&state, &job_id, "publishing", attempt).await {
            return;
        }
        let Some(_permit) =
            acquire_job_permit_or_cancelled(&state, &job_id, &cancellation, ResourceClass::Export)
                .await
        else {
            return;
        };
        let (progress, receiver) = mpsc::unbounded_channel();
        let (checkpoints, mut checkpoint_receiver) = mpsc::unbounded_channel();
        let drain = spawn_progress_drain(state.clone(), job_id.clone(), receiver);
        let checkpoint_drain = state.youtube_token_cipher.clone().map(|cipher| {
            let db = state.db.clone();
            let checkpoint_job_id = job_id.clone();
            let actor = work.actor.clone();
            let output_id = work.output_id.clone();
            state.spawn_task(async move {
                while let Some(checkpoint) = checkpoint_receiver.recv().await {
                    if let Err(error) = db.save_youtube_upload_checkpoint(
                        &cipher, &checkpoint_job_id, &actor, &output_id, &checkpoint,
                    ).await {
                        tracing::error!(job.id = %checkpoint_job_id, %error, "persist YouTube upload checkpoint");
                    }
                }
            })
        });
        let outcome = run_publish_work(&state, &job_id, &work, &progress, &checkpoints, &cancellation).await;
        drop(progress);
        drop(checkpoints);
        let _ = drain.await;
        if let Some(drain) = checkpoint_drain { let _ = drain.await; }
        let clear_checkpoint = outcome.is_ok();
        finish_publish_job(&state, &job_id, outcome).await;
        if clear_checkpoint {
            if let Err(error) = state.db.delete_youtube_upload_checkpoint(&job_id).await {
                tracing::warn!(job.id = %job_id, %error, "clear YouTube upload checkpoint");
            }
        }
    });
}

async fn run_publish_work(
    state: &AppState,
    job_id: &str,
    work: &PublishWork,
    progress: &mpsc::UnboundedSender<f64>,
    checkpoints: &mpsc::UnboundedSender<YouTubeUploadCheckpoint>,
    cancellation: &CancellationToken,
) -> anyhow::Result<Option<Value>> {
    anyhow::ensure!(
        state
            .db
            .can_access_output(&work.output_id, &work.actor)
            .await?,
        "Результат экспорта больше недоступен"
    );
    let path = resolve_output_path(state, &work.output_id).await?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let mime_type = match extension {
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => anyhow::bail!("Формат результата не поддерживается YouTube"),
    };
    let client = state
        .youtube_oauth
        .as_ref()
        .context("YouTube OAuth не настроен")?;
    let cipher = state
        .youtube_token_cipher
        .as_ref()
        .context("Хранилище токенов YouTube не настроено")?;
    let mut tokens = state
        .db
        .load_youtube_tokens(cipher, &work.actor)
        .await?
        .context("YouTube не подключён")?;
    if tokens.expires_at <= crate::library::now_secs().saturating_add(120) {
        let refresh = tokens
            .refresh_token
            .as_deref()
            .context("YouTube требует повторного подключения")?;
        tokens = client.refresh_access_token(refresh).await?;
        state
            .db
            .save_youtube_tokens(cipher, &work.actor, tokens.clone())
            .await?;
    }
    let checkpoint = state
        .db
        .load_youtube_upload_checkpoint(cipher, job_id, &work.actor, &work.output_id)
        .await?;
    let video_id = if let Some(checkpoint) = checkpoint {
        match client
            .resume_resumable(
                YouTubeUploadRequest {
                    access_token: &tokens.access_token,
                    path: &path,
                    mime_type,
                    progress,
                    cancellation,
                    checkpoints,
                },
                &checkpoint,
            )
            .await?
        {
            Some(video_id) => video_id,
            None => {
                state.db.delete_youtube_upload_checkpoint(job_id).await?;
                client
                    .upload_resumable(
                        YouTubeUploadRequest {
                            access_token: &tokens.access_token,
                            path: &path,
                            mime_type,
                            progress,
                            cancellation,
                            checkpoints,
                        },
                        &work.metadata,
                    )
                    .await?
            }
        }
    } else {
        client
            .upload_resumable(
                YouTubeUploadRequest {
                    access_token: &tokens.access_token,
                    path: &path,
                    mime_type,
                    progress,
                    cancellation,
                    checkpoints,
                },
                &work.metadata,
            )
            .await?
    };
    if video_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({ "provider": "youtube", "videoId": video_id, "url": format!("https://www.youtube.com/watch?v={video_id}"), "outputId": work.output_id }),
    ))
}

async fn resolve_output_path(
    state: &AppState,
    output_id: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let mut entries = tokio::fs::read_dir(state.outputs_dir()).await?;
    let prefix = format!("{output_id}.");
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && entry.file_type().await?.is_file() {
            return Ok(entry.path());
        }
    }
    anyhow::bail!("Результат экспорта не найден")
}

async fn finish_publish_job(
    state: &AppState,
    job_id: &str,
    outcome: anyhow::Result<Option<Value>>,
) {
    let event = match outcome {
        Ok(Some(result)) => JobEvent::Succeeded { result },
        Ok(None) => JobEvent::Cancelled,
        Err(error) => {
            let message = crate::privacy::redact_text(&error.to_string());
            tracing::error!(job.id = job_id, error = %message, "YouTube publish failed");
            JobEvent::Failed {
                kind: ErrorKind::Internal,
                message,
            }
        }
    };
    let _ = apply_job_event(state, job_id, event).await;
    state.clear_cancel(job_id).await;
}

fn safe_provider_error(value: &str) -> &str {
    match value {
        "access_denied" => "доступ не предоставлен",
        "temporarily_unavailable" => "сервис временно недоступен",
        _ => "ошибка авторизации",
    }
}

pub async fn youtube_connect_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<OAuthConnectResponse>> {
    let actor = authenticated_actor(&state, &headers).await?;
    let client = state
        .youtube_oauth
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("YouTube OAuth не настроен на сервере"))?;
    let csrf_state = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
    state
        .db
        .create_publish_oauth_state("youtube", &actor, &csrf_state)
        .await
        .map_err(|error| AppError::internal("persist YouTube OAuth state", error))?;
    let authorization_url = client
        .authorization_url(&csrf_state)
        .map_err(|error| AppError::internal("build YouTube authorization URL", error))?;
    Ok(Json(OAuthConnectResponse { authorization_url }))
}

async fn authenticated_actor(state: &AppState, headers: &HeaderMap) -> AppResult<String> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::unauthorized("Требуется сессия для публикации"))?;
    state
        .db
        .resolve_auth_session(token)
        .await
        .map_err(|error| AppError::internal("authenticate publishing request", error))?
        .map(|user| user.username)
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}
