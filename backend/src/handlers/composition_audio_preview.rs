//! Ephemeral native stereo PCM blocks. Source authorization precedes probing/decoding.
use crate::{
    domain::{
        nested_audio::{plan_audio_tracks, plan_audio_tracks_with_context},
        nested_composition::NestedCompositionProject,
        nested_dsp::validate_export_processing,
    },
    error::{ApiJson, AppError, AppResult},
    state::AppState,
    tools::{
        native_audio_export::{mix_project_audio_tracks_with_fade_clock, PcmSource},
    },
};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
static SLOTS: Semaphore = Semaphore::const_new(2);
const RATE: u64 = 48_000;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioPreviewRequest {
    pub project: NestedCompositionProject,
    pub composition_id: Option<String>,
    pub start_sample: u64,
    pub frames: u32,
}
impl AudioPreviewRequest {
    pub fn range_us(&self) -> AppResult<std::ops::Range<u64>> {
        if self.frames == 0 || self.frames > 96_000 {
            return Err(AppError::bad_request(
                "audio preview requires 1..96000 frames",
            ));
        }
        let end = self
            .start_sample
            .checked_add(u64::from(self.frames))
            .ok_or_else(|| AppError::bad_request("audio preview clock overflow"))?;
        let ticks = |sample| {
            u64::try_from(u128::from(sample) * 1_000_000 / u128::from(RATE))
                .map_err(|_| AppError::bad_request("audio preview clock overflow"))
        };
        let range = ticks(self.start_sample)?..ticks(end)?;
        if range.end > 9_007_199_254_740_991 {
            return Err(AppError::bad_request(
                "audio preview clock exceeds timeline precision",
            ));
        }
        Ok(range)
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
pub async fn preview(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<AudioPreviewRequest>,
) -> AppResult<Response> {
    let range = request.range_us()?;
    request
        .project
        .dependency_order()
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let focus = request
        .composition_id
        .as_deref()
        .unwrap_or(&request.project.root)
        .to_owned();
    let selected = plan_audio_tracks(&request.project, &focus, range.clone())
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    validate_export_processing(&selected).map_err(|e| AppError::bad_request(e.to_string()))?;
    let context = plan_audio_tracks_with_context(&request.project, &focus, range.clone())
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let mut stream_layers=context.clone();stream_layers.extend(selected.clone());
    let ids = context
        .iter()
        .chain(&selected)
        .map(|l| l.source_id().clone())
        .collect::<BTreeSet<_>>();
    let mut paths = BTreeMap::new();
    for id in ids {
        super::library::authorize_source_access(&state, id.as_str(), &headers).await?;
        let entry = state
            .library
            .get(id.as_str())
            .await
            .filter(|e| e.kind == "source")
            .ok_or_else(|| AppError::not_found("audio preview source not found"))?;
        let path = state
            .library
            .resolve_media_path(&entry)
            .await
            .map_err(|_| AppError::not_found("audio preview source not found"))?;
        paths.insert(id, path);
    }
    if !paths.is_empty() && !state.tools.ffmpeg {
        return Err(AppError::service_unavailable(
            "Audio preview requires FFmpeg/FFprobe",
        ));
    }
    let permit = SLOTS
        .acquire()
        .await
        .map_err(|_| AppError::service_unavailable("audio preview closed"))?;
    let cancel = state.shutdown_token();
    let _guard = CancelOnDrop(cancel.clone());
    let frames = request.frames;
    let start_sample = request.start_sample;
    // The supervised worker owns the permit until cancellation has finished cleanup.
    let worker = state.clone();
    let task = state.spawn_task(async move {
        let _permit = permit;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let remaining = || {
            deadline
                .checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| anyhow::anyhow!("audio preview timeout"))
        };
        let render = async {
            if paths.is_empty() {
                return Ok(vec![0u8; frames as usize * 8]);
            }
            let mut sources = BTreeMap::new();
            for (id, path) in paths {
                let selected=crate::tools::native_audio_export::preferred_audio_stream(&stream_layers,&id);
                let probe = crate::tools::pcm_conform::probe_pcm_metadata_for_stream(
                    &worker.process_runtime,
                    Path::new("ffprobe"),
                    &path,
                    remaining()?,
                    &cancel,
                    selected,
                )
                .await?;
                sources.insert(id, PcmSource::from_probe(path, probe)?);
            }
            let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
            let pcm = mix_project_audio_tracks_with_fade_clock(
                &worker.process_runtime,
                Path::new("ffmpeg"),
                &worker.staging_dir(),
                &request.project,
                &focus,
                range,
                u64::from(frames),
                &sources,
                // Includes per-stage live DSP reservations as well as PCM staging.
                // Two Loudness stages reserve 512 MiB before decoded/index/output.
                1024 * 1024 * 1024,
                remaining()?,
                &cancel,
                &progress,
                false,
            )
            .await?;
            let bytes = tokio::fs::read(pcm.path()).await?;
            anyhow::ensure!(
                bytes.len() == frames as usize * 8,
                "incorrect audio preview length"
            );
            Ok::<_, anyhow::Error>(bytes)
        };
        render.await
    });
    let bytes = task
        .await
        .map_err(|e| AppError::internal("audio preview task", e))?
        .map_err(|e| AppError::internal("native audio preview", e))?;
    Ok((
        [
            ("content-type", "application/octet-stream".to_owned()),
            ("cache-control", "private, no-store".to_owned()),
            ("x-audio-rate", RATE.to_string()),
            ("x-audio-channels", "2".to_owned()),
            ("x-audio-frames", frames.to_string()),
            ("x-audio-start-sample", start_sample.to_string()),
        ],
        bytes,
    )
        .into_response())
}
