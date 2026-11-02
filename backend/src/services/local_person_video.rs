//! Lossless alpha movie from ordered RVM frames; source clocks/audio are verified.
pub use super::local_enhance_video::Tools;
use super::{
    local_enhance_video::{execute, fingerprint, frames, guard, probe, remaining},
    local_person::{self, Config},
    local_person_sequence,
};
use crate::{
    domain::{composition::FrameRateRatio, media_probe::StreamKind},
    process_control::{capture_output_cancellable, ProcessRuntime},
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
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub staging_bytes: u64,
    pub mask_destination: Option<&'a Path>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: FrameRateRatio,
    pub audio_tracks: usize,
    pub audio_codec: Option<String>,
    pub audio_copied: bool,
    pub codec: String,
    pub source_hash: String,
    pub engine_hash: String,
    pub output_hash: String,
    pub mask_hash: String,
    pub mask_width: u32,
    pub mask_height: u32,
    pub duration: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction: Option<CorrectionProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_mask_video: Option<super::local_person_mask_video::Metadata>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CorrectionProvenance {
    pub source_mask_hash: String,
    pub correction_hash: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub region_hashes: std::collections::BTreeMap<String, String>,
}
pub struct CorrectionInput<'a> {
    pub video: Input<'a>,
    pub masks: &'a Path,
    pub mask_hash: &'a str,
    pub corrections: &'a [super::local_person_correct::FrameCorrection],
}
enum Mode<'a> {
    Learned(&'a Config),
    Cached {
        executable: &'a Path,
        masks: &'a Path,
        mask_hash: &'a str,
        corrections: &'a [super::local_person_correct::FrameCorrection],
        regions: Option<&'a super::local_person_regions::Context<'a>>,
    },
}
impl Mode<'_> {
    async fn identity(&self) -> Result<String> {
        match self {
            Self::Learned(config) => Ok(local_person::identity(config).await?.hash),
            Self::Cached {
                executable,
                masks,
                mask_hash,
                corrections,
                regions,
            } => {
                use sha2::{Digest, Sha256};
                ensure!(
                    masks.is_absolute()
                        && tokio::fs::symlink_metadata(masks)
                            .await?
                            .file_type()
                            .is_file(),
                    "invalid cached movie masks"
                );
                ensure!(
                    fingerprint(masks).await?.1 == *mask_hash,
                    "cached movie mask identity mismatch"
                );
                let mut hash = Sha256::new();
                hash.update(if regions.is_some() {
                    b"cached-person-video-qtrle-regions-v1".as_slice()
                } else {
                    b"cached-person-video-qtrle-v1".as_slice()
                });
                hash.update(super::local_mask_edit::identity(executable).await?);
                hash.update(mask_hash.as_bytes());
                if let Some(regions) = regions {
                    let first = regions
                        .regions
                        .first()
                        .context("missing movie tracked regions")?
                        .attachment;
                    let raw = super::local_person_correct::Input {
                        source: masks,
                        source_hash: regions.encoded_source_hash,
                        mask: masks,
                        mask_hash,
                        mask_destination: None,
                        width: first.width,
                        height: first.height,
                        frames: first.frames,
                        corrections,
                    };
                    hash.update(super::local_person_regions::validate(&raw, regions)?);
                    super::local_person_regions::verify(&raw, regions).await?;
                    hash.update(serde_json::to_vec(
                        &regions
                            .regions
                            .iter()
                            .map(|r| r.attachment)
                            .collect::<Vec<_>>(),
                    )?);
                } else {
                    hash.update(super::local_person_correct::validate_edits(
                        corrections,
                        360000,
                    )?);
                }
                Ok(format!("{:x}", hash.finalize()))
            }
        }
    }
}
struct FrameResult {
    output_hash: String,
    mask_hash: String,
    mask_width: u32,
    mask_height: u32,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) async fn timestamps(
    runtime: &ProcessRuntime,
    tool: &Path,
    path: &Path,
    count: u64,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Vec<f64>> {
    let mut command = Command::new(tool);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path);
    let mut policy = runtime.probe_policy(path);
    policy.output.capture_bytes = 32 * 1024 * 1024;
    let result = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(result.status.success(), "video timestamp probe failed");
    let values: Vec<f64> = std::str::from_utf8(&result.stdout)?
        .lines()
        .filter_map(|line| line.split(',').next()?.parse().ok())
        .collect();
    ensure!(
        values.len() as u64 == count && values.iter().all(|v| v.is_finite()),
        "invalid video timestamps/count"
    );
    Ok(values)
}
pub(super) fn constant_clock(times: &[f64], rate: FrameRateRatio, tolerance: f64) -> Result<()> {
    ensure!(
        rate.numerator > 0 && rate.denominator > 0 && !times.is_empty(),
        "invalid person frame rate"
    );
    let start = times[0];
    for (index, value) in times.iter().enumerate() {
        ensure!(
            (value
                - (start + index as f64 * f64::from(rate.denominator) / f64::from(rate.numerator)))
            .abs()
                <= tolerance,
            "variable frame timing requires timestamp-preserving person encoding"
        );
    }
    Ok(())
}
async fn audio_hash(
    runtime: &ProcessRuntime,
    tool: &Path,
    path: &Path,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Vec<String>> {
    let mut command = Command::new(tool);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:a",
            "-vn",
            "-c:a",
            "pcm_f64le",
            "-f",
            "streamhash",
            "-hash",
            "sha256",
            "-",
        ]);
    let output = capture_output_cancellable(
        runtime,
        command,
        runtime.probe_policy(path),
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(output.status.success(), "audio content verification failed");
    let lines: Vec<_> = std::str::from_utf8(&output.stdout)?
        .lines()
        .filter(|line| line.contains(",a,SHA256="))
        .map(str::to_owned)
        .collect();
    ensure!(!lines.is_empty(), "missing audio content hashes");
    Ok(lines)
}
pub(super) async fn require_byte_color(
    runtime: &ProcessRuntime,
    tool: &Path,
    video: &crate::domain::media_probe::StreamMetadata,
    path: &Path,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<()> {
    ensure!(
        !matches!(
            video.color.transfer.as_deref(),
            Some("smpte2084" | "arib-std-b67")
        ),
        "person QTRLE movie requires SDR color; HDR preservation remains unsupported"
    );
    let format = video
        .color
        .pixel_format
        .as_deref()
        .context("missing source pixel format")?;
    let mut command = Command::new(tool);
    command.args(["-v", "error", "-show_pixel_formats", "-of", "json"]);
    let out = capture_output_cancellable(
        runtime,
        command,
        runtime.probe_policy(path),
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(out.status.success(), "pixel format inventory failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let entry = json["pixel_formats"]
        .as_array()
        .context("missing pixel format registry")?
        .iter()
        .find(|v| v["name"] == format)
        .context("source pixel format not in tool registry")?;
    let components = entry["components"]
        .as_array()
        .context("missing component depth")?;
    ensure!(
        !components.is_empty()
            && components
                .iter()
                .all(|v| v["bit_depth"].as_u64().is_some_and(|n| n > 0 && n <= 8)),
        "person QTRLE movie requires source components up to 8 bits"
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub async fn segment(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    process(
        runtime,
        tools,
        Mode::Learned(config),
        input,
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub async fn correct(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    executable: &Path,
    input: CorrectionInput<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    process(
        runtime,
        tools,
        Mode::Cached {
            executable,
            masks: input.masks,
            mask_hash: input.mask_hash,
            corrections: input.corrections,
            regions: None,
        },
        input.video,
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub async fn correct_with_regions(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    executable: &Path,
    input: CorrectionInput<'_>,
    regions: &super::local_person_regions::Context<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    process(
        runtime,
        tools,
        Mode::Cached {
            executable,
            masks: input.masks,
            mask_hash: input.mask_hash,
            corrections: input.corrections,
            regions: Some(regions),
        },
        input.video,
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn process(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    mode: Mode<'_>,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        [input.source, destination, tools.ffmpeg, tools.ffprobe]
            .iter()
            .all(|p| p.is_absolute())
            && !destination.try_exists()?,
        "invalid person movie paths/existing output"
    );
    ensure!(
        !limit.is_zero()
            && limit <= Duration::from_secs(3600)
            && input.staging_bytes > 0
            && input.staging_bytes <= MAX_BYTES,
        "invalid person movie limits"
    );
    if let Some(mask) = input.mask_destination {
        ensure!(
            mask.is_absolute() && mask != destination && !mask.try_exists()?,
            "invalid person movie mask destination"
        );
    }
    let parent = destination
        .parent()
        .context("person movie output parent missing")?;
    ensure!(parent.is_dir(), "person movie parent missing");
    let deadline = Instant::now() + limit;
    let (ffmpeg, ffprobe, tool_hashes, engine, source) = guard(
        async {
            let ffmpeg = tokio::fs::canonicalize(tools.ffmpeg).await?;
            let ffprobe = tokio::fs::canonicalize(tools.ffprobe).await?;
            let hashes = (fingerprint(&ffmpeg).await?, fingerprint(&ffprobe).await?);
            Ok((
                ffmpeg,
                ffprobe,
                hashes,
                mode.identity().await?,
                fingerprint(input.source).await?,
            ))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source.1 == input.source_hash,
        "person movie source digest mismatch"
    );
    let (info, json) = probe(runtime, &ffprobe, input.source, deadline, cancel).await?;
    let count = frames(&json)?;
    ensure!(
        count > 0 && count <= 360000,
        "person source frame count outside bound"
    );
    let video = info
        .streams
        .iter()
        .find(|v| v.kind == StreamKind::Video)
        .context("missing person video stream")?;
    require_byte_color(runtime, &ffprobe, video, input.source, deadline, cancel).await?;
    let rate = video.frame_rate.context("missing person frame rate")?;
    let frame_rate = FrameRateRatio {
        numerator: rate.numerator.try_into()?,
        denominator: rate.denominator.try_into()?,
    };
    ensure!(
        video.rotation_degrees.rem_euclid(90) == 0,
        "unsupported person rotation"
    );
    let (mut width, mut height) = (info.width, info.height);
    if video.rotation_degrees.rem_euclid(180) != 0 {
        std::mem::swap(&mut width, &mut height);
    }
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "person movie frame limit"
    );
    if let Mode::Cached {
        regions: Some(regions),
        ..
    } = &mode
    {
        ensure!(
            regions.encoded_source_hash == input.source_hash
                && regions.frame_rate == frame_rate
                && regions.regions.iter().all(|r| (
                    r.attachment.width,
                    r.attachment.height,
                    r.attachment.frames
                ) == (width, height, count)),
            "movie tracked-region source/clock/layout mismatch"
        );
    }
    let source_times = timestamps(runtime, &ffprobe, input.source, count, deadline, cancel).await?;
    let source_tick = video
        .time_base
        .filter(|v| v.numerator > 0 && v.denominator > 0)
        .map(|v| v.numerator as f64 / v.denominator as f64)
        .unwrap_or(0.);
    constant_clock(&source_times, frame_rate, source_tick + 0.000002)?;
    let raw_bytes = (u64::from(width) * u64::from(height) * 4)
        .checked_mul(count)
        .context("person movie byte overflow")?;
    let scale = 512. / f64::from(width.max(height));
    let round = |n: u32| (((f64::from(n) * scale / 16.).round() as u64) * 16).max(64);
    let mask_bytes = round(width) * round(height) * count;
    let encoded_budget = raw_bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(source.0))
        .and_then(|n| n.checked_add(1024 * 1024))
        .context("person movie budget overflow")?;
    let total = encoded_budget
        .checked_mul(2)
        .and_then(|n| n.checked_add(raw_bytes.checked_mul(3)?))
        .and_then(|n| n.checked_add(mask_bytes))
        .context("person movie staging overflow")?;
    ensure!(
        total <= input.staging_bytes,
        "person movie exceeds staging budget"
    );
    let file_budget = runtime
        .render_policy(vec![input.source.into()], destination)
        .kernel
        .file_size_bytes;
    ensure!(
        raw_bytes <= file_budget && encoded_budget <= file_budget && mask_bytes <= file_budget,
        "person movie exceeds process file limit"
    );
    let directory = parent.join(format!(".person-video-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let work = Owned(directory);
    let raw = work.0.join("decoded.rgba");
    let learned_raw = work.0.join("person.rgba");
    let masks = work.0.join("person.masks");
    let silent = work.0.join("silent.mov");
    let muxed = work.0.join("audio.mov");
    let verified = work.0.join("verified.rgba");
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
        vec![input.source.into(), work.0.clone()],
        &raw,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| (n as f64 / count as f64 * 10.).min(10.))
        },
    )
    .await?;
    let raw_id = guard(fingerprint(&raw), deadline, cancel).await?;
    ensure!(raw_id.0 == raw_bytes, "person decoded frame count mismatch");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let updates = progress.clone();
    let drain = tokio::spawn(async move {
        while let Some(value) = rx.recv().await {
            let _ = updates.send(10. + value * 0.7);
        }
    });
    let learned: Result<FrameResult> = async {
        match &mode {
            Mode::Learned(config) => {
                let result = local_person_sequence::segment(
                    runtime,
                    config,
                    local_person_sequence::Input {
                        source: &raw,
                        source_hash: &raw_id.1,
                        width,
                        height,
                        frames: count,
                        mask_destination: Some(&masks),
                    },
                    &learned_raw,
                    cancel,
                    remaining(deadline, cancel)?,
                    &tx,
                )
                .await?;
                Ok(FrameResult {
                    output_hash: result.output_hash,
                    mask_hash: result.mask_hash,
                    mask_width: result.mask_width,
                    mask_height: result.mask_height,
                })
            }
            Mode::Cached {
                executable,
                masks: original,
                mask_hash,
                corrections,
                regions,
            } => {
                let raw_input = super::local_person_correct::Input {
                    source: &raw,
                    source_hash: &raw_id.1,
                    mask: original,
                    mask_hash,
                    mask_destination: Some(&masks),
                    width,
                    height,
                    frames: count,
                    corrections,
                };
                let result = if let Some(regions) = regions {
                    super::local_person_correct::correct_with_regions(
                        runtime,
                        executable,
                        raw_input,
                        regions,
                        &learned_raw,
                        cancel,
                        remaining(deadline, cancel)?,
                        &tx,
                    )
                    .await?
                } else {
                    super::local_person_correct::correct(
                        runtime,
                        executable,
                        raw_input,
                        &learned_raw,
                        cancel,
                        remaining(deadline, cancel)?,
                        &tx,
                    )
                    .await?
                };
                Ok(FrameResult {
                    output_hash: result.output_hash,
                    mask_hash: result.mask_hash,
                    mask_width: result.mask_width,
                    mask_height: result.mask_height,
                })
            }
        }
    }
    .await;
    drop(tx);
    let _ = drain.await;
    let learned = learned?;
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
        .arg(&learned_raw)
        .args([
            "-an",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-threads",
            "2",
            "-video_track_timescale",
        ])
        .arg(frame_rate.numerator.to_string())
        .args(["-output_ts_offset"])
        .arg(format!("{:.9}", source_times[0]))
        .args(["-avoid_negative_ts", "disabled", "-fs"])
        .arg(encoded_budget.to_string())
        .args(["-progress", "pipe:1", "-f", "mov"]);
    if let Some(value) = &video.color.primaries {
        command.args(["-color_primaries", value]);
    }
    if let Some(value) = &video.color.transfer {
        command.args(["-color_trc", value]);
    }
    command.arg(&silent);
    execute(
        runtime,
        command,
        vec![learned_raw.clone(), work.0.clone()],
        &silent,
        deadline,
        cancel,
        progress,
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|n| 80. + (n as f64 / count as f64 * 10.).min(10.))
        },
    )
    .await?;
    let source_audio: Vec<_> = info
        .streams
        .iter()
        .filter(|v| v.kind == StreamKind::Audio)
        .collect();
    let result = if source_audio.is_empty() {
        &silent
    } else {
        let mut command = Command::new(&ffmpeg);
        command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-n",
                "-copyts",
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
                "1:a",
                "-c",
                "copy",
                "-avoid_negative_ts",
                "disabled",
                "-fs",
            ])
            .arg(encoded_budget.to_string())
            .args(["-f", "mov"])
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
    };
    {
        let path = result.clone();
        let primaries = video.color.primaries.clone();
        let transfer = video.color.transfer.clone();
        let patch = tokio::task::spawn_blocking(move || {
            super::person_mov_color::preserve(&path, primaries.as_deref(), transfer.as_deref())
        });
        let checked = patch
            .await
            .map_err(|e| anyhow::anyhow!("join MOV color preservation: {e}"))?;
        checked?;
        remaining(deadline, cancel)?;
    }
    let (final_info, final_json) = probe(runtime, &ffprobe, result, deadline, cancel).await?;
    ensure!(
        frames(&final_json)? == count
            && final_info.width == width
            && final_info.height == height
            && final_info.duration.is_finite()
            && final_info.duration > 0.,
        "person movie final dimensions/count mismatch"
    );
    ensure!(
        final_json["streams"]
            .as_array()
            .context("missing final streams")?
            .iter()
            .find(|v| v["codec_type"] == "video")
            .is_some_and(|v| v["codec_name"] == "qtrle"),
        "person movie codec changed"
    );
    let final_video = final_info
        .streams
        .iter()
        .find(|s| s.kind == StreamKind::Video)
        .context("missing encoded video")?;
    if let Some(value) = &video.color.primaries {
        ensure!(
            final_video.color.primaries.as_ref() == Some(value),
            "person movie color primaries changed"
        );
    }
    if let Some(value) = &video.color.transfer {
        ensure!(
            final_video.color.transfer.as_ref() == Some(value),
            "person movie color transfer changed"
        );
    }
    let final_times = timestamps(runtime, &ffprobe, result, count, deadline, cancel).await?;
    for (a, b) in source_times.iter().zip(&final_times) {
        ensure!(
            (a - b).abs() <= source_tick + 1. / f64::from(frame_rate.numerator) + 0.000002,
            "person movie frame clock changed"
        );
    }
    let final_audio: Vec<_> = final_info
        .streams
        .iter()
        .filter(|v| v.kind == StreamKind::Audio)
        .collect();
    ensure!(
        source_audio.len() == final_audio.len(),
        "person movie lost audio tracks"
    );
    for (a, b) in source_audio.iter().zip(&final_audio) {
        ensure!(
            a.codec_name == b.codec_name
                && a.channels == b.channels
                && a.sample_rate == b.sample_rate,
            "person movie audio layout changed"
        );
        ensure!(
            (a.start_time_seconds.unwrap_or(0.) - b.start_time_seconds.unwrap_or(0.)).abs()
                <= 1. / f64::from(a.sample_rate.unwrap_or(1000).max(1)) + 0.000002,
            "person movie audio clock changed"
        );
    }
    if !source_audio.is_empty() {
        ensure!(
            audio_hash(runtime, &ffmpeg, input.source, deadline, cancel).await?
                == audio_hash(runtime, &ffmpeg, result, deadline, cancel).await?,
            "person movie audio samples changed"
        );
    }
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
        .arg(result)
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
        .arg(&verified);
    execute(
        runtime,
        command,
        vec![result.clone(), work.0.clone()],
        &verified,
        deadline,
        cancel,
        progress,
        |_| None,
    )
    .await?;
    let verify = guard(fingerprint(&verified), deadline, cancel).await?;
    ensure!(
        verify.0 == raw_bytes && verify.1 == learned.output_hash,
        "person movie lost RGBA alpha or pixels"
    );
    let _ = progress.send(98.);
    let output = guard(
        async {
            ensure!(
                fingerprint(input.source).await? == source
                    && fingerprint(&ffmpeg).await? == tool_hashes.0
                    && fingerprint(&ffprobe).await? == tool_hashes.1
                    && mode.identity().await? == engine,
                "person movie source/engine changed"
            );
            let result = fingerprint(result).await?;
            ensure!(
                result.0 > 0 && result.0 <= encoded_budget,
                "person movie output budget"
            );
            Ok(result)
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
    struct Published(Vec<PathBuf>);
    impl Drop for Published {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let correction = match &mode {
        Mode::Learned(_) => None,
        Mode::Cached {
            mask_hash,
            corrections,
            regions,
            masks: original,
            ..
        } => Some(CorrectionProvenance {
            source_mask_hash: (*mask_hash).to_owned(),
            correction_hash: if let Some(regions) = regions {
                super::local_person_regions::validate(
                    &super::local_person_correct::Input {
                        source: original,
                        source_hash: input.source_hash,
                        mask: original,
                        mask_hash,
                        mask_destination: None,
                        width,
                        height,
                        frames: count,
                        corrections,
                    },
                    regions,
                )?
            } else {
                super::local_person_correct::validate_edits(corrections, count)?
            },
            region_hashes: regions
                .map(super::local_person_regions::hashes)
                .unwrap_or_default(),
        }),
    };
    let mut published = Published(Vec::new());
    remaining(deadline, cancel)?;
    if let Some(path) = input.mask_destination {
        tokio::fs::hard_link(&masks, path).await?;
        published.0.push(path.to_owned());
    }
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(result, destination)
        .await
        .context("publish person movie without replacement")?;
    published.0.push(destination.to_owned());
    remaining(deadline, cancel)?;
    published.0.clear();
    let _ = progress.send(100.);
    Ok(Metadata {
        width,
        height,
        frames: count,
        frame_rate,
        audio_tracks: source_audio.len(),
        audio_codec: source_audio
            .first()
            .and_then(|stream| stream.codec_name.clone()),
        audio_copied: !source_audio.is_empty(),
        codec: "qtrle".into(),
        source_hash: input.source_hash.into(),
        engine_hash: engine,
        output_hash: output.1,
        mask_hash: learned.mask_hash,
        mask_width: learned.mask_width,
        mask_height: learned.mask_height,
        duration: final_info.duration,
        correction,
        automatic_mask_video: None,
    })
}
