//! Original Enhance Voice graph, processed on canonical analysis hops.
use super::{
    concat_compressor::Compressor, concat_deesser::Deesser, concat_denoise::Denoiser,
    concat_limiter::LookaheadLimiter, concat_tone_filter::Shelf,
};
pub(crate) struct EnhanceVoice {
    highpass: Shelf,
    denoise: Denoiser,
    deesser: Deesser,
    compressor: Compressor,
    limiter: LookaheadLimiter,
    frames: usize,
}
impl EnhanceVoice {
    pub(crate) fn new(rate: u32, channels: usize, amount: f32) -> Self {
        let amount = f64::from(amount);
        let fixed = |v: f64, digits: usize| format!("{v:.digits$}").parse::<f64>().unwrap();
        Self {
            highpass: Shelf::highpass(rate, channels, 80.).expect("admitted sample rate"),
            denoise: Denoiser::new(
                rate,
                channels,
                fixed(-25. - amount / 100. * 20., 1),
                fixed(6. + amount / 100. * 18., 1),
            ),
            deesser: Deesser::new(rate, channels, fixed(amount / 100. * 0.5, 2), 0.5, 0.5),
            compressor: Compressor::new(
                rate,
                fixed(0.5 - amount / 100. * 0.3, 2),
                fixed(2. + amount / 100. * 2., 1),
            ),
            limiter: LookaheadLimiter::new(rate, channels, 0.95, true),
            frames: 0,
        }
    }
    pub(crate) fn hop(&self) -> usize {
        self.denoise.hop()
    }
    pub(crate) fn reset(&mut self) {
        self.highpass.reset();
        self.denoise.reset();
        self.deesser.reset();
        self.compressor.reset();
        self.limiter.reset();
        self.frames = 0;
    }
    pub(crate) fn process_hop(&mut self, input: &[Vec<f64>], final_hop: bool) -> Vec<Vec<f64>> {
        self.process_hop_with_odd(input, final_hop, &[])
    }
    fn process_hop_with_odd(
        &mut self,
        input: &[Vec<f64>],
        final_hop: bool,
        odd_input: &[usize],
    ) -> Vec<Vec<f64>> {
        let count = input[0].len();
        let channels = input.len();
        let mut filtered = input.to_vec();
        for ch in 0..channels {
            for i in 0..count {
                let odd = odd_input.contains(&(self.frames + i + 1))
                    || (final_hop && i + 1 == count && (self.frames + count) % 2 != 0);
                filtered[ch][i] = self.highpass.process(ch, filtered[ch][i], true, odd);
            }
        }
        let mut output = self.denoise.process_hop(&filtered);
        let mut frame = [0.; 16];
        for i in 0..count {
            for ch in 0..channels {
                frame[ch] = output[ch][i];
            }
            self.deesser.process_frame(&mut frame[..channels]);
            self.compressor.process_frame(&mut frame[..channels]);
            self.limiter.process_frame(&mut frame[..channels]);
            for ch in 0..channels {
                output[ch][i] = frame[ch];
            }
        }
        self.frames += count;
        output
    }
}

use filmcraft_audio_dsp::{AudioEffect, Category, EffectInfo, ParamSpec, Unit};
use std::collections::VecDeque;
const PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "amount",
    "Amount",
    0.,
    100.,
    60.,
    Unit::Percent,
)];
pub(crate) const INFO: EffectInfo = EffectInfo {
    id: "concat.enhance-voice",
    name: "Concat Enhance Voice",
    category: Category::Filter,
    params: PARAMS,
    create: |rate, ch| Box::new(Stream::new(rate as u32, ch)),
};
const DENOISE_PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "strength",
    "Strength",
    0.,
    100.,
    50.,
    Unit::Percent,
)];
pub(crate) const DENOISE_INFO: EffectInfo = EffectInfo {
    id: "concat.denoise",
    name: "Concat Noise Reduction",
    category: Category::Filter,
    params: DENOISE_PARAMS,
    create: |rate, ch| Box::new(Stream::denoise(rate as u32, ch)),
};
#[derive(Clone, Copy)]
enum Flavor {
    Enhance,
    Denoise,
}
enum Core {
    Enhance(EnhanceVoice),
    Denoise(Denoiser),
}
impl Core {
    fn new(flavor: Flavor, rate: u32, channels: usize, value: f32, double: bool) -> Self {
        match flavor {
            Flavor::Enhance => Self::Enhance(EnhanceVoice::new(rate, channels, value)),
            Flavor::Denoise => {
                let strength = f64::from(value) / 100.;
                let fixed = |v: f64| format!("{v:.1}").parse::<f64>().unwrap();
                Self::Denoise(Denoiser::with_precision(
                    rate,
                    channels,
                    fixed(-20. - strength * 30.),
                    fixed(6. + strength * 30.),
                    !double,
                ))
            }
        }
    }
    fn hop(&self) -> usize {
        match self {
            Self::Enhance(e) => e.hop(),
            Self::Denoise(e) => e.hop(),
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Enhance(e) => e.reset(),
            Self::Denoise(e) => e.reset(),
        }
    }
    fn process_hop_with_odd(
        &mut self,
        input: &[Vec<f64>],
        final_hop: bool,
        odd: &[usize],
    ) -> Vec<Vec<f64>> {
        match self {
            Self::Enhance(e) => e.process_hop_with_odd(input, final_hop, odd),
            Self::Denoise(e) => e.process_hop(input),
        }
    }
}
struct OutputHop {
    ready: i64,
    samples: Vec<Vec<f64>>,
    read: usize,
}
/// The H-1 scheduling delay is reported separately from the original audible delay.
/// Two queued hops suffice even when EOF supplies a short final input hop early.
pub(crate) struct Stream {
    core: Core,
    flavor: Flavor,
    double: bool,
    rate: u32,
    channels: usize,
    amount: f32,
    input: Vec<Vec<f64>>,
    output: VecDeque<OutputHop>,
    cursor: i64,
    origin: i64,
    end: Option<i64>,
    odd_input: Vec<usize>,
}
impl Stream {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        Self::create(rate, channels, Flavor::Enhance, 60.)
    }
    pub(crate) fn denoise(rate: u32, channels: usize) -> Self {
        Self::create(rate, channels, Flavor::Denoise, 50.)
    }
    fn create(rate: u32, channels: usize, flavor: Flavor, amount: f32) -> Self {
        let core = Core::new(flavor, rate, channels, amount, false);
        let hop = core.hop();
        Self {
            core,
            flavor,
            double: false,
            rate,
            channels,
            amount,
            input: (0..channels).map(|_| Vec::with_capacity(hop)).collect(),
            output: VecDeque::with_capacity(2),
            cursor: 0,
            origin: 0,
            end: None,
            odd_input: vec![],
        }
    }
    pub(crate) fn requires_double(&self) -> bool {
        matches!(self.flavor, Flavor::Enhance)
    }
    pub(crate) fn set_cursor(&mut self, cursor: i64) {
        self.cursor = cursor;
    }
    pub(crate) fn frame_flow(
        &mut self,
        origin: i64,
        end: i64,
        odd: &[i64],
        cursor: i64,
    ) -> (i64, Vec<i64>) {
        self.origin = origin;
        self.end = Some(end);
        self.cursor = cursor;
        self.odd_input = odd
            .iter()
            .filter_map(|&v| usize::try_from(v - origin).ok())
            .collect();
        let delay = self.latency() as i64;
        let hop = self.core.hop() as i64;
        let mut ends = vec![];
        if hop % 2 != 0 {
            let mut at = origin + hop;
            while at <= end {
                ends.push(at + delay);
                at += hop;
            }
        }
        if (end - origin) % hop % 2 != 0 {
            ends.push(end + delay);
        }
        (end + delay, ends)
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64], double: bool) {
        if matches!(self.flavor, Flavor::Denoise) && self.double != double {
            assert!(
                self.input[0].is_empty() && self.output.is_empty(),
                "spectral precision is fixed for a composed run"
            );
            self.double = double;
            self.core = Core::new(self.flavor, self.rate, self.channels, self.amount, double);
        }
        let cursor = self.cursor;
        if cursor >= self.origin && self.end.is_none_or(|end| cursor < end) {
            for (channel, &value) in self.input.iter_mut().zip(samples.iter()) {
                channel.push(value);
            }
            let count = self.input[0].len();
            let final_hop = self.end == Some(cursor + 1);
            if count == self.core.hop() || final_hop {
                let samples =
                    self.core
                        .process_hop_with_odd(&self.input, final_hop, &self.odd_input);
                let ready = cursor + 1 - count as i64 + self.latency() as i64;
                assert!(self.output.len() < 2, "bounded spectral output queue");
                self.output.push_back(OutputHop {
                    ready,
                    samples,
                    read: 0,
                });
                for channel in &mut self.input {
                    channel.clear();
                }
            }
        }
        samples.fill(0.);
        if let Some(hop) = self.output.front_mut() {
            if cursor >= hop.ready {
                for (ch, sample) in samples.iter_mut().enumerate() {
                    *sample = hop.samples[ch][hop.read];
                }
                hop.read += 1;
                if hop.read == hop.samples[0].len() {
                    self.output.pop_front();
                }
            }
        }
        self.cursor += 1;
    }
}
impl AudioEffect for Stream {
    fn id(&self) -> &'static str {
        match self.flavor {
            Flavor::Enhance => INFO.id,
            Flavor::Denoise => DENOISE_INFO.id,
        }
    }
    fn params(&self) -> &'static [ParamSpec] {
        match self.flavor {
            Flavor::Enhance => PARAMS,
            Flavor::Denoise => DENOISE_PARAMS,
        }
    }
    fn set_param(&mut self, id: &str, value: f32) -> bool {
        if id != self.params()[0].id || !value.is_finite() || !(0. ..=100.).contains(&value) {
            return false;
        }
        self.amount = value;
        self.core = Core::new(self.flavor, self.rate, self.channels, value, self.double);
        for input in &mut self.input {
            input.clear();
        }
        self.output.clear();
        true
    }
    fn param(&self, id: &str) -> Option<f32> {
        (id == self.params()[0].id).then_some(self.amount)
    }
    fn latency(&self) -> usize {
        self.core.hop() - 1
    }
    fn reset(&mut self) {
        self.core.reset();
        for input in &mut self.input {
            input.clear();
        }
        self.output.clear();
        self.cursor = self.origin;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let frames = channels[0].len();
        let mut samples = [0f64; 16];
        for i in 0..frames {
            for ch in 0..self.channels {
                samples[ch] = f64::from(channels[ch][i]);
            }
            self.process_frame(&mut samples[..self.channels], false);
            for ch in 0..self.channels {
                channels[ch][i] = samples[ch] as f32;
            }
        }
    }
}
