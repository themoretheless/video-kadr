use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ports::CompiledExportCommand;
use crate::process_control::ProcessRuntime;
use crate::render::frame_renderer::{FrameRenderer, FrameRequest};
use crate::tools::{run_compiled_ffmpeg, Done};

/// Turn a full edited-video command into a one-frame image command without
/// changing its temporal filter graph. The selector streams the graph until
/// the first real frame at or after the canonical requested tick.
pub fn select_edited_output_frame(
    command: &mut CompiledExportCommand,
    timeline_tick: u64,
    timeline_time_base: u32,
    quality: u8,
) -> Result<()> {
    if timeline_time_base == 0 || !command.expected_duration_seconds.is_finite() {
        return Err(anyhow!("preview frame selection clock is invalid"));
    }
    let requested = timeline_tick as f64 / f64::from(timeline_time_base);
    // Edited time is half-open. In particular EOF is rejected instead of
    // silently mapping to a source window with no frame behind it.
    if requested < 0.0 || requested >= command.expected_duration_seconds {
        return Err(anyhow!("preview tick is outside edited duration"));
    }
    // `gte` makes a concat boundary half-open in the same direction as the
    // timeline: the boundary belongs to the following segment.
    // Export expresses its optional CFR conversion as output `-r`. Move that
    // conversion into the graph before selection so the preview observes the
    // same duplicated/dropped frame clock rather than the source frame clock.
    let output_fps = command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "-r")
        .map(|pair| pair[1].clone());
    // Export closes to limited Rec.709. JPEG delivery needs full-range sRGB,
    // so convert the pixels here instead of retaining/relabeling MP4 output.
    let selector = format!(
        "colorspace=ispace=bt709:irange=tv:iprimaries=bt709:itrc=bt709:space=bt709:range=pc:primaries=bt709:trc=srgb:format=yuv420p,setparams=range=pc:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709,{}select='gte(t\\,{requested:.6})',setpts=PTS-STARTPTS",
        output_fps
            .as_deref()
            .map(|fps| format!("fps={fps},"))
            .unwrap_or_default()
    );

    if let Some(index) = command
        .arguments
        .iter()
        .position(|arg| arg == "-filter_complex")
    {
        let map_index = command
            .arguments
            .iter()
            .enumerate()
            .skip(index + 2)
            .find(|(_, arg)| arg.as_str() == "-map")
            .map(|(index, _)| index)
            .ok_or_else(|| anyhow!("preview complex graph has no video map"))?;
        let input_label = command
            .arguments
            .get(map_index + 1)
            .filter(|label| label.starts_with('[') && label.ends_with(']'))
            .cloned()
            .ok_or_else(|| anyhow!("preview complex graph video map is invalid"))?;
        let graph = command
            .arguments
            .get_mut(index + 1)
            .ok_or_else(|| anyhow!("preview filter_complex has no graph"))?;
        graph.push_str(&format!(";{input_label}{selector}[preview_frame]"));
        command.arguments[map_index + 1] = "[preview_frame]".into();
    } else if let Some(index) = command.arguments.iter().position(|arg| arg == "-vf") {
        let filters = command
            .arguments
            .get_mut(index + 1)
            .ok_or_else(|| anyhow!("preview -vf has no graph"))?;
        if !filters.is_empty() {
            filters.push(',');
        }
        filters.push_str(&selector);
    } else {
        let output_index = command.arguments.len().saturating_sub(1);
        command
            .arguments
            .splice(output_index..output_index, ["-vf".into(), selector]);
    }

    // Drop video/container options belonging to the intermediate MP4 profile;
    // the filter graph and its ordering above remain byte-for-byte intact.
    let destination = command
        .arguments
        .pop()
        .ok_or_else(|| anyhow!("preview command has no destination"))?;
    let mut cleaned = Vec::with_capacity(command.arguments.len() + 12);
    let mut index = 0;
    while index < command.arguments.len() {
        let option = command.arguments[index].as_str();
        if matches!(
            option,
            "-c:v"
                | "-preset"
                | "-crf"
                | "-pix_fmt"
                | "-tag:v"
                | "-r"
                | "-movflags"
                | "-color_range"
                | "-colorspace"
                | "-color_trc"
                | "-color_primaries"
        ) {
            index += 2;
        } else {
            cleaned.push(command.arguments[index].clone());
            index += 1;
        }
    }
    cleaned.extend([
        "-frames:v".into(),
        "1".into(),
        "-an".into(),
        "-c:v".into(),
        "mjpeg".into(),
        "-q:v".into(),
        ((100_u16.saturating_sub(u16::from(quality))) / 4 + 2)
            .clamp(2, 31)
            .to_string(),
        "-color_range".into(),
        "pc".into(),
        "-colorspace".into(),
        "bt709".into(),
        "-color_trc".into(),
        "iec61966-2-1".into(),
        "-color_primaries".into(),
        "bt709".into(),
        "-f".into(),
        "image2".into(),
        "-update".into(),
        "1".into(),
        destination,
    ]);
    command.arguments = cleaned;
    Ok(())
}

/// Executes a precompiled immutable EditPlan as a single preview image. The
/// command carries the same filter graph/resources as export; only its final
/// destination is replaced with the service-owned staging path.
pub struct FfmpegPreviewFrameRenderer {
    runtime: ProcessRuntime,
    command: CompiledExportCommand,
    compatibility: String,
    timeout: Duration,
}

impl FfmpegPreviewFrameRenderer {
    pub fn new(
        runtime: ProcessRuntime,
        command: CompiledExportCommand,
        compatibility: String,
        timeout: Duration,
    ) -> Result<Self> {
        if command.arguments.is_empty() || compatibility.is_empty() {
            return Err(anyhow!("preview frame renderer configuration is invalid"));
        }
        Ok(Self {
            runtime,
            command,
            compatibility,
            timeout,
        })
    }
}

#[axum::async_trait]
impl FrameRenderer for FfmpegPreviewFrameRenderer {
    fn compatibility(&self) -> &str {
        &self.compatibility
    }

    async fn render_frame(
        &self,
        _request: &FrameRequest,
        staging_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let mut command = self.command.clone();
        let destination = command
            .arguments
            .last_mut()
            .ok_or_else(|| anyhow!("preview command has no destination"))?;
        *destination = staging_path.to_string_lossy().into_owned();
        let (progress, _rx) = mpsc::unbounded_channel();
        match run_compiled_ffmpeg(
            &self.runtime,
            &command,
            &progress,
            cancellation,
            self.timeout,
        )
        .await?
        {
            Done::Completed => Ok(()),
            Done::Cancelled => Err(anyhow!("preview frame render cancelled")),
        }
    }

    async fn validate_frame(&self, request: &FrameRequest, staging_path: &Path) -> Result<()> {
        let probe = crate::tools::probe_video(&self.runtime, staging_path).await?;
        let expected_width = request.settings.width.saturating_add(1) & !1;
        let expected_height = request.settings.height.saturating_add(1) & !1;
        if probe.width != expected_width || probe.height != expected_height {
            return Err(anyhow!("optimized preview dimensions do not match request"));
        }
        let expected_codec = match request.settings.format {
            crate::render::frame_renderer::PreviewFrameFormat::Jpeg => "mjpeg",
            crate::render::frame_renderer::PreviewFrameFormat::Webp => "webp",
        };
        if probe.vcodec.as_deref() != Some(expected_codec) {
            return Err(anyhow!("optimized preview codec does not match request"));
        }
        Ok(())
    }
}
