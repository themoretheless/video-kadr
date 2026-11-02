//! Owned PCM packet transport for native per-piece DSP, before source gain/fades.
//! Matroska SimpleBlocks retain one decoded frame per packet, without lacing.
//! Layout: https://www.matroska.org/technical/elements.html
use anyhow::{ensure, Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmPacketFormat {
    S16,
    S32,
    F32,
    F64,
}
impl PcmPacketFormat {
    pub fn bytes(self) -> usize {
        match self {
            Self::S16 => 2,
            Self::S32 | Self::F32 => 4,
            Self::F64 => 8,
        }
    }
    pub fn planar_name(self) -> &'static str {
        match self {
            Self::S16 => "s16p",
            Self::S32 => "s32p",
            Self::F32 => "fltp",
            Self::F64 => "dblp",
        }
    }
    fn codec(self) -> &'static [u8] {
        match self {
            Self::S16 | Self::S32 => b"A_PCM/INT/LIT",
            Self::F32 | Self::F64 => b"A_PCM/FLOAT/IEEE",
        }
    }
}
pub struct AudioPacketSpool {
    format: PcmPacketFormat,
    path: PathBuf,
    file: Option<File>,
    rate: u32,
    channels: usize,
    frames: u64,
    bytes: u64,
    budget: u64,
    failed: bool,
}
fn size(value: usize) -> Result<Vec<u8>> {
    for width in 1..=8 {
        if (value as u128) < (1u128 << (7 * width)) - 1 {
            let encoded = (value as u64) | (1u64 << (7 * width));
            return Ok(encoded.to_be_bytes()[8 - width..].to_vec());
        }
    }
    anyhow::bail!("audio packet element size overflow")
}
fn element(id: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut out = id.to_vec();
    out.extend(size(data.len())?);
    out.extend_from_slice(data);
    Ok(out)
}
fn uint(id: &[u8], value: u64) -> Result<Vec<u8>> {
    element(id, &value.to_be_bytes())
}
impl AudioPacketSpool {
    pub fn create(staging: &Path, rate: u32, channels: usize, budget: u64) -> Result<Self> {
        Self::create_format(staging, rate, channels, budget, PcmPacketFormat::F32)
    }
    pub fn create_format(
        staging: &Path,
        rate: u32,
        channels: usize,
        budget: u64,
        format: PcmPacketFormat,
    ) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate)
                && (1..=16).contains(&channels)
                && budget > 0
                && budget <= 8 * 1024 * 1024 * 1024,
            "invalid audio packet spool shape/budget"
        );
        let mut ebml = Vec::new();
        for (id, value) in [
            ([0x42, 0x86], 1),
            ([0x42, 0xf7], 1),
            ([0x42, 0xf2], 4),
            ([0x42, 0xf3], 8),
            ([0x42, 0x87], 4),
            ([0x42, 0x85], 2),
        ] {
            ebml.extend(uint(&id, value)?);
        }
        ebml.extend(element(&[0x42, 0x82], b"matroska")?);
        let mut header = element(&[0x1a, 0x45, 0xdf, 0xa3], &ebml)?;
        header.extend([
            0x18, 0x53, 0x80, 0x67, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ]);
        let mut info = uint(&[0x2a, 0xd7, 0xb1], 1)?;
        info.extend(element(&[0x4d, 0x80], b"video-kadr")?);
        info.extend(element(&[0x57, 0x41], b"video-kadr")?);
        header.extend(element(&[0x15, 0x49, 0xa9, 0x66], &info)?);
        let mut audio = element(&[0xb5], &(rate as f64).to_be_bytes())?;
        audio.extend(uint(&[0x9f], channels as u64)?);
        audio.extend(uint(&[0x62, 0x64], (format.bytes() * 8) as u64)?);
        let mut track = uint(&[0xd7], 1)?;
        track.extend(uint(&[0x73, 0xc5], 1)?);
        track.extend(uint(&[0x83], 2)?);
        track.extend(element(&[0x86], format.codec())?);
        track.extend(element(&[0xe1], &audio)?);
        header.extend(element(
            &[0x16, 0x54, 0xae, 0x6b],
            &element(&[0xae], &track)?,
        )?);
        ensure!(
            header.len() as u64 <= budget,
            "audio packet spool header exceeds budget"
        );
        let path = staging.join(format!(".audio-packets-{}.mka", uuid::Uuid::new_v4()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut spool = Self {
            format,
            path,
            file: Some(file),
            rate,
            channels,
            frames: 0,
            bytes: 0,
            budget,
            failed: false,
        };
        spool.write(&header)?;
        Ok(spool)
    }
    fn write(&mut self, data: &[u8]) -> Result<()> {
        let bytes = self
            .bytes
            .checked_add(data.len() as u64)
            .context("audio packet spool byte overflow")?;
        ensure!(
            bytes <= self.budget,
            "audio packet spool staging budget exceeded"
        );
        ensure!(!self.failed, "audio packet spool failed");
        let file = self
            .file
            .as_mut()
            .context("audio packet spool already sealed")?;
        self.failed = true;
        file.write_all(data)?;
        self.failed = false;
        self.bytes = bytes;
        Ok(())
    }
    pub fn push_f32(&mut self, channels: &[Vec<f32>]) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::F32,
            f32::is_finite,
            f32::to_le_bytes,
            None,
        )
    }
    pub fn push_f64(&mut self, channels: &[Vec<f64>]) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::F64,
            f64::is_finite,
            f64::to_le_bytes,
            None,
        )
    }
    pub fn push_i16(&mut self, channels: &[Vec<i16>]) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::S16,
            |_| true,
            i16::to_le_bytes,
            None,
        )
    }
    pub fn push_i32(&mut self, channels: &[Vec<i32>]) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::S32,
            |_| true,
            i32::to_le_bytes,
            None,
        )
    }
    pub fn push_f32_at(&mut self, channels: &[Vec<f32>], pts: u64) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::F32,
            f32::is_finite,
            f32::to_le_bytes,
            Some(pts),
        )
    }
    pub fn push_f64_at(&mut self, channels: &[Vec<f64>], pts: u64) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::F64,
            f64::is_finite,
            f64::to_le_bytes,
            Some(pts),
        )
    }
    pub fn push_i16_at(&mut self, channels: &[Vec<i16>], pts: u64) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::S16,
            |_| true,
            i16::to_le_bytes,
            Some(pts),
        )
    }
    pub fn push_i32_at(&mut self, channels: &[Vec<i32>], pts: u64) -> Result<()> {
        self.push(
            channels,
            PcmPacketFormat::S32,
            |_| true,
            i32::to_le_bytes,
            Some(pts),
        )
    }
    fn push<T: Copy, const N: usize>(
        &mut self,
        channels: &[Vec<T>],
        format: PcmPacketFormat,
        valid: fn(T) -> bool,
        encode: fn(T) -> [u8; N],
        pts: Option<u64>,
    ) -> Result<()> {
        ensure!(
            self.format == format,
            "audio packet spool precision mismatch"
        );
        ensure!(
            channels.len() == self.channels,
            "audio packet spool channel mismatch"
        );
        let count = channels[0].len();
        ensure!(
            count > 0
                && count <= 1048576
                && count * self.channels <= 2097152
                && channels
                    .iter()
                    .all(|c| c.len() == count && c.iter().all(|v| valid(*v))),
            "invalid audio packet spool frame"
        );
        let end = self
            .frames
            .checked_add(count as u64)
            .filter(|v| *v <= self.rate as u64 * 60 * 60 * 6)
            .context("audio packet spool duration exceeded")?;
        let pts = pts.unwrap_or(self.frames);
        ensure!(
            pts.checked_add(count as u64)
                .is_some_and(|v| v <= self.rate as u64 * 60 * 60 * 6),
            "audio packet timestamp extent exceeded"
        );
        let timestamp = u64::try_from(
            (u128::from(pts) * 1_000_000_000 + u128::from(self.rate) / 2) / u128::from(self.rate),
        )?;
        let mut block = vec![0x81, 0, 0, 0x80];
        for i in 0..count {
            for channel in channels {
                block.extend(encode(channel[i]));
            }
        }
        let mut cluster = uint(&[0xe7], timestamp)?;
        cluster.extend(element(&[0xa3], &block)?);
        self.write(&element(&[0x1f, 0x43, 0xb6, 0x75], &cluster)?)?;
        self.frames = end;
        Ok(())
    }
    pub fn seal(&mut self) -> Result<()> {
        ensure!(
            self.frames > 0 && !self.failed,
            "cannot seal empty or failed audio packet spool"
        );
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn format(&self) -> PcmPacketFormat {
        self.format
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}
impl Drop for AudioPacketSpool {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
