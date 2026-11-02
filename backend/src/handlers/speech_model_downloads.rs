//! Pinned public Whisper catalog and durable owner-scoped verified downloads.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
pub const REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CatalogModel {
    pub id: String,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}
fn catalog() -> &'static Vec<CatalogModel> {
    static DATA: std::sync::OnceLock<Vec<CatalogModel>> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../services/whisper-model-catalog.json"))
            .expect("pinned model catalog")
    })
}
pub async fn catalog_handler() -> Json<Value> {
    Json(json!({"revision":REVISION,"models":catalog()}))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DownloadWork {
    schema_version: u32,
    owner: String,
    revision: String,
    model: CatalogModel,
}
impl DownloadWork {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && self.revision == REVISION
                && catalog().iter().any(|m| m == &self.model),
            "unsupported model catalog snapshot"
        );
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Submit {
    catalog_id: String,
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Submit>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let model = catalog()
        .iter()
        .find(|m| m.id == request.catalog_id)
        .cloned()
        .ok_or_else(|| AppError::bad_request("Модель отсутствует в каталоге"))?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO speech_model_download_access(job_id,owner,catalog_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&model.id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save model download access", e))?;
    let work = DownloadWork {
        schema_version: 1,
        owner,
        revision: REVISION.into(),
        model,
    };
    let payload = serde_json::to_value(work)
        .map_err(|e| AppError::internal("serialize model download", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::SpeechModelDownload,
            &payload,
            &format!("speech-model-download:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue model download", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много загрузок"));
    }
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<()> {
    let owner: Option<String> =
        sqlx::query_scalar("SELECT owner FROM speech_model_download_access WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read model download access", e))?;
    if let Some(owner) = owner {
        if super::publish::authenticated_actor(state, headers).await? != owner {
            return Err(AppError::forbidden("Это загрузка другого пользователя"));
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
    let anchor = if let Some(cursor) = page.cursor {
        let time:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN speech_model_download_access a ON a.job_id=j.id WHERE a.owner=? AND j.id=?").bind(&owner).bind(&cursor).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read download cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница загрузок"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.catalog_id,j.created_at FROM jobs j JOIN speech_model_download_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list model downloads",e))?;
    let next = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|r| r.0.clone())
    } else {
        None
    };
    let mut jobs = vec![];
    for (id, catalog_id, created_at) in rows.into_iter().take(limit as usize) {
        if let Some(job) = state
            .get_job(&id)
            .await
            .map_err(|e| AppError::internal("load model download", e))?
        {
            jobs.push(json!({"jobId":id,"catalogId":catalog_id,"createdAt":created_at,"status":job.status,"progress":job.progress}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: DownloadWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move{
 let _lease=lease;if !JobService::mark_queued(&st,&id).await{st.clear_cancel(&id).await;return}let Some(_guards)=JobService::acquire_execution(&st,&id,&token,None,AcquirePlan{resource_class:ResourceClass::Ingest,acquire_render_permit:false}).await else{return};if !JobService::mark_running(&st,&id,"model_download",attempt).await{st.clear_cancel(&id).await;return}
 let(progress,receiver)=tokio::sync::mpsc::unbounded_channel();let drain=JobService::spawn_progress_drain(st.clone(),id.clone(),receiver);
 let future=async{
  let client=reqwest::Client::builder().https_only(true).connect_timeout(std::time::Duration::from_secs(30)).timeout(st.job_timeout()).redirect(reqwest::redirect::Policy::limited(10)).build().map_err(|e|(crate::jobs::ErrorKind::Internal,e.to_string()))?;
  let url=format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/{}/{}?download=true",work.revision,work.model.file);
  let response=client.get(url).send().await.map_err(|e|(if e.is_timeout(){crate::jobs::ErrorKind::Timeout}else{crate::jobs::ErrorKind::ProcessExit},e.to_string()))?.error_for_status().map_err(|e|(crate::jobs::ErrorKind::ProcessExit,e.to_string()))?;
  if let Some(bytes)=response.content_length(){if bytes!=work.model.bytes{return Err((crate::jobs::ErrorKind::Validation,"Размер модели не совпадает с каталогом".into()))}}let _=progress.send(5.);
  let failure=std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));let body=download_body(response,failure.clone());
  let Json(model)=super::speech_models::install_model(&st,work.owner.clone(),work.model.file.clone(),body,Some((work.model.sha256.clone(),work.model.bytes)),Some(progress.clone())).await.map_err(|e|(download_error_kind(&failure),format!("model installation failed: {e:?}")))?;Ok(model)
 };
 let outcome:Result<Value,(crate::jobs::ErrorKind,String)>=tokio::select!{_ = token.cancelled()=>Err((crate::jobs::ErrorKind::Interrupted,"Загрузка отменена".into())),result=tokio::time::timeout(st.job_timeout(),future)=>match result{Ok(result)=>result,Err(_)=>Err((crate::jobs::ErrorKind::Timeout,"Время загрузки истекло".into()))}};drop(progress);let _=drain.await;
 if token.is_cancelled(){JobService::mark_cancelled(&st,&id).await;}else{let event=match outcome{Ok(result)=>JobEvent::Succeeded{result},Err((kind,message))=>JobEvent::Failed{kind,message:crate::privacy::redact_text(&message)}};JobService::transition(&st,&id,event).await;st.clear_cancel(&id).await;}
});
}

// Preserve transport failure identity through the shared HTTP-body installer.
// A checksum/header rejection remains non-retryable validation; a broken HTTPS
// stream must retain the durable payload so that an explicit retry can run it.
fn download_error_kind(failure: &std::sync::atomic::AtomicU8) -> crate::jobs::ErrorKind {
    use std::sync::atomic::Ordering;
    match failure.load(Ordering::Relaxed) {
        1 => crate::jobs::ErrorKind::ProcessExit,
        2 => crate::jobs::ErrorKind::Timeout,
        _ => crate::jobs::ErrorKind::Validation,
    }
}
fn download_body(
    response: reqwest::Response,
    failure: std::sync::Arc<std::sync::atomic::AtomicU8>,
) -> Body {
    let stream = futures_util::stream::try_unfold(
        (response, failure),
        |(mut response, failure)| async move {
            match response.chunk().await {
                Ok(Some(bytes)) => Ok(Some((bytes, (response, failure)))),
                Ok(None) => Ok(None),
                Err(error) => {
                    failure.store(
                        if error.is_timeout() { 2 } else { 1 },
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    Err(error)
                }
            }
        },
    );
    Body::from_stream(stream)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn truncated_transport_is_retryable_but_validation_is_not() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\ntruncated").await.unwrap();
        });
        let response = reqwest::get(format!("http://{address}/model"))
            .await
            .unwrap();
        let failure = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        assert!(
            axum::body::to_bytes(download_body(response, failure.clone()), 2_000_000)
                .await
                .is_err()
        );
        server.await.unwrap();
        assert_eq!(
            download_error_kind(&failure),
            crate::jobs::ErrorKind::ProcessExit
        );
        assert_eq!(
            download_error_kind(&std::sync::atomic::AtomicU8::new(0)),
            crate::jobs::ErrorKind::Validation
        );
        assert_eq!(
            download_error_kind(&std::sync::atomic::AtomicU8::new(2)),
            crate::jobs::ErrorKind::Timeout
        );
    }
}
