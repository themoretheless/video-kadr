use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::resource_classes::ResourceClass;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::media_contract::{validate_conformance, ConformanceSpec, MediaTime};
use crate::domain::media_probe::{Rational, StreamKind};
use crate::domain::output::OutputFormat;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, EnqueueOutcome, ErrorKind, JobEvent, JobKind, JobPermit};
use crate::library::MediaEntry;
use crate::luts::MAX_LUT_FILE_BYTES;
use crate::model::{EditRequest, ImportRequest};
use crate::ports::telemetry::TelemetryEvent;
use crate::ports::{ExportCommandCompiler, ExportCompileRequest};
use crate::services::render::{
    validate_color_grade_request, EditPlan, ExportExecutionProfile, RenderExecution,
    RenderResources, SourceMediaMetadata,
};
use crate::state::{AppState, ToolInfo};
use crate::tools::{self, Done};

mod composition;
mod jobs;
mod library;
mod luts;
mod project_archive;
mod proxy;
mod publish;
mod stock;
mod upload;

pub use composition::composition_render_handler;
use composition::{spawn_composition_job, CompositionWork};
pub use jobs::{
    cancel_handler, discard_job_handler, failed_jobs_handler, job_registry_handler,
    job_status_handler, resume_pending_jobs, retry_job_handler, start_job_dispatcher,
};
use jobs::{dispatch_job, JobLeaseHeartbeat};
pub use library::{
    library_delete_handler, library_filmstrip_handler, library_filmstrip_version_handler,
    library_list_handler, library_metadata_patch_handler, library_metadata_put_handler,
    library_search_handler, library_thumbnail_handler, library_thumbnail_version_handler,
    source_file_handler,
};
pub use luts::{lut_get_handler, lut_list_handler, lut_upload_handler, MAX_LUT_BODY_BYTES};
pub use project_archive::{
    composition_project_archive_export_handler, composition_project_archive_import_handler,
};
pub use proxy::{
    proxy_content_handler, proxy_create_handler, proxy_delete_handler, proxy_list_handler,
};
use proxy::{spawn_proxy_job, ProxyWork};
pub use publish::{
    youtube_callback_handler, youtube_connect_handler, youtube_disconnect_handler,
    youtube_publish_handler, youtube_status_handler,
};
pub use stock::stock_search_handler;
pub use upload::upload_handler;

pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    match state.prometheus_metrics() {
        Some(body) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(
                    "application/openmetrics-text; version=1.0.0; charset=utf-8",
                ),
            )],
            body,
        )
            .into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportWork {
    schema_version: u32,
    request: ImportRequest,
    video_id: String,
    #[serde(default)]
    trace_context: crate::telemetry::context::TraceContext,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditWork {
    schema_version: u32,
    request: EditRequest,
    output_id: String,
    cache_key: String,
    #[serde(default)]
    trace_context: crate::telemetry::context::TraceContext,
}

const EDIT_WORK_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditWorkTimelineSemantics {
    LegacySorted,
    Ordered,
}

impl EditWorkTimelineSemantics {
    fn from_schema_version(schema_version: u32) -> anyhow::Result<Self> {
        match schema_version {
            1 => Ok(Self::LegacySorted),
            EDIT_WORK_SCHEMA_VERSION => Ok(Self::Ordered),
            _ => anyhow::bail!("unsupported edit work version {schema_version}"),
        }
    }

    fn pipeline_version(self) -> &'static str {
        match self {
            Self::LegacySorted => LEGACY_RENDER_CACHE_PIPELINE_VERSION,
            Self::Ordered => RENDER_CACHE_PIPELINE_VERSION,
        }
    }

    fn compile_plan(
        self,
        source_fingerprint: Fingerprint,
        request: EditRequest,
        source: SourceMediaMetadata,
    ) -> anyhow::Result<EditPlan> {
        match self {
            Self::LegacySorted => EditPlan::compile_legacy_v1(source_fingerprint, request, source),
            Self::Ordered => EditPlan::compile(source_fingerprint, request, source),
        }
    }
}

impl EditWork {
    fn timeline_semantics(&self) -> anyhow::Result<EditWorkTimelineSemantics> {
        EditWorkTimelineSemantics::from_schema_version(self.schema_version)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditDedupeIdentity<'a> {
    pipeline_version: &'static str,
    runtime_fingerprint: &'a str,
    request: &'a EditRequest,
}

/// `POST /api/import` — accept a video URL, kick off a download in the background,
/// and immediately return a job id to poll.
pub async fn import_handler(
    State(state): State<AppState>,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(req): ApiJson<ImportRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let job_id = Uuid::new_v4().to_string();
    let work = ImportWork {
        schema_version: 1,
        request: req,
        video_id: Uuid::new_v4().to_string(),
        trace_context: trace.map(|Extension(value)| value).unwrap_or_default(),
    };
    let key = dedupe_key("import", &work.request)
        .map_err(|error| AppError::internal("build import dedupe key", error))?;
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize import job", error))?;
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Import, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue import job", error))?
    {
        EnqueueOutcome::Created(_) => job_id,
        EnqueueOutcome::Existing(existing) => existing,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests(
                "Слишком много новых задач; повторите позже",
            ));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobId": resolved_id }))))
}

fn spawn_import_job(
    state: AppState,
    job_id: String,
    work: ImportWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let trace_context = work.trace_context.clone();
    let req = work.request;
    let vid = work.video_id;
    let span = trace_context.job_span(&job_id, "import");
    state.spawn_task(
        async move {
            let _lease = lease;
            // Reject bad/unsafe URLs before doing any work.
            if let Err(e) = tools::validate_url(&req.url).await {
                apply_job_event(
                    &st,
                    &jid,
                    JobEvent::Failed {
                        kind: ErrorKind::Security,
                        message: crate::privacy::redact_text(&e.to_string()),
                    },
                )
                .await;
                st.clear_cancel(&jid).await;
                return;
            }

            // Wait for a queue slot.
            if !mark_queued(&st, &jid).await {
                st.clear_cancel(&jid).await;
                return;
            }
            let _permit =
                match acquire_job_permit_or_cancelled(&st, &jid, &token, ResourceClass::Ingest)
                    .await
                {
                    Some(p) => p,
                    None => return,
                };
            if token.is_cancelled() {
                mark_cancelled(&st, &jid).await;
                return;
            }

            if !mark_running(&st, &jid, "downloading", attempt).await {
                st.clear_cancel(&jid).await;
                return;
            }

            let (tx, rx) = mpsc::unbounded_channel::<f64>();
            let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

            let sources = st.sources_dir();
            let outcome = async {
                let done = tools::download_video(
                    &st.process_runtime,
                    &req.url,
                    &sources,
                    &vid,
                    req.start,
                    req.end,
                    &tx,
                    &token,
                    st.max_download_height(),
                    st.job_timeout(),
                )
                .instrument(tracing::info_span!("process", process.tool = "yt-dlp"))
                .await?;
                if matches!(done, Done::Cancelled) {
                    cleanup_files_with_prefix(&sources, &vid).await;
                    return Ok::<Option<Value>, anyhow::Error>(None);
                }
                let path = tools::find_source(&sources, &vid).await?;
                let info = tools::probe_video(&st.process_runtime, &path).await?;
                let title = tools::read_title(&sources, &vid).await;
                let size = tokio::fs::metadata(&path).await.map(|m| m.len()).ok();
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("{vid}.mp4"));
                Ok(Some(json!({
                    "id": vid,
                    "url": format!("/files/sources/{filename}"),
                    "filename": filename,
                    "duration": info.duration,
                    "width": info.width,
                    "height": info.height,
                    "title": title,
                    "fps": info.fps,
                    "vcodec": info.vcodec,
                    "acodec": info.acodec,
                    "sizeBytes": size,
                })))
            }
            .await;

            if outcome.is_err() {
                cleanup_files_with_prefix(&sources, &vid).await;
            }

            drop(tx);
            let _ = drain.await;
            finish_job(&st, &jid, outcome, "source").await;
        }
        .instrument(span),
    );
}

/// Content key for the render cache: a hash of the canonical (source + edit)
/// request. Re-serializing the deserialized `EditRequest` normalises omitted
/// defaults, so two equivalent requests map to the same key.
const LEGACY_RENDER_CACHE_PIPELINE_VERSION: &str = "render-cache-v2-color-pipeline-v2";
const RENDER_CACHE_PIPELINE_VERSION: &str = "render-cache-v3-ordered-segments-color-pipeline-v2";

pub fn render_cache_key(req: &EditRequest) -> String {
    render_cache_key_with_context(req, None, None)
}

pub fn render_cache_key_for_tools(req: &EditRequest, tools: &ToolInfo) -> String {
    let fingerprint = render_runtime_fingerprint(tools);
    render_cache_key_with_context(req, Some(&fingerprint), None)
}

fn render_cache_key_with_context(
    req: &EditRequest,
    runtime_fingerprint: Option<&str>,
    lut_sha256: Option<&str>,
) -> String {
    render_cache_key_with_pipeline(
        RENDER_CACHE_PIPELINE_VERSION,
        req,
        runtime_fingerprint,
        lut_sha256,
    )
}

fn render_cache_key_with_pipeline(
    pipeline_version: &str,
    req: &EditRequest,
    runtime_fingerprint: Option<&str>,
    lut_sha256: Option<&str>,
) -> String {
    let canonical = serde_json::to_string(req).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(pipeline_version.as_bytes());
    hasher.update([0]);
    hasher.update(runtime_fingerprint.unwrap_or("missing").as_bytes());
    hasher.update([0]);
    hasher.update(lut_sha256.unwrap_or("no-lut").as_bytes());
    hasher.update([0]);
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn render_runtime_fingerprint(tools: &ToolInfo) -> String {
    let mut encoders = tools.ffmpeg_encoders.clone();
    let mut muxers = tools.ffmpeg_muxers.clone();
    let mut filters = tools.ffmpeg_filters.clone();
    encoders.sort();
    muxers.sort();
    filters.sort();

    let mut hash = Sha256::new();
    hash.update(tools.ffmpeg_version.as_deref().unwrap_or("missing"));
    for values in [&encoders, &muxers, &filters] {
        hash.update([0]);
        for value in values {
            hash.update(value.as_bytes());
            hash.update([0]);
        }
    }
    format!("{:x}", hash.finalize())
}

/// `POST /api/edit` — apply trim/crop/scale/mute/speed to a previously imported
/// video and return a job id to poll for the rendered result.
pub async fn edit_handler(
    State(state): State<AppState>,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(mut req): ApiJson<EditRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    // Audio-only exports have no video filter graph. Canonicalise video-only
    // colour fields before validation, durable dedupe, and cache identity.
    if matches!(req.format.as_deref(), Some("mp3" | "wav")) {
        req.lut = None;
        req.curves = None;
        req.chroma_key = None;
        req.hsl = None;
        req.color_wheels = None;
    } else if req
        .lut
        .as_ref()
        .is_some_and(|lut| (0.0..=1e-9).contains(&lut.intensity))
    {
        req.lut = None;
    }
    validate_color_grade_request(&req).map_err(|_| {
        AppError::bad_request("некорректные параметры цвета, chroma key или audio DSP")
    })?;
    validate_color_grade_capabilities(&state, &req)?;
    let job_id = Uuid::new_v4().to_string();
    let runtime_fingerprint = render_runtime_fingerprint(state.tools.as_ref());
    let cache_key = render_cache_key_with_context(&req, Some(&runtime_fingerprint), None);
    let key = dedupe_key(
        "edit",
        &EditDedupeIdentity {
            pipeline_version: RENDER_CACHE_PIPELINE_VERSION,
            runtime_fingerprint: &runtime_fingerprint,
            request: &req,
        },
    )
    .map_err(|error| AppError::internal("build edit dedupe key", error))?;
    let work = EditWork {
        schema_version: EDIT_WORK_SCHEMA_VERSION,
        request: req,
        output_id: Uuid::new_v4().to_string(),
        cache_key,
        trace_context: trace.map(|Extension(value)| value).unwrap_or_default(),
    };
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize edit job", error))?;
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Edit, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue edit job", error))?
    {
        EnqueueOutcome::Created(_) => job_id,
        EnqueueOutcome::Existing(existing) => existing,
        EnqueueOutcome::RateLimited => {
            return Err(AppError::too_many_requests(
                "Слишком много новых задач; повторите позже",
            ));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobId": resolved_id }))))
}

fn validate_color_grade_capabilities(state: &AppState, request: &EditRequest) -> AppResult<()> {
    let has_filter = |name: &str| {
        state.tools.ffmpeg
            && state
                .tools
                .ffmpeg_filters
                .iter()
                .any(|candidate| candidate == name)
    };
    if request.curves.is_some() && !has_filter("curves") {
        return Err(AppError::bad_request(
            "кривые недоступны: FFmpeg filter curves не найден",
        ));
    }
    if request.hsl.is_some() && !has_filter("huesaturation") {
        return Err(AppError::bad_request(
            "HSL недоступен: FFmpeg filter huesaturation не найден",
        ));
    }
    if request.color_wheels.is_some() && !has_filter("colorbalance") {
        return Err(AppError::bad_request(
            "цветовые колёса недоступны: FFmpeg filter colorbalance не найден",
        ));
    }
    if request.audio_eq.is_some() && !has_filter("equalizer") {
        return Err(AppError::bad_request(
            "аудио EQ недоступен: FFmpeg filter equalizer не найден",
        ));
    }
    if request.pan.abs() > 1e-9 && (!has_filter("aformat") || !has_filter("stereotools")) {
        return Err(AppError::bad_request(
            "стереопанорама недоступна: нужны FFmpeg filters aformat и stereotools",
        ));
    }
    if request.compressor.is_some() && !has_filter("acompressor") {
        return Err(AppError::bad_request(
            "компрессор недоступен: FFmpeg filter acompressor не найден",
        ));
    }
    if request.limiter.is_some() && !has_filter("alimiter") {
        return Err(AppError::bad_request(
            "лимитер недоступен: FFmpeg filter alimiter не найден",
        ));
    }
    if let Some(chroma_key) = &request.chroma_key {
        if !has_filter("chromakey") {
            return Err(AppError::bad_request(
                "chroma key недоступен: FFmpeg filter chromakey не найден",
            ));
        }
        if chroma_key.spill_suppression > 1e-9 && !has_filter("despill") {
            return Err(AppError::bad_request(
                "подавление chroma spill недоступно: FFmpeg filter despill не найден",
            ));
        }
    }
    if let Some(lut) = request.lut.as_ref().filter(|lut| lut.intensity > 1e-9) {
        if !has_filter("lut3d") {
            return Err(AppError::bad_request(
                "3D LUT недоступны: FFmpeg filter lut3d не найден",
            ));
        }
        if lut.intensity < 1.0 - 1e-9 && !has_filter("blend") {
            return Err(AppError::bad_request(
                "частичная интенсивность LUT недоступна: FFmpeg filter blend не найден",
            ));
        }
    }
    Ok(())
}

fn spawn_edit_job(
    state: AppState,
    job_id: String,
    work: EditWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let timeline_semantics = work
        .timeline_semantics()
        .expect("edit work schema is validated before dispatch");
    let EditWork {
        schema_version: _,
        request: req,
        output_id: out_id,
        cache_key: _,
        trace_context,
    } = work;
    let st = state.clone();
    let jid = job_id.clone();
    let span = trace_context.job_span(&job_id, "edit");
    let task = async move {
        let _lease = lease;
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        let resources = match resolve_render_resources(&st, &req).await {
            Ok(resources) => resources,
            Err(error) => {
                finish_job(&st, &jid, Err(error), "output").await;
                return;
            }
        };
        let runtime_fingerprint = render_runtime_fingerprint(st.tools.as_ref());
        let cache_key = render_cache_key_with_pipeline(
            timeline_semantics.pipeline_version(),
            &req,
            Some(&runtime_fingerprint),
            resources.lut_sha256(),
        );
        let render_lock = st.render_lock(&cache_key).await;
        let _render_guard =
            match acquire_render_lock_or_cancelled(&st, &jid, &token, render_lock).await {
                Some(g) => g,
                None => return,
            };
        if finish_from_render_cache(&st, &jid, &cache_key, None).await {
            return;
        }

        let _render_permit = match acquire_render_permit_or_cancelled(&st, &jid, &token).await {
            Some(p) => p,
            None => return,
        };
        let _permit =
            match acquire_job_permit_or_cancelled(&st, &jid, &token, ResourceClass::Export).await {
                Some(p) => p,
                None => return,
            };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }

        if !mark_running(&st, &jid, "processing", attempt).await {
            st.clear_cancel(&jid).await;
            return;
        }

        let (tx, rx) = mpsc::unbounded_channel::<f64>();
        let drain = spawn_progress_drain(st.clone(), jid.clone(), rx);

        let outputs = st.outputs_dir();
        let req = req;
        let requested_format =
            OutputFormat::parse(req.format.as_deref()).unwrap_or(OutputFormat::Mp4);
        let ext = tools::output_ext(requested_format);
        let filename = format!("{out_id}.{ext}");
        let output_path = outputs.join(&filename);
        let staging_output = st.staging_dir().join(format!("{out_id}.render.{ext}"));

        let outcome = async {
            let entry = st
                .library
                .get(&req.video_id)
                .await
                .filter(|entry| entry.kind == "source")
                .ok_or_else(|| anyhow::anyhow!("source {} not found", req.video_id))?;
            let input = st.library.resolve_media_path(&entry).await?;
            let probe = tools::probe_video(&st.process_runtime, &input).await?;
            let source_fingerprint = Fingerprint::digest(req.video_id.as_bytes());
            let source = SourceMediaMetadata::new_with_audio(
                probe.width,
                probe.height,
                probe.duration,
                probe.acodec.is_some(),
            )?;
            let plan =
                Arc::new(timeline_semantics.compile_plan(source_fingerprint, req, source)?);
            let execution = RenderExecution::new_with_resources(
                plan,
                ExportExecutionProfile {
                    encode_budget: st.encode_budget.clone(),
                    verify_checksums: true,
                },
                resources,
            );
            let command = tools::FfmpegExportCompiler.compile(ExportCompileRequest {
                input: &input,
                destination: &staging_output,
                parallel_jobs: st.render_parallelism(),
                execution: &execution,
            })?;
            tracing::info!(output.format = %execution.output().format, "starting render");
            let done = tools::run_compiled_ffmpeg(
                &st.process_runtime,
                &command,
                &tx,
                &token,
                st.job_timeout(),
            )
            .instrument(tracing::info_span!("process", process.tool = "ffmpeg"))
            .await?;
            if matches!(done, Done::Cancelled) {
                let _ = tokio::fs::remove_file(&staging_output).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let output_probe = tools::probe_video(&st.process_runtime, &staging_output).await?;
            let conformance = render_conformance_spec(
                requested_format,
                command.expected_duration_seconds,
                output_probe.duration,
            );
            validate_conformance(&output_probe, &conformance)
                .map_err(|reason| anyhow::anyhow!("post-mux conformance failed: {reason}"))?;
            tokio::fs::rename(&staging_output, &output_path).await?;
            let size = tokio::fs::metadata(&output_path)
                .await
                .map(|m| m.len())
                .ok();
            Ok(Some(json!({
                "id": out_id,
                "url": format!("/files/outputs/{filename}"),
                "filename": filename,
                "sizeBytes": size,
            })))
        }
        .await;

        if outcome.is_err() {
            let _ = tokio::fs::remove_file(&output_path).await;
            let _ = tokio::fs::remove_file(&staging_output).await;
        }

        let cache_info = match &outcome {
            Ok(Some(info)) => Some(info.clone()),
            _ => None,
        };

        drop(tx);
        let _ = drain.await;
        let updated = finish_job(&st, &jid, outcome, "output").await;
        if updated {
            if let Some(info) = &cache_info {
                if let Err(error) = st.db.cache_put(&cache_key, info, &filename).await {
                    tracing::warn!("cache successful render {cache_key}: {error}");
                }
            }
        }
    }
    .instrument(span);
    state.spawn_task(task);
}

fn render_conformance_spec(
    format: OutputFormat,
    expected_duration_seconds: f64,
    actual_duration_seconds: f64,
) -> ConformanceSpec {
    let formats = match format {
        OutputFormat::Mp4 | OutputFormat::Av1 => ["mov", "mp4"].as_slice(),
        OutputFormat::Prores => ["mov"].as_slice(),
        OutputFormat::Webm => ["matroska", "webm"].as_slice(),
        OutputFormat::Gif => ["gif"].as_slice(),
        OutputFormat::Png | OutputFormat::Jpg => ["image2"].as_slice(),
        OutputFormat::Mp3 => ["mp3"].as_slice(),
        OutputFormat::Wav => ["wav"].as_slice(),
    };
    let still = matches!(format, OutputFormat::Png | OutputFormat::Jpg);
    let duration = if still {
        actual_duration_seconds
    } else {
        expected_duration_seconds
    };
    let time_base = Rational {
        numerator: 1,
        denominator: 1_000,
    };
    ConformanceSpec {
        formats: formats.iter().map(|value| (*value).to_owned()).collect(),
        required_streams: BTreeSet::from([
            if matches!(format, OutputFormat::Mp3 | OutputFormat::Wav) {
                StreamKind::Audio
            } else {
                StreamKind::Video
            },
        ]),
        expected_duration: MediaTime::new((duration * 1_000.0).round() as i64, time_base)
            .expect("fixed millisecond time base is valid"),
        duration_tolerance: MediaTime::new(if still { 1_000 } else { 1_100 }, time_base)
            .expect("fixed millisecond time base is valid"),
    }
}

/// Resolve client-visible immutable asset ids to private, regular files. The
/// renderer never accepts a path from the wire request, and the resolved path
/// is carried separately from the serializable edit plan.
async fn resolve_render_resources(
    state: &AppState,
    request: &EditRequest,
) -> anyhow::Result<RenderResources> {
    // Audio-only exports do not compile a video filter graph. Do not require or
    // grant read access to a LUT that an audio-only command cannot consume.
    if matches!(request.format.as_deref(), Some("mp3" | "wav")) {
        return Ok(RenderResources::default());
    }
    let Some(selection) = request.lut.as_ref() else {
        return Ok(RenderResources::default());
    };
    // An intensity of zero is canonicalised to a bypass by EditPlan. Avoid
    // requiring an asset that the resulting plan will not read.
    if selection.intensity <= 1e-9 {
        return Ok(RenderResources::default());
    }
    Uuid::parse_str(&selection.id).map_err(|_| anyhow::anyhow!("LUT не найден"))?;
    let asset = state
        .db
        .get_lut(&selection.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("LUT не найден"))?;
    anyhow::ensure!(asset.id == selection.id, "Некорректная ссылка на LUT");
    let expected_filename = format!("{}.cube", asset.id);
    anyhow::ensure!(
        asset.filename == expected_filename,
        "Некорректная ссылка на файл LUT"
    );
    let luts_dir = tokio::fs::canonicalize(state.luts_dir())
        .await
        .map_err(|_| anyhow::anyhow!("Хранилище LUT недоступно"))?;
    let path = state.luts_dir().join(&asset.filename);
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| anyhow::anyhow!("Файл LUT не найден"))?;
    anyhow::ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "Файл LUT имеет недопустимый тип"
    );
    anyhow::ensure!(
        metadata.len() <= MAX_LUT_FILE_BYTES as u64 && metadata.len() == asset.size_bytes,
        "Файл LUT повреждён"
    );
    let canonical_path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| anyhow::anyhow!("Файл LUT не найден"))?;
    anyhow::ensure!(
        canonical_path.parent() == Some(luts_dir.as_path()),
        "Файл LUT находится вне хранилища"
    );

    let file = tokio::fs::File::open(&canonical_path)
        .await
        .map_err(|_| anyhow::anyhow!("Файл LUT не найден"))?;
    let mut reader = file.take(MAX_LUT_FILE_BYTES as u64 + 1);
    let mut bytes = Vec::with_capacity(asset.size_bytes as usize);
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| anyhow::anyhow!("Не удалось проверить файл LUT"))?;
    anyhow::ensure!(
        bytes.len() as u64 == asset.size_bytes && bytes.len() <= MAX_LUT_FILE_BYTES,
        "Файл LUT повреждён"
    );
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    anyhow::ensure!(actual_sha256 == asset.sha256, "Файл LUT повреждён");
    Ok(RenderResources::with_verified_lut(
        canonical_path,
        actual_sha256,
    ))
}

pub async fn api_not_found_handler() -> AppError {
    AppError::not_found("API-маршрут не найден")
}

pub async fn method_not_allowed_handler() -> AppError {
    AppError::method_not_allowed("Метод не поддерживается")
}

/// Drain numeric progress updates from a worker into the job record.
fn spawn_progress_drain(
    st: AppState,
    jid: String,
    mut rx: mpsc::UnboundedReceiver<f64>,
) -> tokio::task::JoinHandle<()> {
    let owner = st.clone();
    owner.spawn_task(async move {
        let mut last_saved = -1.0_f64;
        while let Some(p) = rx.recv().await {
            if p >= 100.0 || p - last_saved >= 1.0 {
                last_saved = p;
                st.update_job_if_open(&jid, |j| j.progress = Some(p)).await;
            }
        }
    })
}

/// Atomically start an open job. Cancellation may win immediately before this
/// transition; in that case the terminal state must never be overwritten.
async fn mark_running(st: &AppState, jid: &str, stage: &str, attempt: u32) -> bool {
    apply_job_event(
        st,
        jid,
        JobEvent::Started {
            stage: stage.into(),
            attempt,
        },
    )
    .await
}

async fn mark_queued(st: &AppState, jid: &str) -> bool {
    apply_job_event(st, jid, JobEvent::Queued).await
}

async fn apply_job_event(st: &AppState, jid: &str, event: JobEvent) -> bool {
    match st.transition_job(jid, event).await {
        Ok(applied) => applied,
        Err(error) => {
            tracing::error!(job.id = jid, %error, "persist job transition");
            false
        }
    }
}

async fn finish_from_render_cache(
    st: &AppState,
    jid: &str,
    cache_key: &str,
    actor: Option<&str>,
) -> bool {
    let Ok(Some((output, filename))) = st.db.cache_get(cache_key).await else {
        st.telemetry
            .record(TelemetryEvent::CacheLookup { result: "miss" });
        return false;
    };
    if !is_plain_filename(&filename) {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    if tokio::fs::metadata(st.outputs_dir().join(&filename))
        .await
        .is_err()
    {
        let _ = st.db.cache_delete(cache_key).await;
        return false;
    }
    let output_id = output["id"].as_str().map(str::to_owned);
    let updated = apply_job_event(st, jid, JobEvent::Succeeded { result: output }).await;
    if updated {
        if let (Some(actor), Some(output_id)) = (actor, output_id.as_deref()) {
            if let Err(error) = st.db.grant_output_access(output_id, actor).await {
                tracing::error!(%error, %output_id, "grant cached output access");
            }
        }
    }
    st.telemetry
        .record(TelemetryEvent::CacheLookup { result: "hit" });
    st.clear_cancel(jid).await;
    updated
}

fn is_plain_filename(filename: &str) -> bool {
    !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && filename != "."
        && filename != ".."
        && Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == filename)
}

async fn acquire_render_lock_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    lock: Arc<Mutex<()>>,
) -> Option<OwnedMutexGuard<()>> {
    tokio::select! {
        guard = lock.lock_owned() => Some(guard),
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn acquire_job_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
    class: ResourceClass,
) -> Option<JobPermit> {
    tokio::select! {
        permit = st.acquire_job_slot(class) => match permit {
            Ok(p) => Some(p),
            Err(_) => {
                mark_queue_closed(st, jid).await;
                None
            }
        },
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn acquire_render_permit_or_cancelled(
    st: &AppState,
    jid: &str,
    token: &CancellationToken,
) -> Option<JobPermit> {
    tokio::select! {
        permit = st.acquire_render_slot() => match permit {
            Ok(p) => Some(p),
            Err(_) => {
                mark_queue_closed(st, jid).await;
                None
            }
        },
        _ = token.cancelled() => {
            mark_cancelled(st, jid).await;
            None
        }
    }
}

async fn mark_cancelled(st: &AppState, jid: &str) {
    // Shutdown cancellation is recoverable: keep the durable pending/running
    // snapshot for startup reconciliation. Only a user cancellation is terminal.
    if st.is_shutting_down() {
        st.clear_cancel(jid).await;
        return;
    }
    apply_job_event(st, jid, JobEvent::Cancelled).await;
    st.clear_cancel(jid).await;
}

async fn mark_queue_closed(st: &AppState, jid: &str) {
    if st.is_shutting_down() {
        st.clear_cancel(jid).await;
        return;
    }
    apply_job_event(
        st,
        jid,
        JobEvent::Failed {
            kind: ErrorKind::Internal,
            message: "очередь задач закрыта".into(),
        },
    )
    .await;
    st.clear_cancel(jid).await;
}

async fn cleanup_files_with_prefix(dir: &Path, prefix: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let matches_prefix = name == prefix
            || name
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'));
        if name == ".gitkeep" || !matches_prefix {
            continue;
        }
        let _ = tokio::fs::remove_file(entry.path()).await;
    }
}

/// Apply the terminal outcome of a worker to the job and clear its cancel token.
/// `Ok(Some)` -> done (also recorded in the media library under `kind`),
/// `Ok(None)` -> cancelled, `Err` -> error.
async fn finish_job(
    st: &AppState,
    jid: &str,
    outcome: anyhow::Result<Option<Value>>,
    kind: &str,
) -> bool {
    let updated = match outcome {
        Ok(Some(info)) => {
            let updated = apply_job_event(
                st,
                jid,
                JobEvent::Succeeded {
                    result: info.clone(),
                },
            )
            .await;
            if updated {
                let entry = MediaEntry::from_result(kind, &info);
                if st.library.add(entry.clone()).await {
                    st.index_media(&entry).await;
                }
            }
            updated
        }
        Ok(None) if st.is_shutting_down() => false,
        Ok(None) => apply_job_event(st, jid, JobEvent::Cancelled).await,
        Err(e) => {
            let kind = classify_job_error(&e);
            let message = crate::privacy::redact_text(&e.to_string());
            tracing::error!(job.id = jid, error.kind = kind.as_str(), error = %message, "job failed");
            let updated = apply_job_event(st, jid, JobEvent::Failed { kind, message }).await;
            if updated && kind.retryable() {
                match st.job_store.schedule_retry(jid).await {
                    Ok(Some((job, delay))) => {
                        st.replace_job(job).await;
                        tracing::info!(
                            job.id = jid,
                            retry.delay_ms = delay.as_millis(),
                            "job retry scheduled"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::error!(job.id = jid, %error, "schedule job retry");
                    }
                }
            }
            updated
        }
    };
    st.clear_cancel(jid).await;
    updated
}

fn classify_job_error(error: &anyhow::Error) -> ErrorKind {
    let message = error.to_string().to_lowercase();
    if message.contains("timeout") || message.contains("deadline") || message.contains("таймаут")
    {
        ErrorKind::Timeout
    } else if message.contains("недопуст")
        || message.contains("invalid")
        || message.contains("outside duration")
        || (message.contains("source") && message.contains("not found"))
    {
        ErrorKind::Validation
    } else if message.contains("not found")
        || message.contains("no such file")
        || message.contains("metadata")
    {
        ErrorKind::Storage
    } else if message.contains("ffmpeg")
        || message.contains("yt-dlp")
        || message.contains("exit status")
    {
        ErrorKind::ProcessExit
    } else {
        ErrorKind::Internal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::model::{Job, JobStatus};
    use crate::state::{CancelJobOutcome, ToolInfo};
    use serde_json::json;

    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        tokio::fs::create_dir_all(storage.join("sources"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(storage.join("outputs"))
            .await
            .unwrap();
        let lib = Library::load(storage.clone()).await;
        let db = Db::open(&storage).await.unwrap();
        (AppState::new(storage, 2, ToolInfo::default(), lib, db), dir)
    }

    #[tokio::test]
    async fn finish_job_does_not_overwrite_terminal_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("j1".into())).await;
        st.update_job("j1", |j| j.status = JobStatus::Cancelled)
            .await;
        st.persist_job("j1").await;

        let updated = finish_job(
            &st,
            "j1",
            Ok(Some(json!({
                "id": "out",
                "filename": "out.mp4",
                "url": "/files/outputs/out.mp4"
            }))),
            "output",
        )
        .await;

        assert!(!updated);
        let job = st.get_job("j1").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.result.is_none());
        assert!(st.library.list().await.is_empty());
    }

    #[tokio::test]
    async fn shutdown_cancellation_leaves_durable_work_recoverable() {
        let (st, _dir) = state().await;
        let outcome = st
            .enqueue_job(
                "restartable".into(),
                JobKind::Import,
                &json!({"schemaVersion": 1}),
                "restartable-key",
            )
            .await
            .unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Created(_)));

        st.begin_shutdown();
        mark_cancelled(&st, "restartable").await;
        assert!(!finish_job(&st, "restartable", Ok(None), "source").await);
        assert_eq!(
            st.get_job("restartable").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
        assert_eq!(
            st.job_store.deliverable_ids().await.unwrap(),
            vec!["restartable"]
        );
    }

    #[tokio::test]
    async fn mark_running_does_not_revive_a_cancelled_job() {
        let (st, _dir) = state().await;
        st.set_job(Job::pending("cancelled-before-start".into()))
            .await;
        assert_eq!(
            st.cancel_open_job("cancelled-before-start").await.unwrap(),
            CancelJobOutcome::Cancelled
        );

        assert!(!mark_running(&st, "cancelled-before-start", "processing", 1).await);
        assert!(!mark_queued(&st, "cancelled-before-start").await);
        let job = st.get_job("cancelled-before-start").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.stage.is_none());
        assert!(job.progress.is_none());
    }

    #[tokio::test]
    async fn mp3_render_does_not_resolve_an_unused_lut() {
        let (st, _dir) = state().await;
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "format": "mp3",
            "lut": { "id": "missing-lut", "intensity": 1.0 }
        }))
        .unwrap();

        let resources = resolve_render_resources(&st, &request).await.unwrap();
        assert!(resources.lut_path().is_none());
    }

    #[tokio::test]
    async fn chroma_key_capabilities_fail_closed_for_key_and_spill_filters() {
        let (mut st, _dir) = state().await;
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "chromaKey": {
                "keyColor": "#00ff00",
                "similarity": 0.1,
                "blend": 0.05,
                "spillSuppression": 0.5
            }
        }))
        .unwrap();

        assert!(validate_color_grade_capabilities(&st, &request).is_err());
        st.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["chromakey".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&st, &request).is_err());
        st.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["chromakey".into(), "despill".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&st, &request).is_ok());

        let without_spill: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "chromaKey": {"keyColor": "#00ff00", "spillSuppression": 0.0}
        }))
        .unwrap();
        st.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["chromakey".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&st, &without_spill).is_ok());
    }

    #[tokio::test]
    async fn manual_color_capabilities_fail_closed_for_hsl_and_wheels() {
        let (mut state, _directory) = state().await;
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "hsl": {"red": {"hue": 12.0}},
            "colorWheels": {"shadows": {"blue": 0.2}}
        }))
        .unwrap();

        assert!(validate_color_grade_capabilities(&state, &request).is_err());
        state.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["huesaturation".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&state, &request).is_err());
        state.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["huesaturation".into(), "colorbalance".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&state, &request).is_ok());
    }

    #[tokio::test]
    async fn deterministic_audio_dsp_capabilities_fail_closed() {
        let (mut state, _directory) = state().await;
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "pan": 0.2,
            "audioEq": {"lowGainDb": 2.0},
            "compressor": {},
            "limiter": {}
        }))
        .unwrap();

        assert!(validate_color_grade_capabilities(&state, &request).is_err());
        state.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec![
                "aformat".into(),
                "stereotools".into(),
                "equalizer".into(),
                "acompressor".into(),
                "alimiter".into(),
            ],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities(&state, &request).is_ok());
    }

    #[tokio::test]
    async fn resolved_lut_is_uuid_scoped_and_content_verified() {
        let (st, _dir) = state().await;
        tokio::fs::create_dir_all(st.luts_dir()).await.unwrap();
        let id = "11111111-1111-4111-8111-111111111111";
        let filename = format!("{id}.cube");
        let bytes = b"LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let sha256 = format!("{:x}", Sha256::digest(bytes));
        tokio::fs::write(st.luts_dir().join(&filename), bytes)
            .await
            .unwrap();
        st.db
            .insert_or_get_lut(&crate::luts::LutAsset::new(
                id.into(),
                "Verified".into(),
                filename,
                2,
                bytes.len() as u64,
                sha256.clone(),
                1,
            ))
            .await
            .unwrap();
        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "lut": { "id": id, "intensity": 1.0 }
        }))
        .unwrap();

        let resources = resolve_render_resources(&st, &request).await.unwrap();
        assert_eq!(resources.lut_sha256(), Some(sha256.as_str()));

        let mut corrupt = bytes.to_vec();
        let last_value = corrupt.len() - 2;
        corrupt[last_value] = b'0';
        tokio::fs::write(st.luts_dir().join(format!("{id}.cube")), corrupt)
            .await
            .unwrap();
        assert!(resolve_render_resources(&st, &request).await.is_err());
    }

    #[test]
    fn render_cache_identity_includes_runtime_and_lut_content() {
        let request: EditRequest = serde_json::from_value(json!({ "videoId": "source" })).unwrap();
        let baseline = render_cache_key_with_context(&request, None, None);
        assert_ne!(
            baseline,
            render_cache_key_with_context(&request, Some("ffmpeg 8.1"), None)
        );
        assert_ne!(
            baseline,
            render_cache_key_with_context(&request, None, Some("lut-sha"))
        );
    }

    #[test]
    fn edit_work_v2_keeps_a_compatible_legacy_v1_route() {
        assert_eq!(
            EditWorkTimelineSemantics::from_schema_version(1).unwrap(),
            EditWorkTimelineSemantics::LegacySorted
        );
        assert_eq!(
            EditWorkTimelineSemantics::from_schema_version(EDIT_WORK_SCHEMA_VERSION).unwrap(),
            EditWorkTimelineSemantics::Ordered
        );
        assert!(EditWorkTimelineSemantics::from_schema_version(3).is_err());

        let request: EditRequest = serde_json::from_value(json!({
            "videoId": "source",
            "segments": [
                { "start": 6.0, "end": 8.0 },
                { "start": 1.0, "end": 4.0 }
            ]
        }))
        .unwrap();
        let legacy = render_cache_key_with_pipeline(
            LEGACY_RENDER_CACHE_PIPELINE_VERSION,
            &request,
            None,
            None,
        );
        let ordered = render_cache_key_with_context(&request, None, None);
        assert_ne!(legacy, ordered);

        let work = EditWork {
            schema_version: EDIT_WORK_SCHEMA_VERSION,
            request,
            output_id: "output".into(),
            cache_key: ordered,
            trace_context: Default::default(),
        };
        assert_eq!(
            serde_json::to_value(work).unwrap()["schemaVersion"],
            EDIT_WORK_SCHEMA_VERSION
        );
    }

    #[test]
    fn cache_filenames_must_be_plain_output_names() {
        assert!(is_plain_filename("output.mp4"));
        assert!(is_plain_filename("b5b2b5b2-clip.webm"));

        for filename in ["", ".", "..", "../x.mp4", "dir/x.mp4", "dir\\x.mp4"] {
            assert!(
                !is_plain_filename(filename),
                "{filename:?} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn finish_from_render_cache_validates_filename_and_file() {
        let (st, _dir) = state().await;

        st.set_job(Job::pending("valid".into())).await;
        let output = json!({
            "id": "out",
            "filename": "out.mp4",
            "url": "/files/outputs/out.mp4"
        });
        tokio::fs::write(st.outputs_dir().join("out.mp4"), b"video")
            .await
            .unwrap();
        st.db
            .cache_put("valid-key", &output, "out.mp4")
            .await
            .unwrap();

        assert!(finish_from_render_cache(&st, "valid", "valid-key", Some("publisher")).await);
        assert!(st.db.can_access_output("out", "publisher").await.unwrap());
        let job = st.get_job("valid").await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.progress, Some(100.0));
        assert_eq!(job.result.unwrap()["filename"], "out.mp4");

        st.set_job(Job::pending("unsafe".into())).await;
        st.db
            .cache_put(
                "unsafe-key",
                &json!({ "filename": "../leak.mp4" }),
                "../leak.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "unsafe", "unsafe-key", None).await);
        assert!(st.db.cache_get("unsafe-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("unsafe").await.unwrap().unwrap().status,
            JobStatus::Pending
        );

        st.set_job(Job::pending("missing".into())).await;
        st.db
            .cache_put(
                "missing-key",
                &json!({ "filename": "missing.mp4" }),
                "missing.mp4",
            )
            .await
            .unwrap();
        assert!(!finish_from_render_cache(&st, "missing", "missing-key", None).await);
        assert!(st.db.cache_get("missing-key").await.unwrap().is_none());
        assert_eq!(
            st.get_job("missing").await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }

    #[tokio::test]
    async fn cleanup_files_with_prefix_removes_partial_imports() {
        let (st, _dir) = state().await;
        let dir = st.sources_dir();
        tokio::fs::write(dir.join("abc.mp4.part"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abc.info.json"), b"x")
            .await
            .unwrap();
        tokio::fs::write(dir.join("abcd.mp4"), b"x").await.unwrap();
        tokio::fs::write(dir.join("abc"), b"x").await.unwrap();
        tokio::fs::write(dir.join(".gitkeep"), b"").await.unwrap();

        cleanup_files_with_prefix(&dir, "abc").await;

        assert!(!dir.join("abc.mp4.part").exists());
        assert!(!dir.join("abc.info.json").exists());
        assert!(!dir.join("abc").exists());
        assert!(dir.join("abcd.mp4").exists());
        assert!(dir.join(".gitkeep").exists());
    }
}
