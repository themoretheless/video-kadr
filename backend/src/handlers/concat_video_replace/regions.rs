use anyhow::{ensure, Result};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

use crate::{
    domain::{composition::CompositionSource, cutout::Stroke, video_region::Attachment},
    library::MediaEntry,
    services::{
        concat_video_sam_seed, local_enhance_video::ResolvedTools, local_sam::Config,
        local_sam_video,
    },
    state::AppState,
};

pub(crate) struct Regions {
    files: Vec<(Attachment, String, PathBuf)>,
    entries: Vec<MediaEntry>,
    staging: tempfile::TempDir,
    published: Vec<(String, PathBuf)>,
    bytes: u64,
}

impl Regions {
    pub(super) fn new(state: &AppState) -> Result<Self> {
        Ok(Self {
            files: Vec::new(),
            entries: Vec::new(),
            published: Vec::new(),
            bytes: 0,
            staging: tempfile::tempdir_in(state.staging_dir())?,
        })
    }

    pub(super) async fn track(
        &mut self,
        state: &AppState,
        config: &Config,
        tools: &ResolvedTools,
        source: &CompositionSource,
        path: &Path,
        source_hash: &str,
        stroke: &mut Stroke,
        cancel: &CancellationToken,
    ) -> Result<(CompositionSource, Attachment)> {
        let time = (stroke.at.expect("validated track has a timed stroke") * 1e6).round() as u64;
        let seed =
            concat_video_sam_seed::resolve_held(&state.process_runtime, path, time, cancel).await?;
        if seed.source_us != time {
            stroke.at = Some(seed.source_us as f64 / 1e6);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{id}.mkv");
        let movie_path = self.staging.path().join(&filename);
        let (progress, _) = tokio::sync::mpsc::unbounded_channel();
        let movie = local_sam_video::track(
            &state.process_runtime,
            local_sam_video::Tools {
                ffmpeg: &tools.ffmpeg,
                ffprobe: &tools.ffprobe,
            },
            config,
            local_sam_video::Input {
                source: path,
                source_hash,
                seed_frame: seed.seed_frame,
                points: &stroke.points,
                mask_destination: None,
                staging_bytes: super::CACHE_BUDGET - self.bytes,
            },
            &movie_path,
            cancel,
            Duration::from_secs(1800),
            &progress,
        )
        .await?;
        let file_bytes = tokio::fs::metadata(&movie_path).await?.len();
        self.bytes = self
            .bytes
            .checked_add(file_bytes)
            .ok_or_else(|| anyhow::anyhow!("SAM region budget overflow"))?;
        ensure!(
            self.bytes <= super::CACHE_BUDGET,
            "SAM regions exceed their storage budget"
        );
        let duration = if let Some(end) =
            movie.frame_times_us.as_ref().and_then(|times| times.last())
        {
            *end
        } else {
            u64::try_from(
                (u128::from(movie.frames) * u128::from(movie.frame_rate.denominator) * 1_000_000)
                    .div_ceil(u128::from(movie.frame_rate.numerator)),
            )?
        };
        let snapshot = Attachment {
            source_id: source.id.as_str().into(),
            mask_source_id: id.clone(),
            source_hash: movie.source_hash,
            mask_hash: movie.tracking.output_hash,
            mask_file_hash: movie.region_video.file_hash,
            encoder_hash: movie.tracking.encoder_hash,
            decoder_hash: movie.tracking.decoder_hash,
            runtime_hash: movie.tracking.runtime_hash,
            prompt_hash: movie.seed_region.prompt_hash,
            points: stroke.points.clone(),
            width: movie.width,
            height: movie.height,
            frames: movie.frames,
            frame_rate: movie.frame_rate,
            frame_times_us: movie.frame_times_us,
            seed_frame: movie.seed_frame,
            tracked_frames: movie.tracking.tracked_frames,
            forward_lost_at: movie.tracking.forward_lost_at,
            backward_lost_at: movie.tracking.backward_lost_at,
        };
        snapshot.validate()?;
        let descriptor = serde_json::from_value(json!({
            "id":id,"kind":"video","durationTicks":duration,"width":256,"height":256,"hasAudio":false,
        }))?;
        self.files.push((snapshot.clone(), id.clone(), movie_path));
        self.entries.push(MediaEntry::from_result("source", &json!({
            "id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video",
            "duration":duration as f64 / 1e6,"width":256,"height":256,"vcodec":"ffv1","sizeBytes":file_bytes,
            "title":"Видеорегион SAM",
        })));
        Ok((descriptor, snapshot))
    }

    pub(crate) fn files(&self) -> &[(Attachment, String, PathBuf)] {
        &self.files
    }

    pub(crate) fn entries(&self) -> &[MediaEntry] {
        &self.entries
    }

    pub(super) async fn publish(&mut self, state: &AppState, space_id: Option<&str>) -> Result<()> {
        for (entry, (_, _, staged)) in self.entries.iter().zip(&self.files) {
            let path = state.sources_dir().join(&entry.filename);
            tokio::fs::hard_link(staged, &path).await?;
            self.published.push((entry.id.clone(), path));
            ensure!(
                state.library.add(entry.clone()).await,
                "Не удалось сохранить область SAM"
            );
            if let Some(space) = space_id {
                state
                    .library
                    .relocate_source_to_space(&entry.id, space)
                    .await?;
            }
        }
        Ok(())
    }

    pub(super) async fn rollback(&mut self, state: &AppState) {
        for (id, path) in self.published.drain(..).rev() {
            if !state.library.remove(&id).await {
                let _ = tokio::fs::remove_file(path).await;
            }
        }
    }
}
