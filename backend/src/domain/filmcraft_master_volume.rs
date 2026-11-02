//! Compiled source master-fader automation on exact Filmcraft sequence ticks.
//! Reading automation is distinct from recording gestures or rendering the graph.
use super::{composition::AnimatableValue, filmcraft_sequence_source::SourceSequence};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum AutomationMode {
    Off,
    #[default]
    Read,
    Latch,
    Touch,
    Write,
}
impl AutomationMode {
    pub fn reads(self) -> bool {
        self != Self::Off
    }
    pub fn writes(self) -> bool {
        matches!(self, Self::Latch | Self::Touch | Self::Write)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMasterVolume {
    pub mode: AutomationMode,
    curve: AnimatableValue,
}
impl SourceMasterVolume {
    pub fn from_sequence(sequence: &SourceSequence<'_>) -> Result<Self> {
        Self::read(sequence.raw)
    }
    pub fn read(sequence: &Value) -> Result<Self> {
        let mode: AutomationMode = sequence
            .pointer("/master_mixer/mode")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let static_db = match sequence.get("master_volume_db") {
            None => 0.,
            Some(value) => value.as_f64().context("invalid master volume")?,
        };
        ensure!(static_db.is_finite(), "nonfinite master volume");
        let lane = sequence.pointer("/master_mixer/lanes/volume");
        let animated = lane
            .and_then(|lane| lane.get("keyframes"))
            .and_then(Value::as_array)
            .is_some_and(|keys| !keys.is_empty());
        let curve = if mode.reads() && animated {
            super::filmcraft_effect_scalar::convert(lane.expect("animated lane"),super::filmcraft_effect_scalar::Evaluation::MixerLane)?
        } else {
            AnimatableValue::Constant { value: static_db }
        };
        Ok(Self { mode, curve })
    }
    /// Apply the master fader to a planar bus at absolute content sample positions.
    /// Call after pre-fader inserts and before post-fader inserts; latency belongs
    /// to the graph caller. All bounds are checked before changing the buffer.
    pub fn apply_bus(
        &self,
        channels: &mut [Vec<f32>],
        start_sample: i64,
        sample_rate: u32,
    ) -> Result<()> {
        ensure!(sample_rate > 0, "zero audio sample rate");
        let frames = channels.first().map_or(0, Vec::len);
        ensure!(
            channels.iter().all(|channel| channel.len() == frames),
            "inconsistent audio channel lengths"
        );
        let tick = |sample: i128| -> Result<i64> {
            Ok(i64::try_from(
                sample * 254016000000i128 / sample_rate as i128,
            )?)
        };
        tick(start_sample as i128)?;
        if frames > 0 {
            tick(start_sample as i128 + frames as i128 - 1)?;
        }
        let gain = |db: f64| {
            if db <= -96. {
                0.
            } else {
                10f64.powf(db / 20.) as f32
            }
        };
        if let AnimatableValue::Constant { value } = &self.curve {
            let gain = gain(*value);
            if gain != 1. {
                for channel in channels {
                    for value in channel {
                        *value *= gain;
                    }
                }
            }
        } else {
            for i in 0..frames {
                let gain = gain(self.sample(tick(start_sample as i128 + i as i128)?));
                for channel in channels.iter_mut() {
                    channel[i] *= gain;
                }
            }
        }
        Ok(())
    }
    pub fn sample(&self, timeline_tick: i64) -> f64 {
        match &self.curve {
            AnimatableValue::Constant { value } => *value,
            AnimatableValue::Keyframes { track } => {
                track.sample_filmcraft_source_ticks(timeline_tick)
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_original_mix_graph_master_fader_pcm_matches_and_chunks_are_invariant() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-master-fader-pcm.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 900);
        for (i, case) in cases.iter().enumerate() {
            let fader = SourceMasterVolume::read(&case["sequence"]).unwrap();
            let input: Vec<Vec<u32>> = serde_json::from_value(case["input"].clone()).unwrap();
            let input: Vec<Vec<f32>> = input
                .into_iter()
                .map(|c| c.into_iter().map(f32::from_bits).collect())
                .collect();
            let mut whole = input.clone();
            let start = case["start"].as_i64().unwrap();
            let rate = case["rate"].as_u64().unwrap() as u32;
            fader.apply_bus(&mut whole, start, rate).unwrap();
            let bits: Vec<Vec<u32>> = whole
                .iter()
                .map(|c| c.iter().map(|v| v.to_bits()).collect())
                .collect();
            assert_eq!(
                serde_json::to_value(&bits).unwrap(),
                case["bits"],
                "PCM case {i}"
            );
            let mut split: Vec<Vec<f32>> = input.iter().map(|c| c[..5].to_vec()).collect();
            fader.apply_bus(&mut split, start, rate).unwrap();
            let mut tail: Vec<Vec<f32>> = input.iter().map(|c| c[5..].to_vec()).collect();
            fader.apply_bus(&mut tail, start + 5, rate).unwrap();
            for (head, tail) in split.iter_mut().zip(tail) {
                head.extend(tail);
            }
            assert_eq!(split, whole, "chunk case {i}");
        }
    }
    #[test]
    fn invalid_bus_is_rejected_before_mutation() {
        let fader = SourceMasterVolume::read(&serde_json::json!({"master_volume_db":-6.})).unwrap();
        for (rate, start, bus) in [
            (0, 0, vec![vec![1.]]),
            (48000, 0, vec![vec![1., 2.], vec![3.]]),
            (48000, i64::MAX, vec![vec![1.]]),
        ] {
            let mut output = bus.clone();
            assert!(fader.apply_bus(&mut output, start, rate).is_err());
            assert_eq!(output, bus);
        }
    }

    #[test]
    fn original_master_fader_modes_and_exact_keyframes_match() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-master-volume.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 13245);
        for (i, case) in cases.iter().enumerate() {
            let curve = SourceMasterVolume::read(&case["sequence"]).unwrap();
            let curve: SourceMasterVolume =
                serde_json::from_value(serde_json::to_value(curve).unwrap()).unwrap();
            let at = case["at"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                curve.sample(at).to_bits(),
                case["bits"].as_u64().unwrap(),
                "master sample {i}"
            );
            assert_eq!(curve.mode.reads(), case["reads"].as_bool().unwrap());
            assert_eq!(curve.mode.writes(), case["writes"].as_bool().unwrap());
        }
    }
}
