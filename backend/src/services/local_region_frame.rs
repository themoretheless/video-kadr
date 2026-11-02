//! Exact ordinal PNG/plane identity from an immutable portable FFV1 region video.
//! Caller owns source ACL and an Analysis lease. No inference/runtime models.
pub use super::local_enhance_video::Tools;
use super::{
    local_enhance_video::{fingerprint, frames, guard, probe, remaining, same_rate},
    local_person_mask_video::Metadata,
};
use crate::{
    domain::media_probe::StreamKind,
    process_control::{capture_output_cancellable, ProcessRuntime},
};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::{process::Command, time::Instant};
use tokio_util::sync::CancellationToken;
pub struct Frame {
    pub png: Vec<u8>,
    pub mask_hash: String,
}
pub async fn png(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    movie: &Path,
    metadata: &Metadata,
    frame: u64,
    cancel: &CancellationToken,
    limit: Duration,
) -> Result<Frame> {
    ensure!(
        [movie, tools.ffmpeg, tools.ffprobe]
            .iter()
            .all(|p| p.is_absolute())
            && (metadata.width, metadata.height) == (256, 256)
            && metadata.frames > 0
            && metadata.frames <= 360000
            && frame < metadata.frames
            && metadata.frame_rate.numerator > 0
            && metadata.frame_rate.denominator > 0
            && metadata.size_bytes > 0
            && metadata.size_bytes <= 64 * 1024 * 1024 * 1024,
        "invalid region frame layout/clock"
    );
    for hash in [&metadata.mask_hash, &metadata.file_hash] {
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid region frame identity"
        );
    }
    ensure!(
        !cancel.is_cancelled() && !limit.is_zero() && limit <= Duration::from_secs(60),
        "invalid region frame deadline"
    );
    let deadline = Instant::now() + limit;
    let (tool, probe_tool, source, engine, probe_engine) = guard(
        async {
            ensure!(
                tokio::fs::symlink_metadata(movie)
                    .await?
                    .file_type()
                    .is_file(),
                "invalid region frame source"
            );
            let tool = tokio::fs::canonicalize(tools.ffmpeg).await?;
            let probe_tool = tokio::fs::canonicalize(tools.ffprobe).await?;
            let source = fingerprint(movie).await?;
            let engine = fingerprint(&tool).await?;
            let probe_engine = fingerprint(&probe_tool).await?;
            Ok((tool, probe_tool, source, engine, probe_engine))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source == (metadata.size_bytes, metadata.file_hash.clone()),
        "region frame file changed"
    );
    let (info, json) = probe(runtime, &probe_tool, movie, deadline, cancel).await?;
    let stream = info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Video)
        .context("region video missing")?;
    ensure!(
        info.streams.len() == 1
            && stream.codec_name.as_deref() == Some("ffv1")
            && stream.width == Some(256)
            && stream.height == Some(256)
            && stream.rotation_degrees.rem_euclid(360) == 0
            && frames(&json)? == metadata.frames
            && stream
                .frame_rate
                .is_some_and(|r| same_rate(metadata.frame_rate, r)),
        "region frame source layout changed"
    );
    let mut command = Command::new(&tool);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-threads",
            "2",
            "-i",
        ])
        .arg(movie)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf"])
        .arg(format!("select=eq(n\\,{frame})"))
        .args([
            "-frames:v",
            "1",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let mut policy = runtime.probe_policy(movie);
    policy.output.capture_bytes = 65537;
    let pixels = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(
        pixels.status.success() && pixels.stdout.len() == 65536,
        "incomplete region frame pixels"
    );
    let mask_hash = format!("{:x}", Sha256::digest(&pixels.stdout));
    // Always join bounded PNG encoding before returning/releasing caller's lease.
    let png = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let image =
            image::GrayImage::from_raw(256, 256, pixels.stdout).context("invalid region frame")?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image).write_to(&mut png, image::ImageFormat::Png)?;
        Ok(png.into_inner())
    })
    .await??;
    remaining(deadline, cancel)?;
    guard(
        async {
            ensure!(
                fingerprint(movie).await? == source
                    && fingerprint(&tool).await? == engine
                    && fingerprint(&probe_tool).await? == probe_engine,
                "region frame file/tool changed"
            );
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    Ok(Frame { png, mask_hash })
}
