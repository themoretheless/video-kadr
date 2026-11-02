//! Lossless ordinal mask-video transport for project ownership and archives.
pub use super::local_enhance_video::Tools;
use super::local_enhance_video::{
    execute, fingerprint, frames, guard, probe, remaining, same_rate,
};
use crate::{
    domain::{composition::FrameRateRatio, media_probe::StreamKind},
    process_control::ProcessRuntime,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::mpsc::UnboundedSender, time::Instant};
use tokio_util::sync::CancellationToken;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub struct Input<'a> {
    pub masks: &'a Path,
    pub mask_hash: &'a str,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: FrameRateRatio,
    pub staging_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: FrameRateRatio,
    pub mask_hash: String,
    pub file_hash: String,
    pub size_bytes: u64,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn layout(input: &Input<'_>) -> Result<(u64, u64)> {
    ensure!(
        [input.width, input.height]
            .iter()
            .all(|n| (64..=512).contains(n) && n % 16 == 0)
            && input.frames > 0
            && input.frames <= 360000,
        "invalid mask-video layout"
    );
    ensure!(
        input.frame_rate.numerator > 0 && input.frame_rate.denominator > 0,
        "invalid mask-video frame clock"
    );
    ensure!(
        input.masks.is_absolute()
            && input.mask_hash.len() == 64
            && input
                .mask_hash
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
        "invalid mask-video source identity"
    );
    let bytes = u64::from(input.width) * u64::from(input.height) * input.frames;
    let encoded = (bytes * 2 + 1024 * 1024).min(MAX_BYTES);
    ensure!(
        input.staging_bytes > 0
            && input.staging_bytes <= MAX_BYTES
            && bytes + encoded <= input.staging_bytes,
        "mask-video staging budget"
    );
    Ok((bytes, encoded))
}
#[allow(clippy::too_many_arguments)]
pub async fn encode(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    let (bytes, budget) = layout(&input)?;
    ensure!(
        destination.is_absolute()
            && tools.ffmpeg.is_absolute()
            && tools.ffprobe.is_absolute()
            && !destination.try_exists()?
            && !cancel.is_cancelled()
            && !limit.is_zero()
            && limit <= Duration::from_secs(3600),
        "invalid mask-video output/deadline"
    );
    let deadline = Instant::now() + limit;
    let ffmpeg_path = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffmpeg).await?) },
        deadline,
        cancel,
    )
    .await?;
    let ffprobe_path = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffprobe).await?) },
        deadline,
        cancel,
    )
    .await?;
    let tools = Tools {
        ffmpeg: &ffmpeg_path,
        ffprobe: &ffprobe_path,
    };
    let parent = destination.parent().context("mask-video parent missing")?;
    ensure!(parent.is_dir(), "mask-video parent missing");
    let (source, ffmpeg, ffprobe) = guard(
        async {
            Ok((
                fingerprint(input.masks).await?,
                fingerprint(tools.ffmpeg).await?,
                fingerprint(tools.ffprobe).await?,
            ))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source.0 == bytes && source.1 == input.mask_hash,
        "mask-video source identity mismatch"
    );
    let work = Owned(parent.join(format!(".person-mask-video-{}", uuid::Uuid::new_v4())));
    let movie = work.0.join("masks.mkv");
    let raw = work.0.join("verified.raw");
    guard(
        async {
            tokio::fs::create_dir(&work.0).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
            }
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    let mut command = Command::new(tools.ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-f",
            "rawvideo",
            "-pixel_format",
            "gray",
            "-video_size",
        ])
        .arg(format!("{}x{}", input.width, input.height))
        .arg("-framerate")
        .arg(input.frame_rate.to_string())
        .arg("-i")
        .arg(input.masks)
        .args([
            "-an",
            "-c:v",
            "ffv1",
            "-level",
            "3",
            "-pix_fmt",
            "gray",
            "-threads",
            "2",
            "-frames:v",
        ])
        .arg(input.frames.to_string())
        .arg("-fs")
        .arg(budget.to_string())
        .args(["-f", "matroska", "-progress", "pipe:1"])
        .arg(&movie);
    execute(
        runtime,
        command,
        vec![input.masks.to_owned(), work.0.clone()],
        &movie,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| (n as f64 / input.frames as f64 * 45.).min(45.))
        },
    )
    .await?;
    let (info, json) = probe(runtime, tools.ffprobe, &movie, deadline, cancel).await?;
    let stream = info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Video)
        .context("mask video missing")?;
    ensure!(
        info.streams.len() == 1
            && stream.codec_name.as_deref() == Some("ffv1")
            && stream.width == Some(input.width)
            && stream.height == Some(input.height)
            && frames(&json)? == input.frames
            && stream
                .frame_rate
                .is_some_and(|r| same_rate(input.frame_rate, r)),
        "encoded mask-video layout changed"
    );
    let mut command = Command::new(tools.ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(&movie)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-fps_mode",
            "passthrough",
            "-frames:v",
        ])
        .arg((input.frames + 1).to_string())
        .args(["-pix_fmt", "gray", "-f", "rawvideo", "-fs"])
        .arg((bytes + 1).to_string())
        .args(["-progress", "pipe:1"])
        .arg(&raw);
    execute(
        runtime,
        command,
        vec![work.0.clone()],
        &raw,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| 50. + (n as f64 / input.frames as f64 * 40.).min(40.))
        },
    )
    .await?;
    let file = guard(
        async {
            ensure!(
                fingerprint(&raw).await? == source,
                "lossless mask-video verification failed"
            );
            ensure!(
                fingerprint(input.masks).await? == source
                    && fingerprint(tools.ffmpeg).await? == ffmpeg
                    && fingerprint(tools.ffprobe).await? == ffprobe,
                "mask-video source/tools changed"
            );
            let file = fingerprint(&movie).await?;
            ensure!(file.0 > 0 && file.0 <= budget, "mask-video output budget");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&movie, std::fs::Permissions::from_mode(0o600)).await?;
            }
            tokio::fs::File::open(&movie).await?.sync_all().await?;
            Ok(file)
        },
        deadline,
        cancel,
    )
    .await?;
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(&movie, destination)
        .await
        .context("publish mask video without replacement")?;
    if cancel.is_cancelled() || Instant::now() >= deadline {
        tokio::fs::remove_file(destination).await?;
        anyhow::bail!("mask-video publication cancelled/expired");
    }
    let _ = progress.send(100.);
    Ok(Metadata {
        width: input.width,
        height: input.height,
        frames: input.frames,
        frame_rate: input.frame_rate,
        mask_hash: source.1,
        file_hash: file.1,
        size_bytes: file.0,
    })
}

/// Restore original learned planes from an accepted/archive-imported resource.
/// Caller supplies the saved identity, source ACL and the Analysis lease.
#[allow(clippy::too_many_arguments)]
pub async fn decode(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    movie: &Path,
    metadata: &Metadata,
    destination: &Path,
    staging_bytes: u64,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<()> {
    let (bytes, _) = layout(&Input {
        masks: movie,
        mask_hash: &metadata.mask_hash,
        width: metadata.width,
        height: metadata.height,
        frames: metadata.frames,
        frame_rate: metadata.frame_rate,
        staging_bytes,
    })?;
    ensure!(
        metadata.size_bytes > 0
            && metadata.size_bytes <= MAX_BYTES
            && metadata.file_hash.len() == 64
            && metadata
                .file_hash
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            && destination.is_absolute()
            && !destination.try_exists()?
            && tools.ffmpeg.is_absolute()
            && tools.ffprobe.is_absolute()
            && !cancel.is_cancelled()
            && !limit.is_zero()
            && limit <= Duration::from_secs(3600),
        "invalid mask-video decode identity/output/deadline"
    );
    let deadline = Instant::now() + limit;
    let ffmpeg_path = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffmpeg).await?) },
        deadline,
        cancel,
    )
    .await?;
    let ffprobe_path = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffprobe).await?) },
        deadline,
        cancel,
    )
    .await?;
    let tools = Tools {
        ffmpeg: &ffmpeg_path,
        ffprobe: &ffprobe_path,
    };
    let parent = destination.parent().context("mask decode parent missing")?;
    ensure!(parent.is_dir(), "mask decode parent missing");
    let (source, ffmpeg, ffprobe) = guard(
        async {
            Ok((
                fingerprint(movie).await?,
                fingerprint(tools.ffmpeg).await?,
                fingerprint(tools.ffprobe).await?,
            ))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source == (metadata.size_bytes, metadata.file_hash.clone()),
        "mask-video identity mismatch"
    );
    let (info, json) = probe(runtime, tools.ffprobe, movie, deadline, cancel).await?;
    let stream = info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Video)
        .context("mask video missing")?;
    ensure!(
        info.streams.len() == 1
            && stream.codec_name.as_deref() == Some("ffv1")
            && stream.width == Some(metadata.width)
            && stream.height == Some(metadata.height)
            && frames(&json)? == metadata.frames
            && stream
                .frame_rate
                .is_some_and(|r| same_rate(metadata.frame_rate, r)),
        "saved mask-video layout changed"
    );
    let work = Owned(parent.join(format!(".person-mask-video-{}", uuid::Uuid::new_v4())));
    guard(
        async {
            tokio::fs::create_dir(&work.0).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
            }
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    let raw = work.0.join("restored.raw");
    let mut command = Command::new(tools.ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(movie)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-fps_mode",
            "passthrough",
            "-frames:v",
        ])
        .arg((metadata.frames + 1).to_string())
        .args(["-pix_fmt", "gray", "-f", "rawvideo", "-fs"])
        .arg((bytes + 1).to_string())
        .args(["-progress", "pipe:1"])
        .arg(&raw);
    execute(
        runtime,
        command,
        vec![movie.to_owned(), work.0.clone()],
        &raw,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| (n as f64 / metadata.frames as f64 * 90.).min(90.))
        },
    )
    .await?;
    guard(
        async {
            ensure!(
                fingerprint(&raw).await? == (bytes, metadata.mask_hash.clone()),
                "restored mask planes changed"
            );
            ensure!(
                fingerprint(movie).await? == source
                    && fingerprint(tools.ffmpeg).await? == ffmpeg
                    && fingerprint(tools.ffprobe).await? == ffprobe,
                "mask-video source/tools changed"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                tokio::fs::set_permissions(&raw, std::fs::Permissions::from_mode(0o600)).await?;
            }
            tokio::fs::File::open(&raw).await?.sync_all().await?;
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(&raw, destination)
        .await
        .context("publish restored masks without replacement")?;
    if cancel.is_cancelled() || Instant::now() >= deadline {
        tokio::fs::remove_file(destination).await?;
        anyhow::bail!("mask restoration cancelled/expired");
    }
    let _ = progress.send(100.);
    Ok(())
}
