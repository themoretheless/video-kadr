//! Filmcraft mixer output and send routing, before route compensation delays.
//! MIT: render/src/mixer.rs pan51/convert_width/run_strip and project/src/mixer.rs
//! at 5231852443363f001c3f6b396dd9b1e6461ae2be. Graph scheduling/inserts remain callers' work.
use super::{
    composition::AnimatableValue,
    filmcraft_effect_scalar::{convert, sample, Evaluation},
    filmcraft_master_volume::{AutomationMode, SourceMasterVolume},
};
use anyhow::{ensure, Context, Result};
use filmcraft_audio_dsp::channels::{self, Layout, Mixdown, Pan51};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const T: i128 = 254_016_000_000;
const KEYS: [&str; 4] = ["pan51.x", "pan51.y", "pan51.center", "pan51.lfe"];
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum SourceLayout {
    Mono,
    Stereo,
    Surround51,
}
impl SourceLayout {
    fn read(track: &Value) -> Result<Self> {
        Ok(
            match track
                .get("channels")
                .and_then(Value::as_str)
                .unwrap_or("Stereo")
            {
                "Mono" => Self::Mono,
                "Stereo" | "Adaptive" => Self::Stereo,
                "Surround51" => Self::Surround51,
                _ => anyhow::bail!("invalid source strip layout"),
            },
        )
    }
    fn native(self) -> Layout {
        match self {
            Self::Mono => Layout::Mono,
            Self::Stereo => Layout::Stereo,
            Self::Surround51 => Layout::Surround51,
        }
    }
    fn width(self) -> usize {
        if self == Self::Surround51 {
            6
        } else {
            2
        }
    }
    fn check(self, bus: &[Vec<f32>]) -> Result<usize> {
        ensure!(
            bus.len() == self.width(),
            "source strip channel count mismatch"
        );
        let n = bus[0].len();
        ensure!(
            bus.iter()
                .all(|c| c.len() == n && c.iter().all(|s| s.is_finite())),
            "invalid source routing bus"
        );
        Ok(n)
    }
    fn to_width(self, bus: &[Vec<f32>], width: usize) -> Result<Vec<Vec<f32>>> {
        let n = self.check(bus)?;
        ensure!(matches!(width, 2 | 6), "invalid target bus width");
        if bus.len() == width {
            return Ok(bus.to_vec());
        }
        if self == Self::Mono && width == 6 {
            let mut out = vec![vec![0.; n]; 6];
            out[channels::C] = bus[0].clone();
            return Ok(out);
        }
        Ok(channels::convert(
            &bus.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            Layout::from_channels(bus.len()),
            Layout::from_channels(width),
            Mixdown::FrontRear,
        ))
    }
}
fn scalar(value: &Value) -> Option<f64> {
    value
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
}
fn lane(track: &Value, key: &str, default: f64, stored_value: bool) -> Result<AnimatableValue> {
    let mode: AutomationMode = track
        .pointer("/mixer/mode")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?
        .unwrap_or_default();
    let source = track.pointer("/mixer/lanes").and_then(|v| v.get(key));
    if mode.reads()
        && source
            .and_then(|v| v.get("keyframes"))
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
    {
        return convert(source.unwrap(), Evaluation::MixerLane);
    }
    let value = if stored_value {
        source
            .and_then(|v| v.get("value"))
            .and_then(scalar)
            .unwrap_or(default)
    } else {
        default
    };
    ensure!(value.is_finite(), "nonfinite routing lane");
    Ok(AnimatableValue::Constant { value })
}
fn tick(start: i64, index: usize, rate: u32) -> Result<i64> {
    ensure!(rate > 0, "zero routing sample rate");
    Ok(i64::try_from(
        (i128::from(start) + index as i128) * T / i128::from(rate),
    )?)
}
fn gain(db: f64) -> f32 {
    if db <= -96. {
        0.
    } else {
        10f64.powf(db / 20.) as f32
    }
}
fn balance_gains(p: f32, mono: bool) -> [f32; 2] {
    let a = (p.clamp(-1., 1.) + 1.) * std::f32::consts::FRAC_PI_4;
    let (l, r) = if mono {
        (a.cos(), a.sin())
    } else {
        (
            (a.cos() * std::f32::consts::SQRT_2).min(1.),
            (a.sin() * std::f32::consts::SQRT_2).min(1.),
        )
    };
    [l, r]
}
fn balance(bus: &mut [Vec<f32>], p: f32, mono: bool, index: usize) {
    let [l, r] = balance_gains(p, mono);
    bus[0][index] *= l;
    bus[1][index] *= r;
}
fn constant_balance(bus: &mut [Vec<f32>], p: f32, mono: bool) {
    for (channel, gain) in bus.iter_mut().zip(balance_gains(p, mono)) {
        if gain != 1. {
            for sample in channel {
                *sample *= gain;
            }
        }
    }
}

/// Post-insert strip output: position to 5.1, or BS.775 fold then stereo balance.
/// Mono input is the already folded two-channel strip bus, as in the source mixer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceSurroundRoute {
    layout: SourceLayout,
    panner: [AnimatableValue; 4],
    pan: AnimatableValue,
}
impl SourceSurroundRoute {
    pub fn read(track: &Value) -> Result<Self> {
        let layout = SourceLayout::read(track)?;
        let defaults = [
            0.,
            100.,
            100.,
            if layout == SourceLayout::Surround51 {
                0.
            } else {
                -96.
            },
        ];
        let panner = KEYS
            .iter()
            .zip(defaults)
            .map(|(key, value)| lane(track, key, value, true))
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| anyhow::anyhow!("panner lane count"))?;
        let pan = lane(
            track,
            "pan",
            track.get("pan").and_then(Value::as_f64).unwrap_or(0.),
            false,
        )?;
        Ok(Self {
            layout,
            panner,
            pan,
        })
    }
    /// `start` is the latency-compensated content clock, not the input clock.
    pub fn output(
        &self,
        bus: &[Vec<f32>],
        width: usize,
        start: i64,
        rate: u32,
    ) -> Result<Vec<Vec<f32>>> {
        let n = self.layout.check(bus)?;
        tick(start, 0, rate)?;
        tick(start, n, rate)?;
        ensure!(matches!(width, 2 | 6), "invalid target bus width");
        if width == 2 {
            let mut out = self.layout.to_width(bus, 2)?;
            if let AnimatableValue::Constant { value } = &self.pan {
                constant_balance(
                    &mut out,
                    *value as f32 / 100.,
                    self.layout == SourceLayout::Mono,
                );
                return Ok(out);
            }
            for i in 0..n {
                balance(
                    &mut out,
                    sample(&self.pan, tick(start, i, rate)?) as f32 / 100.,
                    self.layout == SourceLayout::Mono,
                    i,
                );
            }
            return Ok(out);
        }
        let input = if self.layout == SourceLayout::Mono {
            vec![bus[0].as_slice()]
        } else {
            bus.iter().map(Vec::as_slice).collect()
        };
        let matrix = |values: [f64; 4]| {
            channels::pan51_matrix(
                self.layout.native(),
                Pan51 {
                    x: (values[0] / 100.) as f32,
                    y: (values[1] / 100.) as f32,
                    center: (values[2] / 100.) as f32,
                    lfe: gain(values[3]),
                },
            )
        };
        if self
            .panner
            .iter()
            .all(|v| matches!(v, AnimatableValue::Constant { .. }))
        {
            return Ok(channels::apply_matrix(
                &matrix(std::array::from_fn(|k| sample(&self.panner[k], 0))),
                &input,
            ));
        }
        let mut out = vec![vec![0.; n]; 6];
        for i in 0..n {
            let time = tick(start, i, rate)?;
            let m = matrix(std::array::from_fn(|k| sample(&self.panner[k], time)));
            for (o, row) in m.iter().enumerate() {
                let mut acc = 0f32;
                for (g, x) in row.iter().zip(&input) {
                    acc += g * x[i];
                }
                out[o][i] = acc;
            }
        }
        Ok(out)
    }
}

/// A send's gain and width conversion. Unlike strip output it never uses pan51.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceSurroundSend {
    pub target: u64,
    pub pre_fader: bool,
    layout: SourceLayout,
    muted: bool,
    level: SourceMasterVolume,
    pan: f64,
}
impl SourceSurroundSend {
    pub fn read(track: &Value, slot: usize) -> Result<Option<Self>> {
        ensure!(slot < 5, "send slot exceeds source limit");
        let Some(send) = track
            .pointer("/mixer/sends")
            .and_then(Value::as_array)
            .and_then(|v| v.get(slot))
        else {
            return Ok(None);
        };
        let mut mixer = track.get("mixer").cloned().unwrap_or(json!({}));
        mixer["lanes"] = track
            .pointer("/mixer/lanes")
            .and_then(|v| v.get(format!("send.{slot}.level")))
            .map(|v| json!({"volume":v}))
            .unwrap_or(json!({}));
        let level = SourceMasterVolume::read(
            &json!({"master_volume_db":send.get("level_db").cloned().unwrap_or(json!(0.)),"master_mixer":mixer}),
        )?;
        let flag = |key: &str| -> Result<bool> {
            Ok(send
                .get(key)
                .map(|v| v.as_bool().context("invalid send flag"))
                .transpose()?
                .unwrap_or(false))
        };
        let pan = send
            .get("pan")
            .map(|v| v.as_f64().context("invalid send pan"))
            .transpose()?
            .unwrap_or(0.);
        ensure!(pan.is_finite(), "nonfinite send pan");
        Ok(Some(Self {
            target: send["target"].as_u64().context("invalid send target")?,
            pre_fader: flag("pre_fader")?,
            layout: SourceLayout::read(track)?,
            muted: flag("muted")?,
            level,
            pan,
        }))
    }
    pub fn render(
        &self,
        bus: &[Vec<f32>],
        width: usize,
        start: i64,
        rate: u32,
    ) -> Result<Vec<Vec<f32>>> {
        let n = self.layout.check(bus)?;
        ensure!(matches!(width, 2 | 6), "invalid target bus width");
        tick(start, 0, rate)?;
        tick(start, n, rate)?;
        if self.muted {
            return Ok(vec![vec![0.; n]; width]);
        }
        let mut signal = bus.to_vec();
        self.level.apply_bus(&mut signal, start, rate)?;
        let mut out = self.layout.to_width(&signal, width)?;
        if width == 2 {
            constant_balance(&mut out, (self.pan / 100.) as f32, false);
        }
        Ok(out)
    }
}
