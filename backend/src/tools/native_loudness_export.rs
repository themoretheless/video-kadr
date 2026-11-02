//! Original Filmcraft export/audio_out.rs measurement, gain and compensated limiter.
//! MIT reference: commit 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::native_intra_export::{GuardedFile, Staging};
use crate::runtime::cpu_pool::CpuPool;
use anyhow::{ensure, Context, Result};
use filmcraft_audio_dsp::{effects::Limiter, normalize_gain_db, AudioEffect, LoudnessMeter};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LoudnessSettings {
    pub enabled: bool,
    pub target_lufs: f64,
    pub true_peak_dbtp: f64,
}
impl Default for LoudnessSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lufs: -23.,
            true_peak_dbtp: -2.,
        }
    }
}
impl LoudnessSettings {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.target_lufs.is_finite() && self.true_peak_dbtp.is_finite(),
            "nonfinite loudness settings"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoudnessReport {
    pub measured_lufs: Option<f64>,
    pub gain_db: f64,
}

fn read_planar(file: &mut GuardedFile, count: usize, channels: usize) -> Result<Vec<Vec<f32>>> {
    let mut raw = vec![0u8; count * channels * 4];
    file.read_exact(&mut raw)?;
    let mut out = vec![vec![0.; count]; channels];
    for (i, pair) in raw.chunks_exact(channels * 4).enumerate() {
        for channel in 0..channels {
            let at = channel * 4;
            let value = f32::from_le_bytes(pair[at..at + 4].try_into()?);
            ensure!(value.is_finite(), "nonfinite loudness input");
            out[channel][i] = value;
        }
    }
    Ok(out)
}

/// Trusted stereo 48 kHz float-file API; caller owns access and source snapshots.
pub async fn normalize(
    raw: &Path,
    frames: u64,
    settings: LoudnessSettings,
    output: &Path,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    pool: CpuPool,
) -> Result<LoudnessReport> {
    normalize_with_rate(raw, frames, 48000, settings, output, timeout, parent_cancel, progress, pool).await
}

/// Trusted stereo float-file API at the resolved export sample rate.
/// The legacy entrypoint retains its 48 kHz contract.
pub async fn normalize_with_rate(
    raw: &Path,
    frames: u64,
    sample_rate: u32,
    settings: LoudnessSettings,
    output: &Path,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    pool: CpuPool,
) -> Result<LoudnessReport> {
    normalize_layout(raw, frames, sample_rate, 2, settings, output, timeout, parent_cancel, progress, pool).await
}

/// Trusted interleaved float input in mono, stereo or Filmcraft 5.1 order
/// (L, R, C, LFE, Ls, Rs). No channel folding occurs during normalization.
pub async fn normalize_layout(
    raw: &Path,
    frames: u64,
    sample_rate: u32,
    channels: usize,
    settings: LoudnessSettings,
    output: &Path,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    pool: CpuPool,
) -> Result<LoudnessReport> {
    ensure!(matches!(channels, 1 | 2 | 6), "unsupported loudness channel layout");
    ensure!((8000..=192000).contains(&sample_rate), "unsupported loudness sample rate");
    settings.validate()?;
    ensure!(
        settings.enabled && frames > 0,
        "loudness normalization is disabled or empty"
    );
    ensure!(
        !timeout.is_zero() && !parent_cancel.is_cancelled(),
        "loudness cancelled or zero timeout"
    );
    ensure!(
        std::fs::metadata(raw)?.len() == frames.checked_mul(channels as u64 * 4).context("loudness size overflow")?,
        "loudness input byte count mismatch"
    );
    ensure!(!output.try_exists()?, "loudness output already exists");
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("loudness output parent missing")?;
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("loudness deadline overflow")?;
    let cancel = parent_cancel.child_token();
    let _guard = cancel.clone().drop_guard();
    let path = parent.join(format!(".native-loudness-{}", uuid::Uuid::new_v4()));
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
        let mut meter = LoudnessMeter::new(f64::from(sample_rate), channels);
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(u64::from(sample_rate)) as usize;
            let data = read_planar(&mut source, n, channels)?;
            meter.process(&data.iter().map(Vec::as_slice).collect::<Vec<_>>());
            done += n as u64;
            let _ = signal.send(40. + 20. * done as f64 / frames as f64);
        }
        let measured = meter.integrated();
        let gain_db = normalize_gain_db(measured, settings.target_lufs);
        let gain = 10f64.powf(gain_db / 20.) as f32;
        ensure!(gain.is_finite(), "nonfinite normalization gain");
        let mut limiter = Limiter::new(sample_rate as f32, channels);
        limiter.set_param("ceiling", settings.true_peak_dbtp.clamp(-24., 0.) as f32);
        limiter.set_param("true_peak", 1.);
        limiter.set_param("release", 80.);
        limiter.reset();
        let latency = limiter.latency() as u64;
        let total = frames
            .checked_add(latency)
            .context("loudness latency overflow")?;
        source.seek(SeekFrom::Start(0))?;
        let (mut position, mut written) = (0u64, 0u64);
        while position < total {
            let n = (total - position).min(4096) as usize;
            let real = frames.saturating_sub(position).min(n as u64) as usize;
            let mut data = read_planar(&mut source, real, channels)?;
            for channel in &mut data {
                channel.resize(n, 0.);
                if gain != 1. {
                    for sample in channel {
                        *sample *= gain;
                    }
                }
            }
            {
                limiter.process(&mut data.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>());
            }
            let drop = latency.saturating_sub(position).min(n as u64) as usize;
            let mut bytes = Vec::with_capacity((n - drop) * channels * 4);
            for i in drop..n {
                for channel in &data {
                    ensure!(channel[i].is_finite(), "nonfinite normalized audio");
                    bytes.extend_from_slice(&channel[i].to_le_bytes());
                }
            }
            destination.write_all(&bytes)?;
            written += (n - drop) as u64;
            position += n as u64;
            let _ = signal.send(60. + 40. * position as f64 / total as f64);
        }
        ensure!(written == frames, "normalization changed audio duration");
        destination.check()?;
        destination.file.sync_all()?;
        Ok(LoudnessReport {
            measured_lufs: measured.is_finite().then_some(measured),
            gain_db,
        })
    });
    let report = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("loudness cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("loudness timeout"),result=work=>result.map_err(|e|anyhow::anyhow!("loudness CPU worker: {e:?}"))?,};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "loudness cancelled/timeout before publication"
    );
    // No asynchronous gap between this temporary-file publication and the
    // caller taking ownership of the returned normalized mix.
    std::fs::hard_link(&path, output).context("publish normalized audio without overwrite")?;
    Ok(report)
}
