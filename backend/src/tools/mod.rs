//! External tools the backend shells out to. `args` is the pure ffmpeg command
//! compiler, while `net` and `egress_proxy` own import network policy and its
//! enforced transport. This module holds process/download/probe orchestration
//! and re-exports the public surface so callers keep using `tools::*`.

mod args;
mod egress_proxy;
mod net;

pub use args::{build_ffmpeg_args, expected_output_secs, output_ext};
pub use net::validate_url;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

use egress_proxy::EgressProxy;

/// Probed metadata about a source video.
#[derive(Debug, Clone)]
pub struct ProbeInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    /// Comma-separated demuxer aliases reported by ffprobe.
    pub format_name: Option<String>,
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
    let proxy = EgressProxy::start()
        .await
        .context("failed to start protected yt-dlp network proxy")?;
    let proxy_url = proxy.url();

    // Cap download resolution so imports stay fast. "best" can be 1080p/4K and
    // hundreds of MB; <=720 is a sensible default and is overridable via MAX_HEIGHT.
    let max_height: u32 = std::env::var("MAX_HEIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(720);
    let format = ytdlp_format(max_height);

    let mut cmd = Command::new("yt-dlp");
    configure_ytdlp_network(&mut cmd, &proxy_url);
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

    let run = run_with_progress(cmd, parse_ytdlp_progress, progress, cancel, timeout).await;
    let egress_blocked = proxy.was_blocked();
    proxy.shutdown().await;
    let (status, stderr) = run.context("failed to spawn yt-dlp (is it installed and on PATH?)")?;

    match status {
        ProcStatus::Ok => Ok(Done::Completed),
        ProcStatus::Cancelled => Ok(Done::Cancelled),
        ProcStatus::TimedOut => Err(anyhow!("Превышен лимит времени обработки")),
        ProcStatus::Failed => Err(anyhow!("{}", map_ytdlp_error(&stderr, egress_blocked))),
    }
}

fn ytdlp_format(max_height: u32) -> String {
    // Non-HTTP media protocols can launch downloaders that do not honor the
    // local HTTP proxy, so every fallback must carry the same URL constraint.
    let web_url = "[url~='(?i)^https?://']";
    format!(
        "bv*[height<={max_height}]{web_url}+ba{web_url}/\
         b[height<={max_height}]{web_url}/\
         bv*{web_url}+ba{web_url}/b{web_url}"
    )
}

fn configure_ytdlp_network(cmd: &mut Command, proxy_url: &str) {
    cmd.arg("--ignore-config").arg("--proxy").arg(proxy_url);
    for name in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        cmd.env(name, proxy_url);
    }
    cmd.env_remove("NO_PROXY").env_remove("no_proxy");
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
fn map_ytdlp_error(stderr: &str, egress_blocked: bool) -> String {
    let l = stderr.to_lowercase();
    if egress_blocked {
        "Импорт заблокирован: ссылка ведёт в приватную сеть".into()
    } else if l.contains("private") || l.contains("login") || l.contains("sign in") {
        "Видео приватное или требует входа в аккаунт".into()
    } else if l.contains("geo") || l.contains("your country") || l.contains("country") {
        "Видео недоступно в этом регионе".into()
    } else if l.contains("404")
        || l.contains("not found")
        || l.contains("unavailable")
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
    let start = prefix
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(0);
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

    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse ffprobe JSON output")?;

    let duration = v["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let format_name = v["format"]["format_name"].as_str().map(str::to_owned);

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

    Ok(ProbeInfo {
        duration,
        width,
        height,
        fps,
        vcodec,
        acodec,
        format_name,
    })
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
    cmd.arg("-nostats")
        .arg("-progress")
        .arg("pipe:1")
        .args(args);

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
    configure_process_group(&mut cmd);
    let mut child = cmd.spawn()?;
    let child_id = child.id();
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
            res = child.wait() => {
                if !out_done || !err_done {
                    signal_process_group(child_id, SignalKind::Terminate);
                }
                break match res {
                    Ok(es) if es.success() => ProcStatus::Ok,
                    _ => ProcStatus::Failed,
                };
            }
        }
    };

    if matches!(status, ProcStatus::Cancelled | ProcStatus::TimedOut) {
        terminate_child_tree(&mut child, child_id).await;
    }

    Ok((status, err_buf))
}

#[cfg(unix)]
fn configure_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_cmd: &mut Command) {}

#[derive(Clone, Copy)]
enum SignalKind {
    Terminate,
    Kill,
}

#[cfg(unix)]
fn signal_process_group(child_id: Option<u32>, signal: SignalKind) {
    let Some(pid) = child_id.and_then(|id| i32::try_from(id).ok()) else {
        return;
    };
    let signal = match signal {
        SignalKind::Terminate => libc::SIGTERM,
        SignalKind::Kill => libc::SIGKILL,
    };
    // SAFETY: `pid` came from a successfully spawned child and `process_group(0)`
    // placed it into a new group whose id equals that pid. Negative pid targets
    // exactly that group; errors (already exited, permission, etc.) are ignored.
    let _ = unsafe { libc::kill(-pid, signal) };
}

#[cfg(not(unix))]
fn signal_process_group(_child_id: Option<u32>, _signal: SignalKind) {}

async fn terminate_child_tree(child: &mut tokio::process::Child, child_id: Option<u32>) {
    signal_process_group(child_id, SignalKind::Terminate);
    #[cfg(not(unix))]
    let _ = child.start_kill();
    if timeout(Duration::from_secs(3), child.wait()).await.is_ok() {
        return;
    }

    signal_process_group(child_id, SignalKind::Kill);
    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
}

/// Keep the last `n` non-empty lines of a log blob.
fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_progress_parsing() {
        assert_eq!(
            parse_ytdlp_progress("[download]  42.3% of 10MiB"),
            Some(42.3)
        );
        assert_eq!(
            parse_ytdlp_progress("[download] 100% of 10MiB"),
            Some(100.0)
        );
        assert_eq!(parse_ytdlp_progress("some other line"), None);
    }

    #[test]
    fn ytdlp_network_ignores_user_config_and_no_proxy() {
        let mut cmd = Command::new("yt-dlp");
        configure_ytdlp_network(&mut cmd, "http://127.0.0.1:43210");
        let command = cmd.as_std();
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            ["--ignore-config", "--proxy", "http://127.0.0.1:43210"]
        );

        let env = command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for name in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            assert_eq!(
                env.get(name).and_then(Option::as_deref),
                Some("http://127.0.0.1:43210")
            );
        }
        assert_eq!(env.get("NO_PROXY"), Some(&None));
        assert_eq!(env.get("no_proxy"), Some(&None));
    }

    #[tokio::test]
    async fn ytdlp_format_allows_https_and_rejects_direct_rtmp() {
        match Command::new("yt-dlp").arg("--version").output().await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("could not check yt-dlp: {error}"),
            Ok(output) => assert!(output.status.success(), "yt-dlp --version failed"),
        }

        let directory = tempfile::tempdir().unwrap();
        let info_path = directory.path().join("media.info.json");
        let mut info = serde_json::json!({
            "id": "media",
            "title": "media",
            "extractor": "generic",
            "webpage_url": "https://video.example/watch",
            "formats": [
                {
                    "format_id": "https",
                    "url": "https://cdn.example/video.mp4",
                    "ext": "mp4",
                    "protocol": "https",
                    "vcodec": "h264",
                    "acodec": "aac",
                    "height": 720
                },
                {
                    "format_id": "rtmp",
                    "url": "rtmp://127.0.0.1/live",
                    "ext": "flv",
                    "protocol": "rtmp",
                    "vcodec": "h264",
                    "acodec": "aac",
                    "height": 1080
                }
            ]
        });
        tokio::fs::write(&info_path, serde_json::to_vec(&info).unwrap())
            .await
            .unwrap();

        let output = Command::new("yt-dlp")
            .args(["--ignore-config", "--simulate", "--load-info-json"])
            .arg(&info_path)
            .arg("-f")
            .arg(ytdlp_format(720))
            .args(["--print", "%(url)s"])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "https://cdn.example/video.mp4"
        );

        info["formats"] = serde_json::json!([{
            "format_id": "rtmp",
            "url": "rtmp://127.0.0.1/live",
            "ext": "flv",
            "protocol": "rtmp",
            "vcodec": "h264",
            "acodec": "aac",
            "height": 720
        }]);
        tokio::fs::write(&info_path, serde_json::to_vec(&info).unwrap())
            .await
            .unwrap();
        let output = Command::new("yt-dlp")
            .args(["--ignore-config", "--simulate", "--load-info-json"])
            .arg(&info_path)
            .arg("-f")
            .arg(ytdlp_format(720))
            .output()
            .await
            .unwrap();
        assert!(!output.status.success(), "RTMP-only media was selected");
    }

    #[test]
    fn egress_proxy_errors_have_a_user_facing_message() {
        assert_eq!(
            map_ytdlp_error("ProxyError: Tunnel connection failed: 472", true),
            "Импорт заблокирован: ссылка ведёт в приватную сеть"
        );
        assert!(map_ytdlp_error("HTTP Error 472", false).starts_with("yt-dlp:"));
        assert!(map_ytdlp_error("HTTP Error 403: Forbidden", false).starts_with("yt-dlp:"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_with_progress_does_not_wait_for_background_pipe_holders() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 2 & printf 'progress=50\\n'");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();

        let result = timeout(
            Duration::from_millis(700),
            run_with_progress(
                cmd,
                |line| line.strip_prefix("progress=").and_then(|p| p.parse().ok()),
                &tx,
                &cancel,
                Duration::from_secs(10),
            ),
        )
        .await
        .expect("child exit must win over pipe EOF")
        .unwrap();

        assert!(matches!(result.0, ProcStatus::Ok));
        assert_eq!(rx.try_recv().unwrap(), 50.0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_with_progress_kills_process_group_on_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("escaped-child");
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg("(sleep 1; touch \"$1\") & wait")
            .arg("runner")
            .arg(&marker);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = CancellationToken::new();

        let (status, _stderr) =
            run_with_progress(cmd, |_| None, &tx, &cancel, Duration::from_millis(50))
                .await
                .unwrap();

        assert!(matches!(status, ProcStatus::TimedOut));
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!marker.exists(), "background child survived timeout");
    }
}
