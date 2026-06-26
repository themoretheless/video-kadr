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

/// Map a named look preset to an ffmpeg filter string.
fn filter_preset(name: &str) -> Option<&'static str> {
    match name {
        "grayscale" => Some("hue=s=0"),
        "sepia" => Some("colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131"),
        "warm" => Some("colorbalance=rs=0.2:gs=0.05:bs=-0.2"),
        "cold" => Some("colorbalance=rs=-0.2:gs=0:bs=0.2"),
        // Shadows toward teal, highlights toward orange (the blockbuster look).
        "teal-orange" => Some("colorbalance=rs=-0.15:bs=0.15:rm=0.1:bm=-0.05:rh=0.15:bh=-0.15"),
        // Lifted blacks + lowered whites for a flat, matte film look.
        "faded" => Some("curves=all='0/0.08 1/0.92'"),
        // High-contrast black and white.
        "noir" => Some("hue=s=0,eq=contrast=1.4"),
        // Warm, slightly faded vintage.
        "vintage" => Some("curves=all='0/0.06 1/0.95',colorbalance=rs=0.15:gs=0.05:bs=-0.1"),
        _ => None,
    }
}

/// Whitelist a censor-box colour to a safe token (default black).
fn sanitize_color(name: Option<&str>) -> &'static str {
    match name.unwrap_or("black") {
        "white" => "white",
        "gray" | "grey" => "gray",
        "red" => "red",
        _ => "black",
    }
}

/// Parse an aspect like "9:16" into (w, h), only allowing small sane values.
fn parse_aspect(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(':')?;
    let w: u32 = a.trim().parse().ok()?;
    let h: u32 = b.trim().parse().ok()?;
    if w == 0 || h == 0 || w > 100 || h > 100 {
        return None;
    }
    Some((w, h))
}

/// Output file extension for a requested export format.
pub fn output_ext(format: Option<&str>) -> &'static str {
    match format.unwrap_or("mp4") {
        "webm" => "webm",
        "gif" => "gif",
        "png" => "png",
        "mp3" => "mp3",
        _ => "mp4",
    }
}

/// Build the geometry/colour (and optionally temporal) video filter chain.
/// Order matters: crop -> rotate -> flip -> scale -> eq -> preset -> reverse
/// -> setpts -> fade. `temporal=false` skips speed/fade (used for still frames).
fn video_filters(edit: &EditRequest, out_dur: f64, temporal: bool) -> Vec<String> {
    let mut vf: Vec<String> = Vec::new();
    // Censor box first, in source coordinates (matches the on-video selection).
    if let Some(c) = &edit.censor {
        let color = sanitize_color(edit.censor_color.as_deref());
        vf.push(format!(
            "drawbox=x={}:y={}:w={}:h={}:color={color}:t=fill",
            c.x, c.y, c.w, c.h
        ));
    }
    if let Some(c) = &edit.crop {
        // Force even dimensions; libx264 + yuv420p requires them.
        let w = c.w & !1;
        let h = c.h & !1;
        vf.push(format!("crop={w}:{h}:{}:{}", c.x, c.y));
    }
    match edit.rotate.rem_euclid(360) {
        90 => vf.push("transpose=1".into()),
        180 => {
            vf.push("transpose=1".into());
            vf.push("transpose=1".into());
        }
        270 => vf.push("transpose=2".into()),
        _ => {}
    }
    if edit.flip_h {
        vf.push("hflip".into());
    }
    if edit.flip_v {
        vf.push("vflip".into());
    }
    if let Some(s) = &edit.scale {
        vf.push(format!("scale={}:{}", s.w, s.h));
    }
    // Letterbox/pillarbox to a target aspect (adds bars, keeps whole frame).
    if let Some((tw, th)) = edit.pad.as_deref().and_then(parse_aspect) {
        vf.push(format!(
            "pad=w='ceil(max(iw,ih*{tw}/{th})/2)*2':h='ceil(max(ih,iw*{th}/{tw})/2)*2':x='(ow-iw)/2':y='(oh-ih)/2':color=black"
        ));
    }
    if edit.denoise {
        vf.push("hqdn3d".into());
    }
    let eq_changed = edit.brightness.abs() > 1e-6
        || (edit.contrast - 1.0).abs() > 1e-6
        || (edit.saturation - 1.0).abs() > 1e-6;
    if eq_changed {
        vf.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            edit.brightness, edit.contrast, edit.saturation
        ));
    }
    if let Some(f) = edit.filter.as_deref().and_then(filter_preset) {
        vf.push(f.into());
    }
    if edit.sharpen > 1e-6 {
        vf.push(format!(
            "unsharp=5:5:{:.3}:5:5:0.0",
            edit.sharpen.clamp(0.0, 5.0)
        ));
    }
    if edit.vignette {
        vf.push("vignette".into());
    }
    if edit.grain > 1e-6 {
        vf.push(format!(
            "noise=alls={:.0}:allf=t",
            edit.grain.clamp(0.0, 100.0)
        ));
    }
    if edit.reverse {
        vf.push("reverse".into());
    }
    if temporal {
        let speed = edit.speed;
        if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
            vf.push(format!("setpts={:.6}*PTS", 1.0 / speed));
        }
        if edit.fade_in > 0.0 {
            vf.push(format!("fade=t=in:st=0:d={:.3}", edit.fade_in));
        }
        if edit.fade_out > 0.0 && out_dur > edit.fade_out {
            vf.push(format!(
                "fade=t=out:st={:.3}:d={:.3}",
                out_dur - edit.fade_out,
                edit.fade_out
            ));
        }
    }
    vf
}

/// Build the audio filter chain: areverse -> volume -> atempo -> afade.
fn audio_filters(edit: &EditRequest, out_dur: f64) -> Vec<String> {
    let mut af: Vec<String> = Vec::new();
    if edit.reverse {
        af.push("areverse".into());
    }
    if (edit.volume - 1.0).abs() > 1e-6 {
        af.push(format!("volume={:.3}", edit.volume.max(0.0)));
    }
    let speed = edit.speed;
    if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
        // atempo only accepts 0.5..=2.0; the frontend clamps speed to that range.
        af.push(format!("atempo={:.6}", speed.clamp(0.5, 2.0)));
    }
    if edit.fade_in > 0.0 {
        af.push(format!("afade=t=in:st=0:d={:.3}", edit.fade_in));
    }
    if edit.fade_out > 0.0 && out_dur > edit.fade_out {
        af.push(format!(
            "afade=t=out:st={:.3}:d={:.3}",
            out_dur - edit.fade_out,
            edit.fade_out
        ));
    }
    af
}

/// Append audio options: `-an` when muted, otherwise the filter chain + codec.
fn push_audio(args: &mut Vec<String>, edit: &EditRequest, out_dur: f64, codec: &str) {
    if edit.mute {
        args.push("-an".into());
        return;
    }
    let af = audio_filters(edit, out_dur);
    if !af.is_empty() {
        args.push("-af".into());
        args.push(af.join(","));
    }
    args.push("-c:a".into());
    args.push(codec.into());
    args.push("-b:a".into());
    args.push("128k".into());
}

fn push_fps(args: &mut Vec<String>, edit: &EditRequest) {
    if let Some(fps) = edit.fps {
        if fps > 0.0 {
            args.push("-r".into());
            args.push(format!("{fps:.3}"));
        }
    }
}

/// Build the ffmpeg argument list for an edit request.
///
/// Trim is applied as an *input* option (`-ss` + `-t`) so it happens before the
/// filter graph; geometry/colour/speed/fade then operate on the trimmed stream.
/// `source_duration` is the probed length of the input, used to time fade-outs.
/// The output container/codecs depend on `edit.format` (mp4/webm/gif/png/mp3).
pub fn build_ffmpeg_args(
    input: &Path,
    output: &Path,
    edit: &EditRequest,
    source_duration: f64,
) -> Vec<String> {
    let format = edit.format.as_deref().unwrap_or("mp4");

    // Multi-segment edits (cut from the middle / stitch ranges) need a concat
    // filter graph; only meaningful for the video containers.
    let segs = valid_segments(edit);
    if !segs.is_empty() && matches!(format, "mp4" | "webm") {
        return build_concat_args(input, output, edit, &segs, source_duration);
    }

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

    let out_dur = expected_output_secs(edit, source_duration);

    match format {
        "mp3" => {
            // Audio-only extraction.
            let af = audio_filters(edit, out_dur);
            if !af.is_empty() {
                args.push("-af".into());
                args.push(af.join(","));
            }
            args.push("-vn".into());
            args.push("-c:a".into());
            args.push("libmp3lame".into());
            args.push("-q:a".into());
            args.push("2".into());
        }
        "png" => {
            // Single still frame at the trim start (positioned by -ss above).
            let vf = video_filters(edit, out_dur, false);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            args.push("-frames:v".into());
            args.push("1".into());
            args.push("-an".into());
        }
        "gif" => {
            // Generate a per-clip palette for a good-looking gif (single pass
            // via split + palettegen/paletteuse).
            let mut parts = video_filters(edit, out_dur, true);
            let fps = edit.fps.filter(|f| *f > 0.0).unwrap_or(12.0);
            parts.push(format!("fps={fps:.3}"));
            let graph = format!(
                "{},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
                parts.join(",")
            );
            args.push("-vf".into());
            args.push(graph);
            args.push("-an".into());
        }
        "webm" => {
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            push_audio(&mut args, edit, out_dur, "libopus");
            push_video_codec(&mut args, edit, "webm");
        }
        _ => {
            // mp4 (default): H.264 or H.265.
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            push_audio(&mut args, edit, out_dur, "aac");
            push_video_codec(&mut args, edit, "mp4");
        }
    }

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Push the video encoder and its quality/pixel-format options for `format`
/// (VP9 for webm, otherwise H.264/H.265), then fps and (for mp4) faststart.
/// Shared by the single-pass and concat paths so codec settings live in one place.
fn push_video_codec(args: &mut Vec<String>, edit: &EditRequest, format: &str) {
    if format == "webm" {
        args.push("-c:v".into());
        args.push("libvpx-vp9".into());
        args.push("-crf".into());
        args.push(edit.quality.unwrap_or(32).to_string());
        args.push("-b:v".into());
        args.push("0".into());
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    } else {
        let h265 = edit.codec.as_deref() == Some("h265");
        args.push("-c:v".into());
        args.push(if h265 { "libx265" } else { "libx264" }.into());
        args.push("-preset".into());
        args.push("veryfast".into());
        args.push("-crf".into());
        args.push(
            edit.quality
                .unwrap_or(if h265 { 28 } else { 23 })
                .to_string(),
        );
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
        if h265 {
            // hvc1 tag keeps the result playable in QuickTime/Safari.
            args.push("-tag:v".into());
            args.push("hvc1".into());
        }
    }
    push_fps(args, edit);
    if format != "webm" {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }
}

/// Build args for a multi-segment edit: trim each keep-segment, concat them,
/// then apply the usual geometry/colour/speed/fade filters to the result.
fn build_concat_args(
    input: &Path,
    output: &Path,
    edit: &EditRequest,
    segments: &[&crate::model::Trim],
    source_duration: f64,
) -> Vec<String> {
    let format = edit.format.as_deref().unwrap_or("mp4");
    let muted = edit.mute;
    let n = segments.len();
    let out_dur = expected_output_secs(edit, source_duration);

    let mut graph = String::new();
    for (i, s) in segments.iter().enumerate() {
        graph.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{i}];",
            s.start, s.end
        ));
        if !muted {
            graph.push_str(&format!(
                "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}];",
                s.start, s.end
            ));
        }
    }
    for i in 0..n {
        graph.push_str(&format!("[v{i}]"));
        if !muted {
            graph.push_str(&format!("[a{i}]"));
        }
    }
    if muted {
        graph.push_str(&format!("concat=n={n}:v=1:a=0[cv]"));
    } else {
        graph.push_str(&format!("concat=n={n}:v=1:a=1[cv][ca]"));
    }

    // Effects apply to the concatenated stream.
    let vf = video_filters(edit, out_dur, true);
    let vmap = if vf.is_empty() {
        "[cv]".to_string()
    } else {
        graph.push_str(&format!(";[cv]{}[vout]", vf.join(",")));
        "[vout]".to_string()
    };
    let amap = if muted {
        None
    } else {
        let af = audio_filters(edit, out_dur);
        if af.is_empty() {
            Some("[ca]".to_string())
        } else {
            graph.push_str(&format!(";[ca]{}[aout]", af.join(",")));
            Some("[aout]".to_string())
        }
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        graph,
        "-map".into(),
        vmap,
    ];
    if let Some(am) = &amap {
        args.push("-map".into());
        args.push(am.clone());
    }

    push_video_codec(&mut args, edit, format);
    if amap.is_some() {
        args.push("-c:a".into());
        args.push(if format == "webm" { "libopus" } else { "aac" }.into());
        args.push("-b:a".into());
        args.push("128k".into());
    }

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Valid keep-segments (positive length), if any were requested.
fn valid_segments(edit: &EditRequest) -> Vec<&crate::model::Trim> {
    match &edit.segments {
        Some(segs) => segs.iter().filter(|s| s.end - s.start > 0.01).collect(),
        None => Vec::new(),
    }
}

/// Expected output duration (seconds) for an edit, used to scale ffmpeg progress.
pub fn expected_output_secs(edit: &EditRequest, source_duration: f64) -> f64 {
    let segs = valid_segments(edit);
    let base = if !segs.is_empty() {
        segs.iter().map(|s| (s.end - s.start).max(0.0)).sum()
    } else {
        match &edit.trim {
            Some(t) => (t.end - t.start).max(0.0),
            None => source_duration,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edit(v: serde_json::Value) -> EditRequest {
        serde_json::from_value(v).expect("valid EditRequest")
    }

    fn args_for(v: serde_json::Value, dur: f64) -> Vec<String> {
        let input = Path::new("/in.mp4");
        let output = Path::new("/out.mp4");
        build_ffmpeg_args(input, output, &edit(v), dur)
    }

    fn vf(args: &[String]) -> String {
        let i = args.iter().position(|a| a == "-vf").expect("has -vf");
        args[i + 1].clone()
    }

    fn af(args: &[String]) -> Option<String> {
        args.iter()
            .position(|a| a == "-af")
            .map(|i| args[i + 1].clone())
    }

    #[test]
    fn default_edit_reencodes_with_audio() {
        let args = args_for(json!({ "videoId": "x" }), 10.0);
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"aac".to_string()));
        assert!(!args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
    }

    #[test]
    fn trim_is_input_side() {
        let args = args_for(
            json!({ "videoId": "x", "trim": { "start": 2.0, "end": 5.0 } }),
            10.0,
        );
        let ss = args.iter().position(|a| a == "-ss").unwrap();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert!(ss < i, "-ss must precede -i");
        assert_eq!(
            args[args.iter().position(|a| a == "-t").unwrap() + 1],
            "3.000"
        );
    }

    #[test]
    fn mute_drops_audio() {
        let args = args_for(json!({ "videoId": "x", "mute": true }), 10.0);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"aac".to_string()));
        assert!(af(&args).is_none());
    }

    #[test]
    fn geometry_and_colour_order() {
        let args = args_for(
            json!({
                "videoId": "x",
                "crop": { "x": 10, "y": 20, "w": 101, "h": 51 },
                "rotate": 90,
                "flipH": true,
                "scale": { "w": 640, "h": -2 },
                "filter": "grayscale",
                "fadeOut": 1.0
            }),
            10.0,
        );
        let chain = vf(&args);
        // Crop forces even dimensions.
        assert!(chain.contains("crop=100:50:10:20"), "{chain}");
        let order = |s: &str| chain.find(s).unwrap();
        assert!(order("crop") < order("transpose=1"));
        assert!(order("transpose=1") < order("hflip"));
        assert!(order("hflip") < order("scale"));
        assert!(order("scale") < order("hue=s=0"));
        assert!(chain.contains("fade=t=out:st=9.000:d=1.000"), "{chain}");
    }

    #[test]
    fn audio_chain_volume_and_speed() {
        let args = args_for(json!({ "videoId": "x", "volume": 1.5, "speed": 2.0 }), 10.0);
        let chain = af(&args).expect("has -af");
        assert!(chain.contains("volume=1.500"), "{chain}");
        assert!(chain.contains("atempo=2.000000"), "{chain}");
        // Speed halves duration: setpts on the video side.
        assert!(vf(&args).contains("setpts="));
    }

    #[test]
    fn fps_override_present() {
        let args = args_for(json!({ "videoId": "x", "fps": 30.0 }), 10.0);
        let i = args.iter().position(|a| a == "-r").expect("has -r");
        assert_eq!(args[i + 1], "30.000");
    }

    #[test]
    fn validate_url_blocks_local_and_bad_schemes() {
        assert!(validate_url("https://example.com/v").is_ok());
        assert!(validate_url("http://1.2.3.4/v").is_ok());
        assert!(validate_url("ftp://example.com/v").is_err());
        assert!(validate_url("https://localhost/v").is_err());
        assert!(validate_url("http://127.0.0.1/v").is_err());
        assert!(validate_url("http://10.0.0.5/v").is_err());
        assert!(validate_url("http://192.168.1.1/v").is_err());
        assert!(validate_url("http://[::1]/v").is_err());
        assert!(validate_url("not a url").is_err());
    }

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
    fn output_ext_maps_formats() {
        assert_eq!(output_ext(None), "mp4");
        assert_eq!(output_ext(Some("mp4")), "mp4");
        assert_eq!(output_ext(Some("webm")), "webm");
        assert_eq!(output_ext(Some("gif")), "gif");
        assert_eq!(output_ext(Some("png")), "png");
        assert_eq!(output_ext(Some("mp3")), "mp3");
        assert_eq!(output_ext(Some("weird")), "mp4");
    }

    #[test]
    fn h265_codec_and_quality() {
        let args = args_for(
            json!({ "videoId": "x", "codec": "h265", "quality": 20 }),
            10.0,
        );
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "20");
    }

    #[test]
    fn webm_uses_vp9_and_opus() {
        let args = args_for(json!({ "videoId": "x", "format": "webm" }), 10.0);
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.contains(&"libopus".to_string()));
        assert!(!args.contains(&"+faststart".to_string()));
    }

    #[test]
    fn gif_has_palette_graph_and_no_audio() {
        let args = args_for(json!({ "videoId": "x", "format": "gif" }), 10.0);
        let chain = vf(&args);
        assert!(chain.contains("palettegen"), "{chain}");
        assert!(chain.contains("paletteuse"), "{chain}");
        assert!(chain.contains("fps="), "{chain}");
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn png_grabs_single_frame() {
        let args = args_for(
            json!({ "videoId": "x", "format": "png", "trim": { "start": 3.0, "end": 9.0 } }),
            10.0,
        );
        let frames = args.iter().position(|a| a == "-frames:v").unwrap();
        assert_eq!(args[frames + 1], "1");
        assert!(args.contains(&"-an".to_string()));
        // Trim still positions the grab.
        assert_eq!(
            args[args.iter().position(|a| a == "-ss").unwrap() + 1],
            "3.000"
        );
    }

    fn filter_complex(args: &[String]) -> String {
        let i = args
            .iter()
            .position(|a| a == "-filter_complex")
            .expect("has -filter_complex");
        args[i + 1].clone()
    }

    #[test]
    fn censor_vignette_pad_in_chain() {
        let args = args_for(
            json!({
                "videoId": "x",
                "censor": { "x": 10, "y": 20, "w": 100, "h": 50 },
                "censorColor": "white",
                "vignette": true,
                "pad": "9:16",
                "crop": { "x": 0, "y": 0, "w": 320, "h": 240 }
            }),
            10.0,
        );
        let chain = vf(&args);
        assert!(
            chain.contains("drawbox=x=10:y=20:w=100:h=50:color=white:t=fill"),
            "{chain}"
        );
        assert!(chain.contains("vignette"), "{chain}");
        assert!(chain.contains("pad=w="), "{chain}");
        // Censor is applied before crop (source coordinates).
        assert!(chain.find("drawbox").unwrap() < chain.find("crop=").unwrap());
    }

    #[test]
    fn denoise_sharpen_grain_in_chain() {
        let args = args_for(
            json!({ "videoId": "x", "denoise": true, "sharpen": 1.5, "grain": 20.0, "filter": "teal-orange" }),
            10.0,
        );
        let chain = vf(&args);
        assert!(chain.contains("hqdn3d"), "{chain}");
        assert!(chain.contains("unsharp=5:5:1.500"), "{chain}");
        assert!(chain.contains("noise=alls=20"), "{chain}");
        assert!(chain.contains("colorbalance="), "{chain}"); // teal-orange preset
                                                             // Order: denoise -> preset -> sharpen -> grain.
        assert!(chain.find("hqdn3d").unwrap() < chain.find("unsharp").unwrap());
        assert!(chain.find("unsharp").unwrap() < chain.find("noise=alls").unwrap());
    }

    #[test]
    fn look_presets_map_to_filters() {
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "faded" }),
            10.0
        ))
        .contains("curves="));
        assert!(
            vf(&args_for(json!({ "videoId": "x", "filter": "noir" }), 10.0)).contains("hue=s=0")
        );
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "vintage" }),
            10.0
        ))
        .contains("curves="));
    }

    #[test]
    fn censor_color_sanitized() {
        let args = args_for(
            json!({ "videoId": "x", "censor": { "x": 0, "y": 0, "w": 10, "h": 10 }, "censorColor": "; rm -rf" }),
            10.0,
        );
        assert!(vf(&args).contains("color=black"));
    }

    #[test]
    fn segments_concat_graph() {
        let args = args_for(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 4.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("trim=start=0.000:end=1.000"), "{graph}");
        assert!(graph.contains("trim=start=3.000:end=4.000"), "{graph}");
        assert!(graph.contains("concat=n=2:v=1:a=1[cv][ca]"), "{graph}");
        assert!(args.contains(&"libx264".to_string()));
        // The concat path replaces input-side trim.
        assert!(!args.contains(&"-ss".to_string()));
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 2);
    }

    #[test]
    fn segments_muted_drops_audio_streams() {
        let args = args_for(
            json!({
                "videoId": "x",
                "mute": true,
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("concat=n=2:v=1:a=0[cv]"), "{graph}");
        assert!(!graph.contains("atrim"), "{graph}");
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 1);
    }

    #[test]
    fn segments_carry_codec_and_faststart() {
        // The concat path must produce the same encoder settings as the simple
        // path (shared via push_video_codec): h265 tag + faststart + mapped audio.
        let args = args_for(
            json!({
                "videoId": "x",
                "codec": "h265",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        assert!(args.contains(&"aac".to_string())); // not muted -> audio mapped
    }

    #[test]
    fn segments_ignored_for_gif() {
        // gif is not a concat target, so it falls back to the normal path.
        let args = args_for(
            json!({ "videoId": "x", "format": "gif", "segments": [{ "start": 0.0, "end": 1.0 }] }),
            5.0,
        );
        assert!(!args.contains(&"-filter_complex".to_string()));
        assert!(vf(&args).contains("palettegen"));
    }

    #[test]
    fn expected_output_secs_trim_speed_segments() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Whole clip.
        assert!(close(
            expected_output_secs(&edit(json!({ "videoId": "x" })), 10.0),
            10.0
        ));
        // Trim narrows to its length.
        assert!(close(
            expected_output_secs(
                &edit(json!({ "videoId": "x", "trim": { "start": 2.0, "end": 7.0 } })),
                10.0
            ),
            5.0
        ));
        // Speed shortens proportionally.
        assert!(close(
            expected_output_secs(&edit(json!({ "videoId": "x", "speed": 2.0 })), 10.0),
            5.0
        ));
        // Segments sum their lengths (and override trim).
        assert!(close(
            expected_output_secs(
                &edit(json!({
                    "videoId": "x",
                    "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 5.0 }]
                })),
                10.0
            ),
            3.0
        ));
    }

    #[test]
    fn mp3_is_audio_only() {
        let args = args_for(
            json!({ "videoId": "x", "format": "mp3", "volume": 0.5 }),
            10.0,
        );
        assert!(args.contains(&"-vn".to_string()));
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
        assert!(af(&args).unwrap().contains("volume=0.500"));
    }
}
