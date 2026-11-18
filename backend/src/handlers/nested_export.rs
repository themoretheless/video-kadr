//! Durable, authenticated native video/audio export jobs (no client filesystem paths).
use super::jobs::JobLeaseHeartbeat;
use crate::domain::caption_frame_clock::CaptionFrameSample;
use crate::{
    artifacts::fingerprint_file,
    domain::{
        artifact_graph::Fingerprint, composition::{SourceId, TrackId},
        nested_composition::NestedCompositionProject,
    },
    error::{ApiJson, AppError, AppResult},
    jobs::{dedupe_key, EnqueueOutcome, JobKind, JobService},
    state::AppState,
    tools::{
        native_audio_export::{mix_project_audio_tracks, PcmSource},
        native_rgba_export::{encode_project_with_clip_filter, AudioCodec, AudioCompression, RasterAudioSettings, PcmDepth, RasterAudio, RasterVideoSpec},
        
    },
};
use axum::{
    extract::{Path as AxPath, State},
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
    AudioOnly,
}
impl NativeExportMode {
    fn has_audio(self) -> bool { self != Self::VideoOnly }
    fn has_video(self) -> bool { self != Self::AudioOnly }
}
impl AudioCodec {
    fn container(self) -> &'static str {
        match self { Self::Aac => "m4a", Self::Opus => "opus" }
    }
    fn muxer(self) -> &'static str {
        match self { Self::Aac => "mp4", Self::Opus => "ogg" }
    }
    fn codec_name(self) -> &'static str {
        match self { Self::Aac => "aac", Self::Opus => "opus" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedExportRequest {
    #[serde(default)]
    saved_project: Option<super::concat_video_bound::SavedProject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caption_sidecar: Option<NestedCaptionSidecar>,
    schema_version: u32,
    mode: NativeExportMode,
    project: NestedCompositionProject,
    composition_id: String,
    start_us: u64,
    end_us: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timecode_range: Option<TimecodeRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    download_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio_compression: Option<AudioCompression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio_channels: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    audio_sample_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pcm_depth: Option<PcmDepth>,
    #[serde(default)]
    rendered_outputs: BTreeMap<String, String>,
}
fn normalize_download_name(name: &str, extension: &str) -> AppResult<Option<String>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let base = match name.rsplit_once('.') {
        Some((base, suffix)) if ["mkv", "wav", "m4a", "opus"].iter().any(|extension| suffix.eq_ignore_ascii_case(extension)) => base,
        _ => name,
    };
    let unsafe_character = |ch: char| ch.is_control() || r#"\/:*?"<>|"#.contains(ch);
    if base.len() > 200 || base.is_empty() || base.starts_with('.') || base.ends_with(['.', ' '])
        || base.chars().any(unsafe_character)
    {
        return Err(AppError::bad_request("Введите имя файла без пути и специальных символов"));
    }
    Ok(Some(format!("{base}.{extension}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TimecodeRange {
    start: String,
    end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportCursorRequest {
    time_us: u64,
    rate: crate::domain::composition::FrameRateRatio,
    #[serde(default)]
    drop_frame: bool,
}

pub async fn cursor_timecode(
    ApiJson(request): ApiJson<ExportCursorRequest>,
) -> AppResult<Json<Value>> {
    let numerator = u128::from(request.rate.numerator);
    let denominator = u128::from(request.rate.denominator);
    if !(denominator > 0 && numerator >= denominator && numerator <= 60 * denominator
        && request.time_us <= 9_007_199_254_740_991)
    {
        return Err(AppError::bad_request("Некорректная позиция или частота кадров"));
    }
    let rate = filmcraft_time::FrameRate::new(request.rate.numerator.into(), request.rate.denominator.into());
    let drop_frame = request.drop_frame && rate.supports_drop_frame();
    // Use the same floored microsecond clock as native export samples.
    let frame = ((u128::from(request.time_us) + 1) * numerator).div_ceil(1_000_000 * denominator) - 1;
    let frame = i64::try_from(frame).map_err(|error| AppError::bad_request(error.to_string()))?;
    let value = filmcraft_time::format_timecode_frames(frame, rate, drop_frame).replacen(';', ":", 2);
    parse_export_timecode(&value, rate).map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(json!({"timecode":value})))
}

fn parse_export_timecode(text: &str, rate: filmcraft_time::FrameRate) -> anyhow::Result<u64> {
    let text = text.trim();
    anyhow::ensure!((11..=12).contains(&text.len()), "Введите таймкод в формате ЧЧ:ММ:СС:КК");
    let fields: Vec<_> = text.split([':', ';']).collect();
    let frame_width = if rate.timecode_base() >= 100 { 3 } else { 2 };
    anyhow::ensure!(fields.len() == 4
        && fields[..3].iter().all(|field| field.len() == 2) && fields[3].len() == frame_width
        && fields.iter().all(|field| field.bytes().all(|byte| byte.is_ascii_digit())),
        "Введите таймкод в формате ЧЧ:ММ:СС:КК");
    let drop_frame = text.as_bytes()[8] == b';';
    anyhow::ensure!(text.as_bytes()[2] == b':' && text.as_bytes()[5] == b':',
        "Некорректные разделители таймкода");
    anyhow::ensure!(!drop_frame || rate.supports_drop_frame(), "Частота кадров не поддерживает drop-frame");
    let frame = filmcraft_time::parse_timecode(text, rate, drop_frame, 0)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let canonical = filmcraft_time::format_timecode_frames(frame, rate, drop_frame);
    anyhow::ensure!(canonical.split([':', ';']).eq(fields),
        "Таймкод не соответствует частоте кадров или указывает на пропущенный номер");
    Ok(u64::try_from(frame)?)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NestedCaptionSidecar {
    track_id: TrackId,
    format: String,
    drop_frame: bool,
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
    fn audio_sample_rate(&self) -> u32 {
        self.audio_sample_rate.unwrap_or(48_000)
    }

    fn container(&self) -> &'static str {
        if self.mode.has_video() { "mkv" } else {
            self.audio_compression.map_or("wav", |settings| settings.codec.container())
        }
    }

    fn timecode_frames(&self) -> anyhow::Result<Option<std::ops::Range<u64>>> {
        let Some(range) = &self.timecode_range else { return Ok(None); };
        let rate = self.project.compositions.get(&self.composition_id)
            .ok_or_else(|| anyhow::anyhow!("invalid native export focus"))?.content.canvas.frame_rate();
        let rate = filmcraft_time::FrameRate::new(rate.numerator.into(), rate.denominator.into());
        let start = parse_export_timecode(&range.start, rate)?;
        let end = parse_export_timecode(&range.end, rate)?;
        anyhow::ensure!(start < end, "Конец экспорта должен быть после начала");
        Ok(Some(start..end))
    }

    fn timecode_samples(&self) -> anyhow::Result<Option<(CaptionFrameSample, CaptionFrameSample)>> {
        let Some(frames) = self.timecode_frames()? else { return Ok(None); };
        let rate = self.project.compositions[&self.composition_id].content.canvas.frame_rate();
        let rate = filmcraft_time::FrameRate::new(rate.numerator.into(), rate.denominator.into());
        let sample = |index| CaptionFrameSample::new(index, rate, 0).map_err(anyhow::Error::msg);
        Ok(Some((sample(frames.start)?, sample(frames.end)?)))
    }

    fn apply_timecode_range(&mut self) -> anyhow::Result<()> {
        if let Some((start, end)) = self.timecode_samples()? {
            (self.start_us, self.end_us) = (start.host_us, end.host_us);
        }
        Ok(())
    }

    fn audio_range(&self) -> anyhow::Result<std::ops::Range<u64>> {
        let Some(frames) = self.timecode_frames()? else {
            return Ok(self.start_us..self.end_us);
        };
        let rate = self.project.compositions[&self.composition_id].content.canvas.frame_rate();
        // The mixer uses ceil(us * 48 kHz). A floored microsecond can land
        // before the intended sample at custom frame rates. Move it forward
        // only when needed to preserve the exact rational sample boundary.
        let boundary = |frame: u64, host_us: u64| {
            let exact = (u128::from(frame) * u128::from(rate.denominator) * 48_000)
                .div_ceil(u128::from(rate.numerator));
            let rounded = (u128::from(host_us) * 48_000).div_ceil(1_000_000);
            host_us + u64::from(rounded < exact)
        };
        let start = boundary(frames.start, self.start_us);
        let mut end = boundary(frames.end, self.end_us);
        if end > self.end_us {
            end = end.min(self.project.durations_us()?[&self.composition_id]);
        }
        Ok(start..end)
    }

    fn audio_only_frames(&self) -> anyhow::Result<u64> {
        let range = self.audio_range()?;
        let sample = |time| (u128::from(time) * 48_000).div_ceil(1_000_000);
        let frames = u64::try_from(sample(range.end) - sample(range.start))?;
        anyhow::ensure!(frames > 0 && frames <= (u64::from(u32::MAX) - 48) / 8,
            "Диапазон звука не помещается в WAV");
        Ok(frames)
    }

    fn caption_source(&self) -> anyhow::Result<Option<(&crate::domain::caption_timeline::CaptionTrack, filmcraft_captions::Format)>> {
        let Some(sidecar) = &self.caption_sidecar else { return Ok(None) };
        let track = self.project.compositions.get(&self.composition_id)
            .and_then(|node| node.content.caption_tracks.iter().find(|track| track.id == sidecar.track_id))
            .ok_or_else(|| anyhow::anyhow!("caption sidecar track not found"))?;
        let format = filmcraft_captions::Format::from_name(&sidecar.format)
            .ok_or_else(|| anyhow::anyhow!("unsupported caption sidecar format"))?;
        Ok(Some((track, format)))
    }

    fn encode_caption_sidecar(&self, rate: filmcraft_time::FrameRate)
        -> anyhow::Result<Option<crate::domain::filmcraft_export_caption_sidecar::Sidecar>>
    {
        use crate::domain::{caption_file_timing::document_for_range, caption_time::CaptionTime};
        use filmcraft_captions::{Cue, Document, WriteOptions};
        let Some((track, format)) = self.caption_source()? else { return Ok(None) };
        let source_tick = |time: CaptionTime| time.source_tick().map_err(anyhow::Error::msg);
        let cues = track.clips.iter().map(|cue| Ok(Cue {
            start: source_tick(cue.timeline_start_tick)?,
            end: source_tick(cue.timeline_end_tick)?,
            text: cue.text.clone(), speaker: cue.speaker.clone(), id: cue.cue_id.clone(), settings: cue.settings.clone(),
        })).collect::<anyhow::Result<Vec<_>>>()?;
        let document = Document { cues, blocks: track.vtt_blocks.clone(), warnings: Vec::new() };
        let (start, end) = if let Some((start, end)) = self.timecode_samples()? {
            let tick = |sample: CaptionFrameSample| -> anyhow::Result<filmcraft_time::Tick> {
                Ok(filmcraft_time::Tick(i64::try_from(sample.source_ticks)?))
            };
            (tick(start)?, tick(end)?)
        } else {
            (source_tick(CaptionTime::Microseconds(self.start_us))?, source_tick(CaptionTime::Microseconds(self.end_us))?)
        };
        let document = document_for_range(&document, start, Some(end))?;
        let bytes = filmcraft_captions::write(&document, format, WriteOptions {
            drop_frame: self.caption_sidecar.as_ref().is_some_and(|sidecar| sidecar.drop_frame),
            rate: Some(rate),
        });
        anyhow::ensure!(bytes.len() <= 4 * 1024 * 1024, "caption sidecar exceeds 4 MiB");
        Ok(Some(crate::domain::filmcraft_export_caption_sidecar::Sidecar {
            extension: format.extension().into(),
            bytes_base64: crate::domain::graphic_template_fonts::base64_encode(&bytes),
        }))
    }

    fn validate(&self) -> anyhow::Result<RasterVideoSpec> {
        anyhow::ensure!(
            self.schema_version == 1,
            "invalid native export schema version"
        );
        anyhow::ensure!(
            self.pcm_depth.is_none() || (self.mode.has_audio() && self.audio_compression.is_none()),
            "Разрядность доступна для звука PCM",
        );
        if let Some(channels) = self.audio_channels {
            anyhow::ensure!(self.mode.has_audio() && matches!(channels, 1 | 2),
                "Для экспорта со звуком доступны моно и стерео");
        }
        if let Some(rate) = self.audio_sample_rate {
            anyhow::ensure!(self.mode.has_audio()
                && matches!(rate, 32000 | 44100 | 48000 | 88200 | 96000 | 192000),
                "Частота дискретизации недоступна для выбранного режима или значения");
            if let Some(settings) = self.audio_compression {
                let supported = match settings.codec {
                    AudioCodec::Aac => rate <= 96_000,
                    AudioCodec::Opus => rate == 48_000,
                };
                anyhow::ensure!(supported, "Кодек не поддерживает выбранную частоту дискретизации");
            }
        }
        if let Some(settings) = self.audio_compression {
            anyhow::ensure!(self.mode.has_audio(),
                "Кодек AAC или Opus требует экспорта со звуком");
            let max_bitrate = match (self.audio_channels, settings.codec) {
                (Some(1), AudioCodec::Opus) => 256,
                (channels, AudioCodec::Aac) => (self.audio_sample_rate() * u32::from(channels.unwrap_or(2)) * 6 / 1000).min(320),
                _ => 320,
            };
            anyhow::ensure!((32..=max_bitrate).contains(&settings.bitrate_kbps),
                "Битрейт звука должен быть от 32 до {max_bitrate} кбит/с");
        }
        let durations = self.project.durations_us()?;
        self.caption_source()?;
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
        let frames = match self.timecode_frames()? {
            Some(range) => u128::from(range.end - range.start),
            None => (u128::from(self.end_us - self.start_us) * u128::from(rate.numerator))
                .div_ceil(1_000_000 * u128::from(rate.denominator)),
        };
        if self.mode.has_video() {
            anyhow::ensure!(frames <= 360000, "invalid native export frame count");
        } else {
            let audio_frames = self.audio_only_frames()?;
            if self.audio_compression.is_none() {
                let delivered = (u128::from(audio_frames) * u128::from(self.audio_sample_rate())).div_ceil(48_000);
                let bytes = delivered * u128::from(self.audio_channels.unwrap_or(2))
                    * u128::from(self.pcm_depth.map_or(4, PcmDepth::bytes));
                anyhow::ensure!(bytes <= u128::from(u32::MAX) - 128, "Диапазон звука не помещается в WAV");
            }
        }
        let spec = RasterVideoSpec {
            width: canvas.width as usize,
            height: canvas.height as usize,
            frame_rate: rate,
            frames: frames as u64,
            staging_bytes: STAGING_BYTES,
        };
        if self.mode.has_video() { spec.validate()?; }
        if self.mode.has_audio() {
            let layers = crate::domain::nested_audio::plan_audio_tracks(
                &self.project,
                &self.composition_id,
                self.audio_range()?,
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
    let mut ids = if request.mode.has_video() { request
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
        .map_err(|e| AppError::bad_request(e.to_string()))? } else { BTreeSet::new() };
    if request.mode.has_audio() {
        let layers = crate::domain::nested_audio::plan_audio_tracks(
            &request.project,
            &request.composition_id,
            request.audio_range().map_err(|error| AppError::bad_request(error.to_string()))?,
        )
        .map_err(|e| AppError::bad_request(e.to_string()))?;
        ids.extend(layers.iter().map(|layer| layer.source_id().clone()));
    }
    let masks = if request.mode.has_video() {
        super::concat_video_bound::BoundMasks::load(state,owner,request.saved_project.as_ref(),&request.project,cancel)
            .await.map_err(|error| AppError::bad_request(error.to_string()))?
    } else {
        super::concat_video_bound::BoundMasks::empty(cancel)
    };
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
// Terminal jobs erase their request. Older completed exports still have an output ACL.
const EXPORT_OWNER_JOIN: &str = "
    JOIN job_requests r ON r.job_id=j.id AND r.kind='nested_export'
    LEFT JOIN nested_export_job_access a ON a.job_id=j.id
    LEFT JOIN output_access o ON o.output_id=json_extract(j.result_json, '$.id')
";
const EXPORT_OWNER: &str = "COALESCE(a.owner, json_extract(r.payload_json, '$.owner'), o.actor)";

pub(super) async fn authorize_job(state: &AppState, id: &str, headers: &HeaderMap) -> AppResult<()> {
    let query = format!("SELECT {EXPORT_OWNER} FROM jobs j {EXPORT_OWNER_JOIN} WHERE j.id=?");
    let owners: Vec<Option<String>> = sqlx::query_scalar(&query)
        .bind(id).fetch_all(state.db.pool()).await
        .map_err(|e| AppError::internal("load nested export access", e))?;
    if !owners.is_empty() {
        let actor = super::publish::authenticated_actor(state, headers).await?;
        if !owners.iter().any(|owner| owner.as_deref() == Some(actor.as_str())) {
            return Err(AppError::forbidden("Это задание другого пользователя"));
        }
    }
    Ok(())
}

async fn owner_queue(state: &AppState, actor: &str) -> AppResult<Vec<Value>> {
    let queue = state.job_store.nested_export_queue(actor).await
        .map_err(|error| AppError::internal("load owner export queue", error))?;
    Ok(queue.into_iter().map(|(id, raised)| json!({"jobId":id,"raised":raised})).collect())
}

#[derive(Deserialize)]
pub struct ExportHistoryQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(page): axum::extract::Query<ExportHistoryQuery>,
) -> AppResult<Json<Value>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    let limit = page.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(AppError::bad_request("Размер страницы должен быть 1–50"));
    }
    let cursor_time = if let Some(cursor) = &page.cursor {
        let query = format!("SELECT j.created_at FROM jobs j {EXPORT_OWNER_JOIN}
            WHERE j.id=? AND {EXPORT_OWNER}=?");
        let time: Option<i64> = sqlx::query_scalar(&query).bind(cursor).bind(&actor)
            .fetch_optional(state.db.pool()).await
            .map_err(|e| AppError::internal("load nested export cursor", e))?;
        Some(time.ok_or_else(|| AppError::bad_request("Некорректная страница экспорта"))?)
    } else {
        None
    };
    let query = format!("SELECT DISTINCT j.id,j.status,j.created_at,COALESCE(p.raised,0),a.download_name
        FROM jobs j {EXPORT_OWNER_JOIN}
        LEFT JOIN nested_export_priority p ON p.job_id=j.id
        WHERE {EXPORT_OWNER}=? AND (? IS NULL OR j.created_at<? OR (j.created_at=? AND j.id<?))
        ORDER BY j.created_at DESC,j.id DESC LIMIT ?");
    let mut rows: Vec<(String, String, i64, bool, Option<String>)> = sqlx::query_as(&query)
        .bind(&actor).bind(cursor_time).bind(cursor_time).bind(cursor_time)
        .bind(page.cursor).bind(i64::from(limit) + 1)
        .fetch_all(state.db.pool()).await
        .map_err(|e| AppError::internal("list nested exports", e))?;
    let next_cursor = if rows.len() > limit as usize {
        rows.pop();
        rows.last().map(|row| row.0.clone())
    } else {
        None
    };
    let jobs: Vec<_> = rows.into_iter().map(|(id,status,created_at,raised,download_name)| {
        let mut entry = json!({"jobId":id,"status":status,"createdAt":created_at,"raised":raised});
        if let Some(name) = download_name { entry["downloadName"] = json!(name); }
        entry
    }).collect();
    Ok(Json(json!({"jobs":jobs,"nextCursor":next_cursor,"queue":owner_queue(&state,&actor).await?})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPriority {
    raised: bool,
}

pub async fn set_priority(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<ExportPriority>,
) -> AppResult<Json<Value>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    authorize_job(&state, &id, &headers).await?;
    let applied = state.job_store.set_nested_export_priority(&id, &actor, input.raised)
        .await.map_err(|error| AppError::internal("set nested export priority", error))?;
    if !applied {
        return Err(AppError::conflict("Приоритет можно изменить у ожидающего экспорта"));
    }
    Ok(Json(json!({"jobId":id,"raised":input.raised,"queue":owner_queue(&state,&actor).await?})))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExportMoveDirection {
    Earlier,
    Later,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportMove {
    direction: ExportMoveDirection,
}

pub async fn move_export(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<ExportMove>,
) -> AppResult<Json<Value>> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    authorize_job(&state, &id, &headers).await?;
    let earlier = matches!(input.direction, ExportMoveDirection::Earlier);
    let applied = state.job_store.move_nested_export(&id, &actor, earlier)
        .await.map_err(|error| AppError::internal("move nested export", error))?;
    if !applied {
        return Err(AppError::conflict("В этом направлении нет ожидающего экспорта с тем же приоритетом"));
    }
    Ok(Json(json!({"jobId":id,"applied":true,"queue":owner_queue(&state,&actor).await?})))
}

pub async fn nested_export_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(mut request): ApiJson<NestedExportRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if request.project.compositions.values().any(|node|node.content.tracks.iter().any(|track|matches!(track,crate::domain::composition::CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.enabled&&clip.concat_cutout.is_some())))){request.rendered_outputs.clear();}

    request.download_name = match request.download_name.as_deref() {
        Some(name) => normalize_download_name(name, request.container())?,
        None => None,
    };
    if !request.mode.has_video() { request.rendered_outputs.clear(); }
    request.apply_timecode_range().map_err(|error| AppError::bad_request(error.to_string()))?;
    request
        .validate()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    if !state.tools.ffmpeg {
        return Err(AppError::service_unavailable("Нативный экспорт требует ffmpeg"));
    }
    if request.mode.has_video()
        && (!state.tools.ffmpeg_encoders.iter().any(|v| v == "ffv1")
            || !state.tools.ffmpeg_muxers.iter().any(|v| v == "matroska"))
    {
        return Err(AppError::service_unavailable(
            "Нативный экспорт требует ffmpeg с FFV1/Matroska",
        ));
    }
    if request.mode.has_audio()
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
    if let Some(depth) = request.pcm_depth {
        if !state.tools.ffmpeg_encoders.iter().any(|value| value == depth.encoder())
            || (!request.mode.has_video()
                && !state.tools.ffmpeg_muxers.iter().any(|value| value == "wav"))
        {
            return Err(AppError::service_unavailable(format!(
                "FFmpeg не поддерживает экспорт {}", depth.encoder(),
            )));
        }
    }
    if let Some(compression) = request.audio_compression {
        let codec = compression.codec;
        if !state.tools.ffmpeg_encoders.iter().any(|value| value == codec.encoder())
            || (!request.mode.has_video()
                && !state.tools.ffmpeg_muxers.iter().any(|value| value == codec.muxer()))
        {
            return Err(AppError::service_unavailable(format!("FFmpeg не поддерживает экспорт {}", codec.codec_name())));
        }
    }
    let container = request.container();
    let mode = request.mode;
    let (start_us, end_us) = (request.start_us, request.end_us);
    let caption_range = request.timecode_samples()
        .map_err(|error| AppError::bad_request(error.to_string()))?
        .map(|(start, end)| json!({"startTick":start.source_ticks.to_string(),"endTick":end.source_ticks.to_string()}));
    let bound = bindings(&state, &request, &owner, &CancellationToken::new()).await?;
    let identity_version = if mode == NativeExportMode::VideoAudio && request.timecode_range.is_some() {
        "nested-export-v8-timecode-audio"
    } else {
        "nested-export-v7-bound-video"
    };
    let key = dedupe_key(
        identity_version,
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
    super::jobs::dispatch_waiting_native_exports(&state).await;
    let mut reply = json!({"jobId":id,"mode":mode,"container":container,"startUs":start_us,"endUs":end_us});
    if let Some(range) = caption_range {
        reply["captionRange"] = range;
    }
    Ok((StatusCode::ACCEPTED, Json(reply)))
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
    guards: crate::jobs::ExecutionGuards,
) {
    let st = state.clone();
    state.spawn_task(async move {
        let _lease = lease;
        if !JobService::mark_queued(&st, &job_id).await {
            st.clear_cancel(&job_id).await;
            return;
        }
        if token.is_cancelled() {
            JobService::mark_cancelled(&st, &job_id).await;
            return;
        }
        let mode = work.request.mode;
        let container = work.request.container();
        let filename = format!("{}.{container}", work.output_id);
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
                    let caption_sidecar = work.request.encode_caption_sidecar(filmcraft_time::FrameRate {
                        num: i64::from(spec.frame_rate.numerator), den: i64::from(spec.frame_rate.denominator),
                    })?;
                    anyhow::ensure!(!ctx.cancel.is_cancelled(), "native export cancelled");
                    let with_audio = mode.has_audio();
                    let with_video = mode.has_video();
                    let channels = work.request.audio_channels.unwrap_or(2);
                    let sample_rate = work.request.audio_sample_rate();
                    let audio_settings = RasterAudioSettings {
                        compression: work.request.audio_compression,
                        pcm_depth: work.request.pcm_depth,
                        channels,
                        sample_rate,
                    };
                    let mixed = if with_audio {
                        let range = work.request.audio_range()?;
                        let plan = crate::domain::nested_audio::plan_audio_tracks(&work.request.project, &work.request.composition_id, range.clone())?;
                        let mut audio_sources = BTreeMap::new();
                        for id in plan.iter().map(|layer| layer.source_id().clone()).collect::<BTreeSet<_>>() {
                            let path = bound.sources.get(&id).ok_or_else(|| anyhow::anyhow!("native audio binding missing"))?.clone();
                            let selected=crate::tools::native_audio_export::preferred_audio_stream(&plan,&id);
                            let probe = crate::tools::pcm_conform::probe_pcm_metadata_for_stream(&st.process_runtime, Path::new("ffprobe"), &path, remaining()?, &ctx.cancel,selected).await?;
                            audio_sources.insert(id, PcmSource::from_probe(path, probe)?);
                        }
                        let video_bytes = if with_video { spec.width as u64 * spec.height as u64 * 4 * spec.frames } else { 0 };
                        let audio_budget = STAGING_BYTES.checked_sub(video_bytes).ok_or_else(|| anyhow::anyhow!("native export staging budget exceeded"))?;
                        let output_frames = if with_video { spec.audio_frames_48k()? } else { work.request.audio_only_frames()? };
                        let phase = ProgressLane::new(&ctx.progress, 0.0, if with_video { 0.3 } else { 0.95 });
                        let staging = if with_video { st.outputs_dir() } else { st.staging_dir() };
                        Some(mix_project_audio_tracks(&st.process_runtime, Path::new("ffmpeg"), &staging, &work.request.project, &work.request.composition_id, range, output_frames, &audio_sources, audio_budget, remaining()?, &ctx.cancel, &phase.sender).await?)
                    } else { None };
                    if with_video {
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
                            work.request.timecode_frames()?,
                            with_audio.then_some(audio_settings),
                        ).await?;
                    } else {
                        let audio = mixed.as_ref().ok_or_else(|| anyhow::anyhow!("audio export mix missing"))?;
                        if work.request.audio_compression.is_some() || sample_rate != 48_000 || work.request.pcm_depth.is_some() {
                            let phase = ProgressLane::new(&ctx.progress, 95.0, 0.05);
                            write_encoded_audio(&st, &result_path, audio, audio_settings, &phase.sender, &ctx.cancel, remaining()?).await?;
                        } else {
                            write_audio_wav(&st.staging_dir(), &result_path, audio, channels, &ctx.cancel).await?;
                        }
                    }
                    published_work.store(true, Ordering::Relaxed);
                    let current = bindings(&st, &work.request, &work.owner, &ctx.cancel).await
                        .map_err(|_| anyhow::anyhow!("invalid native export bindings before publication: доступ к источникам изменился"))?;
                    anyhow::ensure!(current.identity == work.identity, "invalid native export source or mask changed during rendering");
                    anyhow::ensure!(!ctx.cancel.is_cancelled(), "native export cancelled");
                    let bytes = tokio::fs::metadata(&result_path).await?.len();
                    st.db.grant_output_access(&work.output_id, &work.owner).await?;
                    success_work.store(true, Ordering::Relaxed);
                    Ok(Some(json!({
                        "id": work.output_id, "filename": filename, "downloadName": work.request.download_name,
                        "title": work.request.download_name, "originalFilename": work.request.download_name,
                        "url": format!("/files/outputs/{filename}"), "mediaType": if with_video { "video" } else { "audio" },
                        "container": container, "vcodec": with_video.then_some("ffv1"), "videoCodec": with_video.then_some("ffv1"),
                        "audioCodec": with_audio.then(|| work.request.audio_compression.map_or_else(|| work.request.pcm_depth.map_or("pcm_f32le", PcmDepth::encoder), |settings| settings.codec.codec_name())),
                        "audioChannels": with_audio.then_some(channels), "audioSampleRate": with_audio.then_some(sample_rate),
                        "audioBitrateKbps": work.request.audio_compression.map(|settings| settings.bitrate_kbps), "withAudio": with_audio, "mode": work.request.mode,
                        "width": with_video.then_some(spec.width), "height": with_video.then_some(spec.height),
                        "fps": with_video.then_some(spec.frame_rate.numerator as f64 / spec.frame_rate.denominator as f64),
                        "fpsRate": with_video.then_some(spec.frame_rate),
                        "duration": if with_video {
                            spec.frames as f64 * spec.frame_rate.denominator as f64 / spec.frame_rate.numerator as f64
                        } else { mixed.as_ref().map_or(0.0, |audio| audio.frames as f64 / 48000.0) },
                        "sizeBytes": bytes, "captionSidecar": caption_sidecar,
                    })))
                }.await;
                if ctx.cancel.is_cancelled() {
                    success_work.store(false, Ordering::Relaxed);
                    Ok(None)
                } else {
                    outcome
                }
            },
        ).await;
        if published.load(Ordering::Relaxed) && (!updated || !success.load(Ordering::Relaxed)) {
            let _ = tokio::fs::remove_file(output).await;
            let _ = st.db.revoke_output_access(&cleanup_output_id).await;
        }
    });
}

async fn write_encoded_audio(
    state: &AppState,
    output: &std::path::Path,
    audio: &crate::tools::native_audio_export::MixedPcm,
    settings: RasterAudioSettings,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
    cancel: &CancellationToken,
    timeout: std::time::Duration,
) -> anyhow::Result<()> {
    use crate::process_control::{stream_with_progress, ProcessStatus};
    let RasterAudioSettings { compression, pcm_depth, channels, sample_rate } = settings;
    let temporary = tempfile::tempdir_in(state.staging_dir())?;
    let container = compression.map_or("wav", |settings| settings.codec.container());
    let muxer = compression.map_or("wav", |settings| settings.codec.muxer());
    let encoder = compression.map_or_else(|| pcm_depth.map_or("pcm_f32le", PcmDepth::encoder), |settings| settings.codec.encoder());
    let encoded = temporary.path().join(format!("audio.{container}"));
    let mut command = tokio::process::Command::new("ffmpeg");
    command.args(["-nostdin", "-v", "error", "-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
        .arg(audio.path())
        .args(["-map", "0:a:0", "-c:a", encoder]);
    if let Some(settings) = compression {
        command.args(["-b:a", &format!("{}k", settings.bitrate_kbps)]);
        if matches!(settings.codec, AudioCodec::Aac) {
            command.args(["-movflags", "+faststart", "-movie_timescale", &sample_rate.to_string()]);
        }
    }
    if let Some(filter) = crate::tools::native_audio_export::output_audio_filter(channels, sample_rate, audio.frames) {
        command.args(["-af", &filter]);
    }
    command.args(["-f", muxer, "-progress", "pipe:1"]).arg(&encoded);
    let (status, diagnostic) = stream_with_progress(
        &state.process_runtime,
        command,
        state.process_runtime.render_policy(vec![audio.path().to_owned()], &encoded),
        |line| {
            let elapsed = line.strip_prefix("out_time_us=")?.parse::<u64>().ok()?;
            Some((elapsed as f64 * 4.8 / audio.frames as f64).clamp(0.0, 100.0))
        },
        progress, cancel, timeout,
    ).await?;
    anyhow::ensure!(matches!(status, ProcessStatus::Ok), "ffmpeg audio export failed: {status:?} {diagnostic}");
    anyhow::ensure!(!cancel.is_cancelled(), "native audio export cancelled");
    anyhow::ensure!(tokio::fs::metadata(&encoded).await?.len() > 0, "ffmpeg produced empty audio");
    tokio::fs::hard_link(&encoded, output).await?;
    Ok(())
}

async fn write_audio_wav(
    staging: &std::path::Path,
    output: &std::path::Path,
    audio: &crate::tools::native_audio_export::MixedPcm,
    channels: u16,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let bytes_per_frame = channels * 4;
    let size = u32::try_from(audio.frames.checked_mul(u64::from(bytes_per_frame)).ok_or_else(|| anyhow::anyhow!("WAV size overflow"))?)?;
    let riff_size = size.checked_add(48).ok_or_else(|| anyhow::anyhow!("WAV size overflow"))?;
    let mut header = Vec::with_capacity(56);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&riff_size.to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16_u32.to_le_bytes());
    header.extend_from_slice(&3_u16.to_le_bytes()); // IEEE float
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&48_000_u32.to_le_bytes());
    header.extend_from_slice(&(48_000 * u32::from(bytes_per_frame)).to_le_bytes());
    header.extend_from_slice(&bytes_per_frame.to_le_bytes());
    header.extend_from_slice(&32_u16.to_le_bytes());
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4_u32.to_le_bytes());
    header.extend_from_slice(&u32::try_from(audio.frames)?.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&size.to_le_bytes());
    let temporary = tempfile::NamedTempFile::new_in(staging)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut pcm = tokio::fs::File::open(audio.path()).await?;
    tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("native audio export cancelled"),
        result = async {
            file.write_all(&header).await?;
            if channels == 2 {
                let written = tokio::io::copy(&mut pcm, &mut file).await?;
                anyhow::ensure!(written == u64::from(size), "unexpected WAV payload size");
            } else {
                anyhow::ensure!(pcm.metadata().await?.len() == audio.frames * 8, "unexpected stereo mix size");
                const BLOCK_FRAMES: usize = 4096;
                let mut input = vec![0_u8; BLOCK_FRAMES * 8];
                let mut output = vec![0_u8; BLOCK_FRAMES * 4];
                let mut remaining = audio.frames;
                while remaining > 0 {
                    let frames = remaining.min(BLOCK_FRAMES as u64) as usize;
                    pcm.read_exact(&mut input[..frames * 8]).await?;
                    for (stereo, mono) in input[..frames * 8].chunks_exact(8)
                        .zip(output[..frames * 4].chunks_exact_mut(4))
                    {
                        let left = f32::from_le_bytes(stereo[..4].try_into()?);
                        let right = f32::from_le_bytes(stereo[4..].try_into()?);
                        mono.copy_from_slice(&(left * 0.5 + right * 0.5).to_le_bytes());
                    }
                    file.write_all(&output[..frames * 4]).await?;
                    remaining -= frames as u64;
                }
            }
            file.sync_all().await?;
            anyhow::Ok(())
        } => result?,
    }
    anyhow::ensure!(!cancel.is_cancelled(), "native audio export cancelled");
    drop(file);
    temporary.persist_noclobber(output)?;
    Ok(())
}

#[cfg(test)]
mod caption_sidecar_tests {
    use super::*;

    #[tokio::test]
    async fn native_exports_wait_durably_until_execution_slots_are_available() {
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let db = crate::db::Db::open(&storage).await.unwrap();
        let library = crate::library::Library::load(storage.clone()).await;
        let state = AppState::new(storage, 2, crate::state::ToolInfo::default(), library, db);
        let mut held = Vec::new();
        while let Some(guards) = state.try_acquire_export_execution(true) {
            held.push(guards);
        }
        assert!(!held.is_empty());
        let id = "durable-queued-export";
        state.enqueue_job(id.into(), JobKind::NestedExport, &json!({"owner":"test"}), id)
            .await.unwrap();

        super::super::jobs::dispatch_job(&state, id).await;
        let pending: (i64, Option<i64>) = sqlx::query_as(
            "SELECT delivery_count, lease_until FROM job_outbox WHERE job_id = ?",
        ).bind(id).fetch_one(state.db.pool()).await.unwrap();
        assert_eq!(pending, (0, None));
        assert_eq!(state.job_store.deliverable_ids().await.unwrap(), vec![id]);

        drop(held);
        super::super::jobs::resume_pending_jobs(&state).await;
        let delivered: i64 = sqlx::query_scalar("SELECT delivery_count FROM job_outbox WHERE job_id = ?")
            .bind(id).fetch_one(state.db.pool()).await.unwrap();
        assert_eq!(delivered, 1);
    }

    #[tokio::test]
    async fn nested_export_history_and_results_are_owner_scoped() {
        use axum::response::IntoResponse;
        let dir = tempfile::tempdir().unwrap();
        let storage = dir.path().to_path_buf();
        let db = crate::db::Db::open(&storage).await.unwrap();
        let library = crate::library::Library::load(storage.clone()).await;
        let state = AppState::new(storage, 2, crate::state::ToolInfo::default(), library, db);
        let login = |token: &str| {
            let mut headers = HeaderMap::new();
            headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
            headers
        };
        let alice = state.db.register_auth_user("alice-history", "alice-private-password").await.unwrap().unwrap();
        let bob = state.db.register_auth_user("bob-history", "bob-private-password").await.unwrap().unwrap();
        let alice_headers = login(&alice.token);
        let bob_headers = login(&bob.token);
        for (id, owner) in [("alice-job", "alice-history"), ("alice-older", "alice-history"), ("bob-job", "bob-history")] {
            let created = state.enqueue_job(id.into(), JobKind::NestedExport, &json!({"owner":owner}), id).await.unwrap();
            assert!(matches!(created, EnqueueOutcome::Created(_)));
        }
        sqlx::query("UPDATE job_outbox SET available_at=0")
            .execute(state.db.pool()).await.unwrap();
        let denied = set_priority(
            State(state.clone()), AxPath("alice-older".into()), bob_headers.clone(),
            ApiJson(ExportPriority { raised: true }),
        ).await.unwrap_err();
        assert_eq!(denied.into_response().status(), StatusCode::FORBIDDEN);
        assert_eq!(state.job_store.deliverable_ids().await.unwrap(), vec!["alice-job", "alice-older", "bob-job"]);
        let Json(priority) = set_priority(
            State(state.clone()), AxPath("alice-older".into()), alice_headers.clone(),
            ApiJson(ExportPriority { raised: true }),
        ).await.unwrap();
        assert_eq!(priority["raised"], true);
        assert_eq!(state.job_store.deliverable_ids().await.unwrap(), vec!["alice-older", "alice-job", "bob-job"]);
        let Json(reset) = set_priority(
            State(state.clone()), AxPath("alice-older".into()), alice_headers.clone(),
            ApiJson(ExportPriority { raised: false }),
        ).await.unwrap();
        assert_eq!(reset["raised"], false);
        assert_eq!(state.job_store.deliverable_ids().await.unwrap(), vec!["alice-job", "alice-older", "bob-job"]);
        assert!(state.job_store.claim("alice-job", 60).await.unwrap().is_some());
        let claimed = set_priority(
            State(state.clone()), AxPath("alice-job".into()), alice_headers.clone(),
            ApiJson(ExportPriority { raised: true }),
        ).await.unwrap_err();
        assert_eq!(claimed.into_response().status(), StatusCode::CONFLICT);
        sqlx::query("UPDATE job_requests SET payload_json='null' WHERE job_id='alice-job'")
            .execute(state.db.pool()).await.unwrap();
        sqlx::query("UPDATE jobs SET created_at=42")
            .execute(state.db.pool()).await.unwrap();
        let page = |cursor| axum::extract::Query(ExportHistoryQuery { cursor, limit: Some(1) });
        let Json(history) = list(State(state.clone()), alice_headers.clone(), page(None)).await.unwrap();
        assert_eq!(history["jobs"].as_array().unwrap().len(), 1);
        assert_eq!(history["jobs"][0]["jobId"], "alice-older");
        assert_eq!(history["nextCursor"], "alice-older");
        let Json(history) = list(State(state.clone()), alice_headers.clone(), page(Some("alice-older".into()))).await.unwrap();
        assert_eq!(history["jobs"][0]["jobId"], "alice-job");
        assert!(history["nextCursor"].is_null());
        let denied = list(State(state.clone()), alice_headers.clone(), page(Some("bob-job".into()))).await.unwrap_err();
        assert_eq!(denied.into_response().status(), StatusCode::BAD_REQUEST);
        assert!(authorize_job(&state, "alice-job", &alice_headers).await.is_ok());
        let denied = authorize_job(&state, "alice-job", &bob_headers).await.unwrap_err();
        assert_eq!(denied.into_response().status(), StatusCode::FORBIDDEN);
        let denied = authorize_job(&state, "alice-job", &HeaderMap::new()).await.unwrap_err();
        assert_eq!(denied.into_response().status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn nested_caption_sidecar_uses_selected_track_and_video_bounds() {
        let mut request: NestedExportRequest = serde_json::from_value(json!({
            "schemaVersion": 1, "mode": "video_only", "compositionId": "root",
            "startUs": 500_000, "endUs": 1_500_000,
            "captionSidecar": {"trackId":"selected","format":"vtt","dropFrame":false},
            "project": {
                "schemaVersion": 1, "root": "root", "compositions": {"root": {
                    "instances": [], "content": {
                        "schemaVersion": 1, "timeBase": 1000000, "sources": {}, "tracks": [],
                        "canvas":{"width":320,"height":180,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}},
                        "captionTracks": [
                            {"id":"first","name":"Other","clips":[]},
                            {"id":"selected","name":"Selected","hidden":true,"vttBlocks":["NOTE keep"],
                             "clips":[{"id":"cue","cueId":"file-id","timelineStartTick":"1","timelineEndTick":"508032000001",
                                       "text":"Dialogue","speaker":"Voice","settings":"align:start"}]}
                        ]
                    }
                }}
            }
        })).unwrap();
        let spec = request.validate().unwrap();
        let rate = filmcraft_time::FrameRate {
            num: i64::from(spec.frame_rate.numerator),
            den: i64::from(spec.frame_rate.denominator),
        };
        let sidecar = request.encode_caption_sidecar(rate).unwrap().unwrap();
        let bytes = crate::domain::graphic_template_fonts::base64_decode(&sidecar.bytes_base64).unwrap();
        let document = filmcraft_captions::parse(&bytes, filmcraft_captions::Format::WebVtt).unwrap();
        assert_eq!(sidecar.extension, "vtt");
        assert_eq!(document.blocks, ["NOTE keep"]);
        assert_eq!(document.cues.len(), 1);
        let cue = &document.cues[0];
        assert_eq!((cue.start.0, cue.end.0), (0, filmcraft_time::TICKS_PER_SECOND));
        assert_eq!(cue.id.as_deref(), Some("file-id"));
        assert_eq!(cue.speaker.as_deref(), Some("Voice"));
        assert_eq!(cue.settings, "align:start");
        request.caption_sidecar.as_mut().unwrap().track_id = TrackId::parse("missing").unwrap();
        assert!(request.validate().is_err());
    }
}
