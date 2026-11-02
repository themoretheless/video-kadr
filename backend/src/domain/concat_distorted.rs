//! Owned static Concat distortion: float volume, six-bit soft quantizer, linked limiter.
use super::concat_limiter::LookaheadLimiter;
use filmcraft_audio_dsp::{AudioEffect, Category, EffectInfo, ParamSpec, Unit};
pub(crate) const PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "drive",
    "Drive",
    0.,
    100.,
    50.,
    Unit::Percent,
)];
pub(crate) const INFO: EffectInfo = EffectInfo {
    id: "concat.distorted",
    name: "Concat Distorted",
    category: Category::Dynamics,
    params: PARAMS,
    create: |rate, channels| Box::new(Distorted::new(rate as u32, channels)),
};
pub(crate) struct Distorted {
    drive: f32,
    volume: f32,
    mix: f64,
    channels: usize,
    limiter: LookaheadLimiter,
    cursor: i64,
    end: Option<i64>,
}
impl Distorted {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        let mut result = Self {
            drive: 50.,
            volume: 1.,
            mix: 0.,
            channels,
            limiter: LookaheadLimiter::new(rate, channels, 0.85, false),
            cursor: 0,
            end: None,
        };
        result.configure();
        result
    }
    fn configure(&mut self) {
        let drive = f64::from(self.drive) / 100.;
        self.volume = format!("{:.2}", 1. + drive * 7.).parse().unwrap();
        self.mix = format!("{:.2}", drive * 0.7).parse().unwrap();
    }
    pub(crate) fn set_cursor(&mut self, cursor: i64) {
        self.cursor = cursor;
    }
    pub(crate) fn frame_end(&mut self, end: i64, cursor: i64) {
        self.end = Some(end);
        self.cursor = cursor;
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        if self.end.is_some_and(|end| self.cursor >= end) {
            samples.fill(0.);
        } else {
            for sample in samples.iter_mut() {
                *sample = self.crush(*sample as f32);
            }
            self.limiter.process_frame(samples);
        }
        self.cursor += 1;
    }
    fn crush(&self, sample: f32) -> f64 {
        let scaled = f64::from(sample * self.volume);
        // Default sample reduction is one sample; preserve its wet/dry arithmetic.
        let input = self.mix * scaled + scaled * (1. - self.mix);
        let y = input * 63.;
        let rounded = (y as f32).round() as f64;
        let distance = (y - rounded).abs();
        let wet = if distance <= 0.25 {
            rounded / 63.
        } else {
            let blend = 0.5
                * ((std::f64::consts::PI * (distance - 0.25) / 0.5 - std::f64::consts::FRAC_PI_2)
                    .sin()
                    + 1.);
            let direction = if y > rounded { 1. } else { -1. };
            rounded / 63. + direction * (1. / 63.) * blend
        };
        wet + (input - wet) * self.mix
    }
}
impl AudioEffect for Distorted {
    fn id(&self) -> &'static str {
        "concat.distorted"
    }
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }
    fn set_param(&mut self, id: &str, value: f32) -> bool {
        if id != "drive" || !value.is_finite() || !(0. ..=100.).contains(&value) {
            return false;
        }
        self.drive = value;
        self.configure();
        true
    }
    fn param(&self, id: &str) -> Option<f32> {
        (id == "drive").then_some(self.drive)
    }
    fn reset(&mut self) {
        self.limiter.reset();
        self.cursor = 0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let frames = channels
            .iter()
            .take(self.channels)
            .map(|v| v.len())
            .min()
            .unwrap_or(0);
        if channels.len() < self.channels {
            return;
        }
        for frame in 0..frames {
            let mut input = [0.; 16];
            for ch in 0..self.channels {
                input[ch] = f64::from(channels[ch][frame]);
            }
            self.process_frame(&mut input[..self.channels]);
            for ch in 0..self.channels {
                channels[ch][frame] = input[ch] as f32;
            }
        }
    }
}
