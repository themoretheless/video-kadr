//! Auto black-bar crop via FFmpeg `cropdetect`.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tokio::process::Command;

use crate::model::Crop;
use crate::process_control::{capture_output, ProcessRuntime};
use crate::tools::OFFLINE_PROTOCOLS;

const CROPDETECT_TIMEOUT: Duration = Duration::from_secs(60);

/// Parse the last `crop=W:H:X:Y` token from cropdetect stderr/log lines.
pub fn parse_cropdetect_output(log: &str) -> Option<Crop> {
    let mut last = None;
    for line in log.lines() {
        if let Some(crop) = parse_cropdetect_line(line) {
            last = Some(crop);
        }
    }
    last
}

fn parse_cropdetect_line(line: &str) -> Option<Crop> {
    let idx = line.find("crop=")?;
    let rest = &line[idx + "crop=".len()..];
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .next()?;
    let mut parts = token.split(':');
    let w: u32 = parts.next()?.parse().ok()?;
    let h: u32 = parts.next()?.parse().ok()?;
    let x: u32 = parts.next()?.parse().ok()?;
    let y: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || w < 2 || h < 2 {
        return None;
    }
    Some(Crop { x, y, w, h })
}

/// Force even dimensions required by common yuv420 encoders and clamp to frame.
pub fn normalize_detected_crop(mut crop: Crop, frame_w: u32, frame_h: u32) -> Result<Crop> {
    if frame_w < 2 || frame_h < 2 {
        bail!("кадр слишком мал для cropdetect");
    }
    crop.w = crop.w.min(frame_w).max(2) & !1;
    crop.h = crop.h.min(frame_h).max(2) & !1;
    crop.x = crop.x.min(frame_w.saturating_sub(crop.w)) & !1;
    crop.y = crop.y.min(frame_h.saturating_sub(crop.h)) & !1;
    if crop.w < 2 || crop.h < 2 || crop.x + crop.w > frame_w || crop.y + crop.h > frame_h {
        bail!("cropdetect вернул недопустимый прямоугольник");
    }
    // Reject near-full-frame suggestions — no black bars to remove.
    let area = u64::from(crop.w) * u64::from(crop.h);
    let frame_area = u64::from(frame_w) * u64::from(frame_h);
    if area * 100 >= frame_area * 98 {
        bail!("чёрные поля не обнаружены");
    }
    Ok(crop)
}

/// Sample a short window near the middle of the clip and return a crop rect.
pub async fn detect_letterbox_crop(
    runtime: &ProcessRuntime,
    path: &Path,
    duration_seconds: f64,
    frame_w: u32,
    frame_h: u32,
) -> Result<Crop> {
    let seek = if duration_seconds.is_finite() && duration_seconds > 2.0 {
        (duration_seconds * 0.35).clamp(0.0, duration_seconds - 1.5)
    } else {
        0.0
    };
    let sample = if duration_seconds.is_finite() && duration_seconds > 0.5 {
        duration_seconds.clamp(0.5, 2.0)
    } else {
        1.0
    };

    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-nostats")
        .arg("-protocol_whitelist")
        .arg(OFFLINE_PROTOCOLS)
        .arg("-ss")
        .arg(format!("{seek:.3}"))
        .arg("-t")
        .arg(format!("{sample:.3}"))
        .arg("-i")
        .arg(path)
        .arg("-vf")
        .arg("cropdetect=limit=24:round=2:reset=0")
        .arg("-f")
        .arg("null")
        .arg("-");

    let output = capture_output(
        runtime,
        command,
        runtime.probe_policy(path),
        CROPDETECT_TIMEOUT,
    )
    .await
    .context("не удалось запустить ffmpeg cropdetect")?;

    // cropdetect writes suggestions to stderr even when exit is 0.
    let log = String::from_utf8_lossy(&output.stderr);
    let raw = parse_cropdetect_output(&log)
        .ok_or_else(|| anyhow!("cropdetect не вернул crop=W:H:X:Y"))?;
    normalize_detected_crop(raw, frame_w, frame_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_cropdetect_suggestion() {
        let log = "\
[Parsed_cropdetect_0 @ 0x1] x1:0 x2:1279 y1:60 y2:659 w:1280 h:600 x:0 y:60 pts:1 t:0.04 crop=1280:600:0:60
[Parsed_cropdetect_0 @ 0x1] x1:2 x2:1277 y1:62 y2:657 w:1276 h:596 x:2 y:62 pts:2 t:0.08 crop=1276:596:2:62
";
        assert_eq!(
            parse_cropdetect_output(log),
            Some(Crop {
                x: 2,
                y: 62,
                w: 1276,
                h: 596
            })
        );
    }

    #[test]
    fn normalizes_to_even_clamped_rect() {
        let crop = normalize_detected_crop(
            Crop {
                x: 3,
                y: 5,
                w: 101,
                h: 51,
            },
            1280,
            720,
        )
        .unwrap();
        assert_eq!(crop.x % 2, 0);
        assert_eq!(crop.y % 2, 0);
        assert_eq!(crop.w % 2, 0);
        assert_eq!(crop.h % 2, 0);
        assert!(crop.x + crop.w <= 1280);
        assert!(crop.y + crop.h <= 720);
    }

    #[test]
    fn rejects_near_full_frame() {
        assert!(normalize_detected_crop(
            Crop {
                x: 0,
                y: 0,
                w: 1280,
                h: 720
            },
            1280,
            720
        )
        .is_err());
    }
}
