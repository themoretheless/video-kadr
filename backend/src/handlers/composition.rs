//! Durable multi-source composition render endpoint and worker.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context};
use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::artifacts::fingerprint_file;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::composition::{
    Composition, CompositionClipId, CompositionSource, CompositionTrack, SourceId, SourceKind,
};
use crate::error::{ApiJson, AppError, AppResult};
use crate::jobs::{dedupe_key, EnqueueOutcome, JobKind};
use crate::ports::{
    CompositionAv1Encoder, CompositionExportCommandCompiler, CompositionExportCompileRequest,
    CompositionExportProfile, CompositionExportSpec, CompositionMp4Codec, CompositionTextResource,
    CompositionWebmCodec,
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
pub(super) struct CompositionWork {
    pub schema_version: u32,
    pub request: CompositionRenderRequest,
    pub output_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompositionDedupeIdentity<'a> {
    pipeline_version: &'static str,
    runtime_fingerprint: &'a str,
    request: &'a CompositionRenderRequest,
}

pub async fn composition_render_handler(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<CompositionRenderRequest>,
) -> AppResult<Json<Value>> {
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
    resolve_composition_output(state.tools.as_ref(), request.output)
        .map_err(AppError::bad_request)?;
    let requirements = source_requirements(&request.composition)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if requirements.is_empty() {
        return Err(AppError::bad_request("композиция не содержит media clips"));
    }

    let runtime_fingerprint = render_runtime_fingerprint(state.tools.as_ref());
    let key = dedupe_key(
        "composition",
        &CompositionDedupeIdentity {
            pipeline_version: "composition-render-v2",
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
    Ok(Json(json!({ "jobId": resolved_id })))
}

fn validate_composition_capabilities(
    tools: &crate::state::ToolInfo,
    composition: &Composition,
) -> AppResult<()> {
    let mut required = Vec::new();
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
    if composition.requires_stabilization() {
        required.push("deshake");
    }
    if composition.requires_speed_ramp() {
        required.extend(["setpts", "tpad", "fps", "trim"]);
    }
    if composition.requires_speed_ramp_pitch_audio() {
        required.extend(["atrim", "asetpts", "asplit", "atempo", "concat"]);
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
    }
    if missing.is_empty() {
        Ok(output.export_spec(av1_encoder))
    } else {
        Err(format!(
            "composition delivery {} недоступен: нужны {}",
            output.profile.id(),
            missing.join(", ")
        ))
    }
}

pub(super) fn spawn_composition_job(
    state: AppState,
    job_id: String,
    work: CompositionWork,
    attempt: u32,
    token: CancellationToken,
    lease: JobLeaseHeartbeat,
) {
    let st = state.clone();
    let jid = job_id.clone();
    let span = tracing::info_span!("job", job.id = %job_id, job.kind = "composition");
    let task = async move {
        let _lease = lease;
        if !mark_queued(&st, &jid).await {
            st.clear_cancel(&jid).await;
            return;
        }
        // Give the request/polling task a fairness boundary after publishing
        // the durable queued state before starting filesystem source lookup.
        tokio::task::yield_now().await;

        let output = match resolve_composition_output(st.tools.as_ref(), work.request.output) {
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
        if finish_from_render_cache(&st, &jid, &cache_key).await {
            return;
        }
        let _render_permit = match acquire_render_permit_or_cancelled(&st, &jid, &token).await {
            Some(permit) => permit,
            None => return,
        };
        let _permit = match acquire_job_permit_or_cancelled(&st, &jid, &token).await {
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
            let text_resources =
                PreparedTextResources::create(&st.staging_dir(), plan.composition()).await?;
            let command =
                FfmpegCompositionExportCompiler.compile(CompositionExportCompileRequest {
                    inputs: &inputs,
                    text_resources: text_resources.resources(),
                    destination: &output_path,
                    parallel_jobs: st.render_parallelism(),
                    composition: plan.composition(),
                    output,
                })?;
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
                if let Err(error) = st.db.cache_put(&cache_key, &info, &filename).await {
                    tracing::warn!(%error, "cache composition render");
                }
            }
        }
    }
    .instrument(span);
    state.spawn_task(task);
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
        let path = tools::find_source(&state.sources_dir(), id.as_str())
            .await
            .with_context(|| format!("источник {} не найден", id.as_str()))?;
        let identity = fingerprint_file(&state.cpu_pool, path.clone(), token.clone()).await?;
        let probe = tools::probe_video(&state.process_runtime, &path).await?;
        let has_video = probe.vcodec.is_some() && probe.width > 0 && probe.height > 0;
        let has_audio = probe.acodec.is_some();
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
    resources: BTreeMap<CompositionClipId, CompositionTextResource>,
    temporary_files: Vec<PathBuf>,
}

impl PreparedTextResources {
    async fn create(staging: &Path, composition: &Composition) -> anyhow::Result<Self> {
        let mut prepared = Self {
            resources: BTreeMap::new(),
            temporary_files: Vec::new(),
        };
        let mut fonts = BTreeMap::<String, PathBuf>::new();
        for track in &composition.tracks {
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
                prepared.resources.insert(
                    clip.id.clone(),
                    CompositionTextResource {
                        text_file,
                        font_file,
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
        b"composition-render-cache-v2".as_slice(),
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
) -> Value {
    json!({
        "id": output_id,
        "url": format!("/files/outputs/{filename}"),
        "filename": filename,
        "mediaType": "video",
        "container": profile.extension(),
        "videoCodec": profile.video_codec(),
        "audioCodec": profile.audio_codec(),
        "sizeBytes": size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{
        AnimatableValue, BlendMode, CanvasSpec, ClipPlacement, CompositionClipId,
        CompositionSource, FrameInterpolation, PlaybackMode, Rgba, SpeedRampAudioPolicy,
        SpeedRampInterpolation, SpeedRampPoint, SpeedRampSpec, StabilizationSpec, TextClip,
        TextStyle, TrackId, TransformSpec, VideoClip,
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
            resolve_composition_output(&base, CompositionOutputRequest::default()).unwrap();
        assert_eq!(default.profile, CompositionExportProfile::default());
        assert_eq!(default.video_quality, 23);
        assert_eq!(default.av1_encoder, None);

        let h265 = CompositionOutputRequest {
            profile: CompositionExportProfile::Mp4 {
                codec: CompositionMp4Codec::H265,
            },
            quality_tier: crate::services::composition::CompositionQualityTier::Medium,
        };
        assert_eq!(
            resolve_composition_output(&base, h265).unwrap_err(),
            "composition delivery mp4/h265 недоступен: нужны encoder libx265"
        );

        let av1 = CompositionOutputRequest {
            profile: CompositionExportProfile::Webm {
                codec: CompositionWebmCodec::Av1,
            },
            quality_tier: crate::services::composition::CompositionQualityTier::High,
        };
        assert_eq!(
            resolve_composition_output(&base, av1).unwrap_err(),
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
            resolve_composition_output(&both, av1).unwrap().av1_encoder,
            Some(CompositionAv1Encoder::LibSvtAv1)
        );
        let aom_only = crate::state::ToolInfo {
            ffmpeg_encoders: vec!["libaom-av1".into(), "libopus".into()],
            ..both
        };
        assert_eq!(
            resolve_composition_output(&aom_only, av1)
                .unwrap()
                .av1_encoder,
            Some(CompositionAv1Encoder::LibAomAv1)
        );
        assert_eq!(
            composition_output_filename("result", av1.profile),
            "result.webm"
        );
        assert_eq!(
            composition_result_info("result", "result.webm", av1.profile, Some(42)),
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
    fn optical_flow_capability_is_checked_only_for_requests_that_use_it() {
        let source_id = SourceId::parse("motion-source").unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(
            source_id.clone(),
            CompositionSource {
                id: source_id.clone(),
                kind: SourceKind::Video,
                duration_ticks: 2_000_000,
                width: 640,
                height: 360,
                has_audio: false,
            },
        );
        let clip = VideoClip {
            id: CompositionClipId::parse("motion-clip").unwrap(),
            source_id: source_id.clone(),
            placement: ClipPlacement {
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

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
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
                id: CompositionClipId::parse("title-clip").unwrap(),
                timeline_start_tick: 0,
                timeline_end_tick: 1_000_000,
                text: "Привет\n字幕".to_owned(),
                style: TextStyle {
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
        drop(prepared);
        assert!(!text_path.exists());
    }
}
