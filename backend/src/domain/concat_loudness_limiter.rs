//! Loudnorm's 192 kHz lookahead limiter, not yet production-admitted.
//! SPDX-License-Identifier: LGPL-2.1-or-later
//! Port of FFmpeg af_loudnorm.c, Copyright (c) 2016 Kyle Swanson.
//! License: backend/licenses/FFmpeg-LGPL-2.1.txt; no warranty.
use anyhow::{ensure, Result};
#[derive(Clone, Copy)]
enum State {
    Out,
    Attack,
    Sustain,
    Release,
}
pub(crate) struct Limiter {
    pub(crate) buffer: Vec<f64>,
    pub(crate) index: usize,
    channels: usize,
    ceiling: f64,
    previous: Vec<f64>,
    state: State,
    reduction: [f64; 2],
    peak: usize,
    envelope: usize,
    count: i32,
    attack: i32,
    release: i32,
}
impl Limiter {
    pub(crate) fn new(channels: usize, ceiling: f64) -> Result<Self> {
        ensure!(
            (1..=2).contains(&channels) && ceiling.is_finite() && ceiling > 0. && ceiling <= 1.,
            "invalid Loudness limiter shape"
        );
        Ok(Self {
            buffer: vec![0.; 40320 * channels],
            index: 0,
            channels,
            ceiling,
            previous: vec![0.; channels],
            state: State::Out,
            reduction: [0.; 2],
            peak: 0,
            envelope: 0,
            count: 0,
            attack: 1920,
            release: 19200,
        })
    }
    fn detect(&mut self, offset: i32, frames: i32, first: bool) -> Option<(i32, f64)> {
        let ch = self.channels;
        let size = self.buffer.len();
        let mut index = (self.index as i64 + (offset as i64 + 1920) * ch as i64)
            .rem_euclid(size as i64) as usize;
        if first {
            for c in 0..ch {
                self.previous[c] = self.buffer[(index + c + size - ch) % size].abs();
            }
        }
        for n in 0..frames {
            for c in 0..ch {
                let this = self.buffer[(index + c) % size].abs();
                let next = self.buffer[(index + c + ch) % size].abs();
                if self.previous[c] <= this && next <= this && this > self.ceiling && n > 0 {
                    if (2..12).any(|i| self.buffer[(index + c + i * ch) % size].abs() > this) {
                        continue;
                    }
                    let mut peak = 0.;
                    for c in 0..ch {
                        let v = self.buffer[(index + c) % size].abs();
                        if c == 0 || v > peak {
                            peak = v;
                        }
                        self.previous[c] = v;
                    }
                    self.peak = index;
                    return Some((n, peak));
                }
                self.previous[c] = this;
            }
            index = (index + ch) % size;
        }
        None
    }
    fn multiply(&mut self, gain: f64) {
        for c in 0..self.channels {
            self.buffer[self.envelope + c] *= gain;
        }
        self.envelope = (self.envelope + self.channels) % self.buffer.len();
    }
    pub(crate) fn process(&mut self, frames: usize, first: bool) -> Result<Vec<f64>> {
        ensure!(
            frames > 0
                && frames <= 19200
                && self.index < self.buffer.len()
                && self.index % self.channels == 0,
            "invalid Loudness limiter block"
        );
        ensure!(
            self.buffer.iter().all(|v| v.is_finite()),
            "nonfinite Loudness limiter buffer"
        );
        if first {
            let max = self.buffer[..1920 * self.channels]
                .iter()
                .fold(0_f64, |a, v| a.max(v.abs()));
            if max > self.ceiling {
                self.reduction[1] = self.ceiling / max;
                self.state = State::Sustain;
                for v in &mut self.buffer[..1920 * self.channels] {
                    *v *= self.reduction[1];
                }
            }
        }
        let mut done = 0_i32;
        let frames = frames as i32;
        loop {
            match self.state {
                State::Out => {
                    if let Some((delta, peak)) = self.detect(done, frames - done, first) {
                        self.count = 0;
                        done += delta - self.attack;
                        self.reduction = [1., self.ceiling / peak];
                        self.state = State::Attack;
                        self.envelope = (self.peak as i64
                            - self.attack as i64 * self.channels as i64)
                            .rem_euclid(self.buffer.len() as i64)
                            as usize;
                    } else {
                        done = frames;
                    }
                }
                State::Attack => {
                    while self.count < self.attack {
                        let gain = self.reduction[0]
                            - (self.count as f64 / (self.attack - 1) as f64
                                * (self.reduction[0] - self.reduction[1]));
                        self.multiply(gain);
                        done += 1;
                        self.count += 1;
                        if done >= frames {
                            break;
                        }
                    }
                    if done < frames {
                        self.count = 0;
                        self.attack = 1920;
                        self.state = State::Sustain;
                    }
                }
                State::Sustain => {
                    if let Some((delta, peak)) = self.detect(done, frames, first) {
                        let reduction = self.ceiling / peak;
                        if reduction < self.reduction[1] {
                            self.state = State::Attack;
                            self.attack = delta.max(2);
                            self.reduction = [self.reduction[1], reduction];
                            self.count = 0;
                        } else {
                            self.count = 0;
                            while self.count < delta {
                                self.multiply(self.reduction[1]);
                                done += 1;
                                self.count += 1;
                                if done >= frames {
                                    break;
                                }
                            }
                        }
                    } else {
                        self.state = State::Release;
                        self.reduction = [self.reduction[1], 1.];
                        self.count = 0;
                    }
                }
                State::Release => {
                    while self.count < self.release {
                        let gain = self.reduction[0]
                            + (self.count as f64 / (self.release - 1) as f64)
                                * (self.reduction[1] - self.reduction[0]);
                        self.multiply(gain);
                        done += 1;
                        self.count += 1;
                        if done >= frames {
                            break;
                        }
                    }
                    if done < frames {
                        self.count = 0;
                        self.state = State::Out;
                    }
                }
            }
            if done >= frames {
                break;
            }
        }
        let mut out = Vec::with_capacity(frames as usize * self.channels);
        for i in 0..frames as usize * self.channels {
            let v = self.buffer[(self.index + i) % self.buffer.len()];
            out.push(if v.abs() > self.ceiling {
                self.ceiling * if v < 0. { -1. } else { 1. }
            } else {
                v
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires independently compiled LOUDNESS_LIMITER_ORACLE"]
    fn original_limiter_samples_and_ring() {
        use std::{
            io::{Seek, SeekFrom, Write},
            process::{Command, Stdio},
        };
        let oracle = std::env::var("LOUDNESS_LIMITER_ORACLE").expect("set LOUDNESS_LIMITER_ORACLE");
        let mut compared = 0;
        for channels in [1, 2] {
            for pattern in 0..4 {
                let mut limiter = Limiter::new(channels, 10_f64.powf(-1.5 / 20.)).unwrap();
                let mut input = tempfile::tempfile().unwrap();
                let mut expected = Vec::new();
                for step in 0..12 {
                    let frames = if step % 3 == 1 {
                        137
                    } else if step % 3 == 2 {
                        19063
                    } else {
                        19200
                    };
                    limiter.index = if step == 0 {
                        0
                    } else {
                        (step * 19200 % 40320) * channels
                    };
                    for i in 0..40320 {
                        for c in 0..channels {
                            let t = (i + step * 137) as f64;
                            limiter.buffer[i * channels + c] = match pattern {
                                0 => 0.4 * (t * 0.071).sin(),
                                1 => {
                                    if (i + step * 613 + c * 97) % 3101 == 0 {
                                        2.7
                                    } else {
                                        0.1 * (t * 0.01).sin()
                                    }
                                }
                                2 => {
                                    if i % 7000 < 3000 {
                                        (0.7 + c as f64) * (t * 0.19).sin()
                                    } else {
                                        0.
                                    }
                                }
                                _ => {
                                    if step == 0 && i < 1920 {
                                        1.7 * (t * 0.13).cos()
                                    } else {
                                        (if step % 2 == 0 { 1.3 } else { 0.2 }) * (t * 0.03).sin()
                                    }
                                }
                            };
                        }
                    }
                    for v in [limiter.index as i32, i32::from(step == 0), frames as i32] {
                        input.write_all(&v.to_ne_bytes()).unwrap();
                    }
                    for v in &limiter.buffer {
                        input.write_all(&v.to_ne_bytes()).unwrap();
                    }
                    expected.extend(limiter.process(frames, step == 0).unwrap());
                    expected.extend_from_slice(&limiter.buffer);
                }
                input.seek(SeekFrom::Start(0)).unwrap();
                let output = Command::new(&oracle)
                    .arg(channels.to_string())
                    .stdin(Stdio::from(input))
                    .output()
                    .unwrap();
                assert!(output.status.success());
                assert_eq!(output.stdout.len(), expected.len() * 8);
                for (i, (bytes, a)) in output.stdout.chunks_exact(8).zip(expected).enumerate() {
                    let e = f64::from_ne_bytes(bytes.try_into().unwrap());
                    assert!(
                        (a - e).abs() < 2e-12,
                        "ch{channels} pattern{pattern} sample{i}: {a} != {e}"
                    );
                    compared += 1;
                }
            }
        }
        eprintln!("Original Loudnorm limiter: {compared} output/ring values matched");
    }
}
