//! Movie -> exact oriented RGBA -> prompted seed -> bidirectional regions -> FFV1.
//! Caller owns source ACL and an Analysis lease. No timeline edit or audio rewrite.
pub use super::local_enhance_video::Tools;
use super::{
    local_enhance_video::{execute, fingerprint, frames, guard, probe, remaining},
    local_person_mask_video,
    local_person_video::require_byte_color,
    local_sam, local_sam_track,
};
use crate::{
    domain::{composition::FrameRateRatio, media_probe::StreamKind},
    process_control::ProcessRuntime,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
    sync::mpsc::UnboundedSender,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub seed_frame: u64,
    pub points: &'a [[f64; 2]],
    pub mask_destination: Option<&'a Path>,
    pub staging_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: FrameRateRatio,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_times_us: Option<Vec<u64>>,
    pub seed_frame: u64,
    pub source_hash: String,
    pub decoded_hash: String,
    pub engine_hash: String,
    pub seed_region: local_sam::Metadata,
    pub tracking: local_sam_track::Metadata,
    pub region_video: local_person_mask_video::Metadata,
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
    span: f64,
) -> (UnboundedSender<f64>, tokio::task::JoinHandle<()>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let target = target.clone();
    let task = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            if value.is_finite() {
                let _ = target.send(start + span * value.clamp(0., 100.) / 100.);
            }
        }
    });
    (tx, task)
}
async fn private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await?;
    }
    file.write_all(bytes).await?;
    file.sync_all().await?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub async fn track(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    config: &local_sam::Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    local_sam::validate_points(input.points)?;
    ensure!(
        [input.source, destination, tools.ffmpeg, tools.ffprobe]
            .iter()
            .all(|p| p.is_absolute())
            && !destination.try_exists()?
            && !cancel.is_cancelled()
            && !limit.is_zero()
            && limit <= Duration::from_secs(3600)
            && input.staging_bytes > 0
            && input.staging_bytes <= MAX_BYTES,
        "invalid SAM movie paths/deadline/budget"
    );
    ensure!(
        input.source_hash.len() == 64
            && input
                .source_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid SAM movie identity"
    );
    if let Some(mask) = input.mask_destination {
        ensure!(
            mask.is_absolute() && mask != destination && !mask.try_exists()?,
            "invalid SAM raw region destination"
        );
    }
    let parent = destination.parent().context("SAM movie parent missing")?;
    ensure!(parent.is_dir(), "SAM movie parent missing");
    let deadline = Instant::now() + limit;
    let (ffmpeg, ffprobe, tools_hash, engine, source) = guard(
        async {
            let ffmpeg = tokio::fs::canonicalize(tools.ffmpeg).await?;
            let ffprobe = tokio::fs::canonicalize(tools.ffprobe).await?;
            let hashes = (fingerprint(&ffmpeg).await?, fingerprint(&ffprobe).await?);
            Ok((
                ffmpeg,
                ffprobe,
                hashes,
                local_sam::identity(config).await?,
                fingerprint(input.source).await?,
            ))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source.1 == input.source_hash,
        "SAM movie source identity changed"
    );
    let (info, json) = probe(runtime, &ffprobe, input.source, deadline, cancel).await?;
    let count = frames(&json)?;
    ensure!(
        count > 0 && count <= 360000 && input.seed_frame < count,
        "SAM movie seed/count outside source"
    );
    let video = info
        .streams
        .iter()
        .find(|stream| stream.kind == StreamKind::Video)
        .context("SAM source video missing")?;
    require_byte_color(runtime, &ffprobe, video, input.source, deadline, cancel).await?;
    let rate = video.frame_rate.context("SAM video frame rate missing")?;
    let rate = FrameRateRatio {
        numerator: rate.numerator.try_into()?,
        denominator: rate.denominator.try_into()?,
    };
    ensure!(
        video.rotation_degrees.rem_euclid(90) == 0,
        "unsupported SAM movie orientation"
    );
    let (width, height) = if video.rotation_degrees.rem_euclid(180) == 0 {
        (info.width, info.height)
    } else {
        (info.height, info.width)
    };
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "SAM movie geometry budget"
    );
    let tick = video
        .time_base
        .filter(|t| t.numerator > 0 && t.denominator > 0)
        .map(|t| t.numerator as f64 / t.denominator as f64)
        .unwrap_or(0.);
    let frame_times_us = super::sam_video_clock::probe(runtime, &ffprobe, input.source, count, rate, tick + 0.000002, deadline, cancel).await?;
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    let raw_bytes = frame_bytes
        .checked_mul(count)
        .context("SAM raw source overflow")?;
    let mask_bytes = 65536u64 * count;
    let budget = raw_bytes
        .checked_add(
            mask_bytes
                .checked_mul(5)
                .context("SAM mask budget overflow")?,
        )
        .and_then(|n| n.checked_add(frame_bytes.checked_mul(2)?))
        .and_then(|n| n.checked_add(4 * 1024 * 1024))
        .context("SAM staging overflow")?;
    ensure!(
        budget <= input.staging_bytes,
        "SAM movie staging budget exceeded"
    );
    let file_limit = runtime
        .render_policy(vec![input.source.to_owned()], destination)
        .kernel
        .file_size_bytes;
    ensure!(
        raw_bytes <= file_limit && mask_bytes * 2 + 1024 * 1024 <= file_limit,
        "SAM movie process file budget"
    );
    let work_path = parent.join(format!(".sam-video-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&work_path)?;
    let work = Owned(work_path);
    remaining(deadline, cancel)?;
    let raw = work.0.join("source.rgba");
    let image = work.0.join("seed.png");
    let region = work.0.join("seed-region.png");
    let seed = work.0.join("seed.gray");
    let tracks = work.0.join("tracked.masks");
    let encoded = work.0.join("regions.mkv");
    let mut command = Command::new(&ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-threads",
            "2",
            "-i",
        ])
        .arg(input.source)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-fps_mode",
            "passthrough",
            "-frames:v",
        ])
        .arg((count + 1).to_string())
        .args(["-pix_fmt", "rgba", "-f", "rawvideo", "-fs"])
        .arg((raw_bytes + 1).to_string())
        .args(["-progress", "pipe:1"])
        .arg(&raw);
    execute(
        runtime,
        command,
        vec![input.source.to_owned(), work.0.clone()],
        &raw,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| (n as f64 / count as f64 * 20.).min(20.))
        },
    )
    .await?;
    let decoded = guard(
        async {
            let decoded = fingerprint(&raw).await?;
            ensure!(
                decoded.0 == raw_bytes,
                "SAM source decoded frame count changed"
            );
            Ok(decoded)
        },
        deadline,
        cancel,
    )
    .await?;
    let seed_hash = async {
        let mut file = tokio::fs::File::open(&raw).await?;
        file.seek(std::io::SeekFrom::Start(input.seed_frame * frame_bytes))
            .await?;
        let mut rgba = vec![0u8; usize::try_from(frame_bytes)?];
        file.read_exact(&mut rgba).await?;
        let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let source = image::RgbaImage::from_raw(width, height, rgba)
                .context("SAM seed shape invalid")?;
            let mut png = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(source).write_to(&mut png, image::ImageFormat::Png)?;
            Ok(png.into_inner())
        })
        .await??;
        remaining(deadline, cancel)?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        private(&image, &bytes).await?;
        Ok::<_, anyhow::Error>(hash)
    }
    .await?;
    let (tx, drain) = bridge(progress, 20., 20.);
    let result = async {
        local_sam::region(
            runtime,
            config,
            local_sam::Input {
                source: &image,
                source_hash: &seed_hash,
                points: input.points,
            },
            &region,
            cancel,
            remaining(deadline, cancel)?,
            &tx,
        )
        .await
    }
    .await;
    drop(tx);
    let _ = drain.await;
    let seed_region = result?;
    let seed_bytes = async {
        let mut bytes = Vec::new();
        tokio::fs::File::open(&region)
            .await?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        ensure!(
            bytes.len() <= 1024 * 1024
                && format!("{:x}", Sha256::digest(&bytes)) == seed_region.output_hash,
            "SAM seed PNG changed"
        );
        let raw = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut reader = image::ImageReader::with_format(
                std::io::Cursor::new(bytes),
                image::ImageFormat::Png,
            );
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(1024 * 1024);
            reader.limits(limits);
            let mask = reader.decode()?.into_luma8();
            ensure!(
                mask.dimensions() == (256, 256),
                "SAM seed mask shape changed"
            );
            Ok(mask.into_raw())
        })
        .await??;
        remaining(deadline, cancel)?;
        ensure!(
            format!("{:x}", Sha256::digest(&raw)) == seed_region.mask_hash,
            "SAM seed raw bytes changed"
        );
        private(&seed, &raw).await?;
        Ok::<_, anyhow::Error>(raw)
    }
    .await?;
    drop(seed_bytes);
    let (tx, drain) = bridge(progress, 40., 45.);
    let result = async {
        local_sam_track::track(
            runtime,
            config,
            local_sam_track::Input {
                source: &raw,
                source_hash: &decoded.1,
                seed: &seed,
                seed_hash: &seed_region.mask_hash,
                width,
                height,
                frames: count,
                seed_frame: input.seed_frame,
            },
            &tracks,
            cancel,
            remaining(deadline, cancel)?,
            &tx,
        )
        .await
    }
    .await;
    drop(tx);
    let _ = drain.await;
    let tracking = result?;
    let (tx, drain) = bridge(progress, 85., 14.);
    let result = async {
        local_person_mask_video::encode(
            runtime,
            Tools {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
            },
            local_person_mask_video::Input {
                masks: &tracks,
                mask_hash: &tracking.output_hash,
                width: 256,
                height: 256,
                frames: count,
                frame_rate: rate,
                staging_bytes: input.staging_bytes,
            },
            &encoded,
            cancel,
            remaining(deadline, cancel)?,
            &tx,
        )
        .await
    }
    .await;
    drop(tx);
    let _ = drain.await;
    let region_video = result?;
    guard(
        async {
            ensure!(
                local_sam::identity(config).await? == engine
                    && fingerprint(input.source).await? == source
                    && fingerprint(&ffmpeg).await? == tools_hash.0
                    && fingerprint(&ffprobe).await? == tools_hash.1
                    && fingerprint(&raw).await? == decoded,
                "SAM movie source/engine/tools changed"
            );
            ensure!(
                seed_region.width == width
                    && seed_region.height == height
                    && seed_region.source_hash == seed_hash
                    && seed_region.runtime_hash == engine.runtime_hash
                    && tracking.runtime_hash == engine.runtime_hash
                    && tracking.source_hash == decoded.1
                    && tracking.seed_hash == seed_region.mask_hash
                    && tracking.output_hash == region_video.mask_hash,
                "SAM movie stage identities mismatch"
            );
            ensure!(
                fingerprint(&tracks).await? == (mask_bytes, tracking.output_hash.clone())
                    && fingerprint(&encoded).await?
                        == (region_video.size_bytes, region_video.file_hash.clone()),
                "SAM movie region outputs changed"
            );
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    remaining(deadline, cancel)?;
    let mut published = Published(Vec::new());
    if let Some(mask) = input.mask_destination {
        tokio::fs::hard_link(&tracks, mask)
            .await
            .context("publish SAM raw regions without replacement")?;
        published.0.push(mask.to_owned());
    }
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(&encoded, destination)
        .await
        .context("publish SAM region movie without replacement")?;
    published.0.push(destination.to_owned());
    remaining(deadline, cancel)?;
    published.0.clear();
    let _ = progress.send(100.);
    Ok(Metadata {
        schema_version: 1,
        width,
        height,
        frames: count,
        frame_rate: rate,
        frame_times_us,
        seed_frame: input.seed_frame,
        source_hash: input.source_hash.to_owned(),
        decoded_hash: decoded.1,
        engine_hash: engine.hash,
        seed_region,
        tracking,
        region_video,
    })
}
