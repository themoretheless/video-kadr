//! Original Concat still extraction policy. Caller owns immutable source ACL and publication.
use crate::{
    domain::{
        concat_retime_edit::{StillCapture, StillCaptureSelection},
        media_probe::Rational,
    },
    process_control::{capture_output_cancellable, ProcessRuntime},
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tokio::{process::Command, time::Instant};
use tokio_util::sync::CancellationToken;
pub struct Tools<'a> {
    pub ffmpeg: &'a Path,
    pub ffprobe: &'a Path,
}
pub struct CapturedStill {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub used_first_frame: bool,
}
struct Layout {
    width: u32,
    height: u32,
    index: u64,
    origin: i64,
    base: Rational,
}
fn remaining(deadline: Instant, cancel: &CancellationToken) -> Result<Duration> {
    ensure!(!cancel.is_cancelled(), "freeze capture cancelled");
    let left = deadline.saturating_duration_since(Instant::now());
    ensure!(!left.is_zero(), "freeze capture expired");
    Ok(left)
}
async fn layout(
    runtime: &ProcessRuntime,
    tool: &Path,
    input: &Path,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Layout> {
    let mut command = Command::new(tool);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-show_streams",
            "-show_library_versions",
            "-of",
            "json",
        ])
        .arg(input);
    let mut policy = runtime.probe_policy(input);
    policy.output.capture_bytes = 128 * 1024;
    let out = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(out.status.success(), "freeze source probe failed");
    let value: Value = serde_json::from_slice(&out.stdout)?;
    let streams = value["streams"]
        .as_array()
        .context("missing freeze streams")?;
    let videos: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "video")
        .collect();
    let stream = if videos.len() == 1 {
        videos[0]
    } else {
        ensure!(!videos.is_empty(), "missing freeze video stream");
        let selected = best_video_stream(runtime, tool, input, &value, deadline, cancel).await?;
        streams
            .iter()
            .find(|s| s["index"].as_u64() == Some(selected) && s["codec_type"] == "video")
            .context("native selector returned an unknown video stream")?
    };
    let width = u32::try_from(stream["width"].as_u64().context("missing freeze width")?)?;
    let height = u32::try_from(stream["height"].as_u64().context("missing freeze height")?)?;
    ensure!(width > 0 && height > 0, "invalid freeze source dimensions");
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|v| v.iter().find_map(|s| s["rotation"].as_i64()))
        .or_else(|| {
            stream["tags"]["rotate"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0)
        .rem_euclid(360);
    let (width, height) = if rotation == 90 || rotation == 270 {
        (height, width)
    } else {
        (width, height)
    };
    let base = Rational::parse(
        stream["time_base"]
            .as_str()
            .context("missing freeze time base")?,
    )
    .context("invalid freeze time base")?;
    ensure!(
        base.numerator > 0 && base.denominator > 0,
        "invalid freeze time base"
    );
    Ok(Layout {
        width,
        height,
        index: stream["index"]
            .as_u64()
            .context("missing freeze stream index")?,
        origin: stream["start_pts"].as_i64().unwrap_or(0),
        base,
    })
}
async fn best_video_stream(
    runtime: &ProcessRuntime,
    ffprobe: &Path,
    input: &Path,
    probe: &Value,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<u64> {
    let exe = std::env::current_exe()?;
    let directory = exe.parent().context("missing media probe directory")?;
    let directory = if directory.file_name().is_some_and(|n| n == "deps") {
        directory
            .parent()
            .context("missing media probe directory")?
    } else {
        directory
    };
    let worker = directory.join(if cfg!(windows) {
        "video-kadr-media-probe.exe"
    } else {
        "video-kadr-media-probe"
    });
    ensure!(
        worker.is_file(),
        "native video stream selector is not installed"
    );
    let probe_binary = tokio::fs::canonicalize(ffprobe).await?;
    let prefix = probe_binary
        .parent()
        .and_then(Path::parent)
        .context("invalid ffprobe layout")?;
    let mut directories = vec![prefix.join("lib")];
    #[cfg(target_os = "linux")]
    {
        directories.extend([
            std::path::PathBuf::from("/usr/lib/x86_64-linux-gnu"),
            std::path::PathBuf::from("/usr/lib/aarch64-linux-gnu"),
            std::path::PathBuf::from("/usr/lib64"),
        ]);
    }
    let expected = probe["library_versions"]
        .as_array()
        .and_then(|a| a.iter().find(|v| v["name"] == "libavformat"))
        .and_then(|v| {
            Some((v["major"].as_u64()? << 16) | (v["minor"].as_u64()? << 8) | v["micro"].as_u64()?)
        })
        .context("ffprobe did not report libavformat version")?;
    let mut candidates = vec![];
    for directory in directories {
        if let Ok(mut entries) = tokio::fs::read_dir(directory).await {
            while let Some(entry) = entries.next_entry().await? {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "libavformat.dylib"
                    || name == "libavformat.so"
                    || name.starts_with("libavformat.so.")
                    || name.starts_with("libavformat.") && name.ends_with(".dylib")
                {
                    candidates.push(entry.path());
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    let major = expected >> 16;
    let library = candidates
        .iter()
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n == "libavformat.dylib" || n == "libavformat.so")
        })
        .cloned()
        .or_else(|| {
            candidates.into_iter().find(|p| {
                p.file_name().is_some_and(|n| {
                    n == format!("libavformat.{major}.dylib").as_str()
                        || n == format!("libavformat.so.{major}").as_str()
                })
            })
        });
    let library =
        library.context("trusted libavformat shared library is not installed beside ffprobe")?;
    let mut command = Command::new(worker);
    command.arg(&library).arg(input);
    let mut policy = runtime.probe_policy(input);
    policy.filesystem.read_only.push(library);
    policy.output.capture_bytes = 4096;
    policy.output.line_bytes = 1024;
    let out = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(out.status.success(), "native video stream selection failed");
    let text = std::str::from_utf8(&out.stdout)?;
    let fields: Vec<_> = text.split_whitespace().collect();
    ensure!(
        fields.len() == 2,
        "invalid native video stream selection result"
    );
    ensure!(
        fields[1].parse::<u64>()? == expected,
        "ffprobe and native media selector use different libraries"
    );
    Ok(fields[0].parse()?)
}

fn stamp(micros: i128) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let n = micros.unsigned_abs();
    format!("{sign}{}.{:06}", n / 1_000_000, n % 1_000_000)
}
async fn picture(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    input: &Path,
    layout: &Layout,
    us: u64,
    width: u32,
    height: u32,
    quality: u8,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Option<Vec<u8>>> {
    let top = i128::from(us)
        .checked_mul(i128::from(layout.base.denominator))
        .context("freeze seek overflow")?;
    let bottom = i128::from(layout.base.numerator) * 1_000_000;
    let first = top / bottom + i128::from(top % bottom != 0);
    ensure!(
        first + i128::from(layout.origin).abs() <= 9_007_199_254_740_991,
        "freeze PTS exceeds exact filter range"
    );
    let origin_us = i128::from(layout.origin)
        .checked_mul(i128::from(layout.base.numerator))
        .and_then(|v| v.checked_mul(1_000_000))
        .context("freeze origin overflow")?
        .div_euclid(i128::from(layout.base.denominator));
    let seek = stamp(origin_us + i128::from(us));
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-ss",
            &seek,
            "-noaccurate_seek",
            "-seek_timestamp",
            "1",
            "-copyts",
            "-i",
        ])
        .arg(input);
    command.args(["-map",&format!("0:{}",layout.index),"-an","-sn","-dn","-vf"])
        .arg(format!("setpts=PTS-({}),select='gte(pts,{first})',scale={width}:{height}:flags=bilinear,format=rgba,scale={width}:{height}:flags=bilinear,format=yuvj420p,setsar=0,setpts=0",layout.origin))
        .args(["-frames:v","1","-fps_mode","passthrough","-c:v","mjpeg","-qmin",&quality.to_string(),"-qmax",&quality.to_string(),"-enc_time_base","1:25","-f","image2pipe","pipe:1"]);
    let mut policy = runtime.probe_policy(input);
    policy.output.capture_bytes =
        usize::try_from(u64::from(width) * u64::from(height) * 4 + 65536)?;
    let out = capture_output_cancellable(
        runtime,
        command,
        policy,
        remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(
        out.status.success(),
        "freeze picture decode/encode failed: {}",
        String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(2048)
            .collect::<String>()
    );
    if out.stdout.is_empty() {
        return Ok(None);
    }
    ensure!(
        out.stdout.starts_with(&[255, 216]) && out.stdout.ends_with(&[255, 217]),
        "incomplete freeze JPEG"
    );
    Ok(Some(out.stdout))
}
/// Fixed-width bilinear RGBA decode followed by original YUVJ420P JPEG quality 4.
/// A successful empty seek alone triggers the original first-picture fallback.
pub async fn capture(
    runtime: &ProcessRuntime,
    tools: Tools<'_>,
    input: &Path,
    plan: &StillCapture,
    limit: Duration,
    cancel: &CancellationToken,
) -> Result<CapturedStill> {
    ensure!(
        input.is_absolute() && tools.ffmpeg.is_absolute() && tools.ffprobe.is_absolute(),
        "freeze capture needs absolute authorized paths"
    );
    ensure!(
        matches!(&plan.selection, StillCaptureSelection::AtOrAfterThenFirst)
            && plan.width == 1280
            && plan.jpeg_quality == 4
            && plan.source_seconds.is_finite()
            && plan.source_seconds >= 0.
            && (plan.source_seconds * 1_000_000.).round() == plan.seek_time_us as f64
            && (plan.source_seconds * 1000.).round() == plan.cache_time_ms as f64,
        "invalid freeze capture contract"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(120),
        "invalid freeze capture deadline"
    );
    let deadline = Instant::now() + limit;
    remaining(deadline, cancel)?;
    ensure!(
        tokio::fs::symlink_metadata(input).await?.is_file(),
        "freeze source must be a regular immutable file"
    );
    let layout = layout(runtime, tools.ffprobe, input, deadline, cancel).await?;
    let width = plan.width;
    let height = ((f64::from(width) * f64::from(layout.height) / f64::from(layout.width)).round()
        as u32)
        .max(2)
        & !1;
    ensure!(
        height > 0 && height <= 32768 && u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "freeze picture exceeds pixel budget"
    );
    if let Some(jpeg) = picture(
        runtime,
        tools.ffmpeg,
        input,
        &layout,
        plan.seek_time_us,
        width,
        height,
        plan.jpeg_quality,
        deadline,
        cancel,
    )
    .await?
    {
        return Ok(CapturedStill {
            jpeg,
            width,
            height,
            used_first_frame: false,
        });
    }
    let jpeg = picture(
        runtime,
        tools.ffmpeg,
        input,
        &layout,
        0,
        width,
        height,
        plan.jpeg_quality,
        deadline,
        cancel,
    )
    .await?
    .context("freeze source has no picture")?;
    Ok(CapturedStill {
        jpeg,
        width,
        height,
        used_first_frame: true,
    })
}
