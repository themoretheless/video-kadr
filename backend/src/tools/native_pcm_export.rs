//! Guarded, bounded file writer for Filmcraft's built-in audio-only presets.
use super::native_intra_export::{GuardedFile, Staging};
use crate::{
    domain::filmcraft_pcm_export::{self, PcmFormat},
    runtime::cpu_pool::CpuPool,
};
use anyhow::{ensure, Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug)]
pub enum PcmPreset {
    Wav16,
    Wav24,
    Aiff16,
}
impl PcmPreset {
    pub fn bits(self) -> u16 {
        if matches!(self, Self::Wav24) {
            24
        } else {
            16
        }
    }
    fn format(self) -> PcmFormat {
        if matches!(self, Self::Aiff16) {
            PcmFormat::Aiff
        } else {
            PcmFormat::Wav
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PcmLayout {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
}

/// Trusted file API; authorization and source snapshots belong to the caller.
pub async fn encode(
    raw: &Path,
    frames: u64,
    preset: PcmPreset,
    output: &Path,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    pool: CpuPool,
) -> Result<()> {
    encode_layout(
        raw,
        frames,
        preset,
        PcmLayout {
            sample_rate: 48000,
            channels: 2,
            bits: preset.bits(),
        },
        output,
        timeout,
        parent_cancel,
        progress,
        pool,
    )
    .await
}
/// Trusted resolved-layout file API. Existing callers retain stereo/48 kHz.
pub async fn encode_layout(
    raw: &Path,
    frames: u64,
    preset: PcmPreset,
    layout: PcmLayout,
    output: &Path,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    pool: CpuPool,
) -> Result<()> {
    let header = filmcraft_pcm_export::header(
        frames,
        layout.sample_rate,
        layout.channels,
        layout.bits,
        preset.format(),
    )?;
    let frame_bytes = usize::from(layout.channels) * 4;
    let pcm_frame_bytes = usize::from(layout.channels) * usize::from(layout.bits / 8);
    ensure!(
        !timeout.is_zero() && !parent_cancel.is_cancelled(),
        "audio export cancelled or zero timeout"
    );
    let expected = frames
        .checked_mul(frame_bytes as u64)
        .context("audio PCM byte size overflow")?;
    ensure!(
        std::fs::metadata(raw)?.len() == expected,
        "audio PCM byte count mismatch"
    );
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("audio output parent missing")?;
    ensure!(!output.try_exists()?, "audio output already exists");
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("audio deadline overflow")?;
    let cancel = parent_cancel.child_token();
    let _guard = cancel.clone().drop_guard();
    let path = parent.join(format!(".native-pcm-{}", uuid::Uuid::new_v4()));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    let _staging = Staging(path.clone(), cancel.clone());
    let mut destination = GuardedFile {
        file,
        cancel: cancel.clone(),
        deadline: deadline.into_std(),
    };
    let mut source = GuardedFile {
        file: File::open(raw)?,
        cancel: cancel.clone(),
        deadline: deadline.into_std(),
    };
    let signal = progress.clone();
    let work = pool.execute(cancel.clone(), move |_| {
        destination.write_all(&header)?;
        let mut done = 0;
        let mut raw = vec![0u8; 4096 * frame_bytes];
        let mut pcm = Vec::with_capacity(4096 * pcm_frame_bytes);
        while done < frames {
            let count = (frames - done).min(4096) as usize;
            source.read_exact(&mut raw[..count * frame_bytes])?;
            pcm.clear();
            for bytes in raw[..count * frame_bytes].chunks_exact(4) {
                let value = f32::from_le_bytes(bytes.try_into()?);
                let quantized = filmcraft_pcm_export::quantise(value, layout.bits);
                if matches!(preset, PcmPreset::Aiff16) {
                    pcm.extend_from_slice(&quantized.to_be_bytes()[4 - layout.bits as usize / 8..]);
                } else {
                    pcm.extend_from_slice(&quantized.to_le_bytes()[..layout.bits as usize / 8]);
                }
            }
            destination.write_all(&pcm)?;
            done += count as u64;
            let _ = signal.send(95. * done as f64 / frames as f64);
        }
        if matches!(preset, PcmPreset::Aiff16) && (frames * pcm_frame_bytes as u64) & 1 == 1 {
            destination.write_all(&[0])?;
        }
        destination.check()?;
        destination.file.sync_all()?;
        Ok(())
    });
    tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("audio export cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("audio export timeout"),result=work=>result.map_err(|e|anyhow::anyhow!("audio PCM worker: {e:?}"))?,}
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "audio export cancelled/timeout before publication"
    );
    tokio::fs::hard_link(&path, output)
        .await
        .context("publish audio without overwrite")?;
    let _ = progress.send(100.);
    Ok(())
}
