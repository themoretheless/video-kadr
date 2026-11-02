//! Durable owner-scoped installation of the pinned IS-Net model.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::{sam_model as sam_model_download, sam_model::Catalog as ModelCatalog},
    state::AppState,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DownloadWork {
    schema_version: u32,
    owner: String,
    catalog_id: String,
    #[serde(default)]
    epoch: i64,
}
impl DownloadWork {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && self.epoch >= 0
                && ModelCatalog::find(&self.catalog_id)?.is_some(),
            "unsupported SAM model snapshot"
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
    let catalog = ModelCatalog::find(&request.catalog_id)
        .map_err(|e| AppError::internal("read SAM catalog", e))?
        .ok_or_else(|| AppError::bad_request("Модель отсутствует в каталоге"))?;
    let _operation = state.sam_model_ops.lock().await;
    let epoch: i64 =
        sqlx::query_scalar("SELECT epoch FROM sam_model_epochs WHERE owner=? AND id=?")
            .bind(&owner)
            .bind(&catalog.id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("capture SAM download epoch", e))?
            .unwrap_or(0);
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sam_model_download_access(job_id,owner,catalog_id) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&catalog.id)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save SAM download owner", e))?;
    let payload = serde_json::to_value(DownloadWork {
        schema_version: 1,
        epoch,
        owner,
        catalog_id: catalog.id,
    })
    .map_err(|e| AppError::internal("encode SAM download", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::SamModelDownload,
            &payload,
            &format!("sam-model-download:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue SAM download", e))?;
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
        sqlx::query_scalar("SELECT owner FROM sam_model_download_access WHERE job_id=?")
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
        let time:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN sam_model_download_access a ON a.job_id=j.id WHERE a.owner=? AND j.id=?").bind(&owner).bind(&cursor).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read download cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница загрузок"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.catalog_id,j.created_at FROM jobs j JOIN sam_model_download_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list model downloads",e))?;
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
    state.spawn_task(async move {
  let _lease=lease;if !JobService::mark_queued(&st,&id).await{st.clear_cancel(&id).await;return;}
  let Some(_permit)=JobService::acquire_execution(&st,&id,&token,None,AcquirePlan{resource_class:ResourceClass::Ingest,acquire_render_permit:false}).await else{return;};
  if !JobService::mark_running(&st,&id,"sam_model_download",attempt).await{st.clear_cancel(&id).await;return;}
  let(progress,receiver)=tokio::sync::mpsc::unbounded_channel();let drain=JobService::spawn_progress_drain(st.clone(),id.clone(),receiver);
  let future=async {
   let catalog=ModelCatalog::find(&work.catalog_id)?.ok_or_else(||anyhow::anyhow!("unknown SAM catalog"))?;
   validate_epoch(&st,&work).await?;
   let installed=sam_model_download::directory(&st.storage,std::path::Path::new("models/sam")).await?.join(&catalog.id);
   {
    let _guard=st.sam_model_ops.lock().await;
    if tokio::fs::symlink_metadata(&installed).await.is_ok() {
     let root=installed.clone();let check=catalog.clone();tokio::task::spawn_blocking(move||check.verify_directory(&root)).await??;
     anyhow::ensure!(!token.is_cancelled(),"download cancelled");
     return register(&st,&work,&catalog).await;
    }
   }
   let staging=sam_model_download::directory(&st.storage,&std::path::Path::new("sam-model-downloads").join(&id)).await?;
   let client=reqwest::Client::builder().https_only(true).connect_timeout(std::time::Duration::from_secs(30)).timeout(st.job_timeout()).redirect(reqwest::redirect::Policy::limited(10)).build()?;
   sam_model_download::download_catalog(&client,&catalog,&staging,&token,progress.clone()).await?;
   // Network transfers do not block model list/selection/removal. Recheck the
   // shared destination under the publication lock after this job completes.
   let _guard=st.sam_model_ops.lock().await;
   validate_epoch(&st,&work).await?;
   anyhow::ensure!(!token.is_cancelled(),"download cancelled");
   if tokio::fs::symlink_metadata(&installed).await.is_ok() {
    let root=installed.clone();let check=catalog.clone();tokio::task::spawn_blocking(move||check.verify_directory(&root)).await??;
   } else {tokio::fs::rename(&staging,&installed).await?;}
   anyhow::ensure!(!token.is_cancelled(),"download cancelled");
   register(&st,&work,&catalog).await

  };
  let outcome=tokio::select!{_=token.cancelled()=>Err(anyhow::anyhow!("download cancelled")),result=tokio::time::timeout(st.job_timeout(),future)=>match result{Ok(result)=>result,Err(_)=>Err(anyhow::anyhow!("download timed out"))}};
  drop(progress);let _=drain.await;
  if token.is_cancelled(){JobService::mark_cancelled(&st,&id).await;}else{let event=match outcome{Ok(result)=>JobEvent::Succeeded{result},Err(error)=>{let superseded=error.downcast_ref::<Superseded>().is_some();let kind=if superseded {crate::jobs::ErrorKind::Validation}else if error.chain().any(|cause|cause.downcast_ref::<reqwest::Error>().is_some()){crate::jobs::ErrorKind::ProcessExit}else{crate::jobs::ErrorKind::Storage};JobEvent::Failed{kind,message:if superseded {"Модель удалена после запуска загрузки. Для установки запустите новое скачивание.".into()}else{"Не удалось скачать или проверить модель выделения объекта. Можно повторить загрузку.".into()}}}};JobService::transition(&st,&id,event).await;st.clear_cancel(&id).await;}
 });
}

async fn register(
    state: &AppState,
    work: &DownloadWork,
    catalog: &ModelCatalog,
) -> anyhow::Result<Value> {
    validate_epoch(state, work).await?;
    let owner = &work.owner;
    let mut tx = state.db.pool().begin().await?;
    sqlx::query("INSERT INTO sam_models(owner,id,name,bytes) VALUES(?,?,?,?) ON CONFLICT(owner,id) DO NOTHING").bind(owner).bind(&catalog.id).bind(&catalog.name).bind(catalog.bytes as i64).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sam_model_removals WHERE owner=? AND id=?")
        .bind(owner)
        .bind(&catalog.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(json!({"id":catalog.id,"name":catalog.name,"bytes":catalog.bytes}))
}

#[derive(Debug)]
struct Superseded;
impl std::fmt::Display for Superseded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SAM model download superseded by removal")
    }
}
impl std::error::Error for Superseded {}
async fn validate_epoch(state: &AppState, work: &DownloadWork) -> anyhow::Result<()> {
    let current: i64 =
        sqlx::query_scalar("SELECT epoch FROM sam_model_epochs WHERE owner=? AND id=?")
            .bind(&work.owner)
            .bind(&work.catalog_id)
            .fetch_optional(state.db.pool())
            .await?
            .unwrap_or(0);
    if current != work.epoch {
        return Err(Superseded.into());
    }
    Ok(())
}

#[cfg(test)]
mod epoch_contract_tests {
    use super::*;
    #[test]
    fn legacy_snapshots_default_to_initial_epoch() {
        let catalog = ModelCatalog::bundled().unwrap();
        let legacy = json!({"schema_version":1,"owner":"owner","catalog_id":catalog.id});
        let work: DownloadWork = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(work.epoch, 0);
        work.validate().unwrap();
        let mut invalid = legacy;
        invalid["epoch"] = json!(-1);
        assert!(serde_json::from_value::<DownloadWork>(invalid)
            .unwrap()
            .validate()
            .is_err());
    }
}
