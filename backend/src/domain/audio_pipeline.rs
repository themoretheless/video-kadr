//! Sample-accurate audio planning, loudness, waveform, and drift contracts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::artifact_graph::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioTime {
    pub sample_ticks: i64,
    pub sample_rate: u32,
}

impl AudioTime {
    pub fn new(sample_ticks: i64, sample_rate: u32) -> Option<Self> {
        (sample_rate > 0 && sample_rate <= 384_000).then_some(Self {
            sample_ticks,
            sample_rate,
        })
    }

    pub fn rescale(self, target_rate: u32) -> Option<Self> {
        if target_rate == 0 || target_rate > 384_000 {
            return None;
        }
        let numerator = i128::from(self.sample_ticks).checked_mul(i128::from(target_rate))?;
        let denominator = i128::from(self.sample_rate);
        let adjustment = denominator / 2;
        let ticks = if numerator >= 0 {
            numerator.checked_add(adjustment)? / denominator
        } else {
            numerator.checked_sub(adjustment)? / denominator
        };
        Self::new(i64::try_from(ticks).ok()?, target_rate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioEffectNode {
    pub id: String,
    pub inputs: Vec<String>,
    pub latency_samples: u32,
}

pub fn latency_compensation(
    nodes: &[AudioEffectNode],
) -> Result<BTreeMap<String, u64>, &'static str> {
    let mut totals = BTreeMap::new();
    let known: BTreeSet<_> = nodes.iter().map(|node| node.id.as_str()).collect();
    if known.len() != nodes.len() {
        return Err("duplicate audio node");
    }
    for node in nodes {
        if node
            .inputs
            .iter()
            .any(|input| !known.contains(input.as_str()))
        {
            return Err("missing audio dependency");
        }
        let upstream = node
            .inputs
            .iter()
            .filter_map(|input| totals.get(input))
            .copied()
            .max()
            .unwrap_or(0_u64);
        totals.insert(
            node.id.clone(),
            upstream.saturating_add(u64::from(node.latency_samples)),
        );
    }
    let maximum = totals.values().copied().max().unwrap_or(0);
    Ok(totals
        .into_iter()
        .map(|(id, total)| (id, maximum - total))
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Left,
    Right,
    Center,
    Lfe,
    LeftSurround,
    RightSurround,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelLayout {
    pub channels: Vec<Channel>,
}

impl ChannelLayout {
    pub fn mono() -> Self {
        Self {
            channels: vec![Channel::Center],
        }
    }
    pub fn stereo() -> Self {
        Self {
            channels: vec![Channel::Left, Channel::Right],
        }
    }
    pub fn surround_5_1() -> Self {
        Self {
            channels: vec![
                Channel::Left,
                Channel::Right,
                Channel::Center,
                Channel::Lfe,
                Channel::LeftSurround,
                Channel::RightSurround,
            ],
        }
    }
    pub fn validate(&self) -> bool {
        !self.channels.is_empty()
            && self.channels.len() <= 32
            && self.channels.iter().collect::<BTreeSet<_>>().len() == self.channels.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownmixMatrix {
    pub input: ChannelLayout,
    pub output: ChannelLayout,
    pub coefficients: Vec<Vec<f32>>,
}

impl DownmixMatrix {
    pub fn validate(&self) -> bool {
        self.input.validate()
            && self.output.validate()
            && self.coefficients.len() == self.output.channels.len()
            && self.coefficients.iter().all(|row| {
                row.len() == self.input.channels.len()
                    && row
                        .iter()
                        .all(|value| value.is_finite() && value.abs() <= 4.0)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoudnessMeasurement {
    pub schema_version: u16,
    pub source_fingerprint: Fingerprint,
    pub integrated_lufs: f64,
    pub loudness_range_lu: f64,
    pub true_peak_dbtp: f64,
    pub threshold_lufs: f64,
}

impl LoudnessMeasurement {
    pub fn validate(&self) -> bool {
        self.schema_version == 1
            && [
                self.integrated_lufs,
                self.loudness_range_lu,
                self.true_peak_dbtp,
                self.threshold_lufs,
            ]
            .into_iter()
            .all(f64::is_finite)
            && (-100.0..=10.0).contains(&self.integrated_lufs)
            && (0.0..=100.0).contains(&self.loudness_range_lu)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruePeakPolicy {
    pub ceiling_dbtp: f64,
}

impl TruePeakPolicy {
    pub fn accepts(self, measured_post_codec_dbtp: f64) -> bool {
        self.ceiling_dbtp.is_finite()
            && measured_post_codec_dbtp.is_finite()
            && measured_post_codec_dbtp <= self.ceiling_dbtp
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaveformTile {
    pub source_fingerprint: Fingerprint,
    pub level: u8,
    pub tile: u32,
    pub samples_per_bucket: u32,
    pub minimum: Vec<f32>,
    pub maximum: Vec<f32>,
    pub rms: Vec<f32>,
}

impl WaveformTile {
    pub fn key(&self) -> Fingerprint {
        Fingerprint::combine([
            self.source_fingerprint.as_str().as_bytes(),
            &[self.level],
            &self.tile.to_be_bytes(),
            &self.samples_per_bucket.to_be_bytes(),
        ])
    }
    pub fn validate(&self) -> bool {
        self.samples_per_bucket > 0
            && self.minimum.len() == self.maximum.len()
            && self.minimum.len() == self.rms.len()
            && !self.minimum.is_empty()
            && self
                .minimum
                .iter()
                .zip(&self.maximum)
                .zip(&self.rms)
                .all(|((min, max), rms)| {
                    min.is_finite()
                        && max.is_finite()
                        && rms.is_finite()
                        && min <= max
                        && *rms >= 0.0
                })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StretchProfile {
    Realtime,
    OfflineHighQuality,
}

pub trait AudioStretchPort: Send + Sync {
    fn stretch(
        &self,
        samples: &[f32],
        rate: u32,
        ratio: f64,
        profile: StretchProfile,
    ) -> Result<Vec<f32>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRenderPlan {
    pub sample_rate: u32,
    pub layout: ChannelLayout,
    pub nodes: Vec<AudioEffectNode>,
    pub video_required: bool,
}

impl AudioRenderPlan {
    pub fn validate(&self) -> bool {
        AudioTime::new(0, self.sample_rate).is_some()
            && self.layout.validate()
            && latency_compensation(&self.nodes).is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFault {
    Nan,
    Infinite,
    Denormal,
    UnexpectedSilence,
}

pub fn inspect_samples(samples: &[f32], silence_expected: bool) -> Result<(), SampleFault> {
    if samples.iter().any(|sample| sample.is_nan()) {
        return Err(SampleFault::Nan);
    }
    if samples.iter().any(|sample| sample.is_infinite()) {
        return Err(SampleFault::Infinite);
    }
    if samples
        .iter()
        .any(|sample| *sample != 0.0 && sample.abs() < f32::MIN_POSITIVE)
    {
        return Err(SampleFault::Denormal);
    }
    if !silence_expected
        && !samples.is_empty()
        && samples.iter().all(|sample| sample.abs() <= 1e-12)
    {
        return Err(SampleFault::UnexpectedSilence);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftObservation {
    pub expected_end_samples: i64,
    pub actual_end_samples: i64,
    pub tolerance_samples: u32,
}

impl DriftObservation {
    pub fn passes(self) -> bool {
        (self.actual_end_samples - self.expected_end_samples).unsigned_abs()
            <= u64::from(self.tolerance_samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_time_round_trip_and_latency_alignment_are_exact() {
        let original = AudioTime::new(44_100 * 60, 44_100).unwrap();
        assert_eq!(
            original.rescale(48_000).unwrap().rescale(44_100).unwrap(),
            original
        );
        let delays = latency_compensation(&[
            AudioEffectNode {
                id: "dry".into(),
                inputs: vec![],
                latency_samples: 0,
            },
            AudioEffectNode {
                id: "wet".into(),
                inputs: vec![],
                latency_samples: 128,
            },
            AudioEffectNode {
                id: "mix".into(),
                inputs: vec!["dry".into(), "wet".into()],
                latency_samples: 0,
            },
        ])
        .unwrap();
        assert_eq!(delays["dry"], 128);
        assert_eq!(delays["wet"], 0);
    }

    #[test]
    fn layout_loudness_and_true_peak_are_explicit() {
        let matrix = DownmixMatrix {
            input: ChannelLayout::surround_5_1(),
            output: ChannelLayout::stereo(),
            coefficients: vec![
                vec![1.0, 0.0, 0.707, 0.0, 0.707, 0.0],
                vec![0.0, 1.0, 0.707, 0.0, 0.0, 0.707],
            ],
        };
        assert!(matrix.validate());
        let measurement = LoudnessMeasurement {
            schema_version: 1,
            source_fingerprint: Fingerprint::digest(b"audio"),
            integrated_lufs: -14.1,
            loudness_range_lu: 5.0,
            true_peak_dbtp: -1.2,
            threshold_lufs: -24.0,
        };
        assert!(measurement.validate());
        assert!(TruePeakPolicy { ceiling_dbtp: -1.0 }.accepts(-1.2));
        assert!(!TruePeakPolicy { ceiling_dbtp: -1.0 }.accepts(-0.8));
    }

    #[test]
    fn waveform_sanitizer_graph_and_drift_fail_closed() {
        let tile = WaveformTile {
            source_fingerprint: Fingerprint::digest(b"audio"),
            level: 1,
            tile: 3,
            samples_per_bucket: 256,
            minimum: vec![-0.5],
            maximum: vec![0.7],
            rms: vec![0.2],
        };
        assert!(tile.validate());
        assert_ne!(tile.key(), tile.source_fingerprint);
        assert_eq!(inspect_samples(&[f32::NAN], false), Err(SampleFault::Nan));
        assert!(AudioRenderPlan {
            sample_rate: 48_000,
            layout: ChannelLayout::stereo(),
            nodes: vec![],
            video_required: false
        }
        .validate());
        assert!(DriftObservation {
            expected_end_samples: 48_000,
            actual_end_samples: 48_001,
            tolerance_samples: 2
        }
        .passes());
    }
}
