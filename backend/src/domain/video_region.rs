//! Portable full-source temporal SAM regions; no job IDs or model paths.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, try_from = "Wire")]
pub struct Attachment {
    pub source_id: String,
    pub mask_source_id: String,
    pub source_hash: String,
    pub mask_hash: String,
    pub mask_file_hash: String,
    pub encoder_hash: String,
    pub decoder_hash: String,
    pub runtime_hash: String,
    pub prompt_hash: String,
    pub points: Vec<[f64; 2]>,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: super::composition::FrameRateRatio,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_times_us: Option<Vec<u64>>,
    pub seed_frame: u64,
    pub tracked_frames: u64,
    pub forward_lost_at: Option<u64>,
    pub backward_lost_at: Option<u64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    source_id: String,
    mask_source_id: String,
    source_hash: String,
    mask_hash: String,
    mask_file_hash: String,
    encoder_hash: String,
    decoder_hash: String,
    runtime_hash: String,
    prompt_hash: String,
    points: Vec<[f64; 2]>,
    width: u32,
    height: u32,
    frames: u64,
    frame_rate: super::composition::FrameRateRatio,
    #[serde(default)]
    frame_times_us: Option<Vec<u64>>,
    seed_frame: u64,
    tracked_frames: u64,
    forward_lost_at: Option<u64>,
    backward_lost_at: Option<u64>,
}
impl TryFrom<Wire> for Attachment {
    type Error = anyhow::Error;
    fn try_from(w: Wire) -> Result<Self> {
        let value = Self {
            source_id: w.source_id,
            mask_source_id: w.mask_source_id,
            source_hash: w.source_hash,
            mask_hash: w.mask_hash,
            mask_file_hash: w.mask_file_hash,
            encoder_hash: w.encoder_hash,
            decoder_hash: w.decoder_hash,
            runtime_hash: w.runtime_hash,
            prompt_hash: w.prompt_hash,
            points: w.points,
            width: w.width,
            height: w.height,
            frames: w.frames,
            frame_rate: w.frame_rate,
            frame_times_us: w.frame_times_us,
            seed_frame: w.seed_frame,
            tracked_frames: w.tracked_frames,
            forward_lost_at: w.forward_lost_at,
            backward_lost_at: w.backward_lost_at,
        };
        value.validate()?;
        Ok(value)
    }
}
impl Attachment {
    pub fn validate(&self) -> Result<()> {
        super::composition::SourceId::parse(self.source_id.clone())?;
        super::composition::SourceId::parse(self.mask_source_id.clone())?;
        ensure!(
            self.source_id != self.mask_source_id,
            "region video cannot be its source"
        );
        for hash in [
            &self.source_hash,
            &self.mask_hash,
            &self.mask_file_hash,
            &self.encoder_hash,
            &self.decoder_hash,
            &self.runtime_hash,
            &self.prompt_hash,
        ] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
                "invalid video region identity"
            );
        }
        ensure!(
            !self.points.is_empty()
                && self.points.len() <= 65536
                && self
                    .points
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
            "invalid video region prompts"
        );
        ensure!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(&self.points)?)) == self.prompt_hash,
            "video region prompt identity changed"
        );
        ensure!(
            self.width > 0
                && self.height > 0
                && u64::from(self.width) * u64::from(self.height) <= 64 * 1024 * 1024,
            "invalid video region source shape"
        );
        ensure!(
            self.frames > 0
                && self.frames <= 360000
                && self.seed_frame < self.frames
                && self.frame_rate.numerator > 0
                && self.frame_rate.denominator > 0,
            "invalid video region clock"
        );
        ensure!(
            u64::from(self.width) * u64::from(self.height) * 4 * self.frames
                <= 64 * 1024 * 1024 * 1024,
            "video region source budget"
        );
        ensure!(
            self.forward_lost_at
                .is_none_or(|v| v > self.seed_frame && v < self.frames)
                && self.backward_lost_at.is_none_or(|v| v < self.seed_frame),
            "invalid video region loss positions"
        );
        validate_frame_times(self.frame_times_us.as_deref(), self.frames)?;
        let forward = self.forward_lost_at.unwrap_or(self.frames) - self.seed_frame - 1;
        let backward = self
            .backward_lost_at
            .map(|v| self.seed_frame - v - 1)
            .unwrap_or(self.seed_frame);
        ensure!(
            self.tracked_frames == 1 + forward + backward,
            "invalid video region retained count"
        );
        Ok(())
    }
}

/// Optional VFR clock stores normalized frame starts plus the exclusive end.
/// The encoded mask movie remains an ordinal carrier; source PTS chooses its plane.
pub fn validate_frame_times(times: Option<&[u64]>, frames: u64) -> Result<()> {
    if let Some(times) = times {
        ensure!(
            frames > 0
                && frames <= 360000
                && times.len() as u64 == frames + 1
                && times.first() == Some(&0)
                && times.last().is_some_and(|t| *t <= 9_007_199_254_740_991)
                && times.windows(2).all(|p| p[0] < p[1]),
            "invalid temporal region presentation clock"
        );
    }
    Ok(())
}
/// Callers validate the immutable clock once before repeated random access.
pub fn frame_ordinal(
    source_us: u64,
    frames: u64,
    rate: super::composition::FrameRateRatio,
    times: Option<&[u64]>,
) -> Result<u64> {
    if let Some(times) = times {
        ensure!(
            times.len() as u64 == frames + 1
                && times.first() == Some(&0)
                && times.last().is_some_and(|end| source_us < *end),
            "temporal region source time outside movie"
        );
        return Ok((times.partition_point(|t| *t <= source_us) - 1) as u64);
    }
    ensure!(
        rate.numerator > 0 && rate.denominator > 0,
        "invalid temporal region rate"
    );
    let ordinal = u128::from(source_us) * u128::from(rate.numerator)
        / (1_000_000 * u128::from(rate.denominator));
    ensure!(
        ordinal < u128::from(frames),
        "temporal region source time outside movie"
    );
    Ok(ordinal.try_into()?)
}
