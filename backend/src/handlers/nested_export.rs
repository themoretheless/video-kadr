//! Durable, authenticated native video/audio export jobs (no client filesystem paths).
use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use crate::{
    artifacts::fingerprint_file,
    config::resource_classes::ResourceClass,
    domain::{
        artifact_graph::Fingerprint, composition::SourceId,
        nested_composition::NestedCompositionProject,
    },
    error::{ApiJson, AppError, AppResult},
    jobs::{dedupe_key, AcquirePlan, EnqueueOutcome, JobKind, JobService},
    state::AppState,
    tools::{
        native_audio_export::{mix_project_audio_tracks, PcmSource},
        native_rgba_export::{encode_project_with_clip_filter, RasterAudio, RasterVideoSpec},
        
    },
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio_util::sync::CancellationToken;
const STAGING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeExportMode {
    VideoOnly,
    VideoAudio,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedExportRequest {
    #[serde(default)]
    saved_project: Option<super::concat_video_bound::SavedProject>,
    schema_version: u32,
    mode: NativeExportMode,
    project: NestedCompositionProject,
    composition_id: String,
    start_us: u64,
    end_us: u64,
    #[serde(default)]
    rendered_outputs: BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindingsIdentity {
    #[serde(default,skip_serializing_if="BTreeMap::is_empty")]
    masks: BTreeMap<SourceId,String>,
    sources: BTreeMap<SourceId, Fingerprint>,
    outputs: BTreeMap<String, Fingerprint>,
}
struct Bindings {
    masks: super::concat_video_bound::BoundMasks,
    sources: BTreeMap<SourceId, PathBuf>,
    outputs: BTreeMap<String, PathBuf>,
    identity: BindingsIdentity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NestedExportWork {
    pub schema_version: u32,
    request: NestedExportRequest,
    owner: String,
    output_id: String,
    identity: BindingsIdentity,
}
impl NestedExportRequest {
    fn validate(&self) -> anyhow::Result<RasterVideoSpec> {
        anyhow::ensure!(
            self.schema_version == 1,
            "invalid native export schema version"
        );
        let durations = self.project.durations_us()?;
        let duration = *durations
            .get(&self.composition_id)
            .ok_or_else(|| anyhow::anyhow!("invalid native export focus"))?;
        anyhow::ensure!(
            self.start_us < self.end_us && self.end_us <= duration,
            "invalid native export range"
        );
        let canvas = self
            .project
            .compositions
            .get(&self.composition_id)
            .ok_or_else(|| anyhow::anyhow!("invalid native export focus"))?
            .content
            .canvas;
        let rate = canvas.frame_rate();
        let frames = (u128::from(self.end_us - self.start_us) * u128::from(rate.numerator))
            .div_ceil(1_000_000 * u128::from(rate.denominator));
        anyhow::ensure!(frames <= 360000, "invalid native export frame count");
        let spec = RasterVideoSpec {
            width: canvas.width as usize,
            height: canvas.height as usize,
            frame_rate: rate,
            frames: frames as u64,
            staging_bytes: STAGING_BYTES,
        };
        spec.validate()?;
        if self.mode == NativeExportMode::VideoAudio {
            let layers = crate::domain::nested_audio::plan_audio_tracks(
                &self.project,
                &self.composition_id,
                self.start_us..self.end_us,
            )?;
            crate::domain::nested_dsp::validate_export_processing(&layers)?;
        }
        for (composition, output) in &self.rendered_outputs {
            anyhow::ensure!(
                self.project.compositions.contains_key(composition),
                "invalid rendered composition"
            );
            uuid::Uuid::parse_str(output)?;
        }
        Ok(spec)
    }
}
async fn bindings(
    state: &AppState,
    request: &NestedExportRequest,
    owner: &str,
    cancel: &CancellationToken,
) -> AppResult<Bindings> {
    let mut ids = request
        .project
        .range_image_sources(
            &request.composition_id,
            request.start_us..request.end_us,
            &request
                .rendered_outputs
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
        )
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if request.mode == NativeExportMode::VideoAudio {
        let layers = crate::domain::nested_audio::plan_audio_tracks(
            &request.project,
            &request.composition_id,
            request.start_us..request.end_us,
        )
        .map_err(|e| AppError::bad_request(e.to_string()))?;
        ids.extend(layers.iter().map(|layer| layer.source_id().clone()));
    }
    let masks=super::concat_video_bound::BoundMasks::load(state,owner,request.saved_project.as_ref(),&request.project,cancel).await.map_err(|e|AppError::bad_request(e.to_string()))?;
    let mask_identities=masks.identities().map_err(|e|AppError::bad_request(e.to_string()))?;
    let mut result = Bindings {
        masks,
        sources: BTreeMap::new(),
        outputs: BTreeMap::new(),
        identity: BindingsIdentity {
            masks: mask_identities,
            sources: BTreeMap::new(),
            outputs: BTreeMap::new(),
        },
    };
    for id in ids {
        let physical=request.project.library_source_id(&id);
        if let Some(space) = state
            .db
            .source_space_id(physical.as_str())
            .await
            .map_err(|e| AppError::internal("resolve export source space", e))?
        {
            if !state
                .db
                .is_space_member(&space, owner)
                .await
                .map_err(|e| AppError::internal("authorize native export", e))?
            {
                return Err(AppError::forbidden("Нет доступа к Space media"));
            }
            state
                .library
                .relocate_source_to_space(physical.as_str(), &space)
                .await
                .map_err(|e| AppError::internal("isolate export source", e))?;
        }
        let entry = state
            .library
            .get(physical.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::not_found("export source not found"))?;
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::not_found("export source not found"))?;
        let hash = fingerprint_file(&state.cpu_pool, path.clone(), cancel.child_token())
            .await
            .map_err(|e| AppError::internal("fingerprint export source", e))?;
        result.identity.sources.insert(id.clone(), hash.sha256);
        result.sources.insert(id, path);
    }
    for (composition, output) in &request.rendered_outputs {
        let id = uuid::Uuid::parse_str(output)
            .map_err(|_| AppError::bad_request("invalid export output ID"))?
            .to_string();
        if !state
            .db
            .can_access_output(&id, owner)
            .await
            .map_err(|e| AppError::internal("authorize export output", e))?
        {
            return Err(AppError::not_found("rendered output not found"));
        }
        let path = super::publish::resolve_output_path(state, &id)
            .await
            .map_err(|_| AppError::not_found("rendered output not found"))?;
        let hash = fingerprint_file(&state.cpu_pool, path.clone(), cancel.child_token())
            .await
            .map_err(|e| AppError::internal("fingerprint rendered output", e))?;
        result
            .identity
            .outputs
            .insert(composition.clone(), hash.sha256);
        result.outputs.insert(composition.clone(), path);
    }
    Ok(result)
}
pub async fn nested_export_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(mut request): ApiJson<NestedExportRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if request.project.compositions.values().any(|node|node.content.tracks.iter().any(|track|matches!(track,crate::domain::composition::CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.enabled&&clip.concat_cutout.is_some())))){request.rendered_outputs.clear();}

    request
        .validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if !state.tools.ffmpeg
        || !state.tools.ffmpeg_encoders.iter().any(|v| v == "ffv1")
        || !state.tools.ffmpeg_muxers.iter().any(|v| v == "matroska")
    {
        return Err(AppError::service_unavailable(
            "Нативный экспорт требует ffmpeg с FFV1/Matroska",
        ));
    }
    if request.mode == NativeExportMode::VideoAudio
        && !state
            .tools
            .ffmpeg_encoders
            .iter()
            .any(|value| value == "pcm_f32le")
    {
        return Err(AppError::service_unavailable(
            "Нативный аудиоэкспорт требует PCM float32",
        ));
    }
    let mode = request.mode;
    let bound = bindings(&state, &request, &owner, &CancellationToken::new()).await?;
    let key = dedupe_key(
        "nested-export-v7-bound-video",
        &json!({"owner":owner,"request":request,"identity":bound.identity}),
    )
    .map_err(|e| AppError::internal("build native export identity", e))?;
    let work = NestedExportWork {
        schema_version: 1,
        request,
        owner,
        output_id: uuid::Uuid::new_v4().to_string(),
        identity: bound.identity,
    };
    let payload =
        serde_json::to_value(work).map_err(|e| AppError::internal("serialize native export", e))?;
    let id = uuid::Uuid::new_v4().to_string();
    let id = match state
        .enqueue_job(id.clone(), JobKind::NestedExport, &payload, &key)
        .await
        .map_err(|e| AppError::internal("enqueue native export", e))?
    {
        EnqueueOutcome::Created(_) => id,
        EnqueueOutcome::Existing(id) => id,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests("Слишком много задач экспорта"))
        }
    };
    dispatch_job(&state, &id).await;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"jobId":id,"mode":mode,"container":"mkv"})),
    ))
}
pub(super) struct ProgressLane {
    pub(super) sender: tokio::sync::mpsc::UnboundedSender<f64>,
    task: tokio::task::JoinHandle<()>,
}
impl ProgressLane {
    pub(super) fn new(parent: &tokio::sync::mpsc::UnboundedSender<f64>, base: f64, scale: f64) -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<f64>();
        let parent = parent.clone();
        let task = tokio::spawn(async move {
            while let Some(value) = receiver.recv().await {
                let _ = parent.send(base + scale * value.clamp(0.0, 100.0));
            }
        });
        Self { sender, task }
    }
}
impl Drop for ProgressLane {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn spawn_nested_export_job(
    state: AppState,
    job_id: String,
    work: NestedExportWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        if !JobService::mark_queued(&st, &job_id).await {
            st.clear_cancel(&job_id).await;
            return;
        }
        let Some(guards) = JobService::acquire_execution(
            &st, &job_id, &token, None,
            AcquirePlan { resource_class: ResourceClass::Export, acquire_render_permit: true },
        ).await else { return };
        let filename = format!("{}.mkv", work.output_id);
        let output = st.outputs_dir().join(&filename);
        let published = Arc::new(AtomicBool::new(false));
        let success = Arc::new(AtomicBool::new(false));
        let published_work = published.clone();
        let success_work = success.clone();
        let result_path = output.clone();
        let cleanup_output_id = work.output_id.clone();
        let st_work = st.clone();
        let updated = JobService::run_acquired(
            &st, &job_id, &token, attempt, "native_export", "output", guards,
            |ctx| async move {
                let st = st_work;
                let outcome = async {
                    anyhow::ensure!(work.schema_version == 1, "invalid native export work schema");
                    let spec = work.request.validate()?;
                    let bound = bindings(&st, &work.request, &work.owner, &ctx.cancel).await
                        .map_err(|_| anyhow::anyhow!("invalid native export bindings: источники недоступны"))?;
                    anyhow::ensure!(bound.identity == work.identity, "invalid native export source or mask fingerprint changed");
                    let deadline = std::time::Instant::now().checked_add(st.job_timeout()).ok_or_else(|| anyhow::anyhow!("native export deadline overflow"))?;
                    let remaining = || deadline.checked_duration_since(std::time::Instant::now()).ok_or_else(|| anyhow::anyhow!("native export timeout"));
                    let with_audio = work.request.mode == NativeExportMode::VideoAudio;
                    let mixed = if with_audio {
                        let plan = crate::domain::nested_audio::plan_audio_tracks(&work.request.project, &work.request.composition_id, work.request.start_us..work.request.end_us)?;
                        let mut audio_sources = BTreeMap::new();
                        for id in plan.iter().map(|layer| layer.source_id().clone()).collect::<BTreeSet<_>>() {
                            let path = bound.sources.get(&id).ok_or_else(|| anyhow::anyhow!("native audio binding missing"))?.clone();
                            let selected=crate::tools::native_audio_export::preferred_audio_stream(&plan,&id);
                            let probe = crate::tools::pcm_conform::probe_pcm_metadata_for_stream(&st.process_runtime, Path::new("ffprobe"), &path, remaining()?, &ctx.cancel,selected).await?;
                            audio_sources.insert(id, PcmSource::from_probe(path, probe)?);
                        }
                        let video_bytes = spec.width as u64 * spec.height as u64 * 4 * spec.frames;
                        let audio_budget = STAGING_BYTES.checked_sub(video_bytes).ok_or_else(|| anyhow::anyhow!("native export staging budget exceeded"))?;
                        let phase = ProgressLane::new(&ctx.progress, 0.0, 0.3);
                        Some(mix_project_audio_tracks(&st.process_runtime, Path::new("ffmpeg"), &st.outputs_dir(), &work.request.project, &work.request.composition_id, work.request.start_us..work.request.end_us, spec.audio_frames_48k()?, &audio_sources, audio_budget, remaining()?, &ctx.cancel, &phase.sender).await?)
                    } else { None };
                    let phase = ProgressLane::new(&ctx.progress, if with_audio {30.0} else {0.0}, if with_audio {0.7} else {1.0});
                    let mut masks=bound.masks;
                    let filter:crate::tools::native_rgba_export::SharedClipFilter=Arc::new(std::sync::Mutex::new(Box::new(move|node:&str,clip:&crate::domain::composition::CompositionClipId,source:&SourceId,time,frame:&mut crate::domain::group_surface::GroupSurface|masks.apply(node,clip,source,time,frame))));
                    encode_project_with_clip_filter(
                        &st.process_runtime, Path::new("ffmpeg"), &result_path,
                        work.request.project.clone(), work.request.composition_id.clone(),
                        bound.outputs, bound.sources, work.request.start_us..work.request.end_us,
                        STAGING_BYTES, remaining()?, &ctx.cancel, &phase.sender,
                        mixed.as_ref().map(|pcm| RasterAudio {path: pcm.path(), frames: pcm.frames}),
                        Some(filter),
                    ).await?;
                    published_work.store(true, Ordering::Relaxed);
                    let current = bindings(&st, &work.request, &work.owner, &ctx.cancel).await
                        .map_err(|_| anyhow::anyhow!("invalid native export bindings before publication: доступ к источникам изменился"))?;
                    anyhow::ensure!(current.identity == work.identity, "invalid native export source or mask changed during rendering");
                    anyhow::ensure!(!ctx.cancel.is_cancelled(), "native export cancelled");
                    let bytes = tokio::fs::metadata(&result_path).await?.len();
                    st.db.grant_output_access(&work.output_id, &work.owner).await?;
                    success_work.store(true, Ordering::Relaxed);
                    Ok(Some(json!({
                        "id": work.output_id, "filename": filename,
                        "url": format!("/files/outputs/{filename}"), "mediaType": "video",
                        "container": "mkv", "vcodec": "ffv1", "videoCodec": "ffv1",
                        "audioCodec": if with_audio {Some("pcm_f32le")} else {None}, "withAudio": with_audio, "mode": work.request.mode,
                        "width": spec.width, "height": spec.height,
                        "fps": spec.frame_rate.numerator as f64 / spec.frame_rate.denominator as f64,
                        "fpsRate": spec.frame_rate,
                        "duration": spec.frames as f64 * spec.frame_rate.denominator as f64 / spec.frame_rate.numerator as f64,
                        "sizeBytes": bytes,
                    })))
                }.await;
                if ctx.cancel.is_cancelled() { Ok(None) } else { outcome }
            },
        ).await;
        if published.load(Ordering::Relaxed) && (!updated || !success.load(Ordering::Relaxed)) {
            let _ = tokio::fs::remove_file(output).await;
            let _ = st.db.revoke_output_access(&cleanup_output_id).await;
        }
    });
}
