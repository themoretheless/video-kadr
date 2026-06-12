use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

use crate::model::EditRequest;

/// Probed metadata about a source video.
pub struct ProbeInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
}

/// Download a video by URL using yt-dlp into `sources_dir`, naming it `<id>.<ext>`.
/// Returns the path to the produced file. Works for any site yt-dlp supports,
/// including vkvideo.ru / vk.com.
pub async fn download_video(
    url: &str,
    sources_dir: &Path,
    id: &str,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<PathBuf> {
    let template = sources_dir.join(format!("{id}.%(ext)s"));

    // Cap download resolution so imports stay fast. "best" can be 1080p/4K and
    // hundreds of MB; <=720 is a sensible default and is overridable via MAX_HEIGHT.
    let max_height: u32 = std::env::var("MAX_HEIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(720);
    let format = format!("bv*[height<={max_height}]+ba/b[height<={max_height}]/bv*+ba/b");

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--no-playlist")
        .arg("--no-progress")
        // Prefer separate best video + best audio (merged to mp4), capped to
        // max_height; fall back to best single file, then to best of any size.
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

    let output = cmd
        .output()
        .await
        .context("failed to spawn yt-dlp (is it installed and on PATH?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", tail(&stderr, 8)));
    }

    find_by_id(sources_dir, id)
        .await?
        .ok_or_else(|| anyhow!("download succeeded but no output file was found for id {id}"))
}

/// Locate the source file previously downloaded for `video_id` (extension unknown).
pub async fn find_source(sources_dir: &Path, video_id: &str) -> Result<PathBuf> {
    find_by_id(sources_dir, video_id)
        .await?
        .ok_or_else(|| anyhow!("source video {video_id} not found"))
}

/// Find a file in `dir` whose stem (name without final extension) equals `id`.
async fn find_by_id(dir: &Path, id: &str) -> Result<Option<PathBuf>> {
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

/// Probe a video for duration and dimensions via ffprobe.
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
    if let Some(streams) = v["streams"].as_array() {
        for s in streams {
            if s["codec_type"] == "video" {
                width = s["width"].as_u64().unwrap_or(0) as u32;
                height = s["height"].as_u64().unwrap_or(0) as u32;
                break;
            }
        }
    }

    Ok(ProbeInfo { duration, width, height })
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

/// Run ffmpeg with the given args, surfacing the tail of stderr on failure.
pub async fn run_ffmpeg(args: &[String]) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .await
        .context("failed to spawn ffmpeg (is it installed and on PATH?)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ffmpeg failed: {}", tail(&stderr, 15)));
    }
    Ok(())
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
