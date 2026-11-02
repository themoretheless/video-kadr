//! Bounded native raster + prepared audio -> FFV1/MKV, H.264/MP4 or SDR ProRes/MOV.
//! Trusted paths, source authorization and job registration belong to the caller.
use crate::domain::filmcraft_export_h264::{BitrateMode, H264Profile, ResolvedBitrate};
use crate::domain::filmcraft_export_movie::{H264AudioSettings, Multiplexer};
use crate::{
    domain::{composition::FrameRateRatio, group_surface::GroupSurface},
    process_control::{stream_with_progress, ProcessRuntime, ProcessStatus},
};
use anyhow::{ensure, Context, Result};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, process::Command, sync::mpsc::UnboundedSender};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy)]
pub enum RasterEncoding {
    NativeHevc { target_kbps:u32,max_kbps:u32,keyint:u32,cbr:bool,multiplexer:Multiplexer,audio:H264AudioSettings },
    Ffv1,
    Mjpeg {
        quality: u8,
    },
    NativeH264 {
        target_kbps: u32,
        max_kbps: u32,
        keyint: u32,
        profile: H264Profile,
        mode: BitrateMode,
        multiplexer: Multiplexer,
        audio: H264AudioSettings,
    },
    ImageSequence {
        format: u8,
    },
    Gif,
    H264 {
        target_kbps: u32,
        max_kbps: u32,
        keyint: u32,
        profile: H264Profile,
        mode: BitrateMode,
    },
    H264TwoPass {
        target_kbps: u32,
        max_kbps: u32,
        keyint: u32,
        profile: H264Profile,
        mode: BitrateMode,
    },
    ProRes {
        profile: u8,
    },
    Apv {
        profile: u8,
    },
    Mxf {
        prores: bool,
        start: i64,
        drop_frame: bool,
    },
    MxfAtom {
        start: i64,
        drop_frame: bool,
    },
    MxfH264 {
        start: i64,
        drop_frame: bool,
        keyint: Option<u32>,
        profile: H264Profile,
        bitrate: Option<ResolvedBitrate>,
    },
    DnxHr {
        profile: u8,
    },
}
impl RasterEncoding {
    pub fn two_pass(self) -> bool {
        matches!(
            self,
            Self::H264TwoPass { .. }
                | Self::H264 {
                    mode: BitrateMode::Vbr2Pass,
                    ..
                }
                | Self::NativeH264 {
                    mode: BitrateMode::Vbr2Pass,
                    ..
                }
        )
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::ImageSequence { .. } | Self::MxfAtom { .. } => "zip",
            Self::MxfH264 { .. } | Self::Mxf { .. } => "mxf",
            Self::Gif => "gif",
            Self::Ffv1 => "mkv",
            Self::Mjpeg { .. } | Self::Apv { .. } | Self::ProRes { .. } | Self::DnxHr { .. } => {
                "mov"
            }
            Self::NativeHevc { multiplexer:Multiplexer::Mov,.. } => "mov",
            Self::NativeHevc { .. } => "mp4",
            Self::NativeH264 {
                multiplexer: Multiplexer::Mov,
                ..
            } => "mov",
            Self::NativeH264 { .. } | Self::H264 { .. } | Self::H264TwoPass { .. } => "mp4",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct RasterVideoSpec {
    pub width: usize,
    pub height: usize,
    pub frame_rate: FrameRateRatio,
    pub frames: u64,
    /// Caller admission budget for temporary raw RGBA storage.
    pub staging_bytes: u64,
}
impl RasterVideoSpec {
    pub fn validate(self) -> Result<()> {
        GroupSurface::transparent(self.width, self.height)?;
        ensure!(
            self.frame_rate.numerator > 0 && self.frame_rate.denominator > 0,
            "invalid raster frame rate"
        );
        let fps = self.frame_rate.numerator as f64 / self.frame_rate.denominator as f64;
        ensure!(
            (1.0..=60.0).contains(&fps),
            "raster frame rate outside supported range"
        );
        ensure!(
            (1..=360000).contains(&self.frames),
            "invalid raster frame count"
        );
        ensure!(
            self.staging_bytes > 0 && self.staging_bytes <= 64 * 1024 * 1024 * 1024,
            "invalid raster staging budget"
        );
        let bytes = (self.width as u64)
            .checked_mul(self.height as u64)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_mul(self.frames))
            .context("raster byte size overflow")?;
        ensure!(
            bytes <= self.staging_bytes,
            "raster export exceeds staging budget"
        );
        self.time_us(self.frames - 1)?;
        Ok(())
    }
    pub fn time_us(self, index: u64) -> Result<u64> {
        ensure!(
            index < self.frames && self.frame_rate.numerator > 0,
            "invalid raster frame index"
        );
        let time = u128::from(index) * 1_000_000 * u128::from(self.frame_rate.denominator)
            / u128::from(self.frame_rate.numerator);
        ensure!(
            time <= 9_007_199_254_740_991,
            "raster clock exceeds wire range"
        );
        Ok(time as u64)
    }
}
/// Trusted stereo48k float32LE mix; caller owns its lifetime and source authorization.
/// `frames` is the actual source-floor range length (floor or ceil of nominal duration).
#[derive(Debug, Clone, Copy)]
pub struct RasterAudio<'a> {
    pub path: &'a Path,
    pub frames: u64,
}
impl RasterVideoSpec {
    /// Ceiling for capacity/admission; actual source ranges use absolute floor endpoints.
    pub fn audio_frames_48k(self) -> Result<u64> {
        self.audio_frames_at_rate(48000)
    }
    pub fn audio_frames_at_rate(self, rate: u32) -> Result<u64> {
        self.validate()?;
        ensure!(
            (8000..=192000).contains(&rate),
            "unsupported raster audio rate"
        );
        let frames =
            (u128::from(self.frames) * u128::from(self.frame_rate.denominator) * u128::from(rate))
                .div_ceil(u128::from(self.frame_rate.numerator));
        Ok(u64::try_from(frames)?)
    }
}

struct TemporaryFiles(Vec<PathBuf>);
impl Drop for TemporaryFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Encodes exactly the admitted frames as FFV1/BGRA Matroska, preserving alpha.
/// Successful publication creates a new file atomically; existing output is never replaced.
/// Frame futures must cooperate with cancellation (e.g. bounded blocking raster tasks).
pub async fn encode_frames<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_audio(
        runtime, ffmpeg, output, spec, timeout, cancel, progress, None, frame,
    )
    .await
}

/// Optional audio declares the actual sample count covering the source range.
/// At fractional frame rates this may be floor or ceil of the nominal duration.
pub async fn encode_frames_with_audio<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_options(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        RasterEncoding::Ffv1,
        frame,
    )
    .await
}

pub async fn encode_frames_with_options<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_geometry(
        runtime, ffmpeg, output, spec, timeout, cancel, progress, audio, encoding, None, frame,
    )
    .await
}

pub async fn encode_frames_with_geometry<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_metadata(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        encoding,
        pixel_aspect,
        &[],
        frame,
    )
    .await
}

pub async fn encode_frames_with_metadata<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_cpu_pool(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        encoding,
        pixel_aspect,
        metadata,
        None,
        frame,
    )
    .await
}

pub async fn encode_frames_with_cpu_pool<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    native_pool: Option<crate::runtime::cpu_pool::CpuPool>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_native_audio(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        encoding,
        pixel_aspect,
        metadata,
        native_pool,
        None,
        frame,
    )
    .await
}

pub async fn encode_frames_with_native_audio<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    native_pool: Option<crate::runtime::cpu_pool::CpuPool>,
    native_audio: Option<H264AudioSettings>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_h264_level(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        encoding,
        pixel_aspect,
        metadata,
        native_pool,
        native_audio,
        None,
        frame,
    )
    .await
}

pub async fn encode_frames_with_h264_level<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    native_pool: Option<crate::runtime::cpu_pool::CpuPool>,
    native_audio: Option<H264AudioSettings>,
    h264_level: Option<u8>,
    frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    encode_frames_with_source_range(
        runtime,
        ffmpeg,
        output,
        spec,
        timeout,
        cancel,
        progress,
        audio,
        encoding,
        pixel_aspect,
        metadata,
        native_pool,
        native_audio,
        h264_level,
        None,
        frame,
    )
    .await
}

pub async fn encode_frames_with_source_range<F, Fut>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    encoding: RasterEncoding,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    native_pool: Option<crate::runtime::cpu_pool::CpuPool>,
    native_audio: Option<H264AudioSettings>,
    h264_level: Option<u8>,
    source_range: Option<crate::domain::filmcraft_export_range::SourceRange>,
    mut frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    spec.validate()?;
    ensure!(
        h264_level.is_none()
            || matches!(
                encoding,
                RasterEncoding::NativeH264 { .. } | RasterEncoding::MxfH264 { .. }
            ),
        "explicit H.264 level requires a native H.264 path"
    );
    ensure!(
        native_audio.is_none()
            || matches!(
                encoding,
                RasterEncoding::Mjpeg { .. }
                    | RasterEncoding::ProRes { .. }
                    | RasterEncoding::DnxHr { .. }
                    | RasterEncoding::Apv { .. }
                    | RasterEncoding::Mxf { .. }
                    | RasterEncoding::MxfAtom { .. }
                    | RasterEncoding::MxfH264 { .. }
            ),
        "custom audio requires native MOV/MXF"
    );
    let native_audio = native_audio.map(|mut settings| {
        if matches!(
            encoding,
            RasterEncoding::Mxf { .. }
                | RasterEncoding::MxfAtom { .. }
                | RasterEncoding::MxfH264 { .. }
        ) || settings.codec == crate::domain::filmcraft_export_movie::AudioCodec::Auto
        {
            settings.codec = crate::domain::filmcraft_export_movie::AudioCodec::Pcm;
        }
        settings
    });
    if let Some(settings) = native_audio {
        settings.validate_rate(Multiplexer::Mov)?;
    }
    let pixel_aspect = pixel_aspect.or_else(|| {
        if matches!(
            encoding,
            RasterEncoding::Ffv1
                | RasterEncoding::Mjpeg { .. }
                | RasterEncoding::NativeHevc { .. }
        | RasterEncoding::NativeH264 { .. }
                | RasterEncoding::Gif
                | RasterEncoding::ImageSequence { .. }
        ) {
            None
        } else {
            Some((1, 1))
        }
    });
    ensure!(
        metadata.len() <= 5
            && metadata.iter().all(|(k, v)| matches!(
                k.as_str(),
                "title" | "artist" | "copyright" | "description" | "comment"
            ) && v.len() <= 16 * 1024
                && !v.contains('\0'))
            && metadata.iter().map(|(_, v)| v.len()).sum::<usize>() <= 64 * 1024,
        "invalid raster export metadata"
    );

    if let RasterEncoding::H264 {
        target_kbps,
        max_kbps,
        keyint,
        ..
    }
    | RasterEncoding::H264TwoPass {
        target_kbps,
        max_kbps,
        keyint,
        ..
    }
    | RasterEncoding::NativeH264 {
        target_kbps,
        max_kbps,
        keyint,
        ..
    } = encoding
    {
        ensure!(
            spec.width % 2 == 0
                && spec.height % 2 == 0
                && target_kbps > 0
                && max_kbps >= target_kbps
                && keyint > 0,
            "invalid H.264 raster settings"
        );
    }
    if let RasterEncoding::ProRes { profile } = encoding {
        ensure!(
            profile <= 3 && spec.width % 2 == 0 && spec.height % 2 == 0,
            "invalid ProRes raster settings"
        );
    }
    ensure!(
        !timeout.is_zero() && !cancel.is_cancelled(),
        "raster export cancelled or zero timeout"
    );
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("raster deadline overflow")?;
    if let Some(audio) = audio {
        let rate = if let RasterEncoding::NativeHevc { audio:settings,multiplexer,.. } | RasterEncoding::NativeH264 {
            audio: settings,
            multiplexer,
            ..
        } = encoding
        {
            settings.validate_rate(multiplexer)?;
            settings.sample_rate()
        } else {
            native_audio
                .map(|settings| settings.sample_rate())
                .unwrap_or(48000)
        };
        if let Some(range) = &source_range {
            ensure!(
                range.video_span(spec.frame_rate)?.0 == spec.frames,
                "source range video count changed"
            );
            ensure!(
                audio.frames == range.audio_span(rate)?.1,
                "source range audio count changed"
            );
        } else {
            ensure!(
                audio.frames == spec.audio_frames_at_rate(rate)?
                    || audio.frames
                        == u64::try_from(
                            u128::from(spec.frames)
                                * u128::from(spec.frame_rate.denominator)
                                * u128::from(rate)
                                / u128::from(spec.frame_rate.numerator)
                        )?,
                "raster audio duration does not match video"
            );
        }
        let channels = if let RasterEncoding::NativeH264 {
            audio: settings, ..
        } = encoding
        {
            settings.channels()
        } else {
            native_audio
                .map(|settings| settings.channels())
                .unwrap_or(2)
        };
        let expected = audio
            .frames
            .checked_mul(u64::from(channels) * 4)
            .context("raster audio byte size overflow")?;
        ensure!(
            tokio::fs::metadata(audio.path).await?.len() == expected,
            "raster audio PCM byte count mismatch"
        );
    }
    if matches!(encoding, RasterEncoding::Gif) {
        ensure!(audio.is_none(), "GIF does not contain audio");
        return Box::pin(super::native_image_sequence_export::encode(
            output,
            spec,
            timeout,
            cancel,
            progress,
            3,
            native_pool,
            frame,
        ))
        .await;
    }
    if let RasterEncoding::ImageSequence { format } = encoding {
        ensure!(format <= 2, "invalid image sequence format");
        ensure!(audio.is_none(), "image sequences do not contain audio");
        return Box::pin(super::native_image_sequence_export::encode(
            output,
            spec,
            timeout,
            cancel,
            progress,
            format,
            native_pool,
            frame,
        ))
        .await;
    }
    let native = match encoding {
        RasterEncoding::NativeHevc {target_kbps,max_kbps,keyint,cbr,multiplexer,audio} => Some(super::native_intra_export::Codec::HevcMovie {mov:multiplexer==Multiplexer::Mov,target_kbps,max_kbps,keyint,cbr,audio}),
        RasterEncoding::Mjpeg { quality } => {
            Some(super::native_intra_export::Codec::Mjpeg { quality })
        }
        RasterEncoding::NativeH264 {
            target_kbps,
            max_kbps,
            keyint,
            profile,
            mode,
            multiplexer,
            audio,
        } => Some(super::native_intra_export::Codec::H264Movie {
            multiplexer,
            audio,
            sar: pixel_aspect.unwrap_or((1, 1)),
            keyint: Some(keyint),
            profile,
            level: h264_level,
            bitrate: Some(ResolvedBitrate {
                target_kbps,
                max_kbps,
                mode,
            }),
        }),
        RasterEncoding::MxfH264 {
            start,
            drop_frame,
            keyint,
            profile,
            bitrate,
        } => Some(super::native_intra_export::Codec::MxfH264 {
            start,
            drop_frame,
            keyint,
            profile,
            level: h264_level,
            bitrate,
            sar: pixel_aspect.unwrap_or((1, 1)),
        }),
        RasterEncoding::MxfAtom { start, drop_frame } => {
            Some(super::native_intra_export::Codec::Atom { start, drop_frame })
        }
        RasterEncoding::Mxf {
            prores,
            start,
            drop_frame,
        } => Some(super::native_intra_export::Codec::Mxf {
            prores,
            start,
            drop_frame,
        }),
        RasterEncoding::Apv { profile } => Some(super::native_intra_export::Codec::Apv(profile)),
        RasterEncoding::DnxHr { profile } => Some(super::native_intra_export::Codec::Dnx(profile)),
        RasterEncoding::ProRes { profile } => {
            Some(super::native_intra_export::Codec::ProRes(profile))
        }
        _ => None,
    };
    if let Some(codec) = native {
        return Box::pin(super::native_intra_export::encode(
            output,
            spec,
            timeout,
            cancel,
            progress,
            audio,
            codec,
            pixel_aspect,
            metadata,
            native_pool,
            native_audio,
            frame,
        ))
        .await;
    }
    ensure!(
        !matches!(
            encoding,
            RasterEncoding::H264 {
                mode: BitrateMode::Cbr,
                ..
            } | RasterEncoding::H264TwoPass {
                mode: BitrateMode::Cbr,
                ..
            }
        ),
        "CBR requires a native H264 encoding path"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("raster output requires parent directory")?;
    ensure!(!output.try_exists()?, "raster output already exists");
    let id = uuid::Uuid::new_v4();
    let raw = parent.join(format!(".raster-{id}.rgba"));
    let encoded = parent.join(format!(".raster-{id}.{}", encoding.extension()));
    let mut guard = TemporaryFiles(Vec::new());
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&raw)
        .await?;
    guard.0.push(raw.clone());
    let owned = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&encoded)
        .await?;
    guard.0.push(encoded.clone());
    drop(owned);
    for index in 0..spec.frames {
        let surface = tokio::select! {
            biased;
            _=cancel.cancelled()=>anyhow::bail!("raster export cancelled"),
            _=tokio::time::sleep_until(deadline)=>anyhow::bail!("raster export timeout"),
            result=frame(index,spec.time_us(index)?)=>result?,
        };
        surface.validate()?;
        ensure!(
            surface.width == spec.width && surface.height == spec.height,
            "raster frame dimensions changed"
        );
        let bytes = match encoding {
            RasterEncoding::Mjpeg { .. }
                | RasterEncoding::NativeHevc { .. }
        | RasterEncoding::NativeH264 { .. }
            | RasterEncoding::Gif
            | RasterEncoding::ImageSequence { .. }
            | RasterEncoding::MxfH264 { .. }
            | RasterEncoding::MxfAtom { .. }
            | RasterEncoding::Mxf { .. }
            | RasterEncoding::Apv { .. }
            | RasterEncoding::DnxHr { .. }
            | RasterEncoding::ProRes { .. } => {
                anyhow::bail!("intra formats require native export path")
            }
            RasterEncoding::Ffv1 => surface.rgba,

            RasterEncoding::H264 { .. } | RasterEncoding::H264TwoPass { .. } => {
                tokio::task::spawn_blocking(move || {
                    crate::domain::filmcraft_h264_pixels::yuv420(&surface)
                })
                .await??
            }
        };
        tokio::select! {
            biased;
            _=cancel.cancelled()=>anyhow::bail!("raster export cancelled"),
            _=tokio::time::sleep_until(deadline)=>anyhow::bail!("raster export timeout"),
            result=file.write_all(&bytes)=>result?,
        }
        let _ = progress.send(50.0 * (index + 1) as f64 / spec.frames as f64);
    }
    tokio::select! {
        biased;
        _ = cancel.cancelled() => anyhow::bail!("raster export cancelled"),
        _ = tokio::time::sleep_until(deadline) => anyhow::bail!("raster export timeout"),
        result = file.flush() => result?,
    }
    drop(file);
    ensure!(!cancel.is_cancelled(), "raster export cancelled");
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .context("raster export timeout")?;
    if let Some((n, d)) = pixel_aspect {
        ensure!(
            n <= i32::MAX as u32 && d <= i32::MAX as u32,
            "pixel aspect exceeds encoder rational range"
        );
    }
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            match encoding {
                RasterEncoding::Mjpeg { .. }
                | RasterEncoding::NativeHevc { .. }
        | RasterEncoding::NativeH264 { .. }
                | RasterEncoding::Gif
                | RasterEncoding::ImageSequence { .. }
                | RasterEncoding::MxfH264 { .. }
                | RasterEncoding::MxfAtom { .. }
                | RasterEncoding::Mxf { .. }
                | RasterEncoding::Apv { .. }
                | RasterEncoding::DnxHr { .. }
                | RasterEncoding::ProRes { .. } => {
                    anyhow::bail!("intra formats require native export path")
                }
                RasterEncoding::Ffv1 => "rgba",
                _ => "yuv420p",
            },
            "-s",
        ])
        .arg(format!("{}x{}", spec.width, spec.height))
        .arg("-r")
        .arg(spec.frame_rate.to_string());
    if !matches!(encoding, RasterEncoding::Ffv1) {
        command.args([
            "-color_range",
            "tv",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
        ]);
    }
    command.arg("-i").arg(&raw);
    let mut inputs = vec![raw.clone()];
    if let Some(audio) = audio {
        command
            .args(["-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
            .arg(audio.path)
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:a",
                match encoding {
                    RasterEncoding::Ffv1 => "pcm_f32le",
                    _ => "aac",
                },
            ]);
        if matches!(
            encoding,
            RasterEncoding::H264 { .. } | RasterEncoding::H264TwoPass { .. }
        ) {
            command.args(["-b:a", "320k"]);
        }
        inputs.push(audio.path.to_path_buf());
    } else {
        command.arg("-an");
    }
    command
        .arg("-metadata:s:v:0")
        .arg(format!("VIDEO_KADR_FRAME_RATE={}", spec.frame_rate));
    match encoding {
        RasterEncoding::Mjpeg { .. }
        | RasterEncoding::NativeHevc { .. }
        | RasterEncoding::NativeH264 { .. }
        | RasterEncoding::Gif
        | RasterEncoding::ImageSequence { .. }
        | RasterEncoding::MxfH264 { .. }
        | RasterEncoding::MxfAtom { .. }
        | RasterEncoding::Mxf { .. }
        | RasterEncoding::Apv { .. }
        | RasterEncoding::DnxHr { .. }
        | RasterEncoding::ProRes { .. } => {
            anyhow::bail!("intra formats require native export path")
        }
        RasterEncoding::Ffv1 => {
            command.args(["-c:v", "ffv1", "-pix_fmt", "bgra", "-f", "matroska"]);
        }
        RasterEncoding::H264 {
            target_kbps,
            max_kbps,
            keyint,
            profile,
            ..
        }
        | RasterEncoding::H264TwoPass {
            target_kbps,
            max_kbps,
            keyint,
            profile,
            ..
        } => {
            command
                .args([
                    "-c:v",
                    "libx264",
                    "-profile:v",
                    profile.ffmpeg_name(),
                    "-pix_fmt",
                    "yuv420p",
                    "-b:v",
                ])
                .arg(format!("{target_kbps}k"))
                .arg("-maxrate")
                .arg(format!("{max_kbps}k"))
                .arg("-bufsize")
                .arg(format!("{}k", u64::from(max_kbps) * 2))
                .arg("-g")
                .arg(keyint.to_string())
                .args([
                    "-color_range",
                    "tv",
                    "-color_primaries",
                    "bt709",
                    "-color_trc",
                    "bt709",
                    "-colorspace",
                    "bt709",
                    "-movflags",
                    "+faststart",
                    "-f",
                    "mp4",
                ]);
        }
    }
    if let Some((n, d)) = pixel_aspect {
        command
            .arg("-vf")
            .arg(format!("setsar={}/{}:max=2147483647", n.max(1), d.max(1)));
    }
    for (name, value) in metadata {
        command.arg("-metadata").arg(format!("{name}={value}"));
    }
    struct PassFiles(std::path::PathBuf);
    impl Drop for PassFiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let mut pass_files = None;
    if encoding.two_pass() {
        let directory = parent.join(format!(".raster-pass-{id}"));
        tokio::fs::create_dir(&directory).await?;
        pass_files = Some(PassFiles(directory.clone()));
        let stats = directory.join("stats");
        let mut first = Command::new(ffmpeg);
        first
            .args(command.as_std().get_args())
            .args(["-pass", "1", "-passlogfile"])
            .arg(&stats)
            .args(["-progress", "pipe:1"])
            .arg(&encoded);
        let (status, diagnostic) = stream_with_progress(
            runtime,
            first,
            runtime.render_policy(inputs.clone(), &encoded),
            |line| {
                line.strip_prefix("frame=")
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(|v| 50. + 25. * v as f64 / spec.frames as f64)
            },
            progress,
            cancel,
            remaining,
        )
        .await?;
        ensure!(
            matches!(status, ProcessStatus::Ok),
            "raster first pass failed or cancelled: {status:?} {diagnostic}"
        );
        ensure!(
            !cancel.is_cancelled(),
            "raster export cancelled between passes"
        );
        command.args(["-pass", "2", "-passlogfile"]).arg(&stats);
    }
    let _pass_files = pass_files;
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .context("raster export timeout before final pass")?;
    command.args(["-progress", "pipe:1"]).arg(&encoded);
    let (status, diagnostic) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(inputs, &encoded),
        |line| {
            line.strip_prefix("frame=")
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(|v| {
                    if encoding.two_pass() {
                        75. + 25. * v as f64 / spec.frames as f64
                    } else {
                        50. + 50. * v as f64 / spec.frames as f64
                    }
                })
        },
        progress,
        cancel,
        remaining,
    )
    .await?;
    ensure!(
        matches!(status, ProcessStatus::Ok),
        "raster encoder failed or cancelled: {status:?} {diagnostic}"
    );
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "raster export cancelled or timed out before publication"
    );
    ensure!(
        tokio::fs::metadata(&encoded).await?.len() > 0,
        "raster encoder produced empty output"
    );
    tokio::fs::hard_link(&encoded, output)
        .await
        .context("publish raster output without replacing existing file")?;
    let _ = progress.send(100.0);
    Ok(())
}

/// Video-only export of the native project/focus over a half-open time range.
/// Audio is deliberately a separate output stage; callers must label this mode.
pub async fn encode_project_video_only(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition: String,
    rendered: std::collections::BTreeMap<String, PathBuf>,
    sources: std::collections::BTreeMap<crate::domain::composition::SourceId, PathBuf>,
    range: std::ops::Range<u64>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
) -> Result<()> {
    encode_project_with_audio(
        runtime,
        ffmpeg,
        output,
        project,
        composition,
        rendered,
        sources,
        range,
        staging_bytes,
        timeout,
        cancel,
        progress,
        None,
    )
    .await
}

/// Native visual export with an optional trusted, already mixed PCM track.
#[allow(clippy::too_many_arguments)]
pub async fn encode_project_with_audio(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition: String,
    rendered: std::collections::BTreeMap<String, PathBuf>,
    sources: std::collections::BTreeMap<crate::domain::composition::SourceId, PathBuf>,
    range: std::ops::Range<u64>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
) -> Result<()> {
    encode_project_with_source_filter(
        runtime,
        ffmpeg,
        output,
        project,
        composition,
        rendered,
        sources,
        range,
        staging_bytes,
        timeout,
        cancel,
        progress,
        audio,
        None,
    )
    .await
}

/// A caller-authorized source filter shared by bounded raster workers.
pub type SharedSourceFilter = std::sync::Arc<
    std::sync::Mutex<
        Box<
            dyn FnMut(&crate::domain::composition::SourceId, u64, &mut GroupSurface) -> Result<()>
                + Send,
        >,
    >,
>;

/// Native export with source filtering before crop, transforms and effects.
/// The caller supplies enabled-source policy, verified masks, ACL and optional mixed audio.
#[allow(clippy::too_many_arguments)]
pub async fn encode_project_with_source_filter(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition: String,
    rendered: std::collections::BTreeMap<String, PathBuf>,
    sources: std::collections::BTreeMap<crate::domain::composition::SourceId, PathBuf>,
    range: std::ops::Range<u64>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    filter: Option<SharedSourceFilter>,
) -> Result<()> {
    let filter = filter.map(|source| -> SharedClipFilter {
        std::sync::Arc::new(std::sync::Mutex::new(Box::new(
            move |_node: &str,
                  _clip: &crate::domain::composition::CompositionClipId,
                  id: &crate::domain::composition::SourceId,
                  time,
                  frame: &mut GroupSurface| {
                let mut filter = source
                    .lock()
                    .map_err(|_| anyhow::anyhow!("source filter stopped"))?;
                filter(id, time, frame)
            },
        )))
    });
    encode_project_with_clip_filter(
        runtime,
        ffmpeg,
        output,
        project,
        composition,
        rendered,
        sources,
        range,
        staging_bytes,
        timeout,
        cancel,
        progress,
        audio,
        filter,
    )
    .await
}

pub type SharedClipFilter = std::sync::Arc<
    std::sync::Mutex<
        Box<
            dyn FnMut(
                    &str,
                    &crate::domain::composition::CompositionClipId,
                    &crate::domain::composition::SourceId,
                    u64,
                    &mut GroupSurface,
                ) -> Result<()>
                + Send,
        >,
    >,
>;

/// Per-clip filtering shared by preview and durable native export.
#[allow(clippy::too_many_arguments)]
pub async fn encode_project_with_clip_filter(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    output: &Path,
    project: crate::domain::nested_composition::NestedCompositionProject,
    composition: String,
    rendered: std::collections::BTreeMap<String, PathBuf>,
    sources: std::collections::BTreeMap<crate::domain::composition::SourceId, PathBuf>,
    range: std::ops::Range<u64>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    filter: Option<SharedClipFilter>,
) -> Result<()> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .context("native export deadline overflow")?;
    let durations = project.durations_us()?;
    let duration = *durations
        .get(&composition)
        .context("native export focus is missing")?;
    ensure!(
        range.start < range.end && range.end <= duration,
        "invalid native export range"
    );
    let canvas = project
        .compositions
        .get(&composition)
        .context("native export focus is missing")?
        .content
        .canvas;
    let rate = canvas.frame_rate();
    let numerator = u128::from(range.end - range.start) * u128::from(rate.numerator);
    let denominator = 1_000_000 * u128::from(rate.denominator);
    ensure!(denominator > 0, "invalid native export frame rate");
    let count = numerator.div_ceil(denominator);
    ensure!(
        count <= u64::MAX as u128,
        "native export frame count overflow"
    );
    let spec = RasterVideoSpec {
        width: canvas.width as usize,
        height: canvas.height as usize,
        frame_rate: rate,
        frames: count as u64,
        staging_bytes,
    };
    let project = Arc::new(project);
    let rendered = Arc::new(rendered);
    let sources = Arc::new(sources);
    let ffmpeg_path = Arc::new(ffmpeg.to_path_buf());
    let flag = Arc::new(AtomicBool::new(cancel.is_cancelled()));
    let watch_flag = flag.clone();
    let watch_cancel = cancel.clone();
    struct CancelBridge {
        flag: Arc<AtomicBool>,
        watch: tokio::task::JoinHandle<()>,
    }
    impl Drop for CancelBridge {
        fn drop(&mut self) {
            self.flag.store(true, Ordering::Relaxed);
            self.watch.abort();
        }
    }
    let _bridge = CancelBridge {
        flag: flag.clone(),
        watch: tokio::spawn(async move {
            watch_cancel.cancelled().await;
            watch_flag.store(true, Ordering::Relaxed);
        }),
    };
    let remaining = deadline
        .checked_duration_since(std::time::Instant::now())
        .context("native export timeout")?;
    encode_frames_with_audio(
        runtime,
        ffmpeg,
        output,
        spec,
        remaining,
        cancel,
        progress,
        audio,
        move |index, local_time| {
            let project = project.clone();
            let rendered = rendered.clone();
            let sources = sources.clone();
            let ffmpeg = ffmpeg_path.clone();
            let composition = composition.clone();
            let flag = flag.clone();
            let filter = filter.clone();
            async move {
                let remaining = deadline
                    .checked_duration_since(std::time::Instant::now())
                    .context("native export timeout")?;
                let time = range
                    .start
                    .checked_add(local_time)
                    .context("native export clock overflow")?;
                let sample = crate::domain::caption_frame_clock::CaptionFrameSample::new(
                    index,
                    filmcraft_time::FrameRate::new(
                        spec.frame_rate.numerator as i64,
                        spec.frame_rate.denominator as i64,
                    ),
                    range.start,
                )
                .map_err(anyhow::Error::msg)?;
                tokio::task::spawn_blocking(move || {
                    let mut guard = filter
                        .as_ref()
                        .map(|filter| {
                            filter
                                .lock()
                                .map_err(|_| anyhow::anyhow!("source filter stopped"))
                        })
                        .transpose()?;
                    let callback = guard.as_mut().map(|filter| {
                        &mut ***filter
                            as &mut dyn FnMut(
                                &str,
                                &crate::domain::composition::CompositionClipId,
                                &crate::domain::composition::SourceId,
                                u64,
                                &mut GroupSurface,
                            ) -> Result<()>
                    });
                    crate::domain::group_media::render_project_frame_sources_with_clip_filter(
                        &project,
                        &composition,
                        &ffmpeg,
                        &rendered,
                        &sources,
                        time,
                        remaining,
                        &flag,
                        Some(sample),
                        callback,
                    )
                })
                .await
                .context("native export raster worker stopped")?
            }
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> RasterVideoSpec {
        RasterVideoSpec {
            width: 2,
            height: 2,
            frame_rate: FrameRateRatio {
                numerator: 2,
                denominator: 1,
            },
            frames: 2,
            staging_bytes: 32,
        }
    }
    #[tokio::test]
    async fn actual_ffv1_sharpen_matches_independent_upstream_oracle() {
        if !std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
        {
            eprintln!("FFmpeg unavailable: sharpen export skipped");
            return;
        }
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-color-sharpen.json"
        ))
        .unwrap();
        let input: Vec<u8> = serde_json::from_value(fixture["input"].clone()).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("sharpen.mkv");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let spec = RasterVideoSpec {
            width: 7,
            height: 5,
            frame_rate: FrameRateRatio {
                numerator: 2,
                denominator: 1,
            },
            frames: cases.len() as u64,
            staging_bytes: (7 * 5 * 4 * cases.len()) as u64,
        };
        encode_frames(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            spec,
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
            |index, _| {
                let mut frame = GroupSurface {
                    width: 7,
                    height: 5,
                    rgba: input.clone(),
                };
                let case = &cases[index as usize];
                let parameters: crate::domain::color_sharpen::ColorSharpen =
                    serde_json::from_value(case["parameters"].clone()).unwrap();
                let result = parameters
                    .apply_scaled(
                        &mut frame,
                        case["pixelScale"].as_f64().unwrap() as f32,
                        &mut || Ok(()),
                    )
                    .map(|_| frame);
                async move { result }
            },
        )
        .await
        .unwrap();
        for (index, case) in cases.iter().enumerate() {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                index as u64 * 500000,
                7,
                5,
            )
            .unwrap();
            let expected: Vec<u8> = serde_json::from_value(case["expected"].clone()).unwrap();
            for (pixel, (actual, expected)) in frame
                .rgba
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(actual[3], expected[3]);
                if actual[3] == 0 {
                    continue;
                }
                for c in 0..3 {
                    assert!(
                        actual[c].abs_diff(expected[c]) <= 1,
                        "export case {index} pixel {pixel} channel {c}"
                    );
                }
            }
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    #[test]
    fn rational_clock_and_admission() {
        let mut value = spec();
        value.frames = 100;
        value.staging_bytes = 1600;
        value.frame_rate = FrameRateRatio {
            numerator: 30000,
            denominator: 1001,
        };
        assert_eq!(value.time_us(30).unwrap(), 1001000);
        assert!(value.validate().is_ok());
        value.staging_bytes = 1599;
        assert!(value.validate().is_err());
        value.frame_rate.numerator = 0;
        assert!(value.time_us(0).is_err());
    }
    #[tokio::test]
    async fn actual_ffv1_preserves_frames_alpha_and_cleans_staging() {
        if !std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
        {
            eprintln!("FFmpeg unavailable: raster encoder fixture skipped");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("movie.mkv");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        encode_frames(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            spec(),
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
            |i, time| async move {
                assert_eq!(time, i * 500000);
                Ok(GroupSurface {
                    width: 2,
                    height: 2,
                    rgba: if i == 0 {
                        [255, 0, 0, 255].repeat(4)
                    } else {
                        [0, 0, 255, 128].repeat(4)
                    },
                })
            },
        )
        .await
        .unwrap();
        for (time, pixel) in [(0, [255, 0, 0, 255]), (500000, [0, 0, 255, 128])] {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                time,
                2,
                2,
            )
            .unwrap();
            assert_eq!(frame.rgba, pixel.repeat(4));
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        let saved = std::fs::read(&output).unwrap();
        assert!(encode_frames(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            spec(),
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
            |_, _| async { anyhow::bail!("must not render") }
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&output).unwrap(), saved);
    }
    #[tokio::test]
    async fn invalid_frame_and_cancel_leave_no_output_or_staging() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("movie.mkv");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        assert!(encode_frames(
            &ProcessRuntime::local_default(),
            Path::new("unused"),
            &output,
            spec(),
            Duration::from_secs(1),
            &CancellationToken::new(),
            &progress,
            |_, _| async {
                Ok(GroupSurface {
                    width: 1,
                    height: 1,
                    rgba: vec![0; 4],
                })
            }
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        assert!(encode_frames(
            &ProcessRuntime::local_default(),
            Path::new("unused"),
            &output,
            spec(),
            Duration::from_secs(1),
            &cancel,
            &progress,
            move |_, _| {
                trigger.cancel();
                async {
                    Ok(GroupSurface {
                        width: 2,
                        height: 2,
                        rgba: vec![0; 16],
                    })
                }
            }
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
    #[tokio::test]
    async fn nested_project_export_uses_native_tone_curves_and_group_opacity() {
        use crate::domain::composition::{CanvasSpec, Composition, TransformSpec};
        use crate::domain::nested_composition::NestedCompositionProject;
        use std::io::Write;
        if !std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
        {
            eprintln!("FFmpeg unavailable: nested export fixture skipped");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.nut");
        let mut process = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2", "-r",
                "2", "-i", "pipe:0", "-c:v", "rawvideo", "-f", "nut",
            ])
            .arg(&source)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = process.stdin.take().unwrap();
        input.write_all(&[0, 0, 255, 255].repeat(4)).unwrap();
        input.write_all(&[0, 0, 255, 128].repeat(4)).unwrap();
        drop(input);
        assert!(process.wait().unwrap().success());
        let mut canvas = CanvasSpec::default();
        canvas.width = 2;
        canvas.height = 2;
        canvas.fps_milli = 2000;
        canvas.background.red = 1.0;
        let mut child = Composition::new(canvas);
        child.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":2,"height":2,"hasAudio":false}})).unwrap();
        child.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"tone_curves","curves":{"blue":[[0.0,0.0],[1.0,0.5]],"hueVsHue":[[0.0,0.8333333333333333]]}}]}]}])).unwrap();
        canvas.background.red = 0.0;
        let root = Composition::new(canvas);
        let project:NestedCompositionProject=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{
            "root":{"durationUs":1000000,"content":root,"instances":[{"id":"group","timing":{"composition":"child","timelineStartUs":0,"sourceInUs":0,"sourceOutUs":1000000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0.5},"blendMode":"normal"}]},
            "child":{"durationUs":1000000,"content":child,"instances":[]}
        }})).unwrap();
        let sources = std::collections::BTreeMap::from([(
            crate::domain::composition::SourceId::parse("source").unwrap(),
            source,
        )]);
        let output = directory.path().join("export.mkv");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        encode_project_video_only(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            project.clone(),
            "root".into(),
            Default::default(),
            sources.clone(),
            0..1000000,
            32,
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
        for (time, pixel) in [(0, [64, 0, 0, 255]), (500000, [96, 0, 0, 255])] {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                time,
                2,
                2,
            )
            .unwrap();
            assert_eq!(frame.rgba, pixel.repeat(4));
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
        let mut graded = project;
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut graded.compositions.get_mut("child").unwrap().content.tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::CreativeGrade {
            parameters: crate::domain::creative_grade::CreativeGrade {
                shadows: crate::domain::creative_grade::ColorWheel {
                    x: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            },
            animation: None,
        }];
        let output = directory.path().join("wheels.mkv");
        encode_project_video_only(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            graded.clone(),
            "root".into(),
            Default::default(),
            sources.clone(),
            0..1000000,
            32,
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
        for (time, pixel) in [(0, [33, 0, 111, 255]), (500000, [80, 0, 56, 255])] {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                time,
                2,
                2,
            )
            .unwrap();
            assert_eq!(frame.rgba, pixel.repeat(4));
        }

        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut graded.compositions.get_mut("child").unwrap().content.tracks[0]
        else {
            panic!("video expected")
        };
        let crate::domain::composition::VideoEffect::CreativeGrade { animation, .. } =
            &mut clips[0].effects[0]
        else {
            panic!("grade expected")
        };
        *animation=Some(serde_json::from_value(serde_json::json!({"shadowsX":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":1.0}]}}})).unwrap());
        let split = graded
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        let mut lut_project = graded.clone();
        for (name, project) in [("animated", graded), ("split", split)] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 0, 128, 255]), (500000, [80, 0, 56, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }

        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut lut_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        let table = crate::domain::lut_tables::Lut {
            title: "half swap".into(),
            shaper: Some(crate::domain::lut_tables::Lut1d {
                domain_min: [0.0; 3],
                domain_max: [1.0; 3],
                data: vec![[0.0; 3], [0.5; 3]],
            }),
            cube: Some(crate::domain::lut_tables::Lut3d::from_fn(2, |v| {
                [v[2], v[1], v[0]]
            })),
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::ColorLut {
            lut: crate::domain::color_lut::ColorLut {
                format: crate::domain::color_lut::TableFormat::Cube,
                text: table.to_cube(),
                intensity: 100.0,
            },
        }];
        let mut hsl_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut hsl_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::HslSecondary {
            parameters: crate::domain::hsl_secondary::HslSecondary { hue: 240.0, ..Default::default() },
            animation: Some(serde_json::from_value(serde_json::json!({"hueShift":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":120.0}]}}})).unwrap()),
        }];
        let hsl_split = hsl_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [("hsl", hsl_project.clone()), ("hsl-split", hsl_split)] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 0, 128, 255]), (500000, [128, 0, 0, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut hsl_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::HslSecondary {
            parameters: crate::domain::hsl_secondary::HslSecondary {
                hue: 240.0,
                show_mask: 3,
                denoise: 100.0,
                blur: 100.0,
                ..Default::default()
            },
            animation: None,
        }];
        let output = directory.path().join("hsl-mask.mkv");
        encode_project_video_only(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            hsl_project,
            "root".into(),
            Default::default(),
            sources.clone(),
            0..1000000,
            32,
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
        for (time, pixel) in [(0, [128, 128, 128, 255]), (500000, [128, 64, 64, 255])] {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                time,
                2,
                2,
            )
            .unwrap();
            assert_eq!(frame.rgba, pixel.repeat(4), "HSL mask at {time}");
        }
        let mut limiter_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut limiter_project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![
            crate::domain::composition::VideoEffect::ColorLut {
                lut: crate::domain::color_lut::ColorLut {
                    format: crate::domain::color_lut::TableFormat::Cube,
                    text: crate::domain::lut_tables::Lut {
                        title: "white limiter fixture".into(),
                        shaper: None,
                        cube: Some(crate::domain::lut_tables::Lut3d::from_fn(2, |_| [1.0; 3])),
                    }
                    .to_cube(),
                    intensity: 100.0,
                },
            },
            crate::domain::composition::VideoEffect::VideoLimiter {
                parameters: crate::domain::video_limiter::VideoLimiter {
                    gamut_warning: true,
                    warning_color: [0.0, 1.0, 0.0, 1.0],
                    ..Default::default()
                },
                animation: None,
            },
        ];
        let limiter_split = limiter_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        let mut animated_limiter = limiter_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut animated_limiter
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        let crate::domain::composition::VideoEffect::VideoLimiter { animation, .. } =
            &mut clips[0].effects[1]
        else {
            panic!("limiter expected")
        };
        *animation=Some(serde_json::from_value(serde_json::json!({"green":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.0},{"tick":500000,"value":0.0}]}},"blue":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":1.0}]}}})).unwrap());
        let animated_split = animated_limiter
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [
            ("limiter", limiter_project),
            ("limiter-split", limiter_split),
            ("limiter-animated", animated_limiter),
            ("limiter-animated-split", animated_split),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 128, 0, 255]), (500000, [64, 64, 0, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                let pixel = if time == 500000 && name.contains("animated") {
                    [64, 0, 64, 255]
                } else {
                    pixel
                };
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut cineon_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut cineon_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![
            crate::domain::composition::VideoEffect::ColorLut {lut:crate::domain::color_lut::ColorLut {format:crate::domain::color_lut::TableFormat::Cube,text:"LUT_3D_SIZE 2\n".to_owned()+&"0.5370987 0.735357 0.880825\n".repeat(8),intensity:100.0}},
            crate::domain::composition::VideoEffect::CineonConverter {parameters:crate::domain::cineon_converter::CineonConverter {conversion:1,..Default::default()},animation:Some(serde_json::from_value(serde_json::json!({"gamma":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.7},{"tick":500000,"value":2.55}]}}})).unwrap())},
        ];
        let cineon_split = cineon_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project, range) in [
            ("cineon", cineon_project.clone(), 0..1000000),
            ("cineon-split", cineon_split.clone(), 0..1000000),
            ("cineon-selected", cineon_project, 500000..1000000),
            ("cineon-split-selected", cineon_split, 500000..1000000),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            let selected = range.start != 0;
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                range,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            // Independent source oracle gives RGB137/154/164 at gamma1.7 and
            // RGB121/146/160 at gamma2.55; child red background and half group
            // opacity are composed after conversion, with source alpha255/128.
            let expected = if selected {
                vec![(0, [94, 37, 40, 255])]
            } else {
                vec![(0, [69, 77, 82, 255]), (500000, [94, 37, 40, 255])]
            };
            for (time, pixel) in expected {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut gamma_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut gamma_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![
            crate::domain::composition::VideoEffect::ColorLut {lut:crate::domain::color_lut::ColorLut {format:crate::domain::color_lut::TableFormat::Cube,text:"LUT_3D_SIZE 2\n".to_owned()+&"0.5370987 0.735357 0.880825\n".repeat(8),intensity:100.0}},
            crate::domain::composition::VideoEffect::GammaCorrection {parameters:crate::domain::gamma_correction::GammaCorrection::default(),animation:Some(serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":10.0},{"tick":500000,"value":14.5}]}})).unwrap())},
        ];
        let gamma_split = gamma_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project, range) in [
            ("gamma", gamma_project.clone(), 0..1000000),
            ("gamma-split", gamma_split.clone(), 0..1000000),
            ("gamma-selected", gamma_project, 500000..1000000),
            ("gamma-split-selected", gamma_split, 500000..1000000),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            let selected = range.start != 0;
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                range,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            // Source oracle gamma10 produces RGB137/188/225; gamma14.5 gives
            // RGB104/163/212. Compose actual alpha and group opacity afterward.
            let expected = if selected {
                vec![(0, [90, 41, 53, 255])]
            } else {
                vec![(0, [69, 94, 113, 255]), (500000, [90, 41, 53, 255])]
            };
            for (time, pixel) in expected {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut procamp_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut procamp_project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![
            crate::domain::composition::VideoEffect::ColorLut {lut:crate::domain::color_lut::ColorLut {format:crate::domain::color_lut::TableFormat::Cube,text:"LUT_3D_SIZE 2\n".to_owned()+&"0.5370987 0.735357 0.880825\n".repeat(8),intensity:100.0}},
            crate::domain::composition::VideoEffect::ProcAmp {parameters:crate::domain::proc_amp::ProcAmp::default(),animation:Some(serde_json::from_value(serde_json::json!({"brightness":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":100.0}]}}})).unwrap())},
        ];
        let procamp_split = procamp_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project, range) in [
            ("procamp", procamp_project.clone(), 0..1000000),
            ("procamp-split", procamp_split.clone(), 0..1000000),
            ("procamp-selected", procamp_project, 500000..1000000),
            ("procamp-split-selected", procamp_split, 500000..1000000),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            let selected = range.start != 0;
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                range,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            // Source ProcAmp brightness0 gives RGB137/188/225; brightness100
            // gives RGB239/255/255, before child alpha and group opacity.
            let expected = if selected {
                vec![(0, [124, 64, 64, 255])]
            } else {
                vec![(0, [69, 94, 113, 255]), (500000, [124, 64, 64, 255])]
            };
            for (time, pixel) in expected {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut vignette_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut vignette_project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::ColorVignette {
            parameters: crate::domain::color_vignette::ColorVignette::default(),
            animation: Some(serde_json::from_value(serde_json::json!({"amount":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":-5.0}]}}})).unwrap()),
        }];
        let vignette_split = vignette_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [
            ("vignette", vignette_project),
            ("vignette-split", vignette_split),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            let first = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                0,
                2,
                2,
            )
            .unwrap();
            assert_eq!(
                first.rgba,
                [0, 0, 128, 255].repeat(4),
                "{name} neutral frame"
            );
            let second = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                500000,
                2,
                2,
            )
            .unwrap();
            assert_eq!(&second.rgba[..4], &[64, 0, 0, 255], "{name} dark corner");
            assert_eq!(
                &second.rgba[12..],
                &[64, 0, 64, 255],
                "{name} preserved center"
            );
        }
        let mut basic_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut basic_project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![crate::domain::composition::VideoEffect::BasicGrade {parameters:crate::domain::basic_grade::BasicGrade::default(),animation:Some(serde_json::from_value(serde_json::json!({"saturation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100.0},{"tick":500000,"value":0.0}]}}})).unwrap())}];
        let basic_split = basic_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [("basic", basic_project), ("basic-split", basic_split)] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 0, 128, 255]), (500000, [68, 5, 5, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut creative_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut creative_project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![crate::domain::composition::VideoEffect::CreativeAdjust {parameters:crate::domain::creative_adjust::CreativeAdjust::default(),animation:Some(serde_json::from_value(serde_json::json!({"fadedFilm":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":100.0}]}}})).unwrap())}];
        let creative_split = creative_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [
            ("creative-adjust", creative_project),
            ("creative-adjust-split", creative_split),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 0, 128, 255]), (500000, [72, 8, 56, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut lumetri_project = lut_project.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut lumetri_project
                .compositions
                .get_mut("child")
                .unwrap()
                .content
                .tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects=vec![crate::domain::composition::VideoEffect::Lumetri {parameters:crate::domain::lumetri::Lumetri {animation:Some(serde_json::from_value(serde_json::json!({"creative":{"fadedFilm":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":100.0}]}}}})).unwrap()),..Default::default()}}];
        let lumetri_split = lumetri_project
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, project) in [
            ("lumetri", lumetri_project),
            ("lumetri-split", lumetri_split),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [(0, [0, 0, 128, 255]), (500000, [72, 8, 56, 255])] {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let mut shared = lut_project.clone();
        shared.luts = vec![crate::domain::project_luts::ProjectLut {
            id: "shared".into(),
            name: "Green".into(),
            source_path: None,
            lut: crate::domain::color_lut::ColorLut {
                format: crate::domain::color_lut::TableFormat::Cube,
                text: "LUT_1D_SIZE 2\n0 1 0\n0 1 0\n".into(),
                intensity: 100.0,
            },
        }];
        let crate::domain::composition::CompositionTrack::Video { clips, .. } =
            &mut shared.compositions.get_mut("child").unwrap().content.tracks[0]
        else {
            panic!("video expected")
        };
        clips[0].effects = vec![crate::domain::composition::VideoEffect::Lumetri {
            parameters: crate::domain::lumetri::Lumetri {
                look_lut_library: Some("shared".into()),
                ..Default::default()
            },
        }];
        let mut input_shared = shared.clone();
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut input_shared
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!("video expected")
        };
        let crate::domain::composition::VideoEffect::Lumetri { parameters } =
            &mut clips[0].effects[0]
        else {
            panic!("Lumetri expected")
        };
        parameters.look_lut_library = None;
        parameters.input_lut_library = Some("shared".into());
        let mut updated = shared.clone();
        crate::domain::project_luts::update(
            &mut updated.luts,
            "shared",
            None,
            Some(crate::domain::color_lut::ColorLut {
                format: crate::domain::color_lut::TableFormat::Cube,
                text: "LUT_1D_SIZE 2\n1 1 1\n1 1 1\n".into(),
                intensity: 100.0,
            }),
            None,
        )
        .unwrap();
        let mut removed = shared.clone();
        crate::domain::project_luts::remove(&mut removed.luts, "shared").unwrap();
        let split = shared
            .split_instance("root", "group", "right", 250000)
            .unwrap();
        for (name, document, pixels) in [
            ("shared-look", shared, [[0, 128, 0, 255], [64, 64, 0, 255]]),
            (
                "shared-input",
                input_shared,
                [[0, 128, 0, 255], [64, 64, 0, 255]],
            ),
            ("shared-split", split, [[0, 128, 0, 255], [64, 64, 0, 255]]),
            (
                "shared-updated",
                updated,
                [[128, 128, 128, 255], [128, 64, 64, 255]],
            ),
            (
                "shared-removed",
                removed,
                [[0, 0, 128, 255], [64, 0, 64, 255]],
            ),
        ] {
            let output = directory.path().join(format!("{name}.mkv"));
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                document,
                "root".into(),
                Default::default(),
                sources.clone(),
                0..1000000,
                32,
                Duration::from_secs(10),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for (time, pixel) in [0, 500000].into_iter().zip(pixels) {
                let frame = crate::domain::group_media::decode_rendered_frame(
                    Path::new("ffmpeg"),
                    &output,
                    time,
                    2,
                    2,
                )
                .unwrap();
                assert_eq!(frame.rgba, pixel.repeat(4), "{name} at {time}");
            }
        }
        let output = directory.path().join("lut.mkv");
        encode_project_video_only(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            lut_project,
            "root".into(),
            Default::default(),
            sources,
            0..1000000,
            32,
            Duration::from_secs(10),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
        for (time, pixel) in [(0, [64, 0, 0, 255]), (500000, [96, 0, 0, 255])] {
            let frame = crate::domain::group_media::decode_rendered_frame(
                Path::new("ffmpeg"),
                &output,
                time,
                2,
                2,
            )
            .unwrap();
            assert_eq!(frame.rgba, pixel.repeat(4), "LUT at {time}");
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn encoder_timeout_and_midflight_cancel_clean_owned_artifacts() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let encoder = directory.path().join("encoder");
        std::fs::write(&encoder, "#!/bin/sh\nexec sleep 10\n").unwrap();
        std::fs::set_permissions(&encoder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = directory.path().join("movie.mkv");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let frame = |_, _| async {
            Ok(GroupSurface {
                width: 2,
                height: 2,
                rgba: vec![0; 16],
            })
        };
        assert!(encode_frames(
            &ProcessRuntime::local_default(),
            &encoder,
            &output,
            spec(),
            Duration::from_millis(50),
            &CancellationToken::new(),
            &progress,
            frame
        )
        .await
        .is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            trigger.cancel();
        });
        assert!(encode_frames(
            &ProcessRuntime::local_default(),
            &encoder,
            &output,
            spec(),
            Duration::from_secs(2),
            &cancel,
            &progress,
            frame
        )
        .await
        .is_err());
        task.await.unwrap();
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[cfg(test)]
mod audio_tests {
    use super::*;
    use std::process::Command;
    fn spec() -> RasterVideoSpec {
        RasterVideoSpec {
            width: 2,
            height: 2,
            frame_rate: FrameRateRatio {
                numerator: 2,
                denominator: 1,
            },
            frames: 2,
            staging_bytes: 32,
        }
    }
    #[test]
    fn rational_video_duration_admits_exact_audio_sample_coverage() {
        let mut spec = spec();
        spec.frame_rate = FrameRateRatio {
            numerator: 30000,
            denominator: 1001,
        };
        spec.frames = 31;
        spec.staging_bytes = 31 * 16;
        assert_eq!(spec.audio_frames_48k().unwrap(), 49650);
    }
    #[tokio::test]
    async fn real_lossless_video_audio_mux_preserves_all_rgba_and_pcm_bytes() {
        if !Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
        {
            eprintln!("FFmpeg unavailable: native PCM mux fixture skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let pcm = dir.path().join("trusted.f32le");
        let output = dir.path().join("combined.mkv");
        let mut samples = Vec::new();
        for i in 0..48000 {
            samples.extend(((i % 100) as f32 / 200.0 - 0.25).to_le_bytes());
            samples.extend(2.0f32.to_le_bytes());
        }
        std::fs::write(&pcm, &samples).unwrap();
        let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        encode_frames_with_audio(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &output,
            spec(),
            Duration::from_secs(5),
            &CancellationToken::new(),
            &progress,
            Some(RasterAudio {
                path: &pcm,
                frames: 48000,
            }),
            |index, _| async move {
                let pixel = if index == 0 {
                    [0, 0, 255, 128]
                } else {
                    [255, 0, 0, 255]
                };
                Ok(GroupSurface {
                    width: 2,
                    height: 2,
                    rgba: pixel.repeat(4),
                })
            },
        )
        .await
        .unwrap();
        let audio = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-map",
                "0:a:0",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            audio.status.success(),
            "{}",
            String::from_utf8_lossy(&audio.stderr)
        );
        assert_eq!(audio.stdout, samples);
        let video = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-map", "0:v:0", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(video.status.success());
        assert_eq!(
            video.stdout,
            [[0, 0, 255, 128].repeat(4), [255, 0, 0, 255].repeat(4)].concat()
        );
        assert_eq!(std::fs::read(&pcm).unwrap(), samples);
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|v| v
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".raster-")));
    }
    #[tokio::test]
    async fn pcm_size_and_duration_mismatch_fail_before_raster_without_removing_caller_pcm() {
        let dir = tempfile::tempdir().unwrap();
        let pcm = dir.path().join("bad.f32le");
        let output = dir.path().join("output.mkv");
        std::fs::write(&pcm, b"bad").unwrap();
        let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        for frames in [47999, 48000] {
            let result = encode_frames_with_audio(
                &ProcessRuntime::local_default(),
                Path::new("missing-ffmpeg"),
                &output,
                spec(),
                Duration::from_secs(5),
                &CancellationToken::new(),
                &progress,
                Some(RasterAudio { path: &pcm, frames }),
                |_, _| async { panic!("invalid PCM refused before raster") },
            )
            .await;
            assert!(result.is_err());
        }
        assert_eq!(std::fs::read(&pcm).unwrap(), b"bad");
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[tokio::test]
    async fn caption_fractional_start_survives_actual_ffv1_export() {
        assert!(
            std::process::Command::new("ffmpeg")
                .arg("-version")
                .output()
                .is_ok_and(|v| v.status.success()),
            "FFmpeg required for caption export regression"
        );
        for nested in [false, true] {
            let project =
                crate::domain::caption_render_integration_tests::fractional_caption_project(
                    nested, 1234567,
                );
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("exact-caption.mkv");
            let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
            encode_project_video_only(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &output,
                project,
                "root".into(),
                std::collections::BTreeMap::new(),
                std::collections::BTreeMap::new(),
                1234567..1934567,
                20 * 320 * 180 * 4,
                Duration::from_secs(30),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            for index in [12u64, 13, 14] {
                // Select by decoded frame index; timestamp seeking would introduce another clock.
                let decoded = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&output)
                    .args([
                        "-vf",
                        &format!("select='eq(n,{index})'"),
                        "-frames:v",
                        "1",
                        "-an",
                        "-pix_fmt",
                        "rgba",
                        "-f",
                        "rawvideo",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    decoded.status.success(),
                    "{}",
                    String::from_utf8_lossy(&decoded.stderr)
                );
                assert_eq!(decoded.stdout.len(), 320 * 180 * 4);
                let visible = decoded
                    .stdout
                    .chunks_exact(4)
                    .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
                assert_eq!(visible, index == 13, "FFV1 nested{nested} frame{index}");
            }
        }
    }
}
