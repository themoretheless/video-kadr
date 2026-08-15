//! Bounded FFmpeg adapter for deterministic 320x180 PNG thumbnails.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::analysis::thumbnail::{
    ThumbnailEncoder, ThumbnailKind, ThumbnailSource, FILMSTRIP_CELL_COUNT, FILMSTRIP_CELL_HEIGHT,
    FILMSTRIP_CELL_WIDTH,
};
use crate::process_control::ProcessRuntime;

use super::{run_ffmpeg, Done};

const THUMBNAIL_TIMEOUT: Duration = Duration::from_secs(20);
const FILMSTRIP_TIMEOUT: Duration = Duration::from_secs(30);
const VISUAL_FILTER: &str = "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:(ow-iw)/2:(oh-ih)/2:color=0x11131a,setsar=1";
const WAVEFORM_FILTER: &str =
    "aformat=channel_layouts=mono,showwavespic=s=320x180:split_channels=0:colors=0x7c5cff[wave]";

#[derive(Debug, Clone)]
pub struct FfmpegThumbnailEncoder {
    runtime: ProcessRuntime,
}

impl FfmpegThumbnailEncoder {
    pub fn new(runtime: ProcessRuntime) -> Self {
        Self { runtime }
    }
}

#[axum::async_trait]
impl ThumbnailEncoder for FfmpegThumbnailEncoder {
    async fn encode(
        &self,
        source: &ThumbnailSource,
        output: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let args = build_thumbnail_args(source, output);
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        match run_ffmpeg(
            &self.runtime,
            &args,
            source.duration_seconds.unwrap_or(1.0).max(0.001),
            &progress,
            cancellation,
            THUMBNAIL_TIMEOUT,
        )
        .await?
        {
            Done::Completed => Ok(()),
            Done::Cancelled => Err(anyhow!("thumbnail generation cancelled")),
        }
    }

    async fn encode_filmstrip(
        &self,
        source: &ThumbnailSource,
        output: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let args = build_filmstrip_args(source, output)?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        match run_ffmpeg(
            &self.runtime,
            &args,
            1.0,
            &progress,
            cancellation,
            FILMSTRIP_TIMEOUT,
        )
        .await?
        {
            Done::Completed => Ok(()),
            Done::Cancelled => Err(anyhow!("filmstrip generation cancelled")),
        }
    }
}

pub fn build_thumbnail_args(source: &ThumbnailSource, output: &Path) -> Vec<String> {
    let mut args = vec!["-y".into(), "-threads".into(), "1".into()];
    if source.kind == ThumbnailKind::Video {
        let seek = source
            .duration_seconds
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .map(|duration| (duration * 0.1).min(30.0))
            .unwrap_or_default();
        args.extend(["-ss".into(), format!("{seek:.3}")]);
    }
    if source.kind == ThumbnailKind::Audio {
        // Decode at most five minutes. The process also has a hard wall-clock
        // timeout, so corrupt or unusually expensive input cannot pin a worker.
        args.extend(["-t".into(), "300".into()]);
    }
    args.extend(["-i".into(), source.path.to_string_lossy().into_owned()]);
    match source.kind {
        ThumbnailKind::Video | ThumbnailKind::Image => {
            args.extend(["-vf".into(), VISUAL_FILTER.into()]);
        }
        ThumbnailKind::Audio => {
            args.extend([
                "-filter_complex".into(),
                WAVEFORM_FILTER.into(),
                "-map".into(),
                "[wave]".into(),
            ]);
        }
    }
    args.extend([
        "-frames:v".into(),
        "1".into(),
        "-an".into(),
        "-map_metadata".into(),
        "-1".into(),
        "-map_chapters".into(),
        "-1".into(),
        "-c:v".into(),
        "png".into(),
        "-f".into(),
        "image2".into(),
        "-update".into(),
        "1".into(),
        output.to_string_lossy().into_owned(),
    ]);
    args
}

pub fn build_filmstrip_args(source: &ThumbnailSource, output: &Path) -> Result<Vec<String>> {
    let duration = source
        .duration_seconds
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| anyhow!("filmstrip requires a finite positive duration"))?;
    if source.kind != ThumbnailKind::Video || duration > 24.0 * 60.0 * 60.0 {
        return Err(anyhow!("filmstrip requires a bounded video source"));
    }

    let mut args = vec!["-y".into()];
    for index in 0..FILMSTRIP_CELL_COUNT {
        let position = duration * (f64::from(index) + 0.5) / f64::from(FILMSTRIP_CELL_COUNT);
        args.extend([
            "-threads".into(),
            "1".into(),
            "-ss".into(),
            format!("{position:.6}"),
            "-i".into(),
            source.path.to_string_lossy().into_owned(),
        ]);
    }

    let mut filters = Vec::with_capacity(FILMSTRIP_CELL_COUNT as usize + 1);
    let mut inputs = String::new();
    for index in 0..FILMSTRIP_CELL_COUNT {
        filters.push(format!(
            "[{index}:v]scale={FILMSTRIP_CELL_WIDTH}:{FILMSTRIP_CELL_HEIGHT}:force_original_aspect_ratio=decrease,\
             pad={FILMSTRIP_CELL_WIDTH}:{FILMSTRIP_CELL_HEIGHT}:(ow-iw)/2:(oh-ih)/2:color=0x11131a,\
             setsar=1[cell{index}]"
        ));
        inputs.push_str(&format!("[cell{index}]"));
    }
    filters.push(format!(
        "{inputs}hstack=inputs={FILMSTRIP_CELL_COUNT}[filmstrip]"
    ));
    args.extend([
        "-filter_complex".into(),
        filters.join(";"),
        "-map".into(),
        "[filmstrip]".into(),
        "-frames:v".into(),
        "1".into(),
        "-an".into(),
        "-map_metadata".into(),
        "-1".into(),
        "-map_chapters".into(),
        "-1".into(),
        "-threads".into(),
        "1".into(),
        "-c:v".into(),
        "png".into(),
        "-f".into(),
        "image2".into(),
        "-update".into(),
        "1".into(),
        output.to_string_lossy().into_owned(),
    ]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(kind: ThumbnailKind) -> ThumbnailSource {
        ThumbnailSource {
            id: "source-1".into(),
            path: "/private/source.bin".into(),
            kind,
            duration_seconds: Some(100.0),
        }
    }

    #[test]
    fn video_uses_representative_seek_and_bounded_canvas() {
        let args = build_thumbnail_args(&source(ThumbnailKind::Video), Path::new("/cache/x.png"));
        assert!(args.windows(2).any(|pair| pair == ["-ss", "10.000"]));
        assert!(args.contains(&VISUAL_FILTER.to_string()));
        assert_eq!(args.last().map(String::as_str), Some("/cache/x.png"));
    }

    #[test]
    fn image_does_not_seek_and_strips_metadata() {
        let args = build_thumbnail_args(&source(ThumbnailKind::Image), Path::new("/cache/x.png"));
        assert!(!args.iter().any(|arg| arg == "-ss"));
        assert!(args.windows(2).any(|pair| pair == ["-map_metadata", "-1"]));
        assert!(args.windows(2).any(|pair| pair == ["-map_chapters", "-1"]));
    }

    #[test]
    fn audio_builds_deterministic_waveform() {
        let args = build_thumbnail_args(&source(ThumbnailKind::Audio), Path::new("/cache/x.png"));
        assert!(args.contains(&WAVEFORM_FILTER.to_string()));
        assert!(args.windows(2).any(|pair| pair == ["-t", "300"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "[wave]"]));
    }

    #[test]
    fn filmstrip_distributes_eight_fixed_cells_without_audio() {
        let mut video = source(ThumbnailKind::Video);
        video.duration_seconds = Some(80.0);
        let args = build_filmstrip_args(&video, Path::new("/cache/filmstrip.png")).unwrap();
        let seeks: Vec<_> = args
            .windows(2)
            .filter(|pair| pair[0] == "-ss")
            .map(|pair| pair[1].as_str())
            .collect();
        assert_eq!(
            seeks,
            [
                "5.000000",
                "15.000000",
                "25.000000",
                "35.000000",
                "45.000000",
                "55.000000",
                "65.000000",
                "75.000000",
            ]
        );
        assert_eq!(args.iter().filter(|arg| *arg == "-i").count(), 8);
        assert_eq!(
            args.windows(2)
                .filter(|pair| *pair == ["-threads", "1"])
                .count(),
            9
        );
        let graph = args
            .windows(2)
            .find(|pair| pair[0] == "-filter_complex")
            .map(|pair| pair[1].as_str())
            .unwrap();
        assert!(graph.contains("scale=160:90"));
        assert!(graph.contains("hstack=inputs=8[filmstrip]"));
        assert!(args.iter().any(|arg| arg == "-an"));
    }
}
