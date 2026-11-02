//! Source mixer insert selection and exact parameter binding; DSP runtime state
//! and latency compensation are separate from this serializable authoring plan.
use super::{
    composition::AnimatableValue,
    filmcraft_dsp::EffectSpec,
    filmcraft_effect_scalar::{self, Evaluation},
    filmcraft_master_volume::AutomationMode,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertHost {
    Track,
    Master,
    Clip,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Insert {
    slot: usize,
    id: String,
    post: bool,
    parameters: BTreeMap<String, AnimatableValue>,
    preroll_parameters: BTreeMap<String, AnimatableValue>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAudioInserts {
    pub source_effects: Vec<Value>,
    inserts: Vec<Insert>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInsert {
    pub slot: usize,
    pub post: bool,
    pub spec: EffectSpec,
    pub preroll: usize,
}
impl SourceAudioInserts {
    pub fn compile(effects: &Value, mixer: &Value, host: InsertHost) -> Result<Self> {
        let source_effects = effects
            .as_array()
            .context("invalid source inserts")?
            .clone();
        let mode: AutomationMode = mixer
            .get("mode")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let mut inserts = vec![];
        for (slot, raw) in source_effects
            .iter()
            .take(if host == InsertHost::Clip {
                usize::MAX
            } else {
                5
            })
            .enumerate()
        {
            let id = raw
                .get("effect")
                .and_then(Value::as_str)
                .context("invalid insert ID")?;
            if !raw
                .get("enabled")
                .and_then(Value::as_bool)
                .context("invalid insert enabled flag")?
            {
                continue;
            }
            if host == InsertHost::Track
                && raw
                    .pointer("/params/bypass/value/Bool")
                    .and_then(Value::as_bool)
                    == Some(true)
            {
                continue;
            }
            let Some(definition) = super::filmcraft_audio_project::catalog()["effects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id && v["intrinsic"] == false)
            else {
                continue;
            };
            let params = raw
                .get("params")
                .and_then(Value::as_object)
                .context("invalid source insert params")?;
            let native_id =
                super::filmcraft_audio_project::resolve(id, &BTreeMap::new(), false, 48000)?
                    .0
                    .id;
            let direct = super::filmcraft_audio_mapping::DIRECT_PROJECT_EFFECTS
                .iter()
                .any(|(project, native, _)| *project == id && *native == native_id);
            let mut parameters = BTreeMap::new();
            let mut preroll_parameters = BTreeMap::new();
            for definition in definition["params"].as_array().unwrap() {
                let key = definition["id"].as_str().unwrap();
                if let Some(param) = params.get(key) {
                    let mut source = param.clone();
                    if !mode.reads() {
                        source["keyframes"] = Value::Array(vec![]);
                    }
                    preroll_parameters.insert(
                        key.into(),
                        filmcraft_effect_scalar::convert(&source, Evaluation::EffectParameter)?,
                    );
                    parameters.insert(
                        key.into(),
                        filmcraft_effect_scalar::convert(
                            &source,
                            if direct {
                                Evaluation::MixerLane
                            } else {
                                Evaluation::EffectParameter
                            },
                        )?,
                    );
                }
            }
            inserts.push(Insert {
                slot,
                id: id.into(),
                post: host != InsertHost::Clip
                    && raw
                        .get("post_fader")
                        .map(|v| v.as_bool().context("invalid post-fader flag"))
                        .transpose()?
                        .unwrap_or(false),
                parameters,
                preroll_parameters,
            });
        }
        Ok(Self {
            source_effects,
            inserts,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty()
    }
    pub fn preroll_samples(&self, tick: i64, rate: u32) -> Result<usize> {
        self.inserts.iter().try_fold(0, |maximum, insert| {
            let values = insert
                .preroll_parameters
                .iter()
                .map(|(key, curve)| (key.clone(), filmcraft_effect_scalar::sample(curve, tick)))
                .collect();
            Ok(maximum
                .max(super::filmcraft_audio_project::resolve(&insert.id, &values, false, rate)?.1))
        })
    }
    pub fn resolve(&self, tick: i64, rate: u32) -> Result<Vec<ResolvedInsert>> {
        self.inserts
            .iter()
            .map(|insert| {
                let values = insert
                    .parameters
                    .iter()
                    .map(|(key, curve)| (key.clone(), filmcraft_effect_scalar::sample(curve, tick)))
                    .collect();
                let (spec, preroll) =
                    super::filmcraft_audio_project::resolve(&insert.id, &values, false, rate)?;
                let preroll = if insert.parameters != insert.preroll_parameters {
                    let values = insert
                        .preroll_parameters
                        .iter()
                        .map(|(key, curve)| {
                            (key.clone(), filmcraft_effect_scalar::sample(curve, tick))
                        })
                        .collect();
                    super::filmcraft_audio_project::resolve(&insert.id, &values, false, rate)?.1
                } else {
                    preroll
                };
                Ok(ResolvedInsert {
                    slot: insert.slot,
                    post: insert.post,
                    spec,
                    preroll,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_original_mixer_selected_slots_and_dsp_targets_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-audio-inserts.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 6600);
        for (i, case) in cases.iter().enumerate() {
            let host = if case["master"].as_bool().unwrap() {
                InsertHost::Master
            } else {
                InsertHost::Track
            };
            let plan = SourceAudioInserts::compile(&case["effects"], &case["mixer"], host).unwrap();
            let plan: SourceAudioInserts =
                serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_value(&plan.source_effects).unwrap(),
                case["effects"]
            );
            let mut resolved = plan
                .resolve(case["at"].as_str().unwrap().parse().unwrap(), 48000)
                .unwrap();
            resolved.sort_by_key(|v| (v.post, v.slot));
            let expected = case["expected"].as_array().unwrap();
            assert_eq!(resolved.len(), expected.len(), "selected slots {i}");
            for (insert, expected) in resolved.iter().zip(expected) {
                assert_eq!(
                    insert.slot,
                    expected["slot"].as_u64().unwrap() as usize,
                    "slot {i}"
                );
                assert_eq!(insert.post, expected["post"].as_bool().unwrap());
                assert_eq!(insert.spec.id, expected["dsp"].as_str().unwrap());
                let mut native =
                    filmcraft_audio_dsp::create_effect(&insert.spec.id, 48000., 2).unwrap();
                for (key, value) in &insert.spec.params {
                    assert!(native.set_param(key, *value));
                }
                assert_eq!(
                    native.params().len(),
                    expected["targets"].as_object().unwrap().len()
                );
                for parameter in native.params() {
                    assert_eq!(
                        native.param(parameter.id).unwrap().to_bits(),
                        (expected["targets"][parameter.id].as_f64().unwrap() as f32).to_bits(),
                        "DSP target {i} {} {}",
                        insert.spec.id,
                        parameter.id
                    );
                }
            }
        }
    }
}

/// Mutable insert state. The graph caller owns preroll, seek reconstruction and
/// compensation delays. Each pre/post chain must receive contiguous input.
pub struct SourceInsertRuntime {
    plan: SourceAudioInserts,
    rate: u32,
    channels: usize,
    states: Vec<InsertState>,
    next: [Option<i64>; 2],
}
struct InsertState {
    slot: usize,
    post: bool,
    id: String,
    dsp: Box<dyn filmcraft_audio_dsp::AudioEffect>,
    latency: usize,
}
impl SourceInsertRuntime {
    pub fn new(plan: SourceAudioInserts, rate: u32, channels: usize) -> Result<Self> {
        anyhow::ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "invalid insert runtime format"
        );
        let mut states = Vec::new();
        for (source, resolved) in plan.inserts.iter().zip(plan.resolve(0, rate)?) {
            let dsp = filmcraft_audio_dsp::create_effect(&resolved.spec.id, rate as f32, channels)
                .context("insert DSP factory unavailable")?;
            let latency = dsp.latency();
            states.push(InsertState {
                slot: source.slot,
                post: source.post,
                id: source.id.clone(),
                dsp,
                latency,
            });
        }
        Ok(Self {
            plan,
            rate,
            channels,
            states,
            next: [None, None],
        })
    }
    pub fn latency_samples(&self, post: bool) -> usize {
        self.states
            .iter()
            .filter(|state| state.post == post)
            .map(|state| state.latency)
            .sum()
    }
    pub fn process(
        &mut self,
        post: bool,
        bus: &mut [Vec<f32>],
        start: i64,
        content_latency: usize,
    ) -> Result<()> {
        let rate = self.rate;
        self.process_with_clock(post, bus, start, 64, |grid| {
            Ok(i64::try_from(
                (grid as i128 - content_latency as i128) * 254016000000i128 / rate as i128,
            )?)
        })
    }
    pub fn process_with_clock(
        &mut self,
        post: bool,
        bus: &mut [Vec<f32>],
        start: i64,
        block: i64,
        tick: impl Fn(i64) -> Result<i64>,
    ) -> Result<()> {
        anyhow::ensure!(matches!(block, 64 | 256), "invalid source parameter grid");
        anyhow::ensure!(bus.len() == self.channels, "insert channel mismatch");
        let n = bus.first().map_or(0, Vec::len);
        anyhow::ensure!(
            n <= 65536
                && bus
                    .iter()
                    .all(|c| c.len() == n && c.iter().all(|v| v.is_finite())),
            "invalid insert PCM"
        );
        anyhow::ensure!(
            self.next[post as usize].is_none_or(|next| next == start),
            "insert seek requires graph reconstruction/preroll"
        );
        let end = start
            .checked_add(n as i64)
            .context("insert sample clock overflow")?;
        tick(start)?;
        tick(end)?;
        let mut offset = 0;
        while offset < n {
            let pos = start + offset as i64;
            let grid = pos.div_euclid(block) * block;
            let length = (block as usize - pos.rem_euclid(block) as usize).min(n - offset);
            let resolved = self.plan.resolve(tick(grid)?, self.rate)?;
            for state in self.states.iter_mut().filter(|state| state.post == post) {
                let spec = &resolved
                    .iter()
                    .find(|insert| insert.slot == state.slot)
                    .context("missing insert slot")?
                    .spec;
                apply_targets(&state.id, state.dsp.as_mut(), &spec.params)?;
            }
            for state in self.states.iter_mut().filter(|state| state.post == post) {
                let mut channels: Vec<_> = bus
                    .iter_mut()
                    .map(|channel| &mut channel[offset..offset + length])
                    .collect();
                state.dsp.process(&mut channels);
            }
            offset += length;
        }
        self.next[post as usize] = Some(end);
        Ok(())
    }
}
fn apply_targets(
    id: &str,
    dsp: &mut dyn filmcraft_audio_dsp::AudioEffect,
    params: &BTreeMap<String, f32>,
) -> Result<()> {
    let direct = super::filmcraft_audio_mapping::DIRECT_PROJECT_EFFECTS
        .iter()
        .any(|(project, native, _)| *project == id && *native == dsp.id());
    let direct_order;
    let order: &[&str] = if direct {
        direct_order = super::filmcraft_audio_project::catalog()["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap()["params"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        &direct_order
    } else {
        match id {
            "amplify" | "volume_a" => &["gain"],
            "dynamics" => &["threshold", "ratio", "attack", "release"],
            "single_band_compressor" => {
                &["knee", "threshold", "ratio", "attack", "release", "makeup"]
            }
            "hard_limiter" => &["ceiling", "input_gain", "release"],
            "delay" => &["time", "feedback", "mix"],
            "parametric_eq" => &[
                "low_freq",
                "low",
                "mid_freq",
                "mid",
                "mid_q",
                "high_freq",
                "high",
            ],
            "highpass" | "lowpass" | "bandpass" | "simple_notch" => {
                &["b1.on", "b1.type", "b1.freq", "b1.q"]
            }
            "bass" | "treble" | "simple_eq" => &["b1.on", "b1.type", "b1.freq", "b1.q", "b1.gain"],
            "denoise" => &["reduction"],
            "dehummer" => &["frequency", "amount", "harmonics", "q"],
            "deesser" => &["frequency", "threshold", "reduction"],
            "dereverb" => &["amount", "rt60"],
            "speech_enhance" => &["mix", "tone"],
            "stereo_width" => &["width"],
            "studio_reverb" => &["size", "decay", "damping", "mix"],
            "pitch_shifter" => &["semitones"],
            "invert_a" => &[],
            _ => anyhow::bail!("missing source insert setter order"),
        }
    };
    anyhow::ensure!(
        params.keys().all(|key| order.contains(&key.as_str())),
        "unbound insert DSP target"
    );
    for key in order {
        if let Some(value) = params.get(*key) {
            let changed_only = direct
                || matches!(id, "single_band_compressor" | "volume_a")
                || id == "dehummer" && matches!(*key, "harmonics" | "q")
                || matches!(id, "bass" | "treble" | "simple_eq") && *key == "b1.gain";
            if !changed_only || dsp.param(key) != Some(*value) {
                anyhow::ensure!(dsp.set_param(key, *value), "insert DSP target rejected");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn actual_original_run_inserts_state_grid_and_pcm_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-audio-inserts-pcm.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 3960);
        let mut failures = Vec::new();
        for (i, case) in cases.iter().enumerate() {
            let plan =
                SourceAudioInserts::compile(&case["effects"], &case["mixer"], InsertHost::Track)
                    .unwrap();
            let post = case["post"].as_bool().unwrap();
            let start = case["start"].as_i64().unwrap();
            let lat = case["contentLatency"].as_u64().unwrap() as usize;
            let mut runtime = SourceInsertRuntime::new(plan.clone(), 48000, 2).unwrap();
            assert_eq!(
                runtime.latency_samples(post),
                case["pcm"]["latency"].as_u64().unwrap() as usize
            );
            let input: Vec<Vec<u32>> =
                serde_json::from_value(case["pcm"]["input"].clone()).unwrap();
            let input: Vec<Vec<f32>> = input
                .into_iter()
                .map(|c| c.into_iter().map(f32::from_bits).collect())
                .collect();
            let bits = |bus: &Vec<Vec<f32>>| {
                serde_json::to_value(
                    bus.iter()
                        .map(|c| c.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            };
            let mut whole = input.clone();
            runtime.process(post, &mut whole, start, lat).unwrap();
            if bits(&whole) != case["pcm"]["bits"] {
                failures.push((i, "whole"));
            }
            let mut runtime = SourceInsertRuntime::new(plan, 48000, 2).unwrap();
            let mut split = vec![Vec::new(), Vec::new()];
            for (a, b) in [(0, 67), (67, 193)] {
                let mut part: Vec<Vec<f32>> = input.iter().map(|c| c[a..b].to_vec()).collect();
                runtime
                    .process(post, &mut part, start + a as i64, lat)
                    .unwrap();
                for (out, c) in split.iter_mut().zip(part) {
                    out.extend(c);
                }
            }
            if bits(&split) != case["pcm"]["splitBits"] {
                failures.push((i, "split"));
            }
        }
        assert!(
            failures.is_empty(),
            "source insert PCM failures: {failures:?}"
        );
    }
}
