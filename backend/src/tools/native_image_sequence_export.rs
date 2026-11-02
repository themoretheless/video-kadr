//! Filmcraft image-sequence semantics, delivered as an owner-scoped ZIP.
//! MIT source: export/lib.rs::encode_still and pcm.rs::image_sequence_path,
//! Filmcraft commit 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::{
    native_intra_export::{GuardedFile, Staging},
    native_rgba_export::RasterVideoSpec,
};
use crate::{domain::group_surface::GroupSurface, runtime::cpu_pool::CpuPool};
use anyhow::{ensure, Context, Result};
use std::{
    fs::OpenOptions,
    future::Future,
    io::{Cursor, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

pub(super) fn frame_name(format: u8, index: u64, count: u64) -> Result<String> {
    let extension = match format {
        0 => "png",
        1 => "tif",
        2 => "bmp",
        _ => anyhow::bail!("invalid image sequence format"),
    };
    let digits = count.saturating_sub(1).max(1).to_string().len().max(3);
    Ok(format!("export{index:0digits$}.{extension}"))
}
struct TrackedFile {
    file: GuardedFile,
    error: Option<String>,
}
#[derive(Clone)]
struct GifSink(Arc<Mutex<TrackedFile>>);
impl Write for GifSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("GIF sink poisoned"))?;
        let result = state.file.write(bytes);
        if let Err(error) = &result {
            state.error = Some(error.to_string());
        }
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| std::io::Error::other("GIF sink poisoned"))?;
        let result = state.file.flush();
        if let Err(error) = &result {
            state.error = Some(error.to_string());
        }
        result
    }
}
enum Output {
    Sequence(zip::ZipWriter<GuardedFile>),
    Gif {
        encoder: image::codecs::gif::GifEncoder<GifSink>,
        sink: GifSink,
    },
}
struct Writer {
    output: Output,
    spec: RasterVideoSpec,
    format: u8,
}
impl Writer {
    fn frame(mut self, index: u64, surface: GroupSurface) -> Result<Self> {
        surface.validate()?;
        ensure!(
            (surface.width, surface.height) == (self.spec.width, self.spec.height),
            "image sequence dimensions changed"
        );
        if let Output::Gif { encoder, .. } = &mut self.output {
            let image = image::RgbaImage::from_raw(
                surface.width as u32,
                surface.height as u32,
                surface.rgba,
            )
            .context("GIF frame size")?;
            let ms = crate::domain::filmcraft_export_clock::gif_delay_ms(self.spec.frame_rate)?;
            let delay = image::Delay::from_numer_denom_ms(ms, 1);
            encoder.encode_frame(image::Frame::from_parts(image, 0, 0, delay))?;
            return Ok(self);
        }
        let mut out = Cursor::new(Vec::new());
        let (w, h) = (surface.width as u32, surface.height as u32);
        match self.format {
            0 => image::ImageEncoder::write_image(
                image::codecs::png::PngEncoder::new(&mut out),
                &surface.rgba,
                w,
                h,
                image::ExtendedColorType::Rgba8,
            )?,
            1 | 2 => {
                let rgba = image::RgbaImage::from_raw(w, h, surface.rgba)
                    .context("image sequence frame size")?;
                let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
                rgb.write_to(
                    &mut out,
                    if self.format == 1 {
                        image::ImageFormat::Tiff
                    } else {
                        image::ImageFormat::Bmp
                    },
                )?;
            }
            _ => anyhow::bail!("invalid image sequence format"),
        }
        let zip = match &mut self.output {
            Output::Sequence(zip) => zip,
            _ => anyhow::bail!("invalid still sequence writer"),
        };
        zip.start_file(
            frame_name(self.format, index, self.spec.frames)?,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .large_file(true),
        )?;
        zip.write_all(&out.into_inner())?;
        Ok(self)
    }
    fn finish(self) -> Result<()> {
        let file = match self.output {
            Output::Sequence(zip) => zip.finish()?,
            Output::Gif { encoder, sink } => {
                // image's GIF encoder writes its trailer on Drop. Capture even
                // those I/O errors before allowing durable publication.
                drop(encoder);
                let state = Arc::try_unwrap(sink.0)
                    .map_err(|_| anyhow::anyhow!("GIF writer still borrowed"))?
                    .into_inner()
                    .map_err(|_| anyhow::anyhow!("GIF sink poisoned"))?;
                ensure!(
                    state.error.is_none(),
                    "GIF finalization failed: {:?}",
                    state.error
                );
                state.file
            }
        };
        file.check()?;
        file.file.sync_all()?;
        Ok(())
    }
}

pub(super) async fn encode<F, Fut>(
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    format: u8,
    pool: Option<CpuPool>,
    mut frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    if format != 3 {
        frame_name(format, 0, spec.frames)?;
    } else {
        u32::try_from(u64::from(spec.frame_rate.denominator) * 1000)
            .context("GIF delay overflow")?;
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("image sequence output parent missing")?;
    ensure!(
        !output.try_exists()?,
        "image sequence output already exists"
    );
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("image sequence deadline overflow")?;
    let cancel = parent_cancel.child_token();
    let _guard = cancel.clone().drop_guard();
    let path = parent.join(format!(".native-images-{}.zip", uuid::Uuid::new_v4()));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .read(true)
        .open(&path)?;
    let _staging = Staging(path.clone(), cancel.clone());
    let file = GuardedFile {
        file,
        cancel: cancel.clone(),
        deadline: deadline.into_std(),
    };
    let encoded_output = if format == 3 {
        let sink = GifSink(Arc::new(Mutex::new(TrackedFile { file, error: None })));
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(sink.clone(), 10);
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite)?;
        Output::Gif { encoder, sink }
    } else {
        Output::Sequence(zip::ZipWriter::new(file))
    };
    let mut writer = Writer {
        output: encoded_output,
        spec,
        format,
    };
    for index in 0..spec.frames {
        let surface = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("image sequence cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("image sequence timeout"),result=frame(index,spec.time_us(index)?)=>result?,};
        writer = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("image sequence cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("image sequence timeout"),result=async {
            if let Some(pool)=&pool {pool.execute(cancel.clone(),move |_|writer.frame(index,surface)).await.map_err(|e|anyhow::anyhow!("image sequence CPU: {e:?}"))}
            else {tokio::task::spawn_blocking(move ||writer.frame(index,surface)).await?}
        }=>result?,};
        let _ = progress.send(95. * (index + 1) as f64 / spec.frames as f64);
    }
    tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("image sequence cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("image sequence timeout"),result=async{if let Some(pool)=&pool{pool.execute(cancel.clone(),move |_|writer.finish()).await.map_err(|error|anyhow::anyhow!("image sequence finish: {error}"))}else{tokio::task::spawn_blocking(move ||writer.finish()).await?}}=>result?,}
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "image sequence cancelled/timeout before publication"
    );
    tokio::fs::hard_link(&path, output)
        .await
        .context("publish image sequence without overwrite")?;
    let _ = progress.send(100.);
    Ok(())
}
