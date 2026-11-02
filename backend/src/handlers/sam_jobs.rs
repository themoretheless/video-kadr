//! Durable source-authorized learned regions. User requests contain IDs and points only.
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobEvent, JobKind, JobService},
    services::{local_enhance_video, local_sam, local_sam_video},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    source_id: String,
    points: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seed_frame: Option<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Work {
    schema_version: u32,
    owner: String,
    request: Request,
    source_hash: String,
    engine_hash: String,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tools_hash: Option<String>,
}
impl Work {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema_version == 1
                && !self.owner.is_empty()
                && [&self.source_hash, &self.engine_hash]
                    .iter()
                    .all(|h| h.len() == 64
                        && h.bytes()
                            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))),
            "invalid SAM snapshot"
        );
        anyhow::ensure!(
            self.request.seed_frame.is_some() == self.tools_hash.is_some(),
            "invalid SAM video tools snapshot"
        );
        if let Some(seed) = self.request.seed_frame {
            anyhow::ensure!(seed < 360000, "SAM seed ordinal outside bound");
            anyhow::ensure!(
                self.tools_hash.as_ref().is_some_and(|h| h.len() == 64
                    && h.bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))),
                "invalid SAM codec identity"
            );
        }
        crate::domain::composition::SourceId::parse(self.request.source_id.clone())?;
        local_sam::validate_points(&self.request.points)
    }
}
pub(super) async fn access(state: &AppState, id: &str) -> AppResult<Option<(String, String)>> {
    sqlx::query_as("SELECT owner,source_id FROM sam_job_access WHERE job_id=?")
        .bind(id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| AppError::internal("read SAM owner", e))
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    require_source: bool,
) -> AppResult<()> {
    if let Some((owner, source)) = access(state, id).await? {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        if actor != owner {
            return Err(AppError::forbidden("Это выделение другого пользователя"));
        }
        if require_source {
            super::library::authorize_source_access(state, &source, headers).await?;
        }
    }
    Ok(())
}
pub(super) fn output(state: &AppState, id: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)?.to_string() == id,
        "invalid SAM job ID"
    );
    Ok(state.storage.join("sam-jobs").join(format!("{id}.png")))
}
fn target(state: &AppState, id: &str, video: bool) -> anyhow::Result<PathBuf> {
    let path = output(state, id)?;
    Ok(if video {
        path.with_extension("mkv")
    } else {
        path
    })
}
fn source_limit(video: bool) -> u64 {
    if video {
        64 * 1024 * 1024 * 1024
    } else {
        256 * 1024 * 1024
    }
}
async fn inputs(state: &AppState, work: &Work) -> anyhow::Result<(PathBuf, local_sam::Config)> {
    let path = super::cutout_jobs::source(state, &work.request.source_id, &work.owner).await?;
    anyhow::ensure!(
        crate::project_archive::fingerprint_file_with_limit(
            &path,
            source_limit(work.request.seed_frame.is_some())
        )
        .await?
        .1 == work.source_hash,
        "SAM source changed"
    );
    let config = super::sam_models::config_for_model(state, &work.owner, work.model_id.as_deref())
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    anyhow::ensure!(
        local_sam::identity(&config).await?.hash == work.engine_hash,
        "SAM engine changed"
    );
    if let Some(expected) = &work.tools_hash {
        anyhow::ensure!(
            local_enhance_video::resolve_tools().await?.identity == *expected,
            "SAM codecs changed"
        );
    }
    Ok((path, config))
}
pub async fn status(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let available = match super::sam_models::config_for_owner(&state, &owner).await {
        Ok((config, _)) => local_sam::identity(&config).await.is_ok(),
        Err(_) => false,
    };
    Ok(Json(
        json!({"available":available,"engine":"slimsam-77-uniform","stillImages":true,"video":available && local_enhance_video::resolve_tools().await.is_ok()}),
    ))
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<(StatusCode, Json<Value>)> {
    submit_named(state,headers,request,None,false,None).await
}
/// A durable caller-owned reservation supplies the ID; public SAM requests cannot.
pub(super) async fn submit_named(state:AppState,headers:HeaderMap,request:Request,named_id:Option<String>,defer_dispatch:bool,expected_source_hash:Option<String>)->AppResult<(StatusCode,Json<Value>)>{
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    local_sam::validate_points(&request.points)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    crate::domain::composition::SourceId::parse(request.source_id.clone())
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    let _operation = state.sam_model_ops.lock().await;
    let named=named_id.is_some();let id=named_id.unwrap_or_else(||uuid::Uuid::new_v4().to_string());
    if named&&reuse_named(&state,&id,&owner,&request).await?{if !defer_dispatch{dispatch_job(&state,&id).await;}return Ok((StatusCode::ACCEPTED,Json(json!({"jobId":id}))))}
    let (config, model_id) = super::sam_models::config_for_owner(&state, &owner).await?;
    let engine = local_sam::identity(&config)
        .await
        .map_err(|_| AppError::service_unavailable("Модель выделения недоступна"))?;
    let path = super::cutout_jobs::source(&state, &request.source_id, &owner)
        .await
        .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let tools_hash = if let Some(seed) = request.seed_frame {
        if seed >= 360000 {
            return Err(AppError::bad_request("Кадр вне допустимого диапазона"));
        }
        let entry = state.library.get(&request.source_id).await;
        if !entry.is_some_and(|entry| entry.media_type.as_deref() == Some("video")) {
            return Err(AppError::bad_request("Нужен исходный видеоклип"));
        }
        Some(
            local_enhance_video::resolve_tools()
                .await
                .map_err(|_| AppError::service_unavailable("Видеокодеки недоступны"))?
                .identity,
        )
    } else {
        None
    };
    if request.seed_frame.is_none() {
        crate::services::local_enhance::dimensions(path.clone(), true)
            .await
            .map_err(|_| AppError::bad_request("Нужно поддерживаемое неподвижное изображение"))?;
    }
    let source_hash = crate::project_archive::fingerprint_file_with_limit(
        &path,
        source_limit(request.seed_frame.is_some()),
    )
    .await
    .map_err(|_| AppError::bad_request("Исходник недоступен или слишком велик"))?
    .1;
    if expected_source_hash.as_ref().is_some_and(|expected|expected!=&source_hash){return Err(AppError::conflict("Исходник изменился до сохранения задачи SAM"))}
    super::library::authorize_source_access(&state, &request.source_id, &headers).await?;
    let settings = if request.seed_frame.is_some() {
        serde_json::to_string(&request)
    } else {
        serde_json::to_string(&request.points)
    }
    .map_err(|e| AppError::internal("serialize SAM prompts", e))?;
    sqlx::query("INSERT OR IGNORE INTO sam_job_access(job_id,owner,source_id,points_json) VALUES(?,?,?,?)")
        .bind(&id)
        .bind(&owner)
        .bind(&request.source_id)
        .bind(&settings)
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save SAM access", e))?;
    let recorded:Option<(String,String,String)>=sqlx::query_as("SELECT owner,source_id,points_json FROM sam_job_access WHERE job_id=?").bind(&id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read SAM reservation",e))?;
    if recorded!=Some((owner.clone(),request.source_id.clone(),settings)){return Err(AppError::conflict("Резерв задачи SAM принадлежит другому запросу"))}
    let work = Work {
        schema_version: 1,
        owner:owner.clone(),
        request:request.clone(),
        source_hash,
        engine_hash: engine.hash,
        model_id,
        tools_hash,
    };
    work.validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::SamRegion,
            &serde_json::to_value(work).map_err(|e| AppError::internal("serialize SAM", e))?,
            &format!("sam-region:{id}"),
        )
        .await;
    let outcome=match outcome{Ok(value)=>value,Err(error)=>{if named&&reuse_named(&state,&id,&owner,&request).await?{if !defer_dispatch{dispatch_job(&state,&id).await;}return Ok((StatusCode::ACCEPTED,Json(json!({"jobId":id}))))}return Err(AppError::internal("enqueue SAM",error))}};
    if matches!(&outcome,EnqueueOutcome::Existing(existing) if existing!=&id){return Err(AppError::conflict("Изменилась привязка задачи SAM"))}
    if matches!(outcome, EnqueueOutcome::RateLimited) {
        return Err(AppError::too_many_requests("Слишком много заданий"));
    }
    if !defer_dispatch{dispatch_job(&state, &id).await;}
    Ok((StatusCode::ACCEPTED, Json(json!({"jobId":id}))))
}
async fn reuse_named(state:&AppState,id:&str,owner:&str,request:&Request)->AppResult<bool>{
 if state.get_job(id).await.map_err(|e|AppError::internal("read reserved SAM",e))?.is_none(){return Ok(false)}
 let row:Option<(String,String)>=sqlx::query_as("SELECT kind,payload_json FROM job_requests WHERE job_id=?").bind(id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read reserved SAM payload",e))?;
 let (kind,payload)=row.ok_or_else(||AppError::conflict("Нет сохранённого запроса SAM"))?;
 let work:Work=serde_json::from_str(&payload).map_err(|_|AppError::conflict("Неверный сохранённый запрос SAM"))?;
 work.validate().map_err(|_|AppError::conflict("Повреждён запрос SAM"))?;
 if kind!="sam_region"||work.owner!=owner||work.request!=*request||access(state,id).await?!=Some((owner.to_owned(),request.source_id.clone())){return Err(AppError::conflict("Идентификатор задачи относится к другому запросу"))}
 let path=super::cutout_jobs::source(state,&request.source_id,owner).await.map_err(|_|AppError::forbidden("Исходник недоступен"))?;
 let fingerprint=crate::project_archive::fingerprint_file_with_limit(&path,source_limit(request.seed_frame.is_some())).await.map_err(|_|AppError::conflict("Исходник недоступен"))?;
 if fingerprint.1!=work.source_hash{return Err(AppError::conflict("Исходник задачи SAM изменился"))}
 Ok(true)
}
pub async fn points(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let (_, source) = access(&state, &id)
        .await?
        .ok_or_else(|| AppError::not_found("Выделение не найдено"))?;
    authorize_job(&state, &id, &headers, true).await?;
    let settings: String =
        sqlx::query_scalar("SELECT points_json FROM sam_job_access WHERE job_id=?")
            .bind(&id)
            .fetch_one(state.db.pool())
            .await
            .map_err(|e| AppError::internal("read SAM prompts", e))?;
    let saved: Value =
        serde_json::from_str(&settings).map_err(|e| AppError::internal("decode SAM prompts", e))?;
    let recipe = if saved.is_array() {
        json!({"sourceId":source,"points":saved})
    } else {
        let request: Request = serde_json::from_value(saved)
            .map_err(|e| AppError::internal("decode SAM video prompts", e))?;
        if request.source_id != source {
            return Err(AppError::conflict("Исходник рецепта изменился"));
        }
        serde_json::to_value(request).map_err(|e| AppError::internal("encode SAM prompts", e))?
    };
    authorize_job(&state, &id, &headers, true).await?;
    Ok(Json(recipe))
}
pub(super) async fn result_metadata(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(local_sam::Metadata, String)> {
    let (_, source) = access(state, id)
        .await?
        .ok_or_else(|| AppError::not_found("Выделение не найдено"))?;
    authorize_job(state, id, headers, true).await?;
    let job = state
        .get_job(&id)
        .await
        .map_err(|e| AppError::internal("read SAM job", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Выделение ещё не готово"));
    }
    let metadata: local_sam::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Неверный результат"))?;
    let source_path = super::cutout_jobs::source(
        &state,
        &source,
        &super::publish::authenticated_actor(state, headers).await?,
    )
    .await
    .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    if crate::project_archive::fingerprint_file_with_limit(&source_path, 256 * 1024 * 1024)
        .await
        .map_err(|_| AppError::conflict("Исходник недоступен"))?
        .1
        != metadata.source_hash
    {
        return Err(AppError::conflict("Исходник изменился"));
    }
    Ok((metadata, source))
}
pub async fn mask(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let (metadata, source) = result_metadata(&state, &id, &headers).await?;
    let source_path = super::cutout_jobs::source(
        &state,
        &source,
        &super::publish::authenticated_actor(&state, &headers).await?,
    )
    .await
    .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let path = output(&state, &id).map_err(|_| AppError::not_found("Выделение не найдено"))?;
    local_sam::verify_png(path.clone(), metadata.clone())
        .await
        .map_err(|_| AppError::conflict("Маска изменилась или недоступна"))?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::internal("read SAM mask", e))?;
    use sha2::Digest;
    if bytes.len() > 1024 * 1024
        || format!("{:x}", sha2::Sha256::digest(&bytes)) != metadata.output_hash
    {
        return Err(AppError::conflict("Маска изменилась"));
    }
    if crate::project_archive::fingerprint_file_with_limit(&source_path, 256 * 1024 * 1024)
        .await
        .map_err(|_| AppError::conflict("Исходник недоступен"))?
        .1
        != metadata.source_hash
    {
        return Err(AppError::conflict("Исходник изменился"));
    }
    authorize_job(&state, &id, &headers, true).await?;
    Response::builder()
        .header("content-type", "image/png")
        .header("cache-control", "no-store")
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal("SAM mask response", e))
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: Work,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        match super::concat_video_operation::is_cancelled(&st,&id).await{
            Ok(true)=>{let _=st.cancel_open_job(&id).await;st.clear_cancel(&id).await;return;},
            Err(_)=>{let _=st.transition_job(&id,JobEvent::Failed{kind:crate::jobs::ErrorKind::Internal,message:"SAM operation unavailable".into()}).await;st.clear_cancel(&id).await;return;},
            Ok(false)=>{}
        }
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
        if !JobService::mark_running(&st, &id, "sam_region", attempt).await {
            st.clear_cancel(&id).await;
            return;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let drain = JobService::spawn_progress_drain(st.clone(), id.clone(), rx);
        let outcome: anyhow::Result<Value> = async {
            anyhow::ensure!(
                access(&st, &id)
                    .await
                    .map_err(|_| anyhow::anyhow!("SAM ownership unavailable"))?
                    == Some((work.owner.clone(), work.request.source_id.clone())),
                "SAM job ownership changed"
            );
            let (path, config) = inputs(&st, &work).await?;
            let target = target(&st, &id, work.request.seed_frame.is_some())?;
            crate::services::tts_model_download::directory(
                &st.storage,
                std::path::Path::new("sam-jobs"),
            )
            .await?;
            if let Ok(meta) = tokio::fs::symlink_metadata(&target).await {
                anyhow::ensure!(meta.file_type().is_file(), "unexpected SAM output");
                tokio::fs::remove_file(&target).await?;
            }
            let raw = target.with_extension("gray");
            if work.request.seed_frame.is_some() {
                if let Ok(meta) = tokio::fs::symlink_metadata(&raw).await {
                    anyhow::ensure!(meta.file_type().is_file(), "unexpected SAM raw output");
                    tokio::fs::remove_file(&raw).await?;
                }
            }
            let result = if let Some(seed_frame) = work.request.seed_frame {
                let codecs = local_enhance_video::resolve_tools().await?;
                anyhow::ensure!(
                    Some(&codecs.identity) == work.tools_hash.as_ref(),
                    "SAM codecs changed"
                );
                serde_json::to_value(
                    local_sam_video::track(
                        &st.process_runtime,
                        local_sam_video::Tools {
                            ffmpeg: &codecs.ffmpeg,
                            ffprobe: &codecs.ffprobe,
                        },
                        &config,
                        local_sam_video::Input {
                            source: &path,
                            source_hash: &work.source_hash,
                            seed_frame,
                            points: &work.request.points,
                            mask_destination: Some(&raw),
                            staging_bytes: 64 * 1024 * 1024 * 1024,
                        },
                        &target,
                        &token,
                        std::time::Duration::from_secs(1800),
                        &tx,
                    )
                    .await?,
                )?
            } else {
                let metadata = local_sam::region(
                    &st.process_runtime,
                    &config,
                    local_sam::Input {
                        source: &path,
                        source_hash: &work.source_hash,
                        points: &work.request.points,
                    },
                    &target,
                    &token,
                    std::time::Duration::from_secs(1800),
                    &tx,
                )
                .await?;
                serde_json::to_value(metadata)?
            };
            inputs(&st, &work).await?;
            anyhow::ensure!(
                access(&st, &id)
                    .await
                    .map_err(|_| anyhow::anyhow!("SAM ownership unavailable"))?
                    == Some((work.owner.clone(), work.request.source_id.clone())),
                "SAM job ownership changed"
            );
            Ok(result)
        }
        .await;
        drop(tx);
        let _ = drain.await;
        if token.is_cancelled() {
            if !st.is_shutting_down() {
                cleanup(&st, &id, work.request.seed_frame.is_some()).await;
            }
            JobService::mark_cancelled(&st, &id).await;
        } else {
            let event = match outcome {
                Ok(result) => JobEvent::Succeeded { result },
                Err(error) => {
                    cleanup(&st, &id, work.request.seed_frame.is_some()).await;
                    JobEvent::Failed {
                        kind: crate::jobs::classify_job_error(&error),
                        message: crate::privacy::redact_text(&error.to_string()),
                    }
                }
            };
            JobService::transition(&st, &id, event).await;
            st.clear_cancel(&id).await;
        }
    });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    cursor: Option<String>,
    limit: Option<u32>,
    source_id: Option<String>,
    #[serde(default)]
    video: bool,
}
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Размер страницы должен быть 1–50"));
    }
    let anchor = if let Some(cursor) = page.cursor {
        let created:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN sam_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=? AND COALESCE(json_type(a.points_json,'$.seedFrame')='integer',0)=? AND (? IS NULL OR a.source_id=?)").bind(&cursor).bind(&owner).bind(page.video).bind(&page.source_id).bind(&page.source_id).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("read SAM cursor",e))?;
        Some((
            created.ok_or_else(|| AppError::bad_request("Некорректная страница выделения"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.source_id,j.created_at FROM jobs j JOIN sam_job_access a ON a.job_id=j.id WHERE a.owner=? AND COALESCE(json_type(a.points_json,'$.seedFrame')='integer',0)=? AND (? IS NULL OR a.source_id=?) AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(page.video).bind(&page.source_id).bind(&page.source_id).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.0)).bind(anchor.as_ref().map(|x|x.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list SAMs",e))?;
    let next = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|row| row.0.clone())
    } else {
        None
    };
    let mut jobs = vec![];
    for (id, source, created_at) in rows.into_iter().take(limit as usize) {
        if let Some(space) = state
            .db
            .source_space_id(&source)
            .await
            .map_err(|e| AppError::internal("read SAM source space", e))?
        {
            if !state
                .db
                .is_space_member(&space, &owner)
                .await
                .map_err(|e| AppError::internal("authorize SAM history", e))?
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
            .map_err(|e| AppError::internal("read SAM history status", e))?
        {
            jobs.push(json!({"jobId":id,"sourceId":source,"createdAt":created_at,"status":job.status,"progress":job.progress}));
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next})))
}

async fn cleanup(state: &AppState, id: &str, video: bool) {
    if let Ok(path) = target(state, id, video) {
        let _ = tokio::fs::remove_file(&path).await;
        if video {
            let _ = tokio::fs::remove_file(path.with_extension("gray")).await;
        }
    }
}

pub(super) async fn checked_video(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
) -> AppResult<(local_sam_video::Metadata, PathBuf)> {
    let (owner, source) = access(state, id)
        .await?
        .ok_or_else(|| AppError::not_found("Выделение не найдено"))?;
    authorize_job(state, id, headers, true).await?;
    let job = state
        .get_job(id)
        .await
        .map_err(|e| AppError::internal("read SAM video", e))?
        .ok_or_else(|| AppError::not_found("Задание не найдено"))?;
    if job.status != crate::model::JobStatus::Done {
        return Err(AppError::conflict("Трекинг ещё не готов"));
    }
    let metadata: local_sam_video::Metadata = serde_json::from_value(
        job.result
            .ok_or_else(|| AppError::conflict("Нет результата"))?,
    )
    .map_err(|_| AppError::conflict("Нет результата видеотрекинга"))?;
    crate::domain::video_region::validate_frame_times(metadata.frame_times_us.as_deref(),metadata.frames).map_err(|_|AppError::conflict("Неверная временная шкала видеотрекинга"))?;
    if metadata.schema_version != 1
        || metadata.frames == 0
        || metadata.frames > 360000
        || metadata.seed_frame >= metadata.frames
        || metadata.region_video.width != 256
        || metadata.region_video.height != 256
        || metadata.region_video.frames != metadata.frames
        || metadata.region_video.frame_rate != metadata.frame_rate
        || metadata.region_video.mask_hash != metadata.tracking.output_hash
        || metadata.region_video.size_bytes == 0
        || metadata.region_video.size_bytes > source_limit(true)
    {
        return Err(AppError::conflict("Неверный результат видеотрекинга"));
    }
    let source_path = super::cutout_jobs::source(state, &source, &owner)
        .await
        .map_err(|_| AppError::forbidden("Исходник недоступен"))?;
    let path = target(state, id, true).map_err(|_| AppError::not_found("Неверное задание"))?;
    let info = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| AppError::conflict("Видео масок недоступно"))?;
    if !info.file_type().is_file() || info.len() != metadata.region_video.size_bytes {
        return Err(AppError::conflict("Видео масок повреждено"));
    }
    let (_, hash) = crate::project_archive::fingerprint_file_with_limit(
        &path,
        metadata.region_video.size_bytes,
    )
    .await
    .map_err(|_| AppError::conflict("Видео масок недоступно"))?;
    if hash != metadata.region_video.file_hash {
        return Err(AppError::conflict("Видео масок изменилось"));
    }
    let (_, hash) =
        crate::project_archive::fingerprint_file_with_limit(&source_path, source_limit(true))
            .await
            .map_err(|_| AppError::conflict("Исходник недоступен"))?;
    if hash != metadata.source_hash {
        return Err(AppError::conflict("Исходник изменился"));
    }
    authorize_job(state, id, headers, true).await?;
    Ok((metadata, path))
}
/// Verified private region transport, with the standard file Range implementation.
pub async fn video(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::http::Request<Body>,
) -> AppResult<Response> {
    use tower::ServiceExt;
    let (_, path) = checked_video(&state, &id, request.headers()).await?;
    let mut response = tower_http::services::ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| AppError::internal("serve SAM video", e))?
        .map(Body::new);
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        "content-type",
        axum::http::HeaderValue::from_static("video/x-matroska"),
    );
    Ok(response)
}
