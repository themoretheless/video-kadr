//! Filmcraft export/settings.rs H264Profile, pinned at
//! 5231852443363f001c3f6b396dd9b1e6461ae2be (MIT).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum H264Profile {
    Baseline,
    Main,
    #[default]
    High,
}
impl H264Profile {
    pub fn native(self) -> filmcraft_h264enc::Profile {
        match self {
            Self::Baseline => filmcraft_h264enc::Profile::Baseline,
            Self::Main => filmcraft_h264enc::Profile::Main,
            Self::High => filmcraft_h264enc::Profile::High,
        }
    }
    pub fn idc(self) -> u8 {
        match self {
            Self::Baseline => 66,
            Self::Main => 77,
            Self::High => 100,
        }
    }
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Main => "main",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BitrateMode {
    Cbr,
    #[default]
    Vbr1Pass,
    Vbr2Pass,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BitrateSettings {
    pub mode: BitrateMode,
    pub target_kbps: u32,
    pub max_kbps: Option<u32>,
    pub adaptive_bpp: Option<f32>,
}
impl Default for BitrateSettings {
    fn default() -> Self {
        Self {
            mode: BitrateMode::Vbr1Pass,
            target_kbps: 10000,
            max_kbps: None,
            adaptive_bpp: None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ResolvedBitrate {
    pub mode: BitrateMode,
    pub target_kbps: u32,
    pub max_kbps: u32,
}
impl BitrateSettings {
    pub fn resolve(
        self,
        width: usize,
        height: usize,
        rate: super::composition::FrameRateRatio,
    ) -> anyhow::Result<ResolvedBitrate> {
        anyhow::ensure!(
            rate.numerator > 0
                && rate.denominator > 0
                && self.adaptive_bpp.is_none_or(f32::is_finite),
            "invalid H264 bitrate settings"
        );
        let fps = rate.numerator as f64 / rate.denominator as f64;
        let target = match self.adaptive_bpp {
            Some(bpp) if bpp > 0. => {
                (width as f64 * height as f64 * fps * f64::from(bpp) / 1000.).round() as u32
            }
            _ => self.target_kbps,
        }
        .max(100);
        let max = if self.mode == BitrateMode::Cbr {
            target
        } else {
            self.max_kbps
                .filter(|v| *v >= target)
                .unwrap_or_else(|| (u64::from(target) * 3 / 2).min(u64::from(u32::MAX)) as u32)
        };
        // Bound per-frame encoder/filler allocation as well as the file staging estimate.
        anyhow::ensure!(
            max <= 1_000_000,
            "H264 bitrate exceeds host 1000 Mbit/s budget"
        );
        Ok(ResolvedBitrate {
            mode: self.mode,
            target_kbps: target,
            max_kbps: max,
        })
    }
}
impl ResolvedBitrate {
    pub fn native(self) -> filmcraft_h264enc::RateControl {
        if self.mode == BitrateMode::Cbr {
            filmcraft_h264enc::RateControl::Cbr {
                kbps: self.target_kbps,
            }
        } else {
            filmcraft_h264enc::RateControl::Vbr {
                target_kbps: self.target_kbps,
                max_kbps: self.max_kbps,
            }
        }
    }
}
