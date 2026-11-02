//! Source-clock stereo/5.1 clip audio and native-width sequence routing.
//! MIT attribution in
//! docs/licenses/filmcraft-MIT.txt; pinned source render/audio.rs.
use super::{
    composition::AnimatableValue,
    filmcraft_effect_scalar::{self, Evaluation},
    filmcraft_sequence_source::{SourceSequence, SourceTrack, SourceTrackItem},
    filmcraft_stereo_graph::{SourceStereoGraph, SourceStereoGraphRuntime},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
/// Media decoder boundary; clip context also selects nested multi-camera audio.
pub trait SourceAudioReader {
    fn read(
        &mut self,
        source: u64,
        clip: &Value,
        start: i64,
        frames: usize,
        rate: u32,
    ) -> Result<Option<Vec<Vec<f32>>>>;
    fn read_width(&mut self,source:u64,clip:&Value,start:i64,frames:usize,rate:u32,_width:usize)->Result<Option<Vec<Vec<f32>>>> {
        self.read(source,clip,start,frames,rate)
    }
}
impl<F> SourceAudioReader for F
where
    F: FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
{
    fn read(
        &mut self,
        source: u64,
        _clip: &Value,
        start: i64,
        frames: usize,
        rate: u32,
    ) -> Result<Option<Vec<Vec<f32>>>> {
        self(source, start, frames, rate)
    }
}
struct ReaderRef<'a, R: SourceAudioReader>(&'a mut R);
impl<R: SourceAudioReader> SourceAudioReader for ReaderRef<'_, R> {
    fn read(
        &mut self,
        source: u64,
        clip: &Value,
        start: i64,
        frames: usize,
        rate: u32,
    ) -> Result<Option<Vec<Vec<f32>>>> {
        self.0.read(source, clip, start, frames, rate)
    }
    fn read_width(&mut self,source:u64,clip:&Value,start:i64,frames:usize,rate:u32,width:usize)->Result<Option<Vec<Vec<f32>>>> {
        self.0.read_width(source,clip,start,frames,rate,width)
    }
}
const T: i128 = 254016000000;
fn tick(sample: i64, rate: u32) -> Result<i64> {
    // Lift to the first tick belonging to this sample. A floor/truncation
    // followed by sample() can move each block back one sample at rates that
    // do not divide T (and at negative positions).
    ensure!(rate > 0, "zero audio sample rate");
    Ok(i64::try_from(-(-(sample as i128 * T)).div_euclid(rate as i128))?)
}
fn sample(tick: i64, rate: u32) -> Result<i64> {
    Ok(i64::try_from((tick as i128 * rate as i128).div_euclid(T))?)
}

#[cfg(test)]
mod export_sample_clock_tests {
    #[test]
    fn sample_tick_lift_retains_positive_and_negative_indices() {
        for rate in [8000, 44100, 44117, 48000, 96000, 192000] {
            for index in [-1_000_001, -4096, -1, 0, 1, 4096, 1_000_001] {
                let tick = super::tick(index, rate).unwrap();
                assert_eq!(super::sample(tick, rate).unwrap(), index);
                assert_eq!(super::sample(tick - 1, rate).unwrap(), index - 1);
            }
        }
        assert!(super::tick(0, 0).is_err());
    }
}
fn gain(db: f64) -> f32 {
    if db <= -96. {
        0.
    } else {
        10f64.powf(db / 20.) as f32
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Clip {
    raw: Value,
    gain: f64,
    muted: bool,
    channels: Vec<u16>,
    volume: Option<AnimatableValue>,
    left: Option<AnimatableValue>,
    right: Option<AnimatableValue>,
    pan: Option<AnimatableValue>,
    #[serde(default)]
    remix: Option<RemixPlan>,
    #[serde(default)]
    effects: Option<super::filmcraft_clip_effects::SourceClipEffects>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RemixPlan {
    xfade: i64,
    pieces: Vec<(i64, i64)>,
}
impl RemixPlan {
    fn read(
        &self,
        id: u64,
        clip: &Value,
        relative: i64,
        frames: usize,
        rate: u32,
        width:usize,
        read: &mut impl SourceAudioReader,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(
            !self.pieces.is_empty() && self.pieces.len() <= 65536,
            "invalid remix piece budget"
        );
        let mut position = 0i64;
        let mut pieces = Vec::with_capacity(self.pieces.len());
        for &(source, length) in &self.pieces {
            let first = sample(position, rate)?;
            position = position
                .checked_add(length)
                .context("remix timeline overflow")?;
            let end = sample(position, rate)?;
            let source = sample(source, rate)?;
            ensure!(
                [first, end, source]
                    .into_iter()
                    .all(|v| v.unsigned_abs() <= (i64::MAX / 4) as u64),
                "remix clock bounds"
            );
            pieces.push(filmcraft_audio_dsp::remix::Piece {
                src: source,
                len: end - first,
            });
        }
        let xfade = sample(self.xfade, rate)?;
        ensure!(
            xfade.unsigned_abs() <= 4_194_304,
            "remix crossfade exceeds input budget"
        );
        let probe = read.read_width(id, clip, pieces[0].src, 1, rate,width)?;
        let channels = probe.as_ref().map_or(2, Vec::len).max(1);
        ensure!(channels <= 16, "invalid remix source channels");
        let plan = filmcraft_audio_dsp::remix::Plan { pieces, xfade };
        let mut failure = None;
        let output = filmcraft_audio_dsp::remix::render(
            &plan,
            relative,
            frames,
            channels,
            &mut |source, len| {
                let result = (|| -> Result<Vec<Vec<f32>>> {
                    ensure!(len <= 4_194_304, "remix source request exceeds budget");
                    let mut bus = read.read_width(id, clip, source, len, rate,width)?.unwrap_or_default();
                    ensure!(
                        bus.len() <= 16
                            && bus
                                .iter()
                                .all(|c| c.len() <= len && c.iter().all(|v| v.is_finite())),
                        "invalid remix source PCM"
                    );
                    bus.resize(channels, Vec::new());
                    for channel in &mut bus {
                        channel.resize(len, 0.);
                    }
                    Ok(bus)
                })();
                match result {
                    Ok(bus) => bus,
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                        vec![vec![0.; len.min(4_194_304)]; channels]
                    }
                }
            },
        );
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(output)
    }
}
impl Clip {
    fn compile(item: &SourceTrackItem<'_>) -> Result<Self> {
        let effects = item
            .raw
            .get("effects")
            .and_then(Value::as_array)
            .context("missing clip effects")?;
        let remix = if effects
            .iter()
            .any(|e| e["effect"] == "remix" && e["enabled"].as_bool().unwrap_or(true))
        {
            effects
                .iter()
                .find(|e| e["effect"] == "remix")
                .and_then(|e| e.pointer("/params/plan/value/Text"))
                .and_then(Value::as_str)
                .and_then(filmcraft_audio_dsp::remix::Plan::from_text)
                .map(|plan| RemixPlan {
                    xfade: plan.xfade,
                    pieces: plan.pieces.iter().map(|p| (p.src, p.len)).collect(),
                })
        } else {
            None
        };
        let parameter = |id: &str, key: &str, bypass: bool| -> Result<Option<AnimatableValue>> {
            let effect = effects.iter().find(|e| e["effect"] == id);
            let Some(effect) = effect.filter(|e| {
                e["enabled"].as_bool().unwrap_or(true)
                    && (!bypass
                        || e.pointer("/params/bypass/value/Bool")
                            .and_then(Value::as_bool)
                            != Some(true))
            }) else {
                return Ok(None);
            };
            Ok(Some(match effect.get("params").and_then(|p| p.get(key)) {
                Some(param) => {
                    filmcraft_effect_scalar::convert(param, Evaluation::EffectParameter)?
                }
                None => AnimatableValue::Constant { value: 0. },
            }))
        };
        let gain = item
            .raw
            .get("gain_db")
            .and_then(Value::as_f64)
            .unwrap_or(0.);
        ensure!(
            gain.is_finite() && crate::domain::filmcraft_clip_audio::gain(gain).is_finite(),
            "invalid clip gain"
        );
        let channels = item
            .raw
            .get("source_channels")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            remix,
            effects: Some(super::filmcraft_clip_effects::SourceClipEffects::compile(
                item,
            )?),
            raw: item.raw.clone(),
            gain,
            muted: item
                .raw
                .pointer("/essential/mute")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            channels,
            volume: parameter("volume", "level", true)?,
            left: parameter("channel_volume", "left", true)?,
            right: parameter("channel_volume", "right", true)?,
            pan: parameter("panner", "balance", false)?,
        })
    }
    fn raw(
        &self,
        start: i64,
        frames: usize,
        rate: u32,
        width: usize,
        read: &mut impl SourceAudioReader,
    ) -> Result<Vec<Vec<f32>>> {
        let item = SourceTrackItem::read(&self.raw)?;
        let mut output = vec![vec![0.; frames]; width];
        let a0 = start.max(sample(item.timing.timeline_start, rate)?);
        let a1 = start
            .checked_add(frames as i64)
            .context("raw clip range overflow")?
            .min(sample(item.end().context("raw clip end overflow")?, rate)?);
        if a1 <= a0 {
            return Ok(output);
        }
        let n = usize::try_from(a1 - a0)?;
        let offset = usize::try_from(a0 - start)?;
        let speed = item.timing.speed.abs().max(1e-6);
        let source = sample(item.clocks(tick(a0, rate)?)?.frame, rate)?;
        let remixed = self
            .remix
            .as_ref()
            .map(|plan| {
                plan.read(
                    item.source_id,
                    &self.raw,
                    a0 - sample(item.timing.timeline_start, rate)?,
                    n,
                    rate,
                    width,
                    read,
                )
            })
            .transpose()?;
        let direct = remixed.is_some() || (speed - 1.).abs() < 1e-9 && !item.timing.reverse;
        let span = if direct {
            n
        } else {
            let span = (n as f64 * speed).ceil();
            ensure!(
                span.is_finite() && span <= 4_194_302.,
                "source varispeed input exceeds budget"
            );
            span as usize + 2
        };
        let source = if !direct && item.timing.reverse {
            source
                .checked_sub(span as i64)
                .context("reverse audio clock overflow")?
        } else {
            source
        };
        let raw = match remixed {
            Some(raw) => raw,
            None => match read.read_width(item.source_id, &self.raw, source, span, rate,width)? {
                Some(raw) => raw,
                None => return Ok(output),
            },
        };
        if raw.is_empty() {
            return Ok(output);
        }
        ensure!(
            raw.len() <= 16
                && raw
                    .iter()
                    .all(|c| c.len() <= span && c.iter().all(|v| v.is_finite())),
            "invalid source PCM"
        );
        let channel = |c: u16| {
            if usize::from(c) < raw.len() {
                usize::from(c)
            } else {
                0
            }
        };
        let picked = if width==2 {
            match self.channels.as_slice() {
                [] => vec![0,usize::from(raw.len()>1)],
                [mono] => vec![channel(*mono);2],
                [l,r,..] => vec![channel(*l),channel(*r)],
            }
        } else {
            match self.channels.len() {
                0 => match raw.len(){0|1=>vec![0],2..=5=>vec![0,1],_=>(0..6).collect()},
                1 => vec![channel(self.channels[0])],
                2..=5 => vec![channel(self.channels[0]),channel(self.channels[1])],
                _ => self.channels.iter().take(6).map(|c|channel(*c)).collect(),
            }
        };
        let mut buffer = vec![vec![0.; n]; picked.len()];
        for (out, &c) in buffer.iter_mut().zip(&picked) {
            let input = &raw[c];
            for (i, out) in out.iter_mut().enumerate() {
                *out = if direct {
                    input.get(i).copied().unwrap_or(0.)
                } else {
                    let pos = if item.timing.reverse {
                        span as f64 - 1. - i as f64 * speed
                    } else {
                        i as f64 * speed
                    };
                    let i0 = pos.floor().max(0.) as usize;
                    let fraction = (pos - i0 as f64) as f32;
                    let a = input.get(i0).copied().unwrap_or(0.);
                    let b = input.get(i0 + 1).copied().unwrap_or(a);
                    a + (b - a) * fraction
                };
            }
        }
        let buffer=if width==6&&picked.len()!=6 {
            use filmcraft_audio_dsp::channels::{convert,Layout,Mixdown};
            convert(&buffer.iter().map(Vec::as_slice).collect::<Vec<_>>(),Layout::from_channels(picked.len()),Layout::Surround51,Mixdown::FrontRear)
        }else{buffer};
        for (out, channel) in output.iter_mut().zip(buffer) {
            out[offset..offset + n].copy_from_slice(&channel);
        }
        Ok(output)
    }
    fn mix(
        &self,
        start: i64,
        frames: usize,
        rate: u32,
        out: &mut [Vec<f32>],
        read: &mut impl SourceAudioReader,
        states: &mut ClipStates,
    ) -> Result<()> {
        if self.muted {
            return Ok(());
        }
        let width=out.len();
        let item = SourceTrackItem::read(&self.raw)?;
        let a0 = start.max(sample(item.timing.timeline_start, rate)?);
        let a1 = start
            .checked_add(frames as i64)
            .context("audio range overflow")?
            .min(sample(item.end().context("clip end overflow")?, rate)?);
        if a1 <= a0 {
            return Ok(());
        }
        let n = usize::try_from(a1 - a0)?;
        let offset = usize::try_from(a0 - start)?;
        let buffer = if let Some(plan) = self.effects.as_ref().filter(|plan| !plan.is_empty()) {
            // The decoder callback uses None for unavailable/non-audio sources.
            if read.read_width(item.source_id, &self.raw, 0, 0, rate,width)?.is_none() {
                return Ok(());
            }
            let key = (item.id, rate);
            if !states.contains_key(&key) {
                states.insert(
                    key,
                    super::filmcraft_clip_effects::SourceClipEffectsRuntime::new(
                        plan.clone(),
                        rate,
                        width,
                    )?,
                );
            }
            states
                .get_mut(&key)
                .unwrap()
                .render(a0, n, |start, count| self.raw(start, count, rate, width, read))?
        } else {
            self.raw(a0, n, rate, width, read)?
        };
        let value = |curve: &Option<AnimatableValue>, t: i64, default: f64| {
            curve
                .as_ref()
                .map_or(default, |c| filmcraft_effect_scalar::sample(c, t))
        };
        let mut block = 0;
        while block < n {
            let position = a0 + block as i64;
            let grid = position.div_euclid(64) * 64;
            // Original clip intrinsics use source_time_at, including frame hold,
            // independently of the Hold Filters switch used by other effects.
            let t = item.clocks(tick(grid, rate)?)?.frame;
            let g = gain(self.gain) * gain(value(&self.volume, t, 0.));
            let cl = gain(value(&self.left, t, 0.));
            let cr = gain(value(&self.right, t, 0.));
            let (pl, pr) = if self.pan.is_some() {
                let pan = (value(&self.pan, t, 0.) / 100.) as f32;
                let angle = (pan + 1.) * std::f32::consts::FRAC_PI_4;
                (
                    (angle.cos() * std::f32::consts::SQRT_2).min(1.),
                    (angle.sin() * std::f32::consts::SQRT_2).min(1.),
                )
            } else {
                (1., 1.)
            };
            let end = (block + 64 - position.rem_euclid(64) as usize).min(n);
            if width==2 {
                for i in block..end {
                    out[0][offset+i]+=buffer[0][i]*g*cl*pl;
                    out[1][offset+i]+=buffer[1][i]*g*cr*pr;
                }
            } else {
                // Original 5.1 intrinsics: Channel Volume affects L/R only;
                // clip Panner is ignored. Preserve source multiplication order.
                for(c,(dst,src))in out.iter_mut().zip(&buffer).enumerate(){
                    let gc=g*match c {0=>cl,1=>cr,_=>1.};
                    for i in block..end {dst[offset+i]+=src[i]*gc;}
                }
            }
            block = end;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Transition {
    start: i64,
    end: i64,
    from: Option<u64>,
    to: Option<u64>,
    effect: String,
}
type ClipStates = BTreeMap<(u64, u32), super::filmcraft_clip_effects::SourceClipEffectsRuntime>;
fn stereo_width()->usize{2}
#[derive(Clone, Serialize, Deserialize)]
pub struct SourceTrackAudio {
    #[serde(default="stereo_width")]
    width:usize,
    clips: Vec<Clip>,
    transitions: Vec<Transition>,
}
impl SourceTrackAudio {
    pub fn compile(track: &SourceTrack<'_>) -> Result<Self> {
        let width=if track.raw["channels"].as_str()==Some("Surround51"){6}else{2};
        let clips = track
            .items
            .iter()
            .map(Clip::compile)
            .collect::<Result<_>>()?;
        let transitions = track
            .raw
            .get("transitions")
            .map(|v| v.as_array().context("invalid source transitions"))
            .transpose()?
            .into_iter()
            .flatten()
            .map(|v| {
                let start = v["start"].as_i64().context("invalid transition start")?;
                Ok(Transition {
                    start,
                    end: start
                        .checked_add(
                            v["duration"]
                                .as_i64()
                                .context("invalid transition duration")?,
                        )
                        .context("transition end overflow")?,
                    from: v["from"].as_u64(),
                    to: v["to"].as_u64(),
                    effect: v
                        .pointer("/effect/effect")
                        .and_then(Value::as_str)
                        .context("invalid transition effect")?
                        .into(),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { width, clips, transitions })
    }
    pub fn render(
        &self,
        start: i64,
        frames: usize,
        rate: u32,
        read: impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_with_states(start, frames, rate, &mut BTreeMap::new(), read)
    }
    fn render_with_states(
        &self,
        start: i64,
        frames: usize,
        rate: u32,
        states: &mut ClipStates,
        mut read: impl SourceAudioReader,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(
            (8000..=192000).contains(&rate) && frames <= 4_194_304 && matches!(self.width,2|6),
            "invalid source audio request"
        );
        let end = start
            .checked_add(frames as i64)
            .context("source audio range overflow")?;
        let mut output = vec![vec![0.; frames]; self.width];
        for clip in &self.clips {
            if SourceTrackItem::read(&clip.raw)?.enabled {
                clip.mix(start, frames, rate, &mut output, &mut read, states)?
            }
        }
        for transition in &self.transitions {
            let first = sample(transition.start, rate)?;
            let last = sample(transition.end, rate)?;
            let a0 = first.max(start);
            let a1 = last.min(end);
            if a1 <= a0 {
                continue;
            }
            let n = usize::try_from(a1 - a0)?;
            let offset = usize::try_from(a0 - start)?;
            let mut a = vec![vec![0.; n]; self.width];
            let mut b = a.clone();
            for (id, bus) in [(transition.from, &mut a), (transition.to, &mut b)] {
                if let Some(clip) = self
                    .clips
                    .iter()
                    .find(|c| c.raw["id"].as_u64() == id && id.is_some())
                {
                    clip.mix(a0, n, rate, bus, &mut read, states)?
                }
            }
            let duration = sample(transition.end - transition.start, rate)?.max(1) as f32;
            let base = (a0 - first) as f32;
            for i in 0..n {
                let p = (base + i as f32) / duration;
                let (ga, gb) = match transition.effect.as_str() {
                    "constant_power" => (
                        (p * std::f32::consts::FRAC_PI_2).cos(),
                        (p * std::f32::consts::FRAC_PI_2).sin(),
                    ),
                    "exponential_fade" => ((1. - p).powi(3), 1. - (1. - p).powi(3)),
                    "constant_gain" => (1. - p, p),
                    _ => (
                        (p * std::f32::consts::FRAC_PI_2).cos(),
                        (p * std::f32::consts::FRAC_PI_2).sin(),
                    ),
                };
                for c in 0..self.width {
                    output[c][offset + i] = 0.;
                    output[c][offset + i] += a[c][i] * ga + b[c][i] * gb;
                }
            }
        }
        Ok(output)
    }
}
/// Per-track clip DSP state, retained across source graph requests.
pub struct SourceTrackAudioRuntime {
    plan: SourceTrackAudio,
    states: ClipStates,
}
impl SourceTrackAudioRuntime {
    pub fn new(plan: SourceTrackAudio) -> Self {
        Self {
            plan,
            states: BTreeMap::new(),
        }
    }
    pub fn render(
        &mut self,
        start: i64,
        frames: usize,
        rate: u32,
        read: impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_with_reader(start, frames, rate, read)
    }
    fn render_with_reader(
        &mut self,
        start: i64,
        frames: usize,
        rate: u32,
        read: impl SourceAudioReader,
    ) -> Result<Vec<Vec<f32>>> {
        self.plan
            .render_with_states(start, frames, rate, &mut self.states, read)
    }
}
/// Connect source clip media reads to the stateful track/submix/master graph.
pub struct SourceSequenceAudioRuntime {
    rate: u32,
    inputs: BTreeMap<u64, SourceTrackAudioRuntime>,
    graph: SourceStereoGraphRuntime,
}
impl SourceSequenceAudioRuntime {
    pub fn new(sequence: &SourceSequence<'_>) -> Result<Self> {
        let rate = u32::try_from(
            sequence
                .raw
                .pointer("/settings/sample_rate")
                .and_then(Value::as_u64)
                .context("missing sequence sample rate")?,
        )?;
        let inputs = sequence
            .audio_tracks
            .iter()
            .map(|track| {
                Ok((
                    track.id,
                    SourceTrackAudioRuntime::new(SourceTrackAudio::compile(track)?),
                ))
            })
            .collect::<Result<_>>()?;
        let graph=SourceStereoGraph::compile(sequence)?;
        let graph=SourceStereoGraphRuntime::new(graph,rate)?;
        Ok(Self {
            rate,
            inputs,
            graph,
        })
    }
    pub fn render(
        &mut self,
        start: i64,
        frames: usize,
        read: impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_with_reader(start, frames, read)
    }
    pub fn render_with_reader(
        &mut self,
        start: i64,
        frames: usize,
        read: impl SourceAudioReader,
    ) -> Result<Vec<Vec<f32>>> {
        let bus=self.render_native_with_reader(start,frames,read)?;
        Ok(filmcraft_audio_dsp::channels::convert_owned(bus,filmcraft_audio_dsp::channels::Layout::Stereo,filmcraft_audio_dsp::channels::Mixdown::FrontRear))
    }
    pub fn render_native_with_reader(&mut self,start:i64,frames:usize,mut read:impl SourceAudioReader)->Result<Vec<Vec<f32>>> {
        self.graph.render(start, frames, |id, start, frames| {
            self.inputs
                .get_mut(&id)
                .context("missing source track input")?
                .render_with_reader(start, frames, self.rate, ReaderRef(&mut read))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_original_clip_audio_input_pcm_matches() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-clip-audio-input.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1080);
        let mut failures = vec![];
        for (case_index, case) in cases.iter().enumerate() {
            let raw = &case["track"];
            let track = SourceTrack {
                id: raw["id"].as_u64().unwrap(),
                raw,
                items: raw["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(SourceTrackItem::read)
                    .collect::<Result<_>>()
                    .unwrap(),
            };
            let plan = SourceTrackAudio::compile(&track).unwrap();
            let plan: SourceTrackAudio =
                serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                let output = plan
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        case["rate"].as_u64().unwrap() as u32,
                        |id, start, frames, _rate| {
                            assert_eq!(Some(id), case["source"].as_u64());
                            Ok(Some(
                                (0..4)
                                    .map(|c| {
                                        (0..frames)
                                            .map(|i| {
                                                let n = start + i as i64;
                                                if n < 0 {
                                                    return 0.;
                                                }
                                                match c {
                                                    0 => ((n % 31) - 15) as f32 / 32.,
                                                    1 => ((n % 23) - 7) as f32 / 64.,
                                                    2 => ((n % 17) - 5) as f32 / 16.,
                                                    _ => ((n % 19) - 13) as f32 / 128.,
                                                }
                                            })
                                            .collect()
                                    })
                                    .collect(),
                            ))
                        },
                    )
                    .unwrap();
                let bits: Vec<Vec<u32>> = output
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != request["bits"] {
                    failures.push((case_index, request_index));
                }
            }
        }
        assert!(failures.is_empty(), "source clip PCM: {failures:?}");
    }
    #[test]
    fn actual_original_clip_effects_pcm_and_continuation_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-clip-effects.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 440);
        let mut failures = vec![];
        for (case_index, case) in cases.iter().enumerate() {
            let raw = &case["track"];
            let track = SourceTrack {
                id: raw["id"].as_u64().unwrap(),
                raw,
                items: raw["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(SourceTrackItem::read)
                    .collect::<Result<_>>()
                    .unwrap(),
            };
            let plan = SourceTrackAudio::compile(&track).unwrap();
            let plan: SourceTrackAudio =
                serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
            let mut plan = SourceTrackAudioRuntime::new(plan);
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                let output = plan
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        case["rate"].as_u64().unwrap() as u32,
                        |id, start, frames, _rate| {
                            assert_eq!(Some(id), case["source"].as_u64());
                            Ok(Some(
                                (0..2)
                                    .map(|c| {
                                        (0..frames)
                                            .map(|i| {
                                                let n = start + i as i64;
                                                if n < 0 {
                                                    return 0.;
                                                }
                                                match c {
                                                    0 => ((n % 31) - 15) as f32 / 32.,
                                                    1 => ((n % 23) - 7) as f32 / 64.,
                                                    2 => ((n % 17) - 5) as f32 / 16.,
                                                    _ => ((n % 19) - 13) as f32 / 128.,
                                                }
                                            })
                                            .collect()
                                    })
                                    .collect(),
                            ))
                        },
                    )
                    .unwrap();
                let bits: Vec<Vec<u32>> = output
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != request["bits"] {
                    failures.push((case_index, request_index));
                }
            }
        }
        assert!(failures.is_empty(), "source clip PCM: {failures:?}");
    }
    #[test]
    fn actual_original_clip_remix_input_pcm_matches() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-clip-remix-input.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1080);
        let mut failures = vec![];
        for (case_index, case) in cases.iter().enumerate() {
            let raw = &case["track"];
            let track = SourceTrack {
                id: raw["id"].as_u64().unwrap(),
                raw,
                items: raw["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(SourceTrackItem::read)
                    .collect::<Result<_>>()
                    .unwrap(),
            };
            let plan = SourceTrackAudio::compile(&track).unwrap();
            let plan: SourceTrackAudio =
                serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                let output = plan
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        case["rate"].as_u64().unwrap() as u32,
                        |id, start, frames, _rate| {
                            assert_eq!(Some(id), case["source"].as_u64());
                            Ok(Some(
                                (0..4)
                                    .map(|c| {
                                        (0..frames)
                                            .map(|i| {
                                                let n = start + i as i64;
                                                if n < 0 {
                                                    return 0.;
                                                }
                                                match c {
                                                    0 => ((n % 31) - 15) as f32 / 32.,
                                                    1 => ((n % 23) - 7) as f32 / 64.,
                                                    2 => ((n % 17) - 5) as f32 / 16.,
                                                    _ => ((n % 19) - 13) as f32 / 128.,
                                                }
                                            })
                                            .collect()
                                    })
                                    .collect(),
                            ))
                        },
                    )
                    .unwrap();
                let bits: Vec<Vec<u32>> = output
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != request["bits"] {
                    failures.push((case_index, request_index));
                }
            }
        }
        assert!(failures.is_empty(), "source clip PCM: {failures:?}");
    }
    #[test]
    fn source_clip_provider_feeds_original_full_graph_pcm() {
        use super::super::{
            filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
        };
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-stereo-insert-graph.json"
        ))
        .unwrap();
        for case in fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| matches!(c["effect"].as_str(), Some("amplify" | "delay")))
        {
            let doc = SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                .unwrap();
            let index = ProjectIndex::new(&doc).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            let sources: Vec<_> = sequence
                .audio_tracks
                .iter()
                .map(|t| t.items[0].source_id)
                .collect();
            let mut runtime = SourceSequenceAudioRuntime::new(&sequence).unwrap();
            let output = runtime
                .render(
                    case["start"].as_i64().unwrap(),
                    257,
                    |id, start, frames, _| {
                        let side = sources.iter().position(|&source| source == id).unwrap() as i64;
                        Ok(Some(
                            (0..2)
                                .map(|channel| {
                                    (0..frames)
                                        .map(|i| {
                                            let n = start + i as i64;
                                            if n < 0 {
                                                0.
                                            } else if channel == 0 {
                                                (((n + side * 7) % 31) - 15) as f32 / 32.
                                            } else {
                                                (((n + side * 11) % 23) - 7) as f32 / 64.
                                            }
                                        })
                                        .collect()
                                })
                                .collect(),
                        ))
                    },
                )
                .unwrap();
            let bits: Vec<Vec<u32>> = output
                .iter()
                .map(|c| c.iter().map(|v| v.to_bits()).collect())
                .collect();
            assert_eq!(
                serde_json::to_value(bits).unwrap(),
                case["bits"],
                "source input graph {} {} {}",
                case["effect"],
                case["host"],
                case["post"]
            );
        }
    }
}
