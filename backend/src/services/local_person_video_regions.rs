//! Encoded portable SAM regions -> model-free alpha movie with original audio.
//! Caller owns source/project ACL and an Analysis lease. No inference model needed.
use super::{
    local_enhance_video::{fingerprint, guard, remaining},
    local_person_mask_video, local_person_regions, local_person_video,
};
use crate::{domain::video_region::Attachment, process_control::ProcessRuntime};
use anyhow::{ensure, Context, Result};
pub use local_person_video::{CorrectionInput, Metadata, Tools};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{sync::mpsc::UnboundedSender, time::Instant};
use tokio_util::sync::CancellationToken;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub struct EncodedRegion<'a> {
    pub video: &'a Path,
    pub attachment: &'a Attachment,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Published(Vec<PathBuf>);
impl Drop for Published {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
fn bridge(
    target: &UnboundedSender<f64>,
    start: f64,
    end: f64,
) -> (UnboundedSender<f64>, tokio::task::JoinHandle<()>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let target = target.clone();
    let task = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            if value.is_finite() {
                let _ = target
                    .send((start + (end - start) * value.clamp(0., 100.) / 100.).clamp(start, end));
            }
        }
    });
    (tx, task)
}
async fn encoded(region: &EncodedRegion<'_>) -> Result<(u64, String)> {
    let info = tokio::fs::symlink_metadata(region.video).await?;
    ensure!(
        info.file_type().is_file() && info.len() > 0 && info.len() <= MAX_BYTES,
        "invalid encoded tracked region"
    );
    let id = fingerprint(region.video).await?;
    ensure!(
        id.1 == region.attachment.mask_file_hash,
        "encoded tracked region changed"
    );
    Ok(id)
}
#[allow(clippy::too_many_arguments)]
pub async fn correct(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    executable: &Path,
    input: CorrectionInput<'_>,
    regions: &[EncodedRegion<'_>],
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        !regions.is_empty()
            && regions.len() <= 1024
            && [
                input.video.source,
                input.masks,
                tools.ffmpeg,
                tools.ffprobe,
                executable,
                destination
            ]
            .iter()
            .all(|p| p.is_absolute())
            && !destination.try_exists()?
            && !cancel.is_cancelled()
            && !limit.is_zero()
            && limit <= Duration::from_secs(3600)
            && input.video.staging_bytes > 0
            && input.video.staging_bytes <= MAX_BYTES,
        "invalid tracked-region movie paths/limits"
    );
    if let Some(path) = input.video.mask_destination {
        ensure!(
            path.is_absolute() && path != destination && !path.try_exists()?,
            "invalid tracked movie mask destination"
        );
    }
    let first = regions[0].attachment;
    first.validate()?;
    let header = regions
        .iter()
        .map(|r| local_person_regions::RegionInput {
            path: r.video,
            attachment: r.attachment,
        })
        .collect::<Vec<_>>();
    let context = local_person_regions::Context {
        encoded_source_hash: input.video.source_hash,
        frame_rate: first.frame_rate,
        regions: &header,
    };
    local_person_regions::validate(
        &super::local_person_correct::Input {
            source: input.video.source,
            source_hash: input.video.source_hash,
            mask: input.masks,
            mask_hash: input.mask_hash,
            mask_destination: None,
            width: first.width,
            height: first.height,
            frames: first.frames,
            corrections: input.corrections,
        },
        &context,
    )?;
    let region_bytes = 65536 * first.frames * regions.len() as u64;
    let inner_budget = input
        .video
        .staging_bytes
        .checked_sub(region_bytes)
        .filter(|v| *v > 0)
        .context("tracked region movie staging budget")?;
    let deadline = Instant::now() + limit;
    let (source, before) = guard(
        async {
            ensure!(
                tokio::fs::symlink_metadata(input.video.source)
                    .await?
                    .file_type()
                    .is_file(),
                "invalid tracked movie source"
            );
            let source = fingerprint(input.video.source).await?;
            ensure!(
                source.1 == input.video.source_hash,
                "tracked movie source changed"
            );
            let mut before = Vec::with_capacity(regions.len());
            for region in regions {
                before.push(encoded(region).await?);
            }
            Ok((source, before))
        },
        deadline,
        cancel,
    )
    .await?;
    let parent = destination
        .parent()
        .context("tracked movie parent missing")?;
    let directory = parent.join(format!(".person-video-regions-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let work = Owned(directory);
    let mut paths = Vec::with_capacity(regions.len());
    for (index, region) in regions.iter().enumerate() {
        let path = work.0.join(format!("region-{index}.gray"));
        let stage_limit = remaining(deadline, cancel)?;
        let (tx, drain) = bridge(
            progress,
            index as f64 / regions.len() as f64 * 10.,
            (index + 1) as f64 / regions.len() as f64 * 10.,
        );
        let result = local_person_mask_video::decode(
            runtime,
            Tools {
                ffmpeg: tools.ffmpeg,
                ffprobe: tools.ffprobe,
            },
            region.video,
            &local_person_mask_video::Metadata {
                width: 256,
                height: 256,
                frames: region.attachment.frames,
                frame_rate: region.attachment.frame_rate,
                mask_hash: region.attachment.mask_hash.clone(),
                file_hash: region.attachment.mask_file_hash.clone(),
                size_bytes: before[index].0,
            },
            &path,
            input.video.staging_bytes - index as u64 * 65536 * first.frames,
            cancel,
            stage_limit,
            &tx,
        )
        .await;
        drop(tx);
        let _ = drain.await;
        result?;
        paths.push(path);
    }
    let raw = paths
        .iter()
        .zip(regions)
        .map(|(path, region)| local_person_regions::RegionInput {
            path,
            attachment: region.attachment,
        })
        .collect::<Vec<_>>();
    let context = local_person_regions::Context {
        encoded_source_hash: input.video.source_hash,
        frame_rate: first.frame_rate,
        regions: &raw,
    };
    let output = work.0.join("result.mov");
    let masks = work.0.join("result.gray");
    let stage_limit = remaining(deadline, cancel)?;
    let (tx, drain) = bridge(progress, 10., 99.);
    let result = local_person_video::correct_with_regions(
        runtime,
        Tools {
            ffmpeg: tools.ffmpeg,
            ffprobe: tools.ffprobe,
        },
        executable,
        CorrectionInput {
            video: local_person_video::Input {
                source: input.video.source,
                source_hash: input.video.source_hash,
                staging_bytes: inner_budget,
                mask_destination: Some(&masks),
            },
            masks: input.masks,
            mask_hash: input.mask_hash,
            corrections: input.corrections,
        },
        &context,
        &output,
        cancel,
        stage_limit,
        &tx,
    )
    .await;
    drop(tx);
    let _ = drain.await;
    let metadata = result?;
    guard(
        async {
            ensure!(
                fingerprint(input.video.source).await? == source,
                "tracked movie source changed"
            );
            for (region, id) in regions.iter().zip(&before) {
                ensure!(
                    encoded(region).await? == *id,
                    "tracked region video changed"
                );
            }
            ensure!(
                fingerprint(&output).await?.1 == metadata.output_hash
                    && fingerprint(&masks).await?.1 == metadata.mask_hash,
                "tracked movie result changed"
            );
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    let mut published = Published(Vec::new());
    remaining(deadline, cancel)?;
    if let Some(path) = input.video.mask_destination {
        tokio::fs::hard_link(&masks, path).await?;
        published.0.push(path.to_owned());
    }
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(&output, destination).await?;
    published.0.push(destination.to_owned());
    remaining(deadline, cancel)?;
    published.0.clear();
    let _ = progress.send(100.);
    Ok(metadata)
}
