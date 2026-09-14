use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::resource_classes::ResourceClass;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::media_contract::{validate_conformance, ConformanceSpec, MediaTime};
use crate::domain::media_probe::{Rational, StreamKind};
use crate::domain::output::OutputFormat;
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, AcquirePlan, EnqueueOutcome, JobKind, JobService};
use crate::luts::MAX_LUT_FILE_BYTES;
use crate::model::EditRequest;
use crate::ports::{ExportCommandCompiler, ExportCompileRequest};
use crate::services::render::{
    canonicalize_export_request, validate_color_grade_capabilities_for_tools,
    validate_color_grade_request, EditPlan, ExportExecutionProfile, RenderExecution,
    RenderResources, SourceMediaMetadata,
};
use crate::state::AppState;
use crate::tools::{self, Done};

use super::jobs::{dispatch_job, JobLeaseHeartbeat};
#[cfg(test)]
use super::render_cache::render_cache_key_with_pipeline;
use super::render_cache::{
    plan_render_cache_key, render_cache_key_with_context, render_runtime_fingerprint,
    LEGACY_RENDER_CACHE_PIPELINE_VERSION, RENDER_CACHE_PIPELINE_VERSION,
};
use super::{acquire_render_lock_or_cancelled, finish_from_render_cache, finish_job, mark_queued};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditWork {
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
    pub(crate) fn validate_schema(&self) -> anyhow::Result<()> {
        EditWorkTimelineSemantics::from_schema_version(self.schema_version).map(|_| ())
    }

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
/// `POST /api/edit` — apply trim/crop/scale/mute/speed to a previously imported
/// video and return a job id to poll for the rendered result.
pub async fn edit_handler(
    State(state): State<AppState>,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(mut req): ApiJson<EditRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    canonicalize_export_request(&mut req);
    validate_color_grade_request(&req)
        .map_err(|_| AppError::bad_request(crate::messages::BAD_COLOR_PARAMS))?;
    validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &req)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
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
            return Err(AppError::too_many_requests(crate::messages::TOO_MANY_JOBS));
        }
    };
    dispatch_job(&state, &resolved_id).await;
    Ok((StatusCode::ACCEPTED, Json(json!({ "jobId": resolved_id }))))
}

pub(crate) fn spawn_edit_job(
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

        let prepared = match prepare_edit_execution(&st, &req, timeline_semantics, &out_id).await {
            Ok(prepared) => prepared,
            Err(error) => {
                finish_job(&st, &jid, Err(error), "output").await;
                return;
            }
        };
        let runtime_fingerprint = render_runtime_fingerprint(st.tools.as_ref());
        let cache_key = plan_render_cache_key(
            timeline_semantics.pipeline_version(),
            &prepared.plan.plan_fingerprint,
            &runtime_fingerprint,
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

        let Some(guards) = JobService::acquire_execution(
            &st,
            &jid,
            &token,
            None,
            AcquirePlan {
                resource_class: ResourceClass::Export,
                acquire_render_permit: true,
            },
        )
        .await
        else {
            return;
        };

        let PreparedEditExecution {
            input,
            plan,
            requested_format,
            filename,
            output_path,
            staging_output,
        } = prepared;
        let plan = Arc::new(plan);
        let cache_key_for_put = cache_key.clone();
        let filename_for_put = filename.clone();
        let updated = JobService::run_acquired(
            &st,
            &jid,
            &token,
            attempt,
            "processing",
            "output",
            guards,
            |ctx| {
                let st = st.clone();
                let filename = filename.clone();
                async move {
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
                        &ctx.progress,
                        &ctx.cancel,
                        st.job_timeout(),
                    )
                    .instrument(tracing::info_span!("process", process.tool = "ffmpeg"))
                    .await?;
                    if matches!(done, Done::Cancelled) {
                        let _ = tokio::fs::remove_file(&staging_output).await;
                        return Ok::<Option<Value>, anyhow::Error>(None);
                    }
                    let output_probe =
                        tools::probe_video(&st.process_runtime, &staging_output).await?;
                    let conformance = render_conformance_spec(
                        requested_format,
                        command.expected_duration_seconds,
                        output_probe.duration,
                    );
                    validate_conformance(&output_probe, &conformance).map_err(|reason| {
                        anyhow::anyhow!("post-mux conformance failed: {reason}")
                    })?;
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
            },
        )
        .await;
        if updated {
            if let Ok(Some(job)) = st.get_job(&jid).await {
                if let Some(info) = job.result {
                    if let Err(error) = st
                        .db
                        .cache_put(&cache_key_for_put, &info, &filename_for_put)
                        .await
                    {
                        tracing::warn!("cache successful render {cache_key_for_put}: {error}");
                    }
                }
            }
        }
    }
    .instrument(span);
    state.spawn_task(task);
}

struct PreparedEditExecution {
    input: std::path::PathBuf,
    plan: EditPlan,
    requested_format: OutputFormat,
    filename: String,
    output_path: std::path::PathBuf,
    staging_output: std::path::PathBuf,
}

async fn prepare_edit_execution(
    st: &AppState,
    req: &EditRequest,
    timeline_semantics: EditWorkTimelineSemantics,
    out_id: &str,
) -> anyhow::Result<PreparedEditExecution> {
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
    let plan = timeline_semantics.compile_plan(source_fingerprint, req.clone(), source)?;
    let requested_format = OutputFormat::parse(req.format.as_deref()).unwrap_or(OutputFormat::Mp4);
    let ext = tools::output_ext(requested_format);
    let filename = format!("{out_id}.{ext}");
    let output_path = st.outputs_dir().join(&filename);
    let staging_output = st.staging_dir().join(format!("{out_id}.render.{ext}"));
    Ok(PreparedEditExecution {
        input,
        plan,
        requested_format,
        filename,
        output_path,
        staging_output,
    })
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
    Uuid::parse_str(&selection.id).map_err(|_| anyhow::anyhow!(crate::messages::LUT_NOT_FOUND))?;
    let asset = state
        .db
        .get_lut(&selection.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!(crate::messages::LUT_NOT_FOUND))?;
    anyhow::ensure!(asset.id == selection.id, crate::messages::LUT_BAD_LINK);
    let expected_filename = format!("{}.cube", asset.id);
    anyhow::ensure!(
        asset.filename == expected_filename,
        crate::messages::LUT_BAD_FILE_LINK
    );
    let luts_dir = tokio::fs::canonicalize(state.luts_dir())
        .await
        .map_err(|_| anyhow::anyhow!(crate::messages::LUT_STORE_UNAVAILABLE))?;
    let path = state.luts_dir().join(&asset.filename);
    let metadata = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| anyhow::anyhow!(crate::messages::LUT_FILE_MISSING))?;
    anyhow::ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        crate::messages::LUT_FILE_BAD_TYPE
    );
    anyhow::ensure!(
        metadata.len() <= MAX_LUT_FILE_BYTES as u64 && metadata.len() == asset.size_bytes,
        crate::messages::LUT_FILE_CORRUPT
    );
    let canonical_path = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| anyhow::anyhow!(crate::messages::LUT_FILE_MISSING))?;
    anyhow::ensure!(
        canonical_path.parent() == Some(luts_dir.as_path()),
        crate::messages::LUT_FILE_OUTSIDE_STORE
    );

    let file = tokio::fs::File::open(&canonical_path)
        .await
        .map_err(|_| anyhow::anyhow!(crate::messages::LUT_FILE_MISSING))?;
    let mut reader = file.take(MAX_LUT_FILE_BYTES as u64 + 1);
    let mut bytes = Vec::with_capacity(asset.size_bytes as usize);
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| anyhow::anyhow!(crate::messages::LUT_FILE_VERIFY_FAILED))?;
    anyhow::ensure!(
        bytes.len() as u64 == asset.size_bytes && bytes.len() <= MAX_LUT_FILE_BYTES,
        crate::messages::LUT_FILE_CORRUPT
    );
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    anyhow::ensure!(
        actual_sha256 == asset.sha256,
        crate::messages::LUT_FILE_CORRUPT
    );
    Ok(RenderResources::with_verified_lut(
        canonical_path,
        actual_sha256,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::library::Library;
    use crate::services::render::validate_color_grade_capabilities_for_tools;
    use crate::state::ToolInfo;
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

        assert!(validate_color_grade_capabilities_for_tools(st.tools.as_ref(), &request).is_err());
        st.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["chromakey".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities_for_tools(st.tools.as_ref(), &request).is_err());
        st.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["chromakey".into(), "despill".into()],
            ..ToolInfo::default()
        });
        assert!(validate_color_grade_capabilities_for_tools(st.tools.as_ref(), &request).is_ok());

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
        assert!(
            validate_color_grade_capabilities_for_tools(st.tools.as_ref(), &without_spill).is_ok()
        );
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

        assert!(
            validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &request).is_err()
        );
        state.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["huesaturation".into()],
            ..ToolInfo::default()
        });
        assert!(
            validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &request).is_err()
        );
        state.tools = Arc::new(ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["huesaturation".into(), "colorbalance".into()],
            ..ToolInfo::default()
        });
        assert!(
            validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &request).is_ok()
        );
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

        assert!(
            validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &request).is_err()
        );
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
        assert!(
            validate_color_grade_capabilities_for_tools(state.tools.as_ref(), &request).is_ok()
        );
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
}
