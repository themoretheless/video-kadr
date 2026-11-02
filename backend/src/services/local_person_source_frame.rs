//! Model-free, exact decoder-ordinal source frame for the video-mask canvas.
//! Caller owns source ACL and an Analysis permit. No seek/time rounding or resize.
pub use super::local_enhance_video::Tools;
use super::local_enhance_video::{fingerprint, guard, remaining};
use crate::process_control::{capture_output_cancellable, ProcessRuntime};
use anyhow::{ensure, Context, Result};
use std::{path::Path, time::Duration};
use tokio::{process::Command, time::Instant};
use tokio_util::sync::CancellationToken;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame: u64,
}
pub async fn png(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    input: Input<'_>,
    cancel: &CancellationToken,
    limit: Duration,
) -> Result<Vec<u8>> {
    ensure!(
        input.source.is_absolute()
            && tools.ffmpeg.is_absolute()
            && tools.ffprobe.is_absolute()
            && input.width > 0
            && input.height > 0
            && u64::from(input.width) * u64::from(input.height) <= 64 * 1024 * 1024
            && input.frames > 0
            && input.frames <= 360000
            && input.frame < input.frames,
        "invalid source-frame layout"
    );
    ensure!(
        input.source_hash.len() == 64
            && input
                .source_hash
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
        "invalid source-frame identity"
    );
    ensure!(
        !cancel.is_cancelled() && !limit.is_zero() && limit <= Duration::from_secs(60),
        "invalid source-frame deadline"
    );
    let deadline = Instant::now() + limit;
    let tool = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffmpeg).await?) },
        deadline,
        cancel,
    )
    .await?;
    let probe_tool = guard(
        async { Ok(tokio::fs::canonicalize(tools.ffprobe).await?) },
        deadline,
        cancel,
    )
    .await?;
    let (source, engine, probe_engine) = guard(
        async {
            Ok((
                fingerprint(input.source).await?,
                fingerprint(&tool).await?,
                fingerprint(&probe_tool).await?,
            ))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(source.1 == input.source_hash, "source-frame file changed");
    let mut command = Command::new(&probe_tool);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-select_streams",
            "v:0",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(input.source);
    let info = capture_output_cancellable(
        runtime,
        command,
        runtime.probe_policy(input.source),
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(info.status.success(), "source-frame geometry probe failed");
    let value: serde_json::Value = serde_json::from_slice(&info.stdout)?;
    let info = crate::domain::media_probe::ProbeResult::from_ffprobe_json(&value)?;
    let video = info
        .streams
        .iter()
        .find(|s| s.kind == crate::domain::media_probe::StreamKind::Video)
        .context("source-frame video missing")?;
    ensure!(
        video.rotation_degrees.rem_euclid(90) == 0,
        "unsupported source-frame orientation"
    );
    let (width, height) = if video.rotation_degrees.rem_euclid(180) == 0 {
        (info.width, info.height)
    } else {
        (info.height, info.width)
    };
    ensure!(
        (width, height) == (input.width, input.height),
        "source-frame geometry changed"
    );
    let bytes = usize::try_from(u64::from(input.width) * u64::from(input.height) * 4)?;
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
        .arg(input.source)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf"])
        .arg(format!("select=eq(n\\,{})", input.frame))
        .args([
            "-frames:v",
            "1",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let mut policy = runtime.probe_policy(input.source);
    policy.output.capture_bytes = bytes.max(64 * 1024);
    let output = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(
        output.status.success() && output.stdout.len() == bytes,
        "incomplete source-frame pixels"
    );
    let width = input.width;
    let height = input.height;
    // Keep the caller's owned task/permit until bounded PNG encoding has actually joined.
    let encoded = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let image = image::RgbaImage::from_raw(width, height, output.stdout)
            .context("invalid source frame")?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image).write_to(&mut png, image::ImageFormat::Png)?;
        Ok(png.into_inner())
    })
    .await??;
    remaining(deadline, cancel)?;
    guard(
        async {
            ensure!(
                fingerprint(input.source).await? == source
                    && fingerprint(&tool).await? == engine
                    && fingerprint(&probe_tool).await? == probe_engine,
                "source-frame source/tool changed"
            );
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    Ok(encoded)
}
