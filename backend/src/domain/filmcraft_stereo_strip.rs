//! Native input-map, mute, fader and pan stage for mono/stereo source strips.
//! Inserts, sends, solo and latency are graph responsibilities.
use super::{
    composition::AnimatableValue,
    filmcraft_master_volume::{AutomationMode, SourceMasterVolume},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum InputMap {
    #[default]
    Stereo,
    Left,
    Right,
    Swap,
    Mono,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStereoStrip {
    mono: bool,
    input_map: InputMap,
    volume: SourceMasterVolume,
    pan: AnimatableValue,
    muted: bool,
    mute_keys: Vec<(i64, f64)>,
}
impl SourceStereoStrip {
    pub fn read(track: &Value) -> Result<Self> {
        let mono = match track
            .get("channels")
            .and_then(Value::as_str)
            .unwrap_or("Stereo")
        {
            "Mono" => true,
            "Stereo" | "Adaptive" | "Surround51" => false,
            _ => anyhow::bail!("strip requires surround/adaptive processing"),
        };
        let mode: AutomationMode = track
            .pointer("/mixer/mode")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let input_map = track
            .pointer("/mixer/input_map")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let volume = SourceMasterVolume::read(
            &json!({"master_volume_db":track.get("volume_db").cloned().unwrap_or(json!(0.)),"master_mixer":track.get("mixer").cloned().unwrap_or(json!({}))}),
        )?;
        let pan_static = track
            .get("pan")
            .map(|v| v.as_f64().context("invalid strip pan"))
            .transpose()?
            .unwrap_or(0.);
        let pan_lane = track.pointer("/mixer/lanes/pan");
        let animated = |lane: Option<&Value>| {
            lane.and_then(|v| v.get("keyframes"))
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
        };
        let pan = if mode.reads() && animated(pan_lane) {
            super::filmcraft_effect_scalar::convert(
                pan_lane.unwrap(),
                super::filmcraft_effect_scalar::Evaluation::MixerLane,
            )?
        } else {
            AnimatableValue::Constant { value: pan_static }
        };
        let muted = track
            .get("muted")
            .map(|v| v.as_bool().context("invalid strip mute"))
            .transpose()?
            .unwrap_or(false);
        let mut mute_keys = vec![];
        if mode.reads() {
            if let Some(keys) = track
                .pointer("/mixer/lanes/mute/keyframes")
                .and_then(Value::as_array)
            {
                for key in keys {
                    let time = key
                        .get("time")
                        .and_then(Value::as_i64)
                        .context("invalid mute key time")?;
                    let value = &key["value"];
                    let value = value
                        .get("Float")
                        .and_then(Value::as_f64)
                        .or_else(|| {
                            value
                                .get("Choice")
                                .and_then(Value::as_u64)
                                .map(|v| v as f64)
                        })
                        .or_else(|| {
                            value
                                .get("Bool")
                                .and_then(Value::as_bool)
                                .map(|v| if v { 1. } else { 0. })
                        })
                        .unwrap_or(0.);
                    mute_keys.push((time, value));
                }
            }
        }
        ensure!(
            mute_keys.windows(2).all(|w| w[0].0 <= w[1].0),
            "unsorted mute keys"
        );
        Ok(Self {
            mono,
            input_map,
            volume,
            pan,
            muted,
            mute_keys,
        })
    }
    pub fn statically_muted(&self) -> bool {
        self.mute_keys.is_empty() && self.muted
    }
    /// For strips without inserts/sends: input mapping → mute → volume → pan.
    pub fn apply_bus(&self, bus: &mut [Vec<f32>], start: i64, rate: u32) -> Result<()> {
        self.apply_bus_with_taps(bus, start, rate, |_, _| Ok(()))
    }
    /// Taps are after input mapping and after fader, both before output pan.
    pub fn apply_bus_with_taps(
        &self,
        bus: &mut [Vec<f32>],
        start: i64,
        rate: u32,
        mut tap: impl FnMut(bool, &[Vec<f32>]) -> Result<()>,
    ) -> Result<()> {
        self.apply_staged(bus, start, start, rate, |pre, bus| tap(pre, bus))
    }
    /// Mutable pre/post insert stages, with latency-adjusted fader/pan clocks.
    pub fn apply_staged(
        &self,
        bus: &mut [Vec<f32>],
        fader_start: i64,
        pan_start: i64,
        rate: u32,
        tap: impl FnMut(bool, &mut [Vec<f32>]) -> Result<()>,
    ) -> Result<()> {
        self.apply_staged_inner(bus,fader_start,pan_start,rate,true,tap)
    }
    /// Native-width input map, inserts/taps, mute and fader; routing owns pan.
    pub fn apply_before_pan(
        &self,bus:&mut [Vec<f32>],start:i64,rate:u32,
        tap:impl FnMut(bool,&mut [Vec<f32>])->Result<()>,
    )->Result<()> {self.apply_staged_inner(bus,start,start,rate,false,tap)}
    fn apply_staged_inner(
        &self,bus:&mut [Vec<f32>],fader_start:i64,pan_start:i64,rate:u32,apply_pan:bool,
        mut tap:impl FnMut(bool,&mut [Vec<f32>])->Result<()>,
    )->Result<()> {
        ensure!(
            rate > 0 && (bus.len()==2 || (!apply_pan && bus.len()==6)) && bus.iter().all(|c|c.len()==bus[0].len()),
            "invalid stereo bus"
        );
        let n = bus[0].len();
        let tick = |i: usize| -> Result<i64> {
            Ok(i64::try_from(
                (fader_start as i128 + i as i128) * 254016000000i128 / rate as i128,
            )?)
        };
        tick(0)?;
        i64::try_from((pan_start as i128 + n as i128) * 254016000000i128 / rate as i128)?;
        if n > 0 {
            tick(n - 1)?;
        }
        if bus.len()==2 {for i in 0..n {
            let (mut l, mut r) = (bus[0][i], bus[1][i]);
            match self.input_map {
                InputMap::Stereo => {}
                InputMap::Left => r = l,
                InputMap::Right => l = r,
                InputMap::Swap => std::mem::swap(&mut l, &mut r),
                InputMap::Mono => {
                    let m = (l + r) * 0.5;
                    l = m;
                    r = m;
                }
            }
            if self.mono && self.input_map != InputMap::Mono {
                let m = (l + r) * 0.5;
                l = m;
                r = m;
            }
            bus[0][i] = l;
            bus[1][i] = r;
        }
        }
        tap(true, bus)?;
        for i in 0..n {
            let muted = if self.mute_keys.is_empty() {
                self.muted
            } else {
                let k = self
                    .mute_keys
                    .partition_point(|(at, _)| *at <= tick(i).expect("prechecked sample range"));
                self.mute_keys[k.saturating_sub(1)].1 >= 0.5
            };
            if muted {
                for channel in bus.iter_mut(){channel[i]*=0.;}
            }
        }
        self.volume.apply_bus(bus, fader_start, rate)?;
        tap(false, bus)?;
        if apply_pan {self.apply_pan_only(bus,pan_start,rate)?;}
        Ok(())
    }
    pub(crate) fn apply_pan_only(&self,bus:&mut [Vec<f32>],pan_start:i64,rate:u32)->Result<()> {
        ensure!(rate>0 && bus.len()==2 && bus[0].len()==bus[1].len(),"invalid stereo pan bus");
        let n=bus[0].len();
        i64::try_from((i128::from(pan_start)+n as i128)*254016000000i128/i128::from(rate))?;
        for i in 0..n {
            let p = match &self.pan {
                AnimatableValue::Constant { value } => *value,
                AnimatableValue::Keyframes { track } => {
                    track.sample_filmcraft_source_ticks(i64::try_from(
                        (pan_start as i128 + i as i128) * 254016000000i128 / rate as i128,
                    )?)
                }
            } as f32
                / 100.;
            let a = (p.clamp(-1., 1.) + 1.) * std::f32::consts::FRAC_PI_4;
            let (l, r) = if self.mono {
                (a.cos(), a.sin())
            } else {
                (
                    (a.cos() * std::f32::consts::SQRT_2).min(1.),
                    (a.sin() * std::f32::consts::SQRT_2).min(1.),
                )
            };
            bus[0][i] *= l;
            bus[1][i] *= r;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_original_mix_graph_stereo_strip_pcm_matches_and_chunks_are_invariant() {
        for fixture in [
            include_str!("../../tests/fixtures/filmcraft-stereo-strip-pcm.json"),
            include_str!("../../tests/fixtures/filmcraft-stereo-strip-asymmetric.json"),
        ] {
            let oracle: Value = serde_json::from_str(fixture).unwrap();
            assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
            let cases = oracle["cases"].as_array().unwrap();
            assert_eq!(cases.len(), 900);
            let mut failures = Vec::new();
            for (i, case) in cases.iter().enumerate() {
                let fader = SourceStereoStrip::read(&case["track"]).unwrap();
                let fader: SourceStereoStrip =
                    serde_json::from_value(serde_json::to_value(fader).unwrap()).unwrap();
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
                    .map(|c| c.iter().map(|v| (0.0f32 + *v).to_bits()).collect())
                    .collect();
                // Original mix_graph accumulates this strip into a zero-initialized bus.
                if serde_json::to_value(&bits).unwrap() != case["bits"] {
                    failures.push(i);
                }
                let mut split: Vec<Vec<f32>> = input.iter().map(|c| c[..5].to_vec()).collect();
                fader.apply_bus(&mut split, start, rate).unwrap();
                let mut tail: Vec<Vec<f32>> = input.iter().map(|c| c[5..].to_vec()).collect();
                fader.apply_bus(&mut tail, start + 5, rate).unwrap();
                for (head, tail) in split.iter_mut().zip(tail) {
                    head.extend(tail);
                }
                assert_eq!(split, whole, "chunk case {i}");
            }
            assert!(failures.is_empty(), "PCM strip cases: {failures:?}");
        }
    }
}
