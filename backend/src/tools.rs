use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::model::EditRequest;

/// Probed metadata about a source video.
pub struct ProbeInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
}

/// How a child process finished from our point of view.
enum ProcStatus {
    Ok,
    Failed,
    Cancelled,
    TimedOut,
}

/// Result of a long-running tool invocation that the caller cares about.
pub enum Done {
    Completed,
    Cancelled,
}

/// Probe availability + version of an external tool. Returns (present, first line).
pub async fn check_tool(bin: &str, version_arg: &str) -> (bool, Option<String>) {
    match Command::new(bin).arg(version_arg).output().await {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            let first = s.lines().next().unwrap_or("").trim().to_string();
            (true, (!first.is_empty()).then_some(first))
        }
        _ => (false, None),
    }
}

/// Reject non-http(s) URLs and ones that point at the local machine / private
/// network (a basic SSRF guard). Returns a Russian error message on rejection.
pub fn validate_url(raw: &str) -> Result<()> {
    let u = Url::parse(raw).map_err(|_| anyhow!("Недопустимый URL"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(anyhow!("Недопустимый URL"));
    }
    let host = u.host_str().ok_or_else(|| anyhow!("Недопустимый URL"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".local") || lower.ends_with(".localhost") {
        return Err(anyhow!("Недопустимый URL"));
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        if is_blocked_ip(&ip) {
            return Err(anyhow!("Недопустимый URL"));
        }
    }
    Ok(())
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            let seg = v6.segments();
            // fc00::/7 (unique local) and fe80::/10 (link local).
            (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Download a video by URL using yt-dlp into `sources_dir`, naming it `<id>.<ext>`.
/// Streams download percent to `progress`, honours `cancel`, and aborts after
/// `timeout`. Works for any site yt-dlp supports, including vkvideo.ru / vk.com.
#[allow(clippy::too_many_arguments)]
pub async fn download_video(
    url: &str,
    sources_dir: &Path,
    id: &str,
    start: Option<f64>,
    end: Option<f64>,
    progress: &UnboundedSender<f64>,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<Done> {
    let template = sources_dir.join(format!("{id}.%(ext)s"));

    // Cap download resolution so imports stay fast. "best" can be 1080p/4K and
    // hundreds of MB; <=720 is a sensible default and is overridable via MAX_HEIGHT.
    let max_height: u32 = std::env::var("MAX_HEIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(720);
    let format = format!("bv*[height<={max_height}]+ba/b[height<={max_height}]/bv*+ba/b");

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--newline")
        .arg("--no-playlist")
        .arg("--write-info-json")
        .arg("-f")
        .arg(&format)
        .arg("--merge-output-format")
        .arg("mp4");

    // Optionally download only a section instead of the whole video. The "*"
    // prefix marks a time range (as opposed to a chapter regex).
    if start.is_some() || end.is_some() {
        let s = start.unwrap_or(0.0).max(0.0);
        let section = match end {
            Some(e) => format!("*{s}-{e}"),
            None => format!("*{s}-inf"),
        };
        cmd.arg("--download-sections").arg(section);
    }

    cmd.arg("-o").arg(&template).arg(url);

    let (status, stderr) = run_with_progress(cmd, parse_ytdlp_progress, progress, cancel, timeout)
        .await
        .context("failed to spawn yt-dlp (is it installed and on PATH?)")?;

    match status {
        ProcStatus::Ok => Ok(Done::Completed),
        ProcStatus::Cancelled => Ok(Done::Cancelled),
        ProcStatus::TimedOut => Err(anyhow!("Превышен лимит времени обработки")),
        ProcStatus::Failed => Err(anyhow!("{}", map_ytdlp_error(&stderr))),
    }
}

/// Read the title out of yt-dlp's sidecar `<id>.info.json`, then delete that file.
/// Best-effort: any failure just yields `None`.
pub async fn read_title(sources_dir: &Path, id: &str) -> Option<String> {
    let p = sources_dir.join(format!("{id}.info.json"));
    let data = tokio::fs::read(&p).await.ok()?;
    let _ = tokio::fs::remove_file(&p).await;
    let v: serde_json::Value = serde_json::from_slice(&data).ok()?;
    v.get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// Map a raw yt-dlp stderr blob to a friendly Russian message.
fn map_ytdlp_error(stderr: &str) -> String {
    let l = stderr.to_lowercase();
    if l.contains("private") || l.contains("login") || l.contains("sign in") {
        "Видео приватное или требует входа в аккаунт".into()
    } else if l.contains("geo") || l.contains("your country") || l.contains("country") {
        "Видео недоступно в этом регионе".into()
    } else if l.contains("404") || l.contains("not found") || l.contains("unavailable")
        || l.contains("removed")
    {
        "Видео не найдено или удалено".into()
    } else {
        format!("yt-dlp: {}", tail(stderr, 6))
    }
}

/// Parse a yt-dlp `[download]  42.3% of ...` line into a percent.
fn parse_ytdlp_progress(line: &str) -> Option<f64> {
    if !line.contains("[download]") {
        return None;
    }
    let idx = line.find('%')?;
    let prefix = &line[..idx];
    let start = prefix.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    prefix[start..].trim().parse::<f64>().ok()
}

/// Locate the source file previously downloaded for `video_id` (extension unknown).
pub async fn find_source(sources_dir: &Path, video_id: &str) -> Result<PathBuf> {
    find_by_id(sources_dir, video_id)
        .await?
        .ok_or_else(|| anyhow!("source video {video_id} not found"))
}

/// Find a file in `dir` whose stem (name without final extension) equals `id`.
/// The `<id>.info.json` sidecar has stem `<id>.info`, so it is never matched.
pub async fn find_by_id(dir: &Path, id: &str) -> Result<Option<PathBuf>> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .with_context(|| format!("failed to read dir {}", dir.display()))?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| stem == id)
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Probe a video for duration, dimensions, fps and codecs via ffprobe.
pub async fn probe_video(path: &Path) -> Result<ProbeInfo> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(path)
        .output()
        .await
        .context("failed to spawn ffprobe (is ffmpeg installed?)")?;

    if !output.status.success() {
        return Err(anyhow!("ffprobe failed for {}", path.display()));
    }

    let v: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("failed to parse ffprobe JSON output")?;

    let duration = v["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let (mut width, mut height) = (0u32, 0u32);
    let mut fps = None;
    let mut vcodec = None;
    let mut acodec = None;
    if let Some(streams) = v["streams"].as_array() {
        for s in streams {
            match s["codec_type"].as_str() {
                Some("video") if vcodec.is_none() => {
                    width = s["width"].as_u64().unwrap_or(0) as u32;
                    height = s["height"].as_u64().unwrap_or(0) as u32;
                    fps = s["r_frame_rate"].as_str().and_then(parse_fraction);
                    vcodec = s["codec_name"].as_str().map(|c| c.to_string());
                }
                Some("audio") if acodec.is_none() => {
                    acodec = s["codec_name"].as_str().map(|c| c.to_string());
                }
                _ => {}
            }
        }
    }

    Ok(ProbeInfo { duration, width, height, fps, vcodec, acodec })
}

/// Parse an ffprobe fraction like "30000/1001" into frames per second.
fn parse_fraction(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let n: f64 = n.parse().ok()?;
    let d: f64 = d.parse().ok()?;
    if d == 0.0 {
        return None;
    }
    Some(n / d)
}

/// Build the ffmpeg argument list for an edit request.
///
/// Trim is applied as an *input* option (`-ss` + `-t`) so it happens before the
/// filter graph; crop/scale/speed then operate on the trimmed stream.
pub fn build_ffmpeg_args(input: &Path, output: &Path, edit: &EditRequest) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".into()];

    // --- input-side trim ---
    if let Some(t) = &edit.trim {
        let dur = (t.end - t.start).max(0.0);
        args.push("-ss".into());
        args.push(format_secs(t.start));
        args.push("-t".into());
        args.push(format_secs(dur));
    }

    args.push("-i".into());
    args.push(input.to_string_lossy().into_owned());

    // --- video filter chain ---
    let mut vf: Vec<String> = Vec::new();
    if let Some(c) = &edit.crop {
        // Force even dimensions; libx264 + yuv420p requires them.
        let w = c.w & !1;
        let h = c.h & !1;
        vf.push(format!("crop={w}:{h}:{}:{}", c.x, c.y));
    }
    if let Some(s) = &edit.scale {
        vf.push(format!("scale={}:{}", s.w, s.h));
    }
    let speed = edit.speed;
    let speed_changed = (speed - 1.0).abs() > 1e-6 && speed > 0.0;
    if speed_changed {
        vf.push(format!("setpts={:.6}*PTS", 1.0 / speed));
    }
    if !vf.is_empty() {
        args.push("-vf".into());
        args.push(vf.join(","));
    }

    // --- audio ---
    if edit.mute {
        args.push("-an".into());
    } else if speed_changed {
        args.push("-af".into());
        // atempo only accepts 0.5..=2.0; the frontend clamps speed to that range.
        args.push(format!("atempo={:.6}", speed.clamp(0.5, 2.0)));
    }

    // --- web-friendly encode settings ---
    args.push("-c:v".into());
    args.push("libx264".into());
    args.push("-preset".into());
    args.push("veryfast".into());
    args.push("-crf".into());
    args.push("23".into());
    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    if !edit.mute {
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push("128k".into());
    }
    args.push("-movflags".into());
    args.push("+faststart".into());

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Expected output duration (seconds) for an edit, used to scale ffmpeg progress.
pub fn expected_output_secs(edit: &EditRequest, source_duration: f64) -> f64 {
    let base = match &edit.trim {
        Some(t) => (t.end - t.start).max(0.0),
        None => source_duration,
    };
    let speed = if edit.speed > 0.0 { edit.speed } else { 1.0 };
    base / speed
}

/// Run ffmpeg with the given args, streaming progress (scaled against the
/// expected output duration), honouring cancellation and a timeout.
pub async fn run_ffmpeg(
    args: &[String],
    expected_secs: f64,
    progress: &UnboundedSender<f64>,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<Done> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-nostats").arg("-progress").arg("pipe:1").args(args);

    let expected = expected_secs.max(0.001);
    let parse = move |line: &str| -> Option<f64> {
        let rest = line
            .strip_prefix("out_time_us=")
            .or_else(|| line.strip_prefix("out_time_ms="))?;
        let us: f64 = rest.trim().parse().ok()?;
        Some((us / 1_000_000.0) / expected * 100.0)
    };

    let (status, stderr) = run_with_progress(cmd, parse, progress, cancel, timeout)
        .await
        .context("failed to spawn ffmpeg (is it installed and on PATH?)")?;

    match status {
        ProcStatus::Ok => Ok(Done::Completed),
        ProcStatus::Cancelled => Ok(Done::Cancelled),
        ProcStatus::TimedOut => Err(anyhow!("Превышен лимит времени обработки")),
        ProcStatus::Failed => Err(anyhow!("ffmpeg failed: {}", tail(&stderr, 15))),
    }
}

/// Spawn `cmd`, reading both stdout and stderr line by line. `parse` extracts a
/// percent from any line (stdout or stderr); the raw stderr is accumulated for
/// error reporting. Cancellation or timeout kills the child.
async fn run_with_progress(
    mut cmd: Command,
    parse: impl Fn(&str) -> Option<f64>,
    progress: &UnboundedSender<f64>,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<(ProcStatus, String)> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut out_lines = BufReader::new(stdout).lines();
    let mut err_lines = BufReader::new(stderr).lines();
    let mut err_buf = String::new();
    let mut out_done = false;
    let mut err_done = false;
    let deadline = Instant::now() + timeout;

    let status = loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break ProcStatus::Cancelled,
            _ = tokio::time::sleep_until(deadline) => break ProcStatus::TimedOut,
            line = out_lines.next_line(), if !out_done => match line {
                Ok(Some(l)) => {
                    if let Some(p) = parse(&l) {
                        let _ = progress.send(p.clamp(0.0, 100.0));
                    }
                }
                _ => out_done = true,
            },
            line = err_lines.next_line(), if !err_done => match line {
                Ok(Some(l)) => {
                    if let Some(p) = parse(&l) {
                        let _ = progress.send(p.clamp(0.0, 100.0));
                    }
                    err_buf.push_str(&l);
                    err_buf.push('\n');
                }
                _ => err_done = true,
            },
            res = child.wait(), if out_done && err_done => {
                break match res {
                    Ok(es) if es.success() => ProcStatus::Ok,
                    _ => ProcStatus::Failed,
                };
            }
        }
    };

    if matches!(status, ProcStatus::Cancelled | ProcStatus::TimedOut) {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    Ok((status, err_buf))
}

/// Format seconds without scientific notation, trimming trailing noise.
fn format_secs(s: f64) -> String {
    format!("{s:.3}")
}

/// Keep the last `n` non-empty lines of a log blob.
fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}
