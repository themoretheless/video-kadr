//! Durable multi-source composition render endpoint and worker.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::artifacts::fingerprint_file;
use crate::config::resource_classes::ResourceClass;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::composition::{
    Composition, CompositionClipId, CompositionSource, CompositionTrack, SourceId, SourceKind,
};
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, EnqueueOutcome, JobKind};
use crate::ports::{
    CompositionAudioCodec, CompositionAv1Encoder, CompositionExportCommandCompiler,
    CompositionExportCompileRequest, CompositionExportProfile, CompositionExportSpec,
    CompositionMp4Codec, CompositionTextResource, CompositionWebmCodec,
};
use crate::services::composition::{
    source_requirements, CompositionOutputRequest, CompositionPlan, CompositionRenderRequest,
    SourceRequirement, TrustedCompositionSource, COMPOSITION_RENDER_SCHEMA_VERSION,
};
use crate::state::AppState;
use crate::tools::{self, Done, FfmpegCompositionExportCompiler};

use super::jobs::{dispatch_job, JobLeaseHeartbeat};
use super::{
    acquire_job_permit_or_cancelled, acquire_render_lock_or_cancelled,
    acquire_render_permit_or_cancelled, finish_from_render_cache, finish_job, mark_cancelled,
    mark_queued, mark_running, render_runtime_fingerprint, spawn_progress_drain,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CompositionWork {
    pub schema_version: u32,
    pub request: CompositionRenderRequest,
    pub output_id: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub trace_context: crate::telemetry::context::TraceContext,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompositionDedupeIdentity<'a> {
    owner: Option<&'a str>,
    pipeline_version: &'static str,
    runtime_fingerprint: &'a str,
    request: &'a CompositionRenderRequest,
}

pub async fn composition_render_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    trace: Option<Extension<crate::telemetry::context::TraceContext>>,
    ApiJson(request): ApiJson<CompositionRenderRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = optional_render_actor(&state, &headers).await?;
    if request.schema_version != COMPOSITION_RENDER_SCHEMA_VERSION {
        return Err(AppError::bad_request(
            "неподдерживаемая версия composition render",
        ));
    }
    request
        .composition
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    validate_composition_capabilities(state.tools.as_ref(), &request.composition)?;
    let effective_duration_seconds = request
        .output
        .effective_duration_seconds(&request.composition)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    resolve_composition_output(
        state.tools.as_ref(),
        request.output,
        effective_duration_seconds,
    )
    .map_err(AppError::bad_request)?;
    let requirements = source_requirements(&request.composition)
        .map_err(|error| AppError::bad_request(error.to_string()))?;

    for id in requirements.keys() {
        super::library::authorize_source_access(&state, id.as_str(), &headers).await?;
    }

    let runtime_fingerprint = render_runtime_fingerprint(state.tools.as_ref());
    let key = dedupe_key(
        "composition",
        &CompositionDedupeIdentity {
            owner: owner.as_deref(),
            pipeline_version: "composition-render-v15-audio-format-run",
            runtime_fingerprint: &runtime_fingerprint,
            request: &request,
        },
    )
    .map_err(|error| AppError::internal("build composition dedupe key", error))?;
    let job_id = Uuid::new_v4().to_string();
    let work = CompositionWork {
        schema_version: COMPOSITION_RENDER_SCHEMA_VERSION,
        request,
        output_id: Uuid::new_v4().to_string(),
        owner,
        trace_context: trace.map(|Extension(value)| value).unwrap_or_default(),
    };
    let payload = serde_json::to_value(&work)
        .map_err(|error| AppError::internal("serialize composition job", error))?;
    let resolved_id = match state
        .enqueue_job(job_id.clone(), JobKind::Composition, &payload, &key)
        .await
        .map_err(|error| AppError::internal("enqueue composition job", error))?
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

fn validate_composition_capabilities(
    tools: &crate::state::ToolInfo,
    composition: &Composition,
) -> AppResult<()> {
    let mut required = Vec::new();
    if composition.tracks.iter().any(|track| matches!(track,
        CompositionTrack::Color { hidden: false, clips, .. }
        if clips.iter().any(|clip| clip.enabled && clip.color_animation.is_some()))) {
        required.extend(["sendcmd", "colorchannelmixer", "overlay"]);
    }
    if composition.tracks.iter().any(|track| matches!(track,
        CompositionTrack::Text { hidden: false, clips, .. }
        if clips.iter().any(|clip| clip.enabled && clip.color_animation.is_some()))) {
        required.extend(["drawtext", "sendcmd", "colorchannelmixer", "overlay"]);
    }
    if composition.requires_canvas_blur() {
        required.push("gblur");
    }
    required.extend(composition.required_style_effect_filters());
    if composition.requires_optical_flow() {
        required.extend(["minterpolate", "tpad"]);
    }
    if composition.requires_reverse_video() {
        required.push("reverse");
    }
    if composition.requires_reverse_audio() {
        required.push("areverse");
    }
    if composition.requires_freeze_frame() {
        required.push("tpad");
    }
    if composition.requires_deshake_stabilization() {
        required.push("deshake");
    }
    if composition.requires_vidstab_stabilization() {
        required.extend(["vidstabdetect", "vidstabtransform"]);
    }
    if composition.requires_speed_ramp() {
        required.extend(["setpts", "tpad", "fps", "trim"]);
    }
    if composition.requires_speed_ramp_pitch_audio() {
        required.extend(["atrim", "asetpts", "asplit", "atempo", "concat"]);
    }
    if composition.requires_audio_ducking() {
        required.extend(["asplit", "sidechaincompress"]);
    }
    if composition.requires_audio_echo() {
        required.extend(["aecho", "atrim", "asetpts"]);
    }
    if composition.requires_audio_robot() {
        required.extend(["tremolo", "highpass", "lowpass"]);
    }
    if composition.requires_audio_tone() {
        required.extend(["bass", "treble"]);
    }
    required.sort_unstable();
    required.dedup();
    let missing = required
        .into_iter()
        .filter(|required| {
            !tools.ffmpeg
                || !tools
                    .ffmpeg_filters
                    .iter()
                    .any(|available| available == required)
        })
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(AppError::bad_request(format!(
            "composition playback недоступен: нужны FFmpeg filters {}",
            missing.join(", ")
        )));
    }
    Ok(())
}

fn resolve_composition_output(
    tools: &crate::state::ToolInfo,
    output: CompositionOutputRequest,
    effective_duration_seconds: f64,
) -> Result<CompositionExportSpec, String> {
    let has_encoder = |name: &str| {
        tools.ffmpeg
            && tools
                .ffmpeg_encoders
                .iter()
                .any(|available| available == name)
    };
    let has_muxer = |name: &str| {
        tools.ffmpeg
            && tools
                .ffmpeg_muxers
                .iter()
                .any(|available| available == name)
    };
    let mut missing = Vec::new();
    if !has_muxer(output.profile.muxer()) {
        missing.push(format!("muxer {}", output.profile.muxer()));
    }
    let mut av1_encoder = None;
    match output.profile {
        CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H264,
        } => {
            for encoder in ["libx264", "aac"] {
                if !has_encoder(encoder) {
                    missing.push(format!("encoder {encoder}"));
                }
            }
        }
        CompositionExportProfile::Mp4 {
            codec: CompositionMp4Codec::H265,
        } => {
            for encoder in ["libx265", "aac"] {
                if !has_encoder(encoder) {
                    missing.push(format!("encoder {encoder}"));
                }
            }
        }
        CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Vp9,
        } => {
            for encoder in ["libvpx-vp9", "libopus"] {
                if !has_encoder(encoder) {
                    missing.push(format!("encoder {encoder}"));
                }
            }
        }
        CompositionExportProfile::Webm {
            codec: CompositionWebmCodec::Av1,
        } => {
            if has_encoder("libsvtav1") {
                av1_encoder = Some(CompositionAv1Encoder::LibSvtAv1);
            } else if has_encoder("libaom-av1") {
                av1_encoder = Some(CompositionAv1Encoder::LibAomAv1);
            } else {
                missing.push("encoder libsvtav1 or libaom-av1".to_owned());
            }
            if !has_encoder("libopus") {
                missing.push("encoder libopus".to_owned());
            }
        }
        CompositionExportProfile::Mov { .. } => {
            for encoder in ["prores_ks", "pcm_s16le"] {
                if !has_encoder(encoder) {
                    missing.push(format!("encoder {encoder}"));
                }
            }
        }
        CompositionExportProfile::Audio { codec } => {
            let encoder = match codec {
                CompositionAudioCodec::Mp3 => "libmp3lame",
                CompositionAudioCodec::Wav => "pcm_s16le",
                CompositionAudioCodec::Aac => "aac",
                CompositionAudioCodec::Flac => "flac",
            };
            if !has_encoder(encoder) {
                missing.push(format!("encoder {encoder}"));
            }
        }
    }
    if missing.is_empty() {
        output
            .export_spec(av1_encoder, effective_duration_seconds)
            .map_err(|error| error.to_string())
    } else {
        Err(format!(
            "composition delivery {} недоступен: нужны {}",
            output.profile.id(),
            missing.join(", ")
        ))
    }
}

pub(crate) fn spawn_composition_job(
    state: AppState,
    job_id: String,
    work: CompositionWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let span = work.trace_context.job_span(&job_id, "composition");
    let task = async move {
        let _lease = lease;
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        // Give the request/polling task a fairness boundary after publishing
        // the durable queued state before starting filesystem source lookup.
        tokio::task::yield_now().await;

        let target_size_bytes = work.request.output.target_size_bytes;
        let effective_duration_seconds = match work
            .request
            .output
            .effective_duration_seconds(&work.request.composition)
        {
            Ok(seconds) => seconds,
            Err(error) => {
                finish_job(&st, &jid, Err(error), "output").await;
                return;
            }
        };
        let output = match resolve_composition_output(
            st.tools.as_ref(),
            work.request.output,
            effective_duration_seconds,
        ) {
            Ok(output) => output,
            Err(reason) => {
                finish_job(&st, &jid, Err(anyhow::anyhow!(reason)), "output").await;
                return;
            }
        };

        let (inputs, trusted) = match resolve_sources(&st, &work.request, &token).await {
            Ok(value) => value,
            Err(error) => {
                finish_job(&st, &jid, Err(error), "output").await;
                return;
            }
        };
        if token.is_cancelled() {
            mark_cancelled(&st, &jid).await;
            return;
        }
        let plan = match CompositionPlan::compile(work.request, trusted) {
            Ok(plan) => plan,
            Err(error) => {
                finish_job(&st, &jid, Err(error), "output").await;
                return;
            }
        };
        let cache_key = composition_cache_key(
            plan.fingerprint(),
            &render_runtime_fingerprint(st.tools.as_ref()),
        );
        let render_lock = st.render_lock(&cache_key).await;
        let _render_guard =
            match acquire_render_lock_or_cancelled(&st, &jid, &token, render_lock).await {
                Some(guard) => guard,
                None => return,
            };
        if finish_from_render_cache(&st, &jid, &cache_key, work.owner.as_deref()).await {
            return;
        }
        let _render_permit = match acquire_render_permit_or_cancelled(&st, &jid, &token).await {
            Some(permit) => permit,
            None => return,
        };
        let _permit =
            match acquire_job_permit_or_cancelled(&st, &jid, &token, ResourceClass::Export).await {
                Some(permit) => permit,
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
        let filename = composition_output_filename(&work.output_id, output.profile);
        let output_path = st.outputs_dir().join(&filename);
        let outcome = async {
            let native = tools::composition_native_audio::needs_native_audio(plan.composition()) || crate::domain::transition_export::sparse_primary_video_track(plan.composition()).is_some();
            let native_video = !output.profile.is_audio_only() && tools::composition_native_video::needs_native_video(plan.composition());
            let native_pipeline = native || native_video;
            let visual_phase = super::nested_export::ProgressLane::new(&tx, 10.0, 0.3);
            let preparation_phase = super::nested_export::ProgressLane::new(&tx, 0.0, if native_pipeline { 0.1 } else { 1.0 });
            let mix_phase = super::nested_export::ProgressLane::new(&tx, if native_video {40.0} else {10.0}, 0.3);
            let render_phase = super::nested_export::ProgressLane::new(&tx, if native_video {70.0} else {40.0}, if native_video {0.3} else {0.6});
            let text_resources =
                PreparedTextResources::create(&st.staging_dir(), plan.composition()).await?;
            let vidstab = PreparedVidstabTransforms::create(
                &st,
                &st.staging_dir(),
                plan.composition(),
                &inputs,
                if native_pipeline { &preparation_phase.sender } else { &tx },
                &token,
            )
            .await?;
            if token.is_cancelled() {
                let _ = tokio::fs::remove_file(&output_path).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let deadline = std::time::Instant::now().checked_add(st.job_timeout()).context("composition native deadline overflow")?;
            let remaining = || deadline.checked_duration_since(std::time::Instant::now()).context("composition native export timeout");
            let mut audio_only_admission = plan.composition().clone();
            if output.profile.is_audio_only() {
                for track in &mut audio_only_admission.tracks {
                    if let CompositionTrack::Image { clips, .. } = track {
                        for clip in clips { clip.effects.clear(); }
                    }
                    if let CompositionTrack::Video { clips, .. } = track {
                        for clip in clips { clip.effects.clear(); }
                    }
                }
            }
            let compile = |pcm: Option<&Path>, movie: Option<&Path>| {
                let request = CompositionExportCompileRequest {
                    inputs: &inputs,
                    text_resources: text_resources.resources(),
                    color_commands: &text_resources.solid_commands,
                    vidstab_transforms: vidstab.transforms(),
                    destination: &output_path,
                    parallel_jobs: st.render_parallelism(),
                    composition: if output.profile.is_audio_only() {&audio_only_admission} else {plan.composition()},
                    output,
                };
                if let Some(movie) = movie { tools::composition_native_video::compile_with_native_video(request, movie, pcm) }
                else if let Some(pcm) = pcm { tools::composition_native_audio::compile_with_native_audio(request, pcm) }
                else { FfmpegCompositionExportCompiler.compile(request) }
            };
            let admission_movie = st.staging_dir().join("native-video-admission.mkv");
            let admission_pcm = st.staging_dir().join("native-audio-admission.f32le");
            let preliminary = compile(if native {Some(admission_pcm.as_path())} else {None}, if native_video {Some(admission_movie.as_path())} else {None})?;
            let range = output.range.map(|range| range.start_ticks..range.end_ticks)
                .unwrap_or(0..(preliminary.expected_duration_seconds * 1000000.0).round() as u64);
            let movie = if native_video {
                Some(tools::composition_native_video::prepare_native_video(&st.process_runtime, &st.staging_dir(), plan.composition(), &inputs, range.clone(), remaining()?, &token, &visual_phase.sender).await?)
            } else {None};
            let mixed = if native {
                ensure!(plan.composition().time_base == 1000000, "native DSP export requires microsecond composition time base");
                Some(tools::composition_native_audio::prepare_native_audio(&st.process_runtime, &st.staging_dir(), plan.composition(), &inputs, range, remaining()?, &token, &mix_phase.sender).await?)
            } else {None};
            let command = compile(mixed.as_ref().map(|pcm|pcm.path()), movie.as_ref().map(|movie|movie.path()))?;
            let done = tools::run_compiled_ffmpeg(
                &st.process_runtime,
                &command,
                if native_pipeline { &render_phase.sender } else { &tx },
                &token,
                if native_pipeline { remaining()? } else { st.job_timeout() },
            )
            .instrument(tracing::info_span!("process", process.tool = "ffmpeg"))
            .await?;
            if matches!(done, Done::Cancelled) {
                let _ = tokio::fs::remove_file(&output_path).await;
                return Ok::<Option<Value>, anyhow::Error>(None);
            }
            let size = tokio::fs::metadata(&output_path)
                .await
                .map(|metadata| metadata.len())
                .ok();
            Ok(Some(composition_result_info(
                &work.output_id,
                &filename,
                output.profile,
                size,
                target_size_bytes,
            )))
        }
        .await;
        if outcome.is_err() {
            let _ = tokio::fs::remove_file(&output_path).await;
        }
        let cache_info = outcome.as_ref().ok().and_then(Clone::clone);
        drop(tx);
        let _ = drain.await;
        let updated = finish_job(&st, &jid, outcome, "output").await;
        if updated {
            if let Some(info) = cache_info {
                if let (Some(owner), Some(output_id)) = (work.owner.as_deref(), info["id"].as_str())
                {
                    if let Err(error) = st.db.grant_output_access(output_id, owner).await {
                        tracing::error!(%error, %output_id, "grant composition output access");
                    }
                }
                if let Err(error) = st.db.cache_put(&cache_key, &info, &filename).await {
                    tracing::warn!(%error, "cache composition render");
                }
            }
        }
    }
    .instrument(span);
    state.spawn_task(task);
}

async fn optional_render_actor(state: &AppState, headers: &HeaderMap) -> AppResult<Option<String>> {
    let Some(token) = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    state
        .db
        .resolve_auth_session(token)
        .await
        .map_err(|error| AppError::internal("authenticate composition render", error))?
        .map(|user| Some(user.username))
        .ok_or_else(|| AppError::unauthorized("Сессия недействительна или истекла"))
}

async fn resolve_sources(
    state: &AppState,
    request: &CompositionRenderRequest,
    token: &CancellationToken,
) -> anyhow::Result<(
    BTreeMap<SourceId, std::path::PathBuf>,
    BTreeMap<SourceId, TrustedCompositionSource>,
)> {
    let requirements = source_requirements(&request.composition)?;
    let mut paths = BTreeMap::new();
    let mut trusted = BTreeMap::new();
    for (id, requirement) in requirements {
        ensure!(
            !token.is_cancelled(),
            "composition source resolution cancelled"
        );
        let entry = state
            .library
            .get(id.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .with_context(|| format!("источник {} не найден", id.as_str()))?;
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .with_context(|| format!("источник {} не найден", id.as_str()))?;
        let identity = fingerprint_file(&state.cpu_pool, path.clone(), token.clone()).await?;
        let probe = tools::probe_video(&state.process_runtime, &path).await?;
        let has_video = probe.vcodec.is_some() && probe.width > 0 && probe.height > 0;
        let has_audio = probe.streams.iter().any(|stream|stream.kind==crate::domain::media_probe::StreamKind::Audio);
        let kind = match requirement {
            SourceRequirement::Video => {
                ensure!(
                    has_video && probe.duration > 0.0,
                    "источник не содержит видео"
                );
                SourceKind::Video
            }
            SourceRequirement::Image => {
                ensure!(has_video && !has_audio, "источник не является изображением");
                SourceKind::Image
            }
            SourceRequirement::Audio => {
                ensure!(
                    has_audio && probe.duration > 0.0,
                    "источник не содержит аудио"
                );
                if has_video {
                    SourceKind::Video
                } else {
                    SourceKind::Audio
                }
            }
        };
        let duration_ticks = duration_ticks(probe.duration, request.composition.time_base, kind)?;
        let source = CompositionSource {
            audio_channels: probe
                .streams
                .iter()
                .find(|stream| stream.kind == crate::domain::media_probe::StreamKind::Audio)
                .and_then(|stream| stream.channels),
            id: id.clone(),
            kind,
            duration_ticks,
            width: probe.width,
            height: probe.height,
            has_audio,
        };
        paths.insert(id.clone(), path);
        trusted.insert(
            id,
            TrustedCompositionSource {
                source,
                fingerprint: identity.sha256,
            },
        );
    }
    Ok((paths, trusted))
}

struct PreparedTextResources {
    solid_commands: BTreeMap<CompositionClipId, PathBuf>,
    resources: BTreeMap<CompositionClipId, CompositionTextResource>,
    temporary_files: Vec<PathBuf>,
    color_files: Vec<PrivateColorCommandFile>,
}

struct PrivateColorCommandFile(PathBuf);
impl Drop for PrivateColorCommandFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl PreparedTextResources {
    async fn create(staging: &Path, composition: &Composition) -> anyhow::Result<Self> {
        let mut prepared = Self {
            solid_commands: BTreeMap::new(),
            resources: BTreeMap::new(),
            temporary_files: Vec::new(),
            color_files: Vec::new(),
        };
        let mut fonts = BTreeMap::<String, PathBuf>::new();
        for track in &composition.tracks {
            if let CompositionTrack::Color { hidden: false, clips, .. } = track {
                for clip in clips.iter().filter(|clip| clip.enabled && clip.color_animation.is_some()) {
                    use crate::domain::animated_color::{ColorExportFrames, ColorSampleRequest};
                    let rate = composition.canvas.frame_rate();
                    let duration_us = u64::try_from((u128::from(clip.timeline_end_tick - clip.timeline_start_tick) * 1_000_000).div_ceil(u128::from(composition.time_base)))?;
                    let frames = ColorExportFrames::new(ColorSampleRequest {
                        track: clip.color_animation.clone().expect("filtered animated solid"),
                        default: clip.color, times_us: Vec::new(),
                    }, rate.numerator, rate.denominator, duration_us).map_err(anyhow::Error::msg)?;
                    let path = staging.join(format!("composition-color-{}.txt", Uuid::new_v4()));
                    let target = crate::domain::animated_color::color_command_target(clip.id.as_str());
                    let owned = tokio::task::spawn_blocking(move || -> anyhow::Result<PrivateColorCommandFile> {
                        let mut options = std::fs::OpenOptions::new(); options.write(true).create_new(true);
                        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
                        let mut file = options.open(&path)?;
                        let owned = PrivateColorCommandFile(path);
                        let mut writer = std::io::BufWriter::new(&mut file);
                        frames.write_solid_commands(&mut writer, &target).map_err(anyhow::Error::msg)?;
                        std::io::Write::flush(&mut writer)?;
                        drop(writer); file.sync_all()?;
                        Ok(owned)
                    }).await.context("solid color command worker")??;
                    prepared.solid_commands.insert(clip.id.clone(), owned.0.clone());
                    prepared.color_files.push(owned);
                }
            }
            let CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } = track
            else {
                continue;
            };
            for clip in clips.iter().filter(|clip| clip.enabled) {
                let font_file = match fonts.get(&clip.style.font_family) {
                    Some(path) => path.clone(),
                    None => {
                        let path = resolve_composition_font(&clip.style.font_family)?;
                        fonts.insert(clip.style.font_family.clone(), path.clone());
                        path
                    }
                };
                let text_file = staging.join(format!(
                    "composition-text-{}-{}.txt",
                    clip.id.as_str(),
                    Uuid::new_v4()
                ));
                let mut options = tokio::fs::OpenOptions::new();
                options.write(true).create_new(true);
                let mut file = options
                    .open(&text_file)
                    .await
                    .context("create private composition text resource")?;
                prepared.temporary_files.push(text_file.clone());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    tokio::fs::set_permissions(&text_file, std::fs::Permissions::from_mode(0o600))
                        .await
                        .context("protect composition text resource")?;
                }
                use tokio::io::AsyncWriteExt as _;
                file.write_all(clip.text.as_bytes())
                    .await
                    .context("write composition text resource")?;
                file.sync_all()
                    .await
                    .context("sync composition text resource")?;
                let color_commands_file = if let Some(track) = &clip.color_animation {
                    use crate::domain::animated_color::{Color, ColorExportFrames, ColorSampleRequest};
                    let duration = clip.timeline_end_tick.checked_sub(clip.timeline_start_tick)
                        .context("invalid color clip duration")?;
                    let duration_us = u64::try_from((u128::from(duration) * 1_000_000)
                        .div_ceil(u128::from(composition.time_base)))?;
                    let rate = composition.canvas.frame_rate();
                    let frames = ColorExportFrames::new(ColorSampleRequest {
                        track: track.clone(), default: Color { r: 0, g: 0, b: 0, a: 255 }, times_us: Vec::new(),
                    }, rate.numerator, rate.denominator, duration_us).map_err(anyhow::Error::msg)?;
                    let path = staging.join(format!("composition-color-{}.txt", Uuid::new_v4()));
                    let target = crate::domain::animated_color::color_command_target(clip.id.as_str());
                    // The worker owns its cleanup guard, including when its awaiting
                    // future is cancelled before the worker finishes.
                    let owned = tokio::task::spawn_blocking(move || -> anyhow::Result<PrivateColorCommandFile> {
                        let mut options = std::fs::OpenOptions::new();
                        options.write(true).create_new(true);
                        #[cfg(unix)] {
                            use std::os::unix::fs::OpenOptionsExt;
                            options.mode(0o600);
                        }
                        let mut file = options.open(&path)?;
                        let owned = PrivateColorCommandFile(path);
                        #[cfg(unix)] {
                            use std::os::unix::fs::PermissionsExt;
                            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
                        }
                        let mut writer = std::io::BufWriter::new(&mut file);
                        frames.write_drawtext_commands(&mut writer, &target).map_err(anyhow::Error::msg)?;
                        std::io::Write::flush(&mut writer)?;
                        drop(writer);
                        file.sync_all()?;
                        Ok(owned)
                    }).await.context("color command worker")??;
                    let path = owned.0.clone();
                    prepared.color_files.push(owned);
                    Some(path)
                } else { None };
                prepared.resources.insert(
                    clip.id.clone(),
                    CompositionTextResource {
                        text_file,
                        font_file,
                        color_commands_file,
                    },
                );
            }
        }
        Ok(prepared)
    }

    fn resources(&self) -> &BTreeMap<CompositionClipId, CompositionTextResource> {
        &self.resources
    }
}

impl Drop for PreparedTextResources {
    fn drop(&mut self) {
        for path in self.temporary_files.drain(..) {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct PreparedVidstabTransforms {
    transforms: BTreeMap<CompositionClipId, PathBuf>,
}

impl PreparedVidstabTransforms {
    async fn create(
        state: &AppState,
        staging: &Path,
        composition: &Composition,
        inputs: &BTreeMap<SourceId, PathBuf>,
        progress: &mpsc::UnboundedSender<f64>,
        token: &CancellationToken,
    ) -> anyhow::Result<Self> {
        let mut prepared = Self {
            transforms: BTreeMap::new(),
        };
        for track in &composition.tracks {
            let CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } = track
            else {
                continue;
            };
            for clip in clips
                .iter()
                .filter(|clip| clip.enabled && clip.stabilization.is_vidstab())
            {
                let input = inputs.get(&clip.source_id).ok_or_else(|| {
                    anyhow::anyhow!("vidstab source missing for clip {}", clip.id.as_str())
                })?;
                let transform_path = staging.join(format!(
                    "composition-vidstab-{}-{}.trf",
                    clip.id.as_str(),
                    Uuid::new_v4()
                ));
                let command = tools::build_vidstab_detect_command(
                    input,
                    &transform_path,
                    clip,
                    composition.canvas.frame_rate(),
                    composition.time_base,
                )?;
                let done = tools::run_compiled_ffmpeg(
                    &state.process_runtime,
                    &command,
                    progress,
                    token,
                    state.job_timeout(),
                )
                .instrument(tracing::info_span!(
                    "process",
                    process.tool = "ffmpeg",
                    process.stage = "vidstabdetect"
                ))
                .await?;
                if matches!(done, Done::Cancelled) {
                    let _ = tokio::fs::remove_file(&transform_path).await;
                    return Ok(prepared);
                }
                ensure!(
                    transform_path.is_file(),
                    "vidstab detect did not write {}",
                    transform_path.display()
                );
                prepared.transforms.insert(clip.id.clone(), transform_path);
            }
        }
        Ok(prepared)
    }

    fn transforms(&self) -> &BTreeMap<CompositionClipId, PathBuf> {
        &self.transforms
    }
}

impl Drop for PreparedVidstabTransforms {
    fn drop(&mut self) {
        for path in self.transforms.values() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn resolve_composition_font(family: &str) -> anyhow::Result<PathBuf> {
    let candidates: &[&str] = match family {
        // The product's canonical cross-platform sans profile. Exact Noto is
        // preferred; deterministic Unicode-capable fallbacks keep local macOS
        // and minimal Linux packages usable without accepting arbitrary paths.
        "Noto Sans" => &[
            "/Library/Fonts/NotoSans-Regular.ttf",
            "/System/Library/Fonts/Supplemental/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/opentype/noto/NotoSans-Regular.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ],
        "Arial Unicode MS" => &["/System/Library/Fonts/Supplemental/Arial Unicode.ttf"],
        "DejaVu Sans" => &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"],
        "Arial" => &[
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
            "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        ],
        _ => bail!("неподдерживаемое семейство шрифта"),
    };
    for candidate in candidates {
        let path = Path::new(candidate);
        if !path.is_file() {
            continue;
        }
        let canonical = std::fs::canonicalize(path).context("resolve composition font")?;
        let metadata = std::fs::metadata(&canonical).context("inspect composition font")?;
        let supported_extension = canonical
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc"
                )
            });
        ensure!(
            canonical.is_absolute() && metadata.is_file() && supported_extension,
            "resolved composition font is not a supported regular font file"
        );
        return Ok(canonical);
    }
    bail!("разрешённый шрифт {family} не установлен")
}

fn duration_ticks(duration: f64, time_base: u32, kind: SourceKind) -> anyhow::Result<u64> {
    if kind == SourceKind::Image {
        return Ok(0);
    }
    ensure!(
        duration.is_finite() && duration > 0.0,
        "некорректная длительность source"
    );
    let ticks = duration * f64::from(time_base);
    ensure!(
        ticks.is_finite() && ticks >= 1.0 && ticks <= u64::MAX as f64,
        "source слишком длинный"
    );
    Ok(ticks.round() as u64)
}

fn composition_cache_key(plan: &Fingerprint, runtime: &str) -> String {
    Fingerprint::combine([
        b"composition-render-cache-v13-audio-format-run".as_slice(),
        plan.as_str().as_bytes(),
        runtime.as_bytes(),
    ])
    .to_string()
}

fn composition_output_filename(output_id: &str, profile: CompositionExportProfile) -> String {
    format!("{output_id}.{}", profile.extension())
}

fn composition_result_info(
    output_id: &str,
    filename: &str,
    profile: CompositionExportProfile,
    size: Option<u64>,
    target_size_bytes: Option<u64>,
) -> Value {
    let mut info = json!({
        "id": output_id,
        "url": format!("/files/outputs/{filename}"),
        "filename": filename,
        "mediaType": if profile.is_audio_only() { "audio" } else { "video" },
        "container": profile.extension(),
        "videoCodec": profile.video_codec(),
        "audioCodec": profile.audio_codec(),
        "sizeBytes": size,
    });
    // A target size is an estimate, not a guarantee: single-pass rate control
    // can miss by double digits depending on codec. Report the miss instead of
    // hiding it so the caller can decide whether to re-encode.
    if let Some(target) = target_size_bytes {
        let deviation = size.map(|actual| (actual as f64 - target as f64) / target as f64 * 100.0);
        info["targetSizeBytes"] = json!(target);
        info["sizeDeviationPercent"] =
            json!(deviation.map(|value| (value * 100.0).round() / 100.0));
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{
        AnimatableValue, BlendMode, CanvasBackgroundMode, CanvasSpec, ClipPlacement,
        CompositionClipId, CompositionSource, FrameInterpolation, PlaybackMode, Rgba,
        SpeedRampAudioPolicy, SpeedRampInterpolation, SpeedRampPoint, SpeedRampSpec,
        StabilizationSpec, TextClip, TextStyle, TrackId, TransformSpec, VideoClip, VideoEffect,
        VideoEffectPreset,
    };
    use axum::http::StatusCode;

    #[test]
    fn cache_identity_binds_plan_and_runtime() {
        let plan = Fingerprint::digest(b"plan-a");
        assert_eq!(
            composition_cache_key(&plan, "runtime-a"),
            composition_cache_key(&plan, "runtime-a")
        );
        assert_ne!(
            composition_cache_key(&plan, "runtime-a"),
            composition_cache_key(&plan, "runtime-b")
        );
    }

    #[test]
    fn delivery_capability_resolution_is_exact_and_av1_preference_is_stable() {
        let base = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libx264".into(), "aac".into()],
            ffmpeg_muxers: vec!["mp4".into()],
            ..crate::state::ToolInfo::default()
        };
        let default =
            resolve_composition_output(&base, CompositionOutputRequest::default(), 10.0).unwrap();
        assert_eq!(default.profile, CompositionExportProfile::default());
        assert_eq!(default.video_quality, 23);
        assert_eq!(default.av1_encoder, None);

        let h265 = CompositionOutputRequest {
            profile: CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H265,
            },
            quality_tier: crate::services::composition::CompositionQualityTier::Medium,
            video_bitrate_kbps: None,
            target_size_bytes: None,
            range: None,
        };
        assert_eq!(
            resolve_composition_output(&base, h265, 10.0).unwrap_err(),
            "composition delivery mp4/h265 недоступен: нужны encoder libx265"
        );

        let av1 = CompositionOutputRequest {
            profile: CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Av1,
            },
            quality_tier: crate::services::composition::CompositionQualityTier::High,
            video_bitrate_kbps: None,
            target_size_bytes: None,
            range: None,
        };
        assert_eq!(
            resolve_composition_output(&base, av1, 10.0).unwrap_err(),
            concat!(
                "composition delivery webm/av1 недоступен: нужны muxer webm, ",
                "encoder libsvtav1 or libaom-av1, encoder libopus"
            )
        );
        let both = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libaom-av1".into(), "libopus".into(), "libsvtav1".into()],
            ffmpeg_muxers: vec!["webm".into()],
            ..crate::state::ToolInfo::default()
        };
        assert_eq!(
            resolve_composition_output(&both, av1, 10.0)
                .unwrap()
                .av1_encoder,
            Some(CompositionAv1Encoder::LibSvtAv1)
        );
        let aom_only = crate::state::ToolInfo {
            ffmpeg_encoders: vec!["libaom-av1".into(), "libopus".into()],
            ..both
        };
        assert_eq!(
            resolve_composition_output(&aom_only, av1, 10.0)
                .unwrap()
                .av1_encoder,
            Some(CompositionAv1Encoder::LibAomAv1)
        );
        assert_eq!(
            composition_output_filename("result", av1.profile),
            "result.webm"
        );
        assert_eq!(
            composition_result_info("result", "result.webm", av1.profile, Some(42), None),
            json!({
                "id": "result",
                "url": "/files/outputs/result.webm",
                "filename": "result.webm",
                "mediaType": "video",
                "container": "webm",
                "videoCodec": "av1",
                "audioCodec": "opus",
                "sizeBytes": 42,
            })
        );
        let targeted =
            composition_result_info("result", "result.webm", av1.profile, Some(50), Some(40));
        assert_eq!(targeted["targetSizeBytes"], 40);
        assert_eq!(targeted["sizeDeviationPercent"], 25.0);
        let wav = CompositionOutputRequest {
            profile: CompositionExportProfile::Audio {
                codec: CompositionAudioCodec::Wav,
            },
            quality_tier: crate::services::composition::CompositionQualityTier::Medium,
            video_bitrate_kbps: None,
            target_size_bytes: None,
            range: Some(crate::ports::CompositionExportRange {
                start_ticks: 250_000,
                end_ticks: 750_000,
            }),
        };
        let wav_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["pcm_s16le".into()],
            ffmpeg_muxers: vec!["wav".into()],
            ..crate::state::ToolInfo::default()
        };
        let wav_spec = resolve_composition_output(&wav_tools, wav, 10.0).unwrap();
        assert_eq!(wav_spec.video_quality, 0);
        assert_eq!(wav_spec.range, wav.range);
        assert_eq!(
            composition_result_info("audio", "audio.wav", wav.profile, Some(84), None)["mediaType"],
            "audio"
        );
    }

    #[test]
    fn image_duration_is_not_trusted_from_probe() {
        assert_eq!(
            duration_ticks(0.0, 1_000_000, SourceKind::Image).unwrap(),
            0
        );
        assert!(duration_ticks(f64::NAN, 1_000_000, SourceKind::Video).is_err());
    }

    #[test]
    fn optional_composition_capabilities_are_checked_only_for_requests_that_use_them() {
        let source_id = SourceId::parse("motion-source").unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(
            source_id.clone(),
            CompositionSource {
                audio_channels: None,
                id: source_id.clone(),
                kind: SourceKind::Video,
                duration_ticks: 2_000_000,
                width: 640,
                height: 360,
                has_audio: false,
            },
        );
        let clip = VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            effect_timing: None,
            filmcraft_motion: None,
            id: CompositionClipId::parse("motion-clip").unwrap(),
            source_id: source_id.clone(),
            placement: ClipPlacement {
                concat_source_eof: false,
source_timeline_offset_tick: None, source_head_timeline_tick: None, concat_retime: None,
                timeline_duration_tick: None,
                timeline_start_tick: 0,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 0.5,
                speed_ramp: None,
            },
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
            effects: Vec::new(),
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
            enabled: true,
        };
        composition.tracks.push(CompositionTrack::Video {
            id: TrackId::parse("motion-track").unwrap(),
            name: "Motion".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        });
        composition.validate().unwrap();
        validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition)
            .unwrap();

        composition.canvas.background_mode = CanvasBackgroundMode::Blur;
        let unavailable =
            validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition)
                .unwrap_err();
        assert_eq!(unavailable.status(), StatusCode::BAD_REQUEST);
        let blur_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["gblur".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&blur_tools, &composition).unwrap();
        composition.canvas.background_mode = CanvasBackgroundMode::Color;

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].effects.push(VideoEffect::Style {
            preset: VideoEffectPreset::Edge,
            intensity: 0.5,
        });
        assert!(validate_composition_capabilities(
            &crate::state::ToolInfo::default(),
            &composition
        )
        .is_err());
        let effect_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["edgedetect".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&effect_tools, &composition).unwrap();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].effects.clear();
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        composition.validate().unwrap();
        let unavailable =
            validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition)
                .unwrap_err();
        assert_eq!(unavailable.status(), StatusCode::BAD_REQUEST);
        assert_eq!(unavailable.code(), "bad_request");

        let tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["minterpolate".into(), "tpad".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].frame_interpolation = FrameInterpolation::Duplicate;
        clips[0].playback_mode = PlaybackMode::Reverse;
        let unavailable =
            validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition)
                .unwrap_err();
        assert_eq!(unavailable.status(), StatusCode::BAD_REQUEST);
        let reverse_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["reverse".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&reverse_tools, &composition).unwrap();

        composition.sources.get_mut(&source_id).unwrap().has_audio = true;
        assert!(validate_composition_capabilities(&reverse_tools, &composition).is_err());
        let reverse_audio_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["reverse".into(), "areverse".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&reverse_audio_tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        assert!(validate_composition_capabilities(&reverse_audio_tools, &composition).is_err());
        let freeze_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["tpad".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&freeze_tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Forward;
        clips[0].stabilization = StabilizationSpec::Deshake {
            radius_x: 16,
            radius_y: 16,
        };
        assert!(validate_composition_capabilities(&freeze_tools, &composition).is_err());
        let stabilization_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["deshake".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&stabilization_tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].stabilization = StabilizationSpec::Vidstab {
            shakiness: 5,
            accuracy: 15,
            smoothing: 10,
        };
        assert!(validate_composition_capabilities(&stabilization_tools, &composition).is_err());
        let vidstab_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["vidstabdetect".into(), "vidstabtransform".into()],
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&vidstab_tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].stabilization = StabilizationSpec::Disabled;
        clips[0].placement.speed_ramp = Some(SpeedRampSpec {
            interpolation: SpeedRampInterpolation::Linear,
            points: vec![
                SpeedRampPoint {
                    source_progress_tick: 0,
                    speed: 0.5,
                },
                SpeedRampPoint {
                    source_progress_tick: 1_000_000,
                    speed: 1.0,
                },
            ],
            audio_policy: SpeedRampAudioPolicy::PreservePitch,
        });
        composition.validate().unwrap();
        let visual_ramp_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: ["setpts", "tpad", "fps", "trim"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ..crate::state::ToolInfo::default()
        };
        assert!(validate_composition_capabilities(&visual_ramp_tools, &composition).is_err());
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed_ramp.as_mut().unwrap().audio_policy = SpeedRampAudioPolicy::Mute;
        validate_composition_capabilities(&visual_ramp_tools, &composition).unwrap();

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].placement.speed_ramp.as_mut().unwrap().audio_policy =
            SpeedRampAudioPolicy::PreservePitch;
        let full_ramp_tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: [
                "setpts", "tpad", "fps", "trim", "atrim", "asetpts", "asplit", "atempo", "concat",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&full_ramp_tools, &composition).unwrap();
        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].enabled = false;
        validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition)
            .unwrap();
    }

    #[tokio::test]
    async fn text_resources_are_utf8_private_scoped_and_removed_on_drop() {
        assert!(resolve_composition_font("../../arbitrary.ttf").is_err());
        if resolve_composition_font("Noto Sans").is_err() {
            eprintln!("skipping text resource lifecycle: no allowlisted font installed");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        composition.tracks.push(CompositionTrack::Text {
            id: TrackId::parse("title-track").unwrap(),
            name: "Title".to_owned(),
            hidden: false,
            locked: false,
            clips: vec![TextClip {
                effects: vec![],
                blend_mode: None,
                concat_title: None,
                color_animation: None,
                id: CompositionClipId::parse("title-clip").unwrap(),
                timeline_start_tick: 0,
                timeline_end_tick: 1_000_000,
                text: "Привет\n字幕".to_owned(),
                style: TextStyle {
                    align: None,
                    font_family: "Noto Sans".to_owned(),
                    font_size: 48.0,
                    color: Rgba {
                        red: 1.0,
                        green: 1.0,
                        blue: 1.0,
                        alpha: 1.0,
                    },
                    background: Rgba::BLACK,
                    stroke: Rgba::BLACK,
                    stroke_width: 1.0,
                    shadow: Rgba::BLACK,
                    shadow_x: 2.0,
                    shadow_y: 2.0,
                },
                transform: TransformSpec::default(),
                opacity: AnimatableValue::constant(1.0),
                enabled: true,
            }],
        });

        let prepared = PreparedTextResources::create(directory.path(), &composition)
            .await
            .unwrap();
        let resource = &prepared.resources()[&CompositionClipId::parse("title-clip").unwrap()];
        let text_path = resource.text_file.clone();
        assert_eq!(
            tokio::fs::read_to_string(&text_path).await.unwrap(),
            "Привет\n字幕"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&text_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(resource.font_file.is_absolute() && resource.font_file.is_file());
        assert!(resource.color_commands_file.is_none());
        drop(prepared);
        assert!(!text_path.exists());

        let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] else { unreachable!() };
        clips[0].color_animation = Some(serde_json::from_value(serde_json::json!({
            "keys":[{"t_us":0,"value":{"r":255,"g":0,"b":0,"a":128},
                "in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}],
            "extrapolate":{"before":"Hold","after":"Hold"}
        })).unwrap());
        let prepared = PreparedTextResources::create(directory.path(), &composition).await.unwrap();
        assert!(validate_composition_capabilities(&crate::state::ToolInfo::default(), &composition).is_err());
        let tools = crate::state::ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: ["drawtext", "sendcmd", "colorchannelmixer", "overlay"].into_iter().map(str::to_owned).collect(),
            ..crate::state::ToolInfo::default()
        };
        validate_composition_capabilities(&tools, &composition).unwrap();
        let resource = &prepared.resources()[&CompositionClipId::parse("title-clip").unwrap()];
        let path = resource.color_commands_file.clone().unwrap();
        let commands = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(commands.lines().count(), 30);
        let target = crate::domain::animated_color::color_command_target("title-clip");
        assert!(commands.starts_with(&format!("0.000000 drawtext@{target} fontcolor 0xFF0000@1.000000000, colorchannelmixer@{target} aa 0.501960784;")));
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        drop(prepared);
        assert!(!path.exists());
    }
}
