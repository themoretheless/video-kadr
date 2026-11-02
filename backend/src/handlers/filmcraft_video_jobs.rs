//! Durable owner-scoped Filmcraft audiovisual export jobs.
use super::{
    filmcraft_project::BackgroundPreview,
    filmcraft_video_export::{encode_video_inner, snapshot, Snapshot, VideoRequest},
    jobs::{dispatch_job, JobLeaseHeartbeat},
};
use crate::{
    config::resource_classes::ResourceClass,
    error::{ApiJson, AppError, AppResult},
    jobs::{AcquirePlan, EnqueueOutcome, JobKind, JobService},
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
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Work {
    pub schema_version: u32,
    request: VideoRequest,
    owner: String,
    output_id: String,
    snapshot: Snapshot,
}
impl Work {
    pub(super) fn upgraded(mut self) -> anyhow::Result<Self> {
        anyhow::ensure!(matches!(self.schema_version,1|2),"unsupported Filmcraft video work");
        if self.schema_version==1 {
            // Version 1 used sequence-rate Auto when these audio fields were absent.
            if self.request.preset.audio_only() {
                let mut audio=self.request.audio.unwrap_or_default();
                if audio.sample_rate.is_none(){audio.sample_rate=Some(0);}
                self.request.audio=Some(audio);
            } else if self.request.h264_movie() && self.request.h264_audio.is_none() {
                self.request.h264_audio=Some(Default::default());
            }
            self.schema_version=2;
        }
        Ok(self)
    }
}
#[derive(Clone)]
pub(crate) struct PreparedExport {
    pub(super) request: VideoRequest,
    pub(super) snapshot: Snapshot,
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(super) struct EnqueuedExport {
    pub job_id:String,
    pub mode:&'static str,
    pub container:&'static str,
}
pub async fn submit(
    State(state): State<AppState>, headers: HeaderMap, ApiJson(body): ApiJson<Value>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner=super::publish::authenticated_actor(&state,&headers).await?;
    let prepared=prepare(&state,&headers,&owner,body).await?;
    let accepted=enqueue_prepared(&state,&owner,prepared).await?;
    Ok((StatusCode::ACCEPTED,Json(json!(accepted))))
}
pub(super) async fn prepare(state:&AppState, headers:&HeaderMap, owner:&str, mut body:Value)->AppResult<PreparedExport> {
    if let Some(name)=body.as_object_mut().and_then(|object|object.remove("libraryPreset")) {
        let name=name.as_str().ok_or_else(||AppError::bad_request("libraryPreset must be a name"))?;
        let library=state.db.filmcraft_export_presets(&owner).await.map_err(|error|AppError::internal("read export presets",error))?;
        let preset=library.find(name).ok_or_else(||AppError::bad_request("unknown export preset"))?;
        let mut resolved=crate::domain::filmcraft_export_preset_resolve::request_patch(&preset.settings).map_err(|error|AppError::bad_request(error.to_string()))?;
        let input=body.as_object().ok_or_else(||AppError::bad_request("export request must be an object"))?;
        for (key,value) in input {resolved[key]=value.clone();}
        body=resolved;
    }
    let request:VideoRequest=serde_json::from_value(body).map_err(|error|AppError::bad_request(error.to_string()))?;
    request
        .spec()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if matches!(request.preset,super::filmcraft_video_export::ExportPreset::Hevc) && !crate::tools::native_hevc_capability::available(&state.cpu_pool).await {
        return Err(AppError::bad_request("native HEVC hardware encoder is unavailable"));
    }
    if !request.native_output() && (!state.tools.ffmpeg
        || !state
            .tools
            .ffmpeg_encoders
            .iter()
            .any(|v| v == request.preset.encoder())
        || !state
            .tools
            .ffmpeg_muxers
            .iter()
            .any(|v| v == request.preset.muxer()))
    {
        return Err(AppError::service_unavailable(
            "Выбранный кодек или контейнер экспорта недоступен",
        ));
    }
    if request.with_audio && !request.native_output()
        && !state
            .tools
            .ffmpeg_encoders
            .iter()
            .any(|v| v == request.audio_codec())
    {
        return Err(AppError::service_unavailable(
            "Аудиокодек выбранного формата недоступен",
        ));
    }
    for source in request.sources.values() {
        super::library::authorize_source_access(&state, source, &headers).await?;
    }
    let snapshot = snapshot(&state, &headers, &request, Some(&owner))
        .await
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(PreparedExport { request, snapshot })
}
pub(super) async fn enqueue_prepared(state:&AppState, owner:&str, prepared:PreparedExport)->AppResult<EnqueuedExport> {
    let PreparedExport { request, snapshot: captured }=prepared;
    let snapshot=snapshot(state,&HeaderMap::new(),&request,Some(owner)).await.map_err(|error|AppError::bad_request(error.to_string()))?;
    if snapshot!=captured {return Err(AppError::bad_request("Исходники изменились после подготовки экспорта"))}
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO filmcraft_video_job_access(job_id,owner,sources_json) VALUES(?,?,?)")
        .bind(&id)
        .bind(owner)
        .bind(
            serde_json::to_string(&snapshot.sources)
                .map_err(|e| AppError::internal("serialize video access", e))?,
        )
        .execute(state.db.pool())
        .await
        .map_err(|e| AppError::internal("save video access", e))?;
    let container = request.extension();
    let mode = if request.preset.audio_only(){"audio_only"}else if request.with_audio {
        "video_audio"
    } else {
        "video_only"
    };
    let work = Work {
        schema_version: 2,
        request,
        owner: owner.to_owned(),
        output_id: uuid::Uuid::new_v4().to_string(),
        snapshot,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize video export", e))?;
    let outcome = state
        .enqueue_job(
            id.clone(),
            JobKind::FilmcraftVideo,
            &payload,
            &format!("filmcraft-video:{id}"),
        )
        .await;
    if !matches!(
        outcome,
        Ok(EnqueueOutcome::Created(_)) | Ok(EnqueueOutcome::Existing(_))
    ) {
        let _ = sqlx::query("DELETE FROM filmcraft_video_job_access WHERE job_id=?")
            .bind(&id)
            .execute(state.db.pool())
            .await;
    }
    match outcome.map_err(|e| AppError::internal("enqueue video export", e))? {
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests("Слишком много задач экспорта"))
        }
        _ => {}
    }
    dispatch_job(&state, &id).await;
    Ok(EnqueuedExport { job_id:id, mode, container })
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
        let time:Option<i64>=sqlx::query_scalar("SELECT j.created_at FROM jobs j JOIN filmcraft_video_job_access a ON a.job_id=j.id WHERE j.id=? AND a.owner=?").bind(&cursor).bind(&owner).fetch_optional(state.db.pool()).await.map_err(|e|AppError::internal("load video history cursor",e))?;
        Some((
            time.ok_or_else(|| AppError::bad_request("Некорректная страница экспорта"))?,
            cursor,
        ))
    } else {
        None
    };
    let rows:Vec<(String,String,i64)>=sqlx::query_as("SELECT j.id,a.sources_json,j.created_at FROM jobs j JOIN filmcraft_video_job_access a ON a.job_id=j.id WHERE a.owner=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?)) ORDER BY j.created_at DESC,j.id DESC LIMIT ?").bind(&owner).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.0)).bind(anchor.as_ref().map(|a|a.1.as_str())).bind(i64::from(limit)+1).fetch_all(state.db.pool()).await.map_err(|e|AppError::internal("list video history",e))?;
    let next_cursor = if rows.len() > limit as usize {
        rows.get(limit as usize - 1).map(|r| r.0.clone())
    } else {
        None
    };
    let mut jobs = Vec::new();
    for (id, sources, created_at) in rows.into_iter().take(limit as usize) {
        let sources: Vec<String> = serde_json::from_str(&sources)
            .map_err(|e| AppError::internal("decode video history sources", e))?;
        let mut allowed = true;
        for source in sources {
            if let Some(space) = state
                .db
                .source_space_id(&source)
                .await
                .map_err(|e| AppError::internal("resolve video history scope", e))?
            {
                if !state
                    .db
                    .is_space_member(&space, &owner)
                    .await
                    .map_err(|e| AppError::internal("authorize video history", e))?
                {
                    allowed = false;
                    break;
                }
            }
            if !state
                .library
                .get(&source)
                .await
                .is_some_and(|entry| entry.kind == "source")
            {
                allowed = false;
                break;
            }
        }
        if allowed {
            if let Some(job) = state
                .db
                .load_job(&id)
                .await
                .map_err(|e| AppError::internal("load video history", e))?
            {
                jobs.push(json!({"jobId":id,"createdAt":created_at,"job":job}));
            }
        }
    }
    Ok(Json(json!({"jobs":jobs,"nextCursor":next_cursor})))
}
pub(super) async fn authorize_job(
    state: &AppState,
    id: &str,
    headers: &HeaderMap,
    require_sources: bool,
) -> AppResult<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT owner,sources_json FROM filmcraft_video_job_access WHERE job_id=?")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| AppError::internal("load video access", e))?;
    if let Some((owner, sources)) = row {
        if super::publish::authenticated_actor(state, headers).await? != owner {
            return Err(AppError::forbidden("Это задание другого пользователя"));
        }
        if require_sources {
            let sources: Vec<String> = serde_json::from_str(&sources)
                .map_err(|e| AppError::internal("decode video access", e))?;
            for source in sources {
                super::stabilization_jobs::authorize_source_owner(state, &source, &owner).await?;
            }
        }
    }
    Ok(())
}
async fn cleanup_staging(path: &std::path::Path) {
    if let Ok(meta) = tokio::fs::symlink_metadata(path).await {
        if meta.is_dir() && !meta.file_type().is_symlink() {
            if let Err(error) = tokio::fs::remove_dir_all(path).await {
                tracing::warn!(%error,"video staging cleanup deferred to recovery");
            }
        }
    }
}
pub(super) fn spawn(
    state: AppState,
    id: String,
    work: Work,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let mut st = state.clone();
    let(pool,scope)=state.cpu_pool.scoped();st.cpu_pool=pool;
    let drain=scope.clone();
    let completion=WorkerCompletion::new(&state,id.clone(),scope);
    state.spawn_task(async move {
        let _completion=completion;
        let _lease=lease;
        // Only server-generated UUIDs can own a destination in outputs/.
        if uuid::Uuid::parse_str(&work.output_id).is_err(){JobService::mark_cancelled(&st,&id).await;return}
        let filename=format!("{}.{}",work.output_id,work.request.extension());let output=st.outputs_dir().join(&filename);
        let workdir=st.storage.join("cache/filmcraft-exports").join(&work.output_id);
        if !JobService::mark_queued(&st,&id).await {
            if !st.is_shutting_down()&&st.get_job(&id).await.ok().flatten().is_some_and(|job|job.status==crate::model::JobStatus::Cancelled){cleanup_staging(&workdir).await;let _=tokio::fs::remove_file(&output).await;let _=st.db.revoke_output_access(&work.output_id).await;}
            st.clear_cancel(&id).await;return
        }

        let Some(guards)=JobService::acquire_execution(&st,&id,&token,None,AcquirePlan{resource_class:ResourceClass::Export,acquire_render_permit:false}).await else{
            if !st.is_shutting_down()&&token.is_cancelled(){cleanup_staging(&workdir).await;let _=tokio::fs::remove_file(&output).await;let _=st.db.revoke_output_access(&work.output_id).await;}return
        };
        let cleanup_workdir=workdir.clone();
        let cleanup_id=work.output_id.clone();let path=output.clone();let render_state=st.clone();
        let success=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let completed=success.clone();
        let worker_id=id.clone();
        let updated=JobService::run_acquired(&st,&id,&token,attempt,"filmcraft_video","output",guards,|ctx|async move {
            let outcome=tokio::time::timeout(render_state.job_timeout(),async {
            let work=work.upgraded()?;
            let (spec,start)=work.request.spec()?;
            let current=snapshot(&render_state,&HeaderMap::new(),&work.request,Some(&work.owner)).await?;
            anyhow::ensure!(current==work.snapshot,"invalid video source snapshot changed while queued");
            // A recovered open job owns this UUID, including any interrupted final publication.
            render_state.db.revoke_output_access(&work.output_id).await?;
            if path.try_exists()?{tokio::fs::remove_file(&path).await?;}
            // The durable output UUID owns this entire staging namespace. A restarted
            // open job can remove its raw/audio/codec leftovers without scanning peers.
            if let Ok(meta)=tokio::fs::symlink_metadata(&workdir).await {
                anyhow::ensure!(meta.is_dir()&&!meta.file_type().is_symlink(),"invalid export staging directory");
                tokio::fs::remove_dir_all(&workdir).await?;
            }
            tokio::fs::create_dir_all(&workdir).await?;
            let rendered=workdir.join(format!("render.{}",work.request.extension()));
            use sha2::Digest;
            let background=BackgroundPreview{principal:sha2::Sha256::digest(work.owner.as_bytes()).into(),owner:work.owner.clone(),cancel:ctx.cancel.clone(),job_id:worker_id.clone()};
            let phase=super::nested_export::ProgressLane::new(&ctx.progress,0.,0.99);
            let loudness=Box::pin(encode_video_inner(&render_state,&HeaderMap::new(),&work.request,&rendered,render_state.job_timeout(),&ctx.cancel,&phase.sender,Some(background))).await?;
            anyhow::ensure!(!ctx.cancel.is_cancelled(),"video export cancelled");
            anyhow::ensure!(work.snapshot==snapshot(&render_state,&HeaderMap::new(),&work.request,Some(&work.owner)).await?,"video source changed before publication");
            let caption_sidecar=if let Some(kind)=work.request.caption_sidecar.clone() {
                let document=work.request.document()?;
                let index=crate::domain::filmcraft_project_index::ProjectIndex::new(&document)?;
                let offset=work.request.source_range.as_ref().map(|range|range.bounds().map(|range|range.0)).transpose()?.unwrap_or(start);
                let sequence=index.item(work.request.sequence_id.parse()?).and_then(|item|item.pointer("/kind/Sequence")).ok_or_else(||anyhow::anyhow!("missing caption sequence"))?.clone();
                render_state.cpu_pool.execute(ctx.cancel.clone(),move |_|crate::domain::filmcraft_export_caption_sidecar::make(&sequence,offset,&kind)).await.map_err(|error|anyhow::anyhow!("caption sidecar: {error:?}"))?
            }else{None};
            anyhow::ensure!(!ctx.cancel.is_cancelled(),"video export cancelled");
            if work.request.caption_sidecar.is_some() {
                anyhow::ensure!(work.snapshot==snapshot(&render_state,&HeaderMap::new(),&work.request,Some(&work.owner)).await?,"video source changed during caption generation");
            }
            tokio::fs::hard_link(&rendered,&path).await?;
            let size=tokio::fs::metadata(&path).await?.len();render_state.db.grant_output_access(&work.output_id,&work.owner).await?;
            // Publish Done only after owned working files have been removed.
            cleanup_staging(&workdir).await;
            completed.store(true,std::sync::atomic::Ordering::Relaxed);
            Ok(Some(json!({"id":work.output_id,"filename":filename,"url":format!("/files/outputs/{filename}"),"mediaType":if work.request.preset.audio_only(){"audio"}else if work.request.preset.image_sequence(){"image_sequence"}else{"video"},"container":work.request.extension(),"preset":work.request.preset,"videoCodec":if work.request.preset.audio_only(){None}else{Some(work.request.preset.video_codec())},"audioCodec":if work.request.has_audio(){Some(work.request.audio_codec())}else{None},"withAudio":work.request.has_audio(),"mode":if work.request.preset.audio_only(){"audio_only"}else if work.request.with_audio{"video_audio"}else{"video_only"},"width":if work.request.preset.audio_only(){None}else{Some(spec.width)},"height":if work.request.preset.audio_only(){None}else{Some(spec.height)},"fpsRate":if work.request.preset.audio_only(){None}else{Some(spec.frame_rate)},"sampleRate":if work.request.has_audio(){Some(work.request.audio_sample_rate()?)}else{None},"channels":if work.request.has_audio(){Some(work.request.audio_channels()?)}else{None},"bitsPerSample":if work.request.preset.audio_only(){Some(work.request.audio_bits())}else{None},"duration":if work.request.preset.audio_only(){work.request.audio_range(spec,start)?.1 as f64/f64::from(work.request.audio_sample_rate()?)}else if matches!(work.request.preset,super::filmcraft_video_export::ExportPreset::Gif){spec.frames as f64*f64::from(crate::domain::filmcraft_export_clock::gif_delay_ms(spec.frame_rate)?)/1000.}else{spec.frames as f64*spec.frame_rate.denominator as f64/spec.frame_rate.numerator as f64},"sizeBytes":size,"loudness":loudness,"captionSidecar":caption_sidecar})))
            }).await.unwrap_or_else(|_|Err(anyhow::anyhow!("video export timeout")));
            // execute() may report cancellation before its Rayon closure exits.
            // Keep the export permit and staging namespace until physical writes
            // finish, before cleanup or retry admission can reuse either.
            drain.wait_idle().await;
            if ctx.cancel.is_cancelled(){Ok(None)}else{outcome}
        }).await;
        cleanup_staging(&cleanup_workdir).await;
        if !updated||!success.load(std::sync::atomic::Ordering::Relaxed){let _=tokio::fs::remove_file(&output).await;let _=st.db.revoke_output_access(&cleanup_id).await;}
    });
}

struct WorkerCompletion {
    workers:std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String,usize>>>,
    id:String,scope:crate::runtime::cpu_pool::CpuScope,
}
impl WorkerCompletion {
    fn new(state:&AppState,id:String,scope:crate::runtime::cpu_pool::CpuScope)->Self{
        let workers=state.filmcraft_video_workers.clone();*workers.lock().unwrap_or_else(|error|error.into_inner()).entry(id.clone()).or_default()+=1;Self{workers,id,scope}
    }
}
impl Drop for WorkerCompletion {
    fn drop(&mut self){
        if self.scope.is_idle(){release_worker(&self.workers,&self.id);}
        else if let Ok(runtime)=tokio::runtime::Handle::try_current(){let workers=self.workers.clone();let id=self.id.clone();let scope=self.scope.clone();runtime.spawn(async move{scope.wait_idle().await;release_worker(&workers,&id);});}
    }
}
pub(super) fn worker_active(state:&AppState,id:&str)->bool{state.filmcraft_video_workers.lock().unwrap_or_else(|error|error.into_inner()).contains_key(id)}

fn release_worker(workers:&std::sync::Mutex<std::collections::HashMap<String,usize>>,id:&str){let mut workers=workers.lock().unwrap_or_else(|error|error.into_inner());if let Some(count)=workers.get_mut(id){*count-=1;if *count==0{workers.remove(id);}}}
