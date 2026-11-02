//! Whole-source floor scan plus independently cached span measurements.
//! Pixel metrics and scene-log reduction adapted from WeftCut (MIT),
//! Copyright (c) 2026 UncleChair. See docs/licenses/weftcut-MIT.txt.
use crate::{
    process_control::{capture_output, ProcessRuntime},
    tools::OFFLINE_PROTOCOLS,
};
use anyhow::{ensure, Context, Result};
use image::RgbImage;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use tokio::process::Command;

pub const SCENE_FLOOR: f64 = 0.05;
pub const MAX_CANDIDATES: usize = 10_000;
pub const MAX_MEASURE_SPANS: usize = 64;
const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cut {
    pub tick: u64,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShotScan {
    pub version: u32,
    pub source_hash: String,
    pub duration_ticks: u64,
    pub floor: f64,
    pub cuts: Vec<Cut>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub start_ticks: u64,
    pub end_ticks: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShotMeasurement {
    pub span: Span,
    pub brightness: f64,
    pub motion: f64,
    pub sharpness: f64,
    pub flags: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameComparison {
    pub left_tick: u64,
    pub right_tick: u64,
    pub similarity: f64,
    pub motion: f64,
}

pub fn parse_scene_cuts(log: &str, duration_ticks: u64) -> Result<Vec<Cut>> {
    let mut time = None;
    let mut cuts = Vec::new();
    for line in log.lines() {
        if let Some((_, value)) = line.split_once("pts_time:") {
            let seconds: f64 = value
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .parse()
                .context("invalid scene time")?;
            ensure!(
                seconds.is_finite() && seconds >= 0.0 && seconds <= 86400.0,
                "invalid scene time"
            );
            time = Some((seconds * 1_000_000.0).round() as u64);
        } else if let Some((_, value)) = line.split_once("lavfi.scene_score=") {
            let score: f64 = value
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .parse()
                .context("invalid scene score")?;
            ensure!(
                score.is_finite() && (0.0..=1.0).contains(&score),
                "invalid scene score"
            );
            if let Some(tick) = time.take() {
                if tick > 0 && tick < duration_ticks && score > SCENE_FLOOR {
                    ensure!(
                        cuts.last().is_none_or(|last: &Cut| last.tick < tick),
                        "non-monotonic scene output"
                    );
                    cuts.push(Cut { tick, score });
                    ensure!(
                        cuts.len() <= MAX_CANDIDATES,
                        "scene candidate budget exceeded"
                    );
                }
            }
        }
    }
    Ok(cuts)
}

pub async fn scan_source(
    runtime: &ProcessRuntime,
    path: &Path,
    source_hash: String,
    duration_ticks: u64,
) -> Result<ShotScan> {
    ensure!(
        duration_ticks > 0 && duration_ticks <= 86_400_000_000,
        "invalid source duration"
    );
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-nostats",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            OFFLINE_PROTOCOLS,
        ])
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            "select='gt(scene,0.05)',metadata=print:key=lavfi.scene_score:file=-",
            "-fps_mode",
            "vfr",
            "-f",
            "null",
            "-",
        ]);
    let output = capture_output(
        runtime,
        command,
        runtime.probe_policy(path),
        Duration::from_secs(180),
    )
    .await?;
    ensure!(output.status.success(), "FFmpeg scene scan failed");
    let cuts = parse_scene_cuts(std::str::from_utf8(&output.stdout)?, duration_ticks)?;
    Ok(ShotScan {
        version: VERSION,
        source_hash,
        duration_ticks,
        floor: SCENE_FLOOR,
        cuts,
    })
}

pub fn valid_cached_scan(scan: &ShotScan, hash: &str, duration: u64) -> bool {
    scan.version == VERSION
        && scan.source_hash == hash
        && scan.duration_ticks == duration
        && scan.floor == SCENE_FLOOR
        && scan.cuts.len() <= MAX_CANDIDATES
        && scan.cuts.iter().all(|cut| {
            cut.tick > 0
                && cut.tick < duration
                && cut.score.is_finite()
                && cut.score > SCENE_FLOOR
                && cut.score <= 1.0
        })
        && scan.cuts.windows(2).all(|pair| pair[0].tick < pair[1].tick)
}

pub fn validate_span(span: &Span, duration: u64) -> Result<()> {
    ensure!(
        span.start_ticks < span.end_ticks && span.end_ticks <= duration,
        "span must be inside source duration"
    );
    Ok(())
}

pub async fn extract_rgb(
    runtime: &ProcessRuntime,
    path: &Path,
    tick: u64,
    width: u32,
    height: u32,
) -> Result<RgbImage> {
    ensure!(
        width > 0 && height > 0 && width <= 320 && height <= 2048,
        "invalid measurement raster"
    );
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-loglevel",
            "error",
            "-protocol_whitelist",
            OFFLINE_PROTOCOLS,
            "-ss",
        ])
        .arg(format!("{:.6}", tick as f64 / 1_000_000.0))
        .arg("-i")
        .arg(path)
        .args(["-map", "0:v:0", "-an", "-frames:v", "1", "-vf"])
        .arg(format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease"
        ))
        .args(["-pix_fmt", "rgb24", "-c:v", "png", "-f", "image2pipe", "-"]);
    let output = capture_output(
        runtime,
        command,
        runtime.probe_policy(path),
        Duration::from_secs(30),
    )
    .await?;
    ensure!(output.status.success(), "FFmpeg frame extraction failed");
    let image =
        image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)?.to_rgb8();
    ensure!(
        image.width() > 0
            && image.height() > 0
            && image.width() <= width
            && image.height() <= height,
        "frame exceeds measurement raster"
    );
    Ok(image)
}

pub fn measurement_raster(width: u32, height: u32) -> Result<(u32, u32)> {
    ensure!(width > 0 && height > 0, "source has no video raster");
    Ok((320, 2048))
}

pub async fn measure_span(
    runtime: &ProcessRuntime,
    path: &Path,
    span: Span,
    width: u32,
    height: u32,
) -> Result<ShotMeasurement> {
    let duration = span
        .end_ticks
        .checked_sub(span.start_ticks)
        .context("invalid span")?;
    ensure!(duration > 0, "empty span");
    let inset = (duration / 8).min(250_000);
    let start = extract_rgb(runtime, path, span.start_ticks + inset, width, height).await?;
    let mid = extract_rgb(
        runtime,
        path,
        span.start_ticks + duration / 2,
        width,
        height,
    )
    .await?;
    let end = extract_rgb(
        runtime,
        path,
        (span.end_ticks - inset).min(span.end_ticks - 1),
        width,
        height,
    )
    .await?;
    Ok(measure_pixels(span, &start, &mid, &end))
}

pub fn similarity(a: &RgbImage, b: &RgbImage) -> f64 {
    image_compare::rgb_similarity_structure(&image_compare::Algorithm::MSSIMSimple, a, b)
        .map(|value| value.score)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}
fn luma(pixel: &image::Rgb<u8>) -> f64 {
    (0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64) / 255.0
}
fn brightness(image: &RgbImage) -> f64 {
    image.pixels().map(luma).sum::<f64>() / (image.width() as f64 * image.height() as f64).max(1.0)
}
fn sharpness(image: &RgbImage) -> f64 {
    if image.width() < 3 || image.height() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut squares = 0.0;
    let mut count = 0.0;
    for y in 1..image.height() - 1 {
        for x in 1..image.width() - 1 {
            let value = luma(image.get_pixel(x - 1, y))
                + luma(image.get_pixel(x + 1, y))
                + luma(image.get_pixel(x, y - 1))
                + luma(image.get_pixel(x, y + 1))
                - 4.0 * luma(image.get_pixel(x, y));
            sum += value;
            squares += value * value;
            count += 1.0;
        }
    }
    (squares / count - (sum / count).powi(2)).max(0.0)
}
fn measure_pixels(span: Span, start: &RgbImage, mid: &RgbImage, end: &RgbImage) -> ShotMeasurement {
    let (a, m, z) = (brightness(start), brightness(mid), brightness(end));
    let mut flags = Vec::new();
    if a < 0.02 && m < 0.02 && z < 0.02 {
        flags.push("black".into());
    }
    if similarity(start, mid) >= 0.985 && similarity(mid, end) >= 0.985 {
        flags.push("freeze".into());
    }
    if ((a <= m && m <= z) || (a >= m && m >= z)) && (a - z).abs() >= 0.15 && a.min(z) < 0.10 {
        flags.push("fade".into());
    }
    ShotMeasurement {
        span,
        brightness: m,
        motion: (1.0 - similarity(start, end)).clamp(0.0, 1.0),
        sharpness: sharpness(mid),
        flags,
    }
}

pub async fn write_cache(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("invalid analysis cache path")?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = parent.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    if let Err(error) = async {
        tokio::fs::write(&temporary, serde_json::to_vec(value)?).await?;
        tokio::fs::rename(&temporary, path).await?;
        Ok::<_, anyhow::Error>(())
    }
    .await
    {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    Ok(())
}

pub async fn read_cache<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let metadata = tokio::fs::metadata(path).await.ok()?;
    if metadata.len() > 2 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonfinite_unsorted_or_out_of_bounds_scan_values() {
        assert!(parse_scene_cuts("pts_time:1.0\nlavfi.scene_score=NaN", 2_000_000).is_err());
        assert!(parse_scene_cuts(
            "pts_time:1.0\nlavfi.scene_score=0.5\npts_time:0.5\nlavfi.scene_score=0.6",
            2_000_000
        )
        .is_err());
        let cuts = parse_scene_cuts(
            "pts_time:0.5\nlavfi.scene_score=0.6\npts_time:2\nlavfi.scene_score=0.8",
            2_000_000,
        )
        .unwrap();
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].tick, 500_000);
    }
    #[test]
    fn metrics_distinguish_black_freeze_fade_and_edges() {
        let span = Span {
            start_ticks: 0,
            end_ticks: 1_000_000,
        };
        let black = RgbImage::from_pixel(32, 32, image::Rgb([0, 0, 0]));
        let gray = RgbImage::from_pixel(32, 32, image::Rgb([128, 128, 128]));
        let white = RgbImage::from_pixel(32, 32, image::Rgb([255, 255, 255]));
        assert_eq!(
            measure_pixels(span.clone(), &black, &black, &black).flags,
            vec!["black", "freeze"]
        );
        assert!(measure_pixels(span.clone(), &black, &gray, &white)
            .flags
            .contains(&"fade".into()));
        assert!(measure_pixels(span.clone(), &white, &black, &white)
            .flags
            .is_empty());
        assert_eq!(similarity(&white, &white), 1.0);
        let checker = RgbImage::from_fn(32, 32, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        assert!(sharpness(&checker) > sharpness(&white));
        assert!(validate_span(&span, 999_999).is_err());
    }
    #[tokio::test]
    async fn real_ffmpeg_finds_cuts_and_measures_exact_frame_pairs() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cuts.mkv");
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=black:s=320x180:r=30:d=1",
                "-f",
                "lavfi",
                "-i",
                "color=white:s=320x180:r=30:d=1",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0",
                "-c:v",
                "ffv1",
                "-y",
            ])
            .arg(&path)
            .status()
            .await
            .expect("FFmpeg required for shot qualification");
        assert!(status.success());
        let runtime = ProcessRuntime::local_default();
        let scan = scan_source(&runtime, &path, "fixture".into(), 2_000_000)
            .await
            .unwrap();
        assert_eq!(scan.cuts.len(), 1);
        assert!(scan.cuts[0].tick.abs_diff(1_000_000) < 35_000);
        let stats = measure_span(
            &runtime,
            &path,
            Span {
                start_ticks: 0,
                end_ticks: 1_000_000,
            },
            320,
            180,
        )
        .await
        .unwrap();
        assert!(stats.flags.contains(&"black".into()));
        assert!(stats.flags.contains(&"freeze".into()));
        let a = extract_rgb(&runtime, &path, 500_000, 320, 180)
            .await
            .unwrap();
        let b = extract_rgb(&runtime, &path, 1_500_000, 320, 180)
            .await
            .unwrap();
        assert!(similarity(&a, &b) < 0.1);
    }
}
