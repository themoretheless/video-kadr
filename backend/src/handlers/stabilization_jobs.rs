//! Durable owner-scoped preparation of source-project preview analysis.
use super::{
    filmcraft_project::{BackgroundPreview, PreviewFrameRequest},
    jobs::{dispatch_job, JobLeaseHeartbeat},
};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    state::AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StabilizationWork {
    pub schema_version: u32,
    request: PreviewFrameRequest,
    owner: String,
    principal: [u8; 32],
    source_hashes: BTreeMap<String, String>,
    #[serde(default)]
    sequence_snapshots: BTreeMap<String, [u8; 32]>,
}
pub(super) async fn authorize_source_owner(
    state: &AppState,
    source: &str,
    owner: &str,
) -> AppResult<()> {
    if let Some(space) = state
        .db
        .source_space_id(source)
        .await
        .map_err(|e| AppError::internal("resolve analysis source", e))?
    {
        if !state
            .db
            .is_space_member(&space, owner)
            .await
            .map_err(|e| AppError::internal("authorize analysis source", e))?
        {
            return Err(AppError::forbidden("Нет доступа к исходнику анализа"));
        }
        state
            .library
            .relocate_source_to_space(source, &space)
            .await
            .map_err(|e| AppError::internal("isolate analysis source", e))?;
    }
    Ok(())
}
async fn source_hash(state: &AppState, source: &str, owner: &str) -> anyhow::Result<String> {
    authorize_source_owner(state, source, owner)
        .await
        .map_err(|e| anyhow::anyhow!("analysis source access denied: {e:?}"))?;
    let entry = state
        .library
        .get(source)
        .await
        .filter(|e| e.kind == "source")
        .ok_or_else(|| anyhow::anyhow!("analysis source missing"))?;
    let path = state.library.resolve_media_path(&entry).await?;
    Ok(crate::project_archive::fingerprint_file(&path).await?.1)
}
async fn sequence_snapshot(state:&AppState, source:&str, owner:&str) -> AppResult<([u8;32],Vec<String>)> {
    authorize_source_owner(state,source,owner).await?;
    let entry=state.library.get(source).await.ok_or_else(||AppError::bad_request("sequence source missing"))?;
    let headers=HeaderMap::new();
    let mut reader=super::filmcraft_image_sequence::catalog(state,&headers,Some(owner),&entry,CancellationToken::new()).await?;
    let (revision,_)=reader.snapshot().await?;
    Ok((revision,reader.member_ids().map(str::to_owned).collect()))
}
async fn verify_sequence_snapshots(state:&AppState, owner:&str, snapshots:&BTreeMap<String,[u8;32]>) -> anyhow::Result<()> {
    for (source,expected) in snapshots {
        let (current,_)=sequence_snapshot(state,source,owner).await.map_err(|e|anyhow::anyhow!("sequence snapshot unavailable: {e:?}"))?;
        anyhow::ensure!(current==*expected,"analysis sequence changed after submission");
    }
    Ok(())
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    require_sources: bool,
) -> AppResult<()> {
    let access: Option<(String, String)> =
        sqlx::query_as("SELECT owner,sources_json FROM stabilization_job_access WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load analysis job access", e))?;
    if let Some((owner, sources)) = access {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        if actor != owner {
            return Err(AppError::forbidden("Это задание другого пользователя"));
        }
        if require_sources {
            let sources: Vec<String> = serde_json::from_str(&sources)
                .map_err(|e| AppError::internal("decode analysis sources", e))?;
            for source in sources {
                authorize_source_owner(state, &source, &owner).await?;
            }
        }
    }
    Ok(())
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<PreviewFrameRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if request.sources.len() > 64 {
        return Err(AppError::bad_request("too many analysis sources"));
    }
    // Validate transport/index before admitting durable work. Rendering validation stays shared.
    let bytes = match &request.document {
        Value::String(s) => s.as_bytes().to_vec(),
        v => serde_json::to_vec(v).map_err(|e| AppError::bad_request(e.to_string()))?,
    };
    let document = crate::domain::filmcraft_project_file::SourceProjectDocument::read(&bytes)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let index = crate::domain::filmcraft_project_index::ProjectIndex::new(&document)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let sequence = request
        .sequence_id
        .parse::<u64>()
        .map_err(|_| AppError::bad_request("invalid analysis sequence"))?;
    if !index.sequences().any(|(id, _)| id == sequence) {
        return Err(AppError::bad_request("analysis sequence missing"));
    }
    super::filmcraft_project::validate_analysis_request(&request)?;
    let mut source_hashes = BTreeMap::new();
    for source in request.sources.values() {
        super::library::authorize_source_access(&state, source, &headers).await?;
        source_hashes.insert(
            source.clone(),
            source_hash(&state, source, &owner)
                .await
                .map_err(|e| AppError::bad_request(e.to_string()))?,
        );
    }
    let mut sequence_snapshots=BTreeMap::new();
    let mut sources:std::collections::BTreeSet<String>=source_hashes.keys().cloned().collect();
    for (item,source) in &request.sources {
        let item=item.parse::<u64>().map_err(|_|AppError::bad_request("invalid analysis source item"))?;
        let media=index.resolve_media(item).filter(|m|m.item_id==item).ok_or_else(||AppError::bad_request("analysis binding must identify root media"))?;
        if media.media["info"]["kind"]=="ImageSequence" {
            let (revision,members)=sequence_snapshot(&state,source,&owner).await?;
            sequence_snapshots.insert(source.clone(),revision);
            sources.extend(members);
        }
    }
    let sources:Vec<_>=sources.into_iter().collect();
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO stabilization_job_access(job_id,owner,sources_json) VALUES(?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(
            serde_json::to_string(&sources)
                .map_err(|e| AppError::internal("serialize analysis access", e))?,
        )
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save analysis access", e))?;
    let work = StabilizationWork {
        schema_version: 1,
        request,
        owner,
        principal: super::filmcraft_project::preview_principal(&headers),
        source_hashes,
        sequence_snapshots,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize analysis job", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::Stabilization,
            &payload,
            &format!("stabilization:{id}"),
        )
        .await
        .map_err(|e| AppError::internal("enqueue analysis job", e))?;
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    dispatch_job(&state, &id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: StabilizationWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move{
 let _lease=lease;if !JobService::mark_queued(&st,&id).await{st.clear_cancel(&id).await;return}
 let Some(_guards)=JobService::acquire_execution(&st,&id,&token,None,AcquirePlan{resource_class:ResourceClass::Analysis,acquire_render_permit:false}).await else{return};
 if !JobService::mark_running(&st,&id,"stabilization",attempt).await{st.clear_cancel(&id).await;return}
 let work_future=async{
  for(source,hash)in &work.source_hashes{let current=source_hash(&st,source,&work.owner).await.map_err(|e|(crate::jobs::ErrorKind::Security,e.to_string()))?;if current!=*hash{return Err((crate::jobs::ErrorKind::Validation,"analysis source fingerprint changed".into()))}}
  verify_sequence_snapshots(&st,&work.owner,&work.sequence_snapshots).await.map_err(|e|(crate::jobs::ErrorKind::Validation,e.to_string()))?;
  let background=BackgroundPreview{owner:work.owner.clone(),principal:work.principal,cancel:token.clone(),job_id:id.clone()};
  let Json(frame)=super::filmcraft_project::execute_preview(st.clone(),HeaderMap::new(),work.request,Some(background)).await.map_err(|e|(crate::jobs::ErrorKind::Validation,format!("analysis failed: {e:?}")))?;
  for(source,hash)in &work.source_hashes{let current=source_hash(&st,source,&work.owner).await.map_err(|e|(crate::jobs::ErrorKind::Security,e.to_string()))?;if current!=*hash{return Err((crate::jobs::ErrorKind::Validation,"analysis source fingerprint changed".into()))}}
  verify_sequence_snapshots(&st,&work.owner,&work.sequence_snapshots).await.map_err(|e|(crate::jobs::ErrorKind::Validation,e.to_string()))?;
  Ok(json!({"ready":true,"sequenceId":frame["sequenceId"],"sampleTicks":frame["sampleTicks"],"width":frame["width"],"height":frame["height"],"analysisResumedFrames":frame["analysisResumedFrames"],"analysisReusedPaths":frame["analysisReusedPaths"]}))
 };
 let outcome:Result<Value,(crate::jobs::ErrorKind,String)>=tokio::select!{_ = token.cancelled()=>Err((crate::jobs::ErrorKind::Interrupted,"analysis cancelled".into())),result=tokio::time::timeout(st.job_timeout(),work_future)=>match result{Ok(result)=>result,Err(_)=>Err((crate::jobs::ErrorKind::Timeout,"analysis job deadline exceeded".into()))}};
 if !st.is_shutting_down()&&(outcome.is_ok()||token.is_cancelled()){if let Err(error)=sqlx::query("DELETE FROM stabilization_checkpoints WHERE job_id=?").bind(&id).execute(st.db.pool()).await{tracing::warn!(%error,"analysis checkpoint cleanup failed");}}
 if token.is_cancelled(){JobService::mark_cancelled(&st,&id).await;}else{let event=match outcome{Ok(result)=>JobEvent::Succeeded{result},Err((kind,message))=>JobEvent::Failed{kind,message:crate::privacy::redact_text(&message)}};JobService::transition(&st,&id,event).await;st.clear_cancel(&id).await;}
});
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
    let anchor = if let Some(cursor) = query.cursor {
        let time:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN stabilization_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=?").bind(&cursor).bind(&owner).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("load analysis history cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница заданий"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.sources_json,j.created_at FROM jobs j JOIN stabilization_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list analysis jobs",e))?;
    let next_cursor = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|r| r.0.clone())
    } else {
        None
    };
    let mut jobs = Vec::new();
    for (id, sources, created_at) in rows.into_iter().take(limit as usize) {
        let sources: Vec<String> = serde_json::from_str(&sources)
            .map_err(|e| AppError::internal("decode analysis history sources", e))?;
        let mut allowed = true;
        for source in sources {
            if let Some(space) = state
                .db
                .source_space_id(&source)
                .await
                .map_err(|e| AppError::internal("resolve analysis history source", e))?
            {
                if !state
                    .db
                    .is_space_member(&space, &owner)
                    .await
                    .map_err(|e| AppError::internal("authorize analysis history", e))?
                {
                    allowed = false;
                    break;
                }
            }
            if !state
                .library
                .get(&source)
                .await
                .is_some_and(|e| e.kind == "source")
            {
                allowed = false;
                break;
            }
        }
        if !allowed {
            continue;
        }
        if let Some(job) = state
            .get_job(&id)
            .await
            .map_err(|e| AppError::internal("load analysis history job", e))?
        {
            jobs.push(json!({"jobId":id,"status":job.status,"progress":job.progress,"createdAt":created_at}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next_cursor})))
}

pub(super) async fn save_checkpoint(state:&AppState,id:&str,key:[u8;32],analysis:&crate::domain::filmcraft_analysis_cache::SharedAnalysis,saved_ready:&mut std::collections::BTreeSet<crate::domain::filmcraft_analysis_cache::StableAnalysisKey>)->anyhow::Result<()>{
 let(snapshot,ready)={let cache=analysis.lock().map_err(|_|anyhow::anyhow!("analysis checkpoint lock poisoned"))?;(cache.checkpoint(),cache.ready_paths())};
 let keys:std::collections::BTreeSet<_>=ready.iter().filter(|(_,path)|path.is_some()).map(|(key,_)|*key).collect();
 if keys.difference(saved_ready).next().is_some(){state.filmcraft_video_previews.persist_completed_paths(key,ready).await?;saved_ready.extend(keys);}
 if snapshot.is_empty(){return Ok(())}let data=serde_json::to_vec(&snapshot)?;use sha2::Digest;let mut bytes=sha2::Sha256::digest(&data).to_vec();bytes.extend(data);anyhow::ensure!(bytes.len()<=32*1024*1024,"analysis checkpoint budget exceeded");
 sqlx::query("INSERT INTO stabilization_checkpoints(job_id,cache_key,state_json) VALUES(?,?,?) ON CONFLICT(job_id) DO UPDATE SET cache_key=excluded.cache_key,state_json=excluded.state_json").bind(id).bind(key.as_slice()).bind(bytes).execute(state.db.pool()).await?;Ok(())
}
pub(super) async fn restore_checkpoint(state:&AppState,id:&str,key:[u8;32],analysis:&crate::domain::filmcraft_analysis_cache::SharedAnalysis)->anyhow::Result<usize>{
 let bytes:Option<Vec<u8>>=sqlx::query_scalar("SELECT state_json FROM stabilization_checkpoints WHERE job_id=? AND cache_key=? AND length(state_json)<=33554432").bind(id).bind(key.as_slice()).fetch_optional(state.db.pool()).await?;
 if let Some(bytes)=bytes{use sha2::Digest;anyhow::ensure!(bytes.len()>=32&&sha2::Sha256::digest(&bytes[32..]).as_slice()==&bytes[..32],"analysis checkpoint checksum mismatch");let snapshot=serde_json::from_slice(&bytes[32..])?;analysis.lock().map_err(|_|anyhow::anyhow!("analysis checkpoint lock poisoned"))?.restore_checkpoint(snapshot)?;return Ok(analysis.lock().map_err(|_|anyhow::anyhow!("analysis checkpoint lock poisoned"))?.committed_frames());}Ok(0)
}
