//! Trusted video decode -> learned frame sequence -> H264 + original audio.
//! Caller owns source authorization, model pinning and Analysis admission.
use super::{
    local_enhance::{self, Config},
    local_enhance_sequence,
};
use crate::{
    domain::{
        composition::FrameRateRatio,
        media_probe::{ProbeResult, StreamKind},
    },
    process_control::{
        capture_output_cancellable, stream_with_progress, ProcessRuntime, ProcessStatus,
    },
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, sync::mpsc::UnboundedSender, time::Instant};
use tokio_util::sync::CancellationToken;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub struct Tools<'a> {
    pub ffmpeg: &'a Path,
    pub ffprobe: &'a Path,
}
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub factor: u32,
    pub staging_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub factor: u32,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub source_frames: u64,
    pub frame_rate: FrameRateRatio,
    pub audio_copied: bool,
    pub audio_codec: Option<String>,
    pub source_hash: String,
    pub engine_hash: String,
    pub output_hash: String,
    pub duration: f64,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn remaining(deadline: Instant, cancel: &CancellationToken) -> Result<Duration> {
    ensure!(!cancel.is_cancelled(), "video enhancement cancelled");
    let left = deadline.saturating_duration_since(Instant::now());
    ensure!(!left.is_zero(), "video enhancement expired");
    Ok(left)
}
pub(super) async fn guard<T>(
    future: impl std::future::Future<Output = Result<T>>,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<T> {
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("video enhancement cancelled"),result=tokio::time::timeout_at(deadline,future)=>result?}
}
pub(super) async fn fingerprint(path: &Path) -> Result<(u64, String)> {
    let m = tokio::fs::symlink_metadata(path).await?;
    ensure!(
        m.file_type().is_file() && m.len() <= MAX_BYTES,
        "invalid video input/output file"
    );
    Ok(crate::project_archive::fingerprint_file_with_limit(path, MAX_BYTES).await?)
}
pub(super) async fn probe(
    runtime: &ProcessRuntime,
    tool: &Path,
    path: &Path,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<(ProbeResult, Value)> {
    let mut command = Command::new(tool);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-count_frames",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path);
    let out = capture_output_cancellable(
        runtime,
        command,
        runtime.probe_policy(path),
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(out.status.success(), "video probe failed");
    let json: Value = serde_json::from_slice(&out.stdout)?;
    Ok((ProbeResult::from_ffprobe_json(&json)?, json))
}
pub(super) fn frames(json: &Value) -> Result<u64> {
    let count = json["streams"]
        .as_array()
        .context("missing video streams")?
        .iter()
        .find(|s| s["codec_type"] == "video")
        .and_then(|s| s["nb_read_frames"].as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .context("video frame count unavailable")?;
    ensure!(
        (1..=360000).contains(&count),
        "video frame count outside limit"
    );
    Ok(count)
}
pub(super) async fn execute(
    runtime: &ProcessRuntime,
    command: Command,
    inputs: Vec<PathBuf>,
    output: &Path,
    deadline: Instant,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    parse: impl Fn(&str) -> Option<f64> + Send + Sync,
) -> Result<()> {
    let (status, diagnostic) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(inputs, output),
        parse,
        progress,
        cancel,
        remaining(deadline, cancel)?,
    )
    .await?;
    ensure!(
        status == ProcessStatus::Ok,
        "video codec stage failed: {status:?} {diagnostic}"
    );
    Ok(())
}
pub(super) fn same_rate(a: FrameRateRatio, b: crate::domain::media_probe::Rational) -> bool {
    b.numerator > 0
        && b.denominator > 0
        && i128::from(a.numerator) * i128::from(b.denominator)
            == i128::from(a.denominator) * i128::from(b.numerator)
}
#[allow(clippy::too_many_arguments)]
pub async fn enhance(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        input.source.is_absolute()
            && destination.is_absolute()
            && tools.ffmpeg.is_absolute()
            && tools.ffprobe.is_absolute()
            && [1, 2, 4].contains(&input.factor),
        "invalid video enhancement paths/factor"
    );
    ensure!(
        !limit.is_zero()
            && limit <= Duration::from_secs(3600)
            && input.staging_bytes > 0
            && input.staging_bytes <= MAX_BYTES,
        "invalid video enhancement limits"
    );
    ensure!(
        !destination.try_exists()?,
        "video enhancement output exists"
    );
    let parent = destination
        .parent()
        .context("video output parent missing")?;
    ensure!(parent.is_dir(), "video output directory missing");
    let deadline = Instant::now() + limit;
    let (ffmpeg, ffprobe, tools_before, engine_before, source_before) = guard(
        async {
            let ffmpeg = tokio::fs::canonicalize(tools.ffmpeg).await?;
            let ffprobe = tokio::fs::canonicalize(tools.ffprobe).await?;
            let hashes = (fingerprint(&ffmpeg).await?, fingerprint(&ffprobe).await?);
            let engine = local_enhance::identity(config).await?;
            let source = fingerprint(input.source).await?;
            anyhow::Ok((ffmpeg, ffprobe, hashes, engine, source))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source_before.1 == input.source_hash,
        "video source digest mismatch"
    );
    let (info, json) = probe(runtime, &ffprobe, input.source, deadline, cancel).await?;
    let source_frames = frames(&json)?;
    let video = info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Video)
        .context("video stream missing")?;
    let rate = video.frame_rate.context("video frame rate missing")?;
    ensure!(
        rate.numerator > 0 && rate.denominator > 0,
        "invalid video frame rate"
    );
    let frame_rate = FrameRateRatio {
        numerator: rate.numerator.try_into()?,
        denominator: rate.denominator.try_into()?,
    };
    ensure!(
        video.rotation_degrees.rem_euclid(90) == 0,
        "unsupported non-quadrant video rotation"
    );
    let (mut w, mut h) = (info.width, info.height);
    if video.rotation_degrees.rem_euclid(180) != 0 {
        std::mem::swap(&mut w, &mut h);
    }
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024,
        "video dimensions exceed limit"
    );
    let width = w
        .checked_mul(input.factor)
        .context("video width overflow")?
        & !1;
    let height = h
        .checked_mul(input.factor)
        .context("video height overflow")?
        & !1;
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "enhanced video dimensions exceed limit"
    );
    let raw_bytes = (u64::from(w) * u64::from(h) * 4)
        .checked_mul(source_frames)
        .context("video raw size overflow")?;
    let enhanced_bytes = (u64::from(width) * u64::from(height) * 4)
        .checked_mul(source_frames)
        .context("enhanced raw size overflow")?;
    let encoded_budget = enhanced_bytes.max(1024 * 1024);
    let total = encoded_budget
        .checked_mul(2)
        .and_then(|b| b.checked_add(raw_bytes))
        .and_then(|b| b.checked_add(enhanced_bytes))
        .context("video staging overflow")?;
    ensure!(total <= input.staging_bytes, "video exceeds staging budget");
    let file_budget = runtime
        .render_policy(vec![input.source.into()], destination)
        .kernel
        .file_size_bytes;
    ensure!(
        raw_bytes <= file_budget && enhanced_bytes <= file_budget && encoded_budget <= file_budget,
        "video exceeds process file budget"
    );
    let work = Owned(parent.join(format!(".enhance-video-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let raw = work.0.join("decoded.rgba");
    let enhanced = work.0.join("enhanced.rgba");
    let silent = work.0.join("silent.mp4");
    let muxed = work.0.join("audio.mp4");
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
        .arg((source_frames + 1).to_string())
        .args(["-pix_fmt", "rgba", "-f", "rawvideo", "-fs"])
        .arg((raw_bytes + 1).to_string())
        .args(["-progress", "pipe:1"])
        .arg(&raw);
    execute(
        runtime,
        command,
        vec![input.source.into(), work.0.clone()],
        &raw,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|n| (n as f64 / source_frames as f64 * 10.).min(10.))
        },
    )
    .await?;
    let raw_identity = guard(fingerprint(&raw), deadline, cancel).await?;
    ensure!(
        raw_identity.0 == raw_bytes,
        "decoded video frame count/dimensions changed"
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let updates = progress.clone();
    let drain = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            let _ = updates.send(10. + value * 0.75);
        }
    });
    let learned = local_enhance_sequence::enhance(
        runtime,
        config,
        local_enhance_sequence::Input {
            source: &raw,
            source_hash: &raw_identity.1,
            width: w,
            height: h,
            frames: source_frames,
            factor: input.factor,
        },
        &enhanced,
        cancel,
        remaining(deadline, cancel)?,
        &tx,
    )
    .await;
    drop(tx);
    let _ = drain.await;
    let learned = learned?;
    ensure!(
        learned.width == width && learned.height == height && learned.frames == source_frames,
        "learned video layout mismatch"
    );
    let mut command = Command::new(&ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
        ])
        .arg(format!("{width}x{height}"))
        .arg("-framerate")
        .arg(frame_rate.to_string())
        .arg("-i")
        .arg(&enhanced)
        .args([
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "16",
            "-pix_fmt",
            "yuv420p",
            "-color_range",
            "tv",
            "-threads",
            "2",
            "-fs",
        ])
        .arg(encoded_budget.to_string())
        .args([
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-f",
            "mp4",
        ])
        .arg(&silent);
    execute(
        runtime,
        command,
        vec![enhanced.clone(), work.0.clone()],
        &silent,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|n| 85. + (n as f64 / source_frames as f64 * 10.).min(10.))
        },
    )
    .await?;
    let (encoded, encoded_json) = probe(runtime, &ffprobe, &silent, deadline, cancel).await?;
    ensure!(
        frames(&encoded_json)? == source_frames
            && encoded.width == width
            && encoded.height == height
            && encoded
                .streams
                .iter()
                .find(|s| s.kind == StreamKind::Video)
                .and_then(|s| s.frame_rate)
                .is_some_and(|rate| same_rate(frame_rate, rate)),
        "encoded video failed frame/rate validation"
    );
    let audio = info.streams.iter().find(|s| s.kind == StreamKind::Audio);
    let result = if audio.is_some() {
        let mut command = Command::new(&ffmpeg);
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
            .arg(&silent)
            .args(["-protocol_whitelist", crate::tools::OFFLINE_PROTOCOLS, "-i"])
            .arg(input.source)
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c",
                "copy",
                "-shortest",
                "-movflags",
                "+faststart",
                "-fs",
            ])
            .arg(encoded_budget.to_string())
            .args(["-f", "mp4"])
            .arg(&muxed);
        execute(
            runtime,
            command,
            vec![silent.clone(), input.source.into(), work.0.clone()],
            &muxed,
            deadline,
            cancel,
            progress,
            |_| None,
        )
        .await?;
        &muxed
    } else {
        &silent
    };
    let _ = progress.send(98.);
    let (final_info, final_json) = probe(runtime, &ffprobe, result, deadline, cancel).await?;
    let count = frames(&final_json)?;
    ensure!(
        final_info.width == width
            && final_info.height == height
            && count <= source_frames
            && (audio.is_some() || count == source_frames)
            && final_info.duration.is_finite()
            && final_info.duration > 0.,
        "final video layout/count invalid"
    );
    let final_audio = final_info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Audio);
    ensure!(
        match (audio, final_audio) {
            (None, None) => true,
            (Some(a), Some(b)) =>
                a.codec_name == b.codec_name
                    && a.channels == b.channels
                    && a.sample_rate == b.sample_rate,
            _ => false,
        },
        "original audio stream was lost or changed"
    );
    let output_identity = guard(
        async {
            ensure!(
                fingerprint(input.source).await? == source_before
                    && fingerprint(&ffmpeg).await? == tools_before.0
                    && fingerprint(&ffprobe).await? == tools_before.1
                    && local_enhance::identity(config).await?.hash == engine_before.hash,
                "video source or engine changed"
            );
            let output = fingerprint(result).await?;
            ensure!(
                output.0 > 0 && output.0 <= encoded_budget,
                "encoded video exceeds budget"
            );
            anyhow::Ok(output)
        },
        deadline,
        cancel,
    )
    .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(result, std::fs::Permissions::from_mode(0o600)).await?;
    }
    tokio::fs::File::open(result).await?.sync_all().await?;
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(result, destination)
        .await
        .context("publish learned video without replacement")?;
    let _ = progress.send(100.);
    Ok(Metadata {
        factor: input.factor,
        width,
        height,
        frames: count,
        source_frames,
        frame_rate,
        audio_copied: audio.is_some(),
        audio_codec: audio.and_then(|s| s.codec_name.clone()),
        source_hash: input.source_hash.into(),
        engine_hash: engine_before.hash,
        output_hash: output_identity.1,
        duration: final_info.duration,
    })
}

/// Resolve only host PATH entries, never a user-provided executable or working directory.
pub struct ResolvedTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub identity: String,
}
pub async fn resolve_tools() -> Result<ResolvedTools> {
    use sha2::{Digest, Sha256};
    async fn resolve(name: &str) -> Result<PathBuf> {
        for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|p| p.is_absolute())
        {
            let path = directory.join(name);
            if let Ok(path) = tokio::fs::canonicalize(path).await {
                let metadata = tokio::fs::metadata(&path).await?;
                if !metadata.is_file() {
                    continue;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if metadata.permissions().mode() & 0o111 == 0 {
                        continue;
                    }
                }
                return Ok(path);
            }
        }
        anyhow::bail!("host video codec executable missing")
    }
    let ffmpeg = resolve("ffmpeg").await?;
    let ffprobe = resolve("ffprobe").await?;
    let mut hash = Sha256::new();
    hash.update(b"enhance-video-h264-crf16-shortest-v1");
    for path in [&ffmpeg, &ffprobe] {
        let (bytes, digest) = fingerprint(path).await?;
        hash.update(bytes.to_le_bytes());
        hash.update(digest.as_bytes());
    }
    Ok(ResolvedTools {
        ffmpeg,
        ffprobe,
        identity: format!("{:x}", hash.finalize()),
    })
}
