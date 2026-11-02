//! Exact source export range. Video samples its output grid; audio and sidecars
//! retain the authored range, matching pinned Filmcraft export/{lib,audio_out}.rs.
use super::composition::FrameRateRatio;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

const T: i128 = 254_016_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRange {
    pub start_ticks: String,
    pub end_ticks: String,
}
impl SourceRange {
    pub fn bounds(&self) -> Result<(i64, i64)> {
        let start = self.start_ticks.parse::<i64>()?;
        let end = self.end_ticks.parse::<i64>()?;
        ensure!(
            start >= 0 && end > start && end <= filmcraft_time::Tick::MAX.0,
            "invalid source export range"
        );
        Ok((start, end))
    }
    pub fn video_span(&self, rate: FrameRateRatio) -> Result<(u64, i64)> {
        let (start, end) = self.bounds()?;
        super::filmcraft_export_clock::span(start, end, rate)
    }
    pub fn audio_span(&self, rate: u32) -> Result<(i64, u64)> {
        ensure!(rate > 0, "invalid audio sample rate");
        let (start, end) = self.bounds()?;
        let rate = i128::from(rate);
        let first = (i128::from(start) * rate).div_euclid(T);
        let last = (i128::from(end) * rate).div_euclid(T);
        let frames = u64::try_from(last - first)?;
        ensure!(frames > 0, "empty audio export range");
        // The graph re-floors this tick to a sample index; ceiling retains `first`.
        Ok((i64::try_from(-((-first * T).div_euclid(rate)))?, frames))
    }
}
