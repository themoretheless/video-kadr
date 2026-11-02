//! Durable encrypted local narration with private immutable result files.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, ErrorKind, JobEvent, JobKind, JobService},
    services::tts_job_identity,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TtsWork {
    pub schema_version: u32,
    owner: String,
    identity: String,
    #[serde(default)]
    model_id: Option<String>,
    sealed: Vec<u8>,
}
impl TtsWork {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.schema_version,1|2)
                && self.identity.len() == 64
                && self.identity.bytes().all(|b| b.is_ascii_hexdigit())
                && self.sealed.len() <= 20000,
            "invalid narration snapshot"
        );
        Ok(())
    }
}
fn path(state: &AppState, id: &str) -> anyhow::Result<std::path::PathBuf> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid narration job ID"
    );
    Ok(state.storage.join("tts-jobs").join(format!("{id}.wav")))
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(submission): ApiJson<super::tts_references::Submission>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let _model_operation = state.tts_model_ops.lock().await;
    let pinned=submission.pin(&state,&owner,&headers).await?;
    let request=&pinned.request;
    let (config, model_id) = super::tts_models::config_for_owner(&state, &owner).await?;
    request.validate_for_model(&config).map_err(|_|AppError::bad_request("Проверьте голос и поддерживаемую скорость выбранной модели"))?;
    super::tts_models::validate_reference(&config,&pinned)?;
    let identity = super::tts_models::identity(&state,&config)
        .await
        .map_err(|_| {
            AppError::service_unavailable("Локальная модель или движок озвучивания недоступны")
        })?;
    let id = uuid::Uuid::new_v4().to_string();
    if !state
        .storage
        .join(".tts-job-key")
        .try_exists()
        .map_err(|e| AppError::internal("inspect narration key", e))?
    {
        let retained: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM job_requests WHERE kind='tts' AND payload_json<>'null')",
        )
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| AppError::internal("inspect retained narration", e))?;
        if retained {
            return Err(AppError::service_unavailable("Локальный ключ озвучивания отсутствует. Восстановите прежний ключ, чтобы продолжить задания."));
        }
    }
    let cipher = tts_job_identity::cipher(&state.storage)
        .await
        .map_err(|e| AppError::internal("open narration encryption key", e))?;
    let sealed = cipher
        .encrypt(
            &tts_job_identity::context(&owner, &id, &identity),
            &serde_json::to_vec(&pinned).map_err(|e| AppError::internal("encode narration", e))?,
        )
        .map_err(|e| AppError::internal("encrypt narration", e))?;
    sqlx::query("INSERT INTO tts_job_access(job_id,owner,voice_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(request.voice_id as i64)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save narration owner", e))?;
    let payload = serde_json::to_value(TtsWork {
        schema_version: 2,
        owner,
        identity,
        model_id,
        sealed,
    })
    .map_err(|e| AppError::internal("encode narration work", e))?;
    let outcome = state
        .enqueue_job(id.clone(), JobKind::Tts, &payload, &format!("tts:{id}"))
        .await
        .map_err(|e| AppError::internal("enqueue narration", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests(
            "Слишком много заданий озвучивания",
        ));
    }
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
async fn owner(state: &AppState, id: &str) -> AppResult<Option<String>> {
    sqlx::query_scalar("SELECT owner FROM tts_job_access WHERE job_id=?")
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read narration owner", e))
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<()> {
    if let Some(owner) = owner(state, id).await? {
        if super::publish::authenticated_actor(state, headers).await? != owner {
            return Err(AppError::forbidden("Это озвучка другого пользователя"));
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    cursor: Option<String>,
    limit: Option<u32>,
}
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Размер страницы должен быть 1–50"));
    }
    let anchor = if let Some(id) = page.cursor {
        let time:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN tts_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=?").bind(&id).bind(&owner).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read narration cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница озвучивания"))?,
            id,
        ))
    } else {
        None
    };
    let rows:Vec<(String,i64,i64)>=sqlx::query_as("SELECT j.id,a.voice_id,j.created_at FROM jobs j JOIN tts_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str())).bind(limit as i64+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list narration",e))?;
    let next = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|r| r.0.clone())
    } else {
        None
    };
    let mut jobs = vec![];
    for (id, voice_id, created_at) in rows.into_iter().take(limit as usize) {
        if let Some(job) = state
            .get_job(&id)
            .await
            .map_err(|e| AppError::internal("load narration", e))?
        {
            jobs.push(json!({"jobId":id,"voiceId":voice_id,"createdAt":created_at,"status":job.status,"progress":job.progress}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}
async fn metadata(state: &AppState, id: &str) -> anyhow::Result<Option<Value>> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT metadata_json FROM tts_job_audio WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await?;
    value
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(Into::into)
}
async fn checked_audio(state: &AppState, id: &str, meta: &Value) -> anyhow::Result<Vec<u8>> {
    let path = path(state, id)?;
    let size = tokio::fs::metadata(&path).await?.len();
    anyhow::ensure!(
        size <= 64 * 1024 * 1024 && meta["audioBytes"].as_u64() == Some(size),
        "narration artifact size changed"
    );
    let bytes = tokio::fs::read(path).await?;
    use sha2::Digest;
    let hash: String = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    anyhow::ensure!(
        meta["audioHash"].as_str() == Some(&hash),
        "narration artifact changed"
    );
    Ok(bytes)
}
pub async fn audio(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if owner(&state, &id).await?.is_none() {
        return Err(AppError::not_found("Озвучка не найдена"));
    }
    authorize_job(&state, &id, &headers).await?;
    let job = state
        .get_job(&id)
        .await
        .map_err(|e| AppError::internal("load narration result", e))?
        .ok_or_else(|| AppError::not_found("Озвучка не найдена"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Озвучка ещё не готова"));
    }
    let meta = metadata(&state, &id)
        .await
        .map_err(|e| AppError::internal("read narration metadata", e))?
        .ok_or_else(|| AppError::not_found("Файл озвучки отсутствует"))?;
    let bytes = checked_audio(&state, &id, &meta)
        .await
        .map_err(|_| AppError::conflict("Сохранённый файл озвучки недоступен или изменён"))?;
    Response::builder()
        .header(header::CONTENT_TYPE, "audio/wav")
        .header(header::CACHE_CONTROL, "no-store")
        .header(
            "x-tts-model-sha256",
            meta["modelHash"].as_str().unwrap_or(""),
        )
        .header(
            "x-tts-sample-rate",
            meta["sampleRate"].as_u64().unwrap_or(0).to_string(),
        )
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"narration.wav\"",
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal("build narration result", e))
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: TtsWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move{
 let _lease=lease;if !JobService::mark_queued(&st,&id).await{st.clear_cancel(&id).await;return;}let Some(_permit)=JobService::acquire_execution(&st,&id,&token,None,AcquirePlan{resource_class:ResourceClass::Analysis,acquire_render_permit:false}).await else{return};
 if !JobService::mark_running(&st,&id,"tts_loading",attempt).await{st.clear_cancel(&id).await;return;}
 let outcome=async{
  let cipher=tts_job_identity::load_cipher(&st.storage).await.map_err(|_|(ErrorKind::Storage,"Ключ озвучивания недоступен"))?;
  let plain=cipher.decrypt(&tts_job_identity::context(&work.owner,&id,&work.identity),&work.sealed).map_err(|_|(ErrorKind::Security,"Запрос озвучивания изменён"))?;
  let pinned=super::tts_references::PinnedInput::decode(work.schema_version,&plain).map_err(|_|(ErrorKind::Validation,"Запрос озвучивания повреждён"))?;let request=&pinned.request;request.validate().map_err(|_|(ErrorKind::Validation,"Некорректный запрос озвучивания"))?;
  let reference_hash=json!(pinned.reference.as_ref().map(|r|r.hash.as_str()));

  if let Some(snapshot)=&pinned.reference{snapshot.authorize_origin(&st,&work.owner).await.map_err(|_|(ErrorKind::Validation,"Доступ к исходнику образца отозван"))?;}
  if let Some(meta)=metadata(&st,&id).await.map_err(|_|(ErrorKind::Storage,"Не удалось прочитать результат"))?{if meta["bundleHash"]==work.identity&&meta["referenceHash"]==reference_hash&&checked_audio(&st,&id,&meta).await.is_ok(){return Ok(meta)}}
  let config=super::tts_models::config_for_model(&st,&work.owner,work.model_id.as_deref()).await.map_err(|_|(ErrorKind::Validation,"Выбранная модель озвучивания недоступна"))?;
  if super::tts_models::identity(&st,&config).await.map_err(|_|(ErrorKind::Validation,"Модель озвучивания недоступна"))?!=work.identity{return Err((ErrorKind::Validation,"Модель озвучивания изменилась"));}
  let reference=if let Some(snapshot)=&pinned.reference{Some(snapshot.resolve(&st,&work.owner).await.map_err(|_|(ErrorKind::Validation,"Образец голоса изменён или недоступен"))?)}else{None};
  let output=path(&st,&id).map_err(|_|(ErrorKind::Validation,"Некорректный номер озвучки"))?;let directory=output.parent().expect("internal narration path");tokio::fs::create_dir_all(directory).await.map_err(|_|(ErrorKind::Storage,"Не удалось подготовить озвучку"))?;
  #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(directory,std::fs::Permissions::from_mode(0o700)).await.map_err(|_|(ErrorKind::Storage,"Не удалось защитить озвучку"))?;}
  // This reserved internal job path can contain an uncommitted prior attempt.
  if output.exists(){tokio::fs::remove_file(&output).await.map_err(|_|(ErrorKind::Storage,"Не удалось подготовить повтор озвучивания"))?;}
  st.update_job_if_open(&id,|job|{job.progress=None;job.stage=Some("tts_synthesizing".into());}).await;
  let(progress,receiver)=tokio::sync::mpsc::unbounded_channel();let drain=JobService::spawn_progress_drain(st.clone(),id.clone(),receiver);
  let generated=super::tts_models::synthesize(&st,&config,request,&output,&token,st.job_timeout().min(std::time::Duration::from_secs(600)),reference.as_deref(),Some(&progress)).await;drop(progress);let _=drain.await;let generated=generated.map_err(|_|(ErrorKind::ProcessExit,"Не удалось создать озвучку локальной моделью"))?;
  if token.is_cancelled(){return Err((ErrorKind::Interrupted,"Озвучивание остановлено"));}
  if super::tts_models::identity(&st,&config).await.map_err(|_|(ErrorKind::Validation,"Модель озвучивания недоступна"))?!=work.identity{return Err((ErrorKind::Validation,"Модель озвучивания изменилась"));}
  if let Some(snapshot)=&pinned.reference{snapshot.resolve(&st,&work.owner).await.map_err(|_|(ErrorKind::Validation,"Образец голоса изменился"))?;}
  let(size,hash)=crate::services::local_speech::fingerprint_model(&output).await.map_err(|_|(ErrorKind::Storage,"Не удалось проверить озвучку"))?;
  let mut meta=serde_json::to_value(generated).map_err(|_|(ErrorKind::Internal,"Не удалось сохранить результат"))?;meta["bundleHash"]=json!(work.identity);meta["referenceHash"]=reference_hash;meta["audioHash"]=json!(hash);meta["audioBytes"]=json!(size);
  sqlx::query("INSERT INTO tts_job_audio(job_id,metadata_json) VALUES(?,?) ON CONFLICT(job_id) DO UPDATE SET metadata_json=excluded.metadata_json").bind(&id).bind(meta.to_string()).execute(st.db.pool()).await.map_err(|_|(ErrorKind::Storage,"Не удалось сохранить результат"))?;Ok::<_,(ErrorKind,&str)>(meta)
 }.await;
 if token.is_cancelled(){if !st.is_shutting_down(){if let Ok(path)=path(&st,&id){let _=tokio::fs::remove_file(path).await;}let _=sqlx::query("DELETE FROM tts_job_audio WHERE job_id=?").bind(&id).execute(st.db.pool()).await;}JobService::mark_cancelled(&st,&id).await;}else{let event=match outcome{Ok(result)=>JobEvent::Succeeded{result},Err((kind,message))=>{if let Ok(path)=path(&st,&id){let _=tokio::fs::remove_file(path).await;}JobEvent::Failed{kind,message:message.into()}}};JobService::transition(&st,&id,event).await;st.clear_cancel(&id).await;}
});
}

/// Called under the admission/removal lock. Invalid retained work conservatively
/// refuses collection, including a missing encryption key; never rotate it here.
pub(super) async fn retained_references(state:&AppState)->anyhow::Result<std::collections::HashSet<String>>{
 let rows:Vec<(String,String)>=sqlx::query_as("SELECT job_id,payload_json FROM job_requests WHERE kind='tts' AND payload_json<>'null'").fetch_all(state.db.pool()).await?;
 let mut references=std::collections::HashSet::new();
 if rows.is_empty(){return Ok(references);}
 let cipher=tts_job_identity::load_cipher(&state.storage).await?;
 for(id,payload)in rows{let work:TtsWork=serde_json::from_str(&payload)?;work.validate()?;
 let plain=cipher.decrypt(&tts_job_identity::context(&work.owner,&id,&work.identity),&work.sealed)?;
 let input=super::tts_references::PinnedInput::decode(work.schema_version,&plain)?;
 if let Some(reference)=input.reference{references.insert(reference.id);}}
 Ok(references)
}
