//! Double-precision de-essing stage for Concat Sweet and Enhance Voice.
//!
//! Algorithm reference: FFmpeg n9.0.2 af_deesser.c, Chris Johnson (2018).
//! Copyright (c) 2018 Chris Johnson
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

#[derive(Clone)]
struct Channel {
    previous: [f64; 2],
    low: [f64; 2],
    ratio: [f64; 2],
    phase: usize,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            previous: [0.; 2],
            low: [0.; 2],
            ratio: [1.; 2],
            phase: 1,
        }
    }
}
pub(crate) struct Deesser {
    channels: Vec<Channel>,
    intensity: f64,
    maximum: f64,
    smoothing: f64,
}
impl Deesser {
    pub(crate) fn new(
        rate: u32,
        channels: usize,
        intensity: f64,
        maximum: f64,
        frequency: f64,
    ) -> Self {
        let scale = if rate < 44100 {
            44100. / f64::from(rate)
        } else {
            f64::from(rate) / 44100.
        };
        Self {
            channels: vec![Channel::default(); channels],
            intensity: intensity.powf(5.) * (8192. / scale),
            maximum: 1. / 10f64.powf(((maximum - 1.) * 48.) / 20.),
            smoothing: frequency.powf(2.) / scale,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.channels.fill(Channel::default());
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        for (input, state) in samples.iter_mut().zip(&mut self.channels) {
            let difference = *input - state.previous[0];
            let older = state.previous[0] - state.previous[1];
            let first = difference * (difference / 1.3);
            let second = older * (difference / 1.3);
            let delta = first - second;
            let curvature = delta * (delta / 1.3);
            state.previous = [*input, state.previous[0]];
            let attack = curvature.mul_add(1024., 7.);
            let sense = (self.intensity * self.intensity)
                .mul_add(curvature, 1.)
                .min(self.intensity);
            let recovery = 1. + 0.01 / sense;
            let amount = (1. - input.abs()) * self.smoothing;
            let phase = state.phase;
            state.low[phase] = state.low[phase].mul_add(1. - amount, *input * amount);
            state.ratio[phase] = if state.ratio[phase] < sense {
                state.ratio[phase].mul_add(attack, sense) / (attack + 1.)
            } else {
                1. + (state.ratio[phase] - 1.) / recovery
            }
            .min(self.maximum);
            *input = state.low[phase] + (*input - state.low[phase]) / state.ratio[phase];
            state.phase ^= 1;
        }
    }
}
