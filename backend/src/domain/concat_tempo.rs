//! Owned streaming waveform-similarity overlap-add for Concat pitch graphs.
use super::concat_denoise::FloatFft;
use std::{collections::VecDeque, f64::consts::PI};
pub(crate) trait Sample: Copy + Default {
    const MONO_LIMIT: f32 = 1.;
    fn as_float(self) -> f32;
    fn from_float(value: f32) -> Self;
}
impl Sample for f32 {
    fn as_float(self) -> f32 {
        self
    }
    fn from_float(value: f32) -> Self {
        value
    }
}
impl Sample for f64 {
    fn as_float(self) -> f32 {
        self as f32
    }
    fn from_float(value: f32) -> Self {
        f64::from(value)
    }
}
impl Sample for u8 {
    const MONO_LIMIT: f32 = 127.;
    fn as_float(self) -> f32 {
        f32::from(self)
    }
    fn from_float(value: f32) -> Self {
        value as u8
    }
}
impl Sample for i16 {
    const MONO_LIMIT: f32 = i16::MAX as f32;
    fn as_float(self) -> f32 {
        f32::from(self)
    }
    fn from_float(value: f32) -> Self {
        value as i16
    }
}
impl Sample for i32 {
    const MONO_LIMIT: f32 = i32::MAX as f32;
    fn as_float(self) -> f32 {
        self as f32
    }
    fn from_float(value: f32) -> Self {
        value as i32
    }
}
struct Fragment<T: Sample> {
    input: i64,
    output: i64,
    count: usize,
    data: Vec<Vec<T>>,
    spectrum: Vec<(f32, f32)>,
}
impl<T: Sample> Fragment<T> {
    fn new(window: usize, channels: usize) -> Self {
        Self {
            input: 0,
            output: 0,
            count: 0,
            data: vec![vec![T::default(); window]; channels],
            spectrum: vec![(0., 0.); window + 1],
        }
    }
}
#[derive(Clone, Copy)]
enum State {
    Load,
    Adjust,
    Reload,
    Overlap,
}
pub(crate) struct Tempo<T: Sample> {
    window: usize,
    channels: usize,
    tempo: f64,
    hann: Vec<f32>,
    ring: Vec<VecDeque<T>>,
    read: i64,
    written: i64,
    fragment: u64,
    fragments: [Fragment<T>; 2],
    state: State,
    ended: bool,
    pending: Vec<Vec<T>>,
    capacity: usize,
    fft: FloatFft,
    mono: Vec<f32>,
    product: Vec<(f32, f32)>,
    correlation: Vec<f32>,
}
impl<T: Sample> Tempo<T> {
    pub(crate) fn new(rate: u32, channels: usize, tempo: f64) -> Self {
        assert!(
            (8000..=192000).contains(&rate)
                && (1..=16).contains(&channels)
                && tempo.is_finite()
                && (0.5..=2.).contains(&tempo)
        );
        let window = (rate as usize / 24).next_power_of_two();
        let hann = (0..window)
            .map(|i| {
                let t = i as f64 / (window - 1) as f64;
                (0.5 * (1. - (2. * PI * t).cos())) as f32
            })
            .collect();
        let mut result = Self {
            window,
            channels,
            tempo,
            hann,
            ring: (0..channels)
                .map(|_| VecDeque::with_capacity(3 * window))
                .collect(),
            read: 0,
            written: 0,
            fragment: 0,
            fragments: [
                Fragment::new(window, channels),
                Fragment::new(window, channels),
            ],
            state: State::Load,
            ended: false,
            pending: vec![vec![]; channels],
            capacity: 0,
            fft: FloatFft::new(2 * window),
            mono: vec![0.; window],
            product: vec![(0., 0.); window + 1],
            correlation: vec![0.; 2 * window],
        };
        result.reset();
        result
    }
    #[cfg(test)]
    pub(crate) fn retained_frames(&self) -> (usize, usize) {
        (self.ring[0].len(), 3 * self.window)
    }
    pub(crate) fn reset(&mut self) {
        for ch in &mut self.ring {
            ch.clear();
        }
        self.read = 0;
        self.written = 0;
        self.fragment = 0;
        self.state = State::Load;
        self.ended = false;
        self.capacity = 0;
        for channel in &mut self.pending {
            channel.clear();
        }
        for f in &mut self.fragments {
            f.input = 0;
            f.output = 0;
            f.count = 0;
            for ch in &mut f.data {
                ch.fill(T::default());
            }
            f.spectrum.fill((0., 0.));
        }
        self.fragments[0].input = -(self.window as i64 / 2);
        self.fragments[0].output = -(self.window as i64 / 2);
    }
    fn current(&self) -> usize {
        self.fragment as usize % 2
    }
    fn advance(&mut self) {
        let old = self.current();
        self.fragment += 1;
        let new = self.current();
        self.fragments[new].input =
            self.fragments[old].input + (self.tempo * (self.window / 2) as f64) as i64;
        self.fragments[new].output = self.fragments[old].output + (self.window / 2) as i64;
        self.fragments[new].count = 0;
    }
    fn read_until(&mut self, target: i64, input: &[&[T]], offset: &mut usize) -> bool {
        let count = (target - self.read).max(0) as usize;
        let count = count.min(input[0].len() - *offset);
        for i in 0..count {
            for ch in 0..self.channels {
                if self.ring[ch].len() == 3 * self.window {
                    self.ring[ch].pop_front();
                }
                self.ring[ch].push_back(input[ch][*offset + i]);
            }
        }
        *offset += count;
        self.read += count as i64;
        self.read >= target
    }
    fn load(&mut self) {
        let idx = self.current();
        let f = &mut self.fragments[idx];
        let missing = (f.input + self.window as i64 - self.read).max(0) as usize;
        f.count = self.window.saturating_sub(missing);
        let first = self.read - self.ring[0].len() as i64;
        let zeros = (first - f.input).max(0).min(f.count as i64) as usize;
        if zeros == f.count {
            return;
        }
        for ch in 0..self.channels {
            f.data[ch][..zeros].fill(T::default());
            for i in zeros..f.count {
                let pos = f.input + i as i64 - first;
                assert!(
                    pos >= 0 && (pos as usize) < self.ring[ch].len(),
                    "retained WSOLA fragment"
                );
                f.data[ch][i] = self.ring[ch][pos as usize];
            }
        }
    }
    fn transform(&mut self) {
        let idx = self.current();
        let f = &self.fragments[idx];
        self.mono.fill(0.);
        for i in 0..f.count {
            let mut value = f.data[0][i].as_float();
            let mut peak = value.abs().min(T::MONO_LIMIT);
            for ch in 1..self.channels {
                let sample = f.data[ch][i].as_float();
                let p = sample.abs().min(T::MONO_LIMIT);
                if p > peak {
                    peak = p;
                    value = sample;
                }
            }
            self.mono[i] = value;
        }
        self.fft
            .forward(&self.mono, &mut self.fragments[idx].spectrum);
    }
    fn adjust(&mut self) -> i64 {
        let idx = self.current();
        let prev = &self.fragments[1 - idx];
        let expected = (prev.output + (self.window / 2) as i64) as f64 * self.tempo;
        let actual = (prev.input + (self.window / 2) as i64) as f64;
        let drift = (expected - actual) as i64;
        for i in 0..=self.window {
            let a = prev.spectrum[i];
            let b = self.fragments[idx].spectrum[i];
            self.product[i] = (a.0.mul_add(b.0, a.1 * b.1), a.1.mul_add(b.0, -a.0 * b.1));
        }
        self.fft.inverse(&self.product, &mut self.correlation);
        let start = (-drift).clamp(0, self.window as i64) as usize;
        let stop = (self.window as i64 - drift)
            .min((self.window - self.window / 16) as i64)
            .max(0) as usize;
        let mut best = -f32::MAX;
        let mut correction = -drift;
        for i in start..stop {
            let weighting = (drift + i as i64) as f32 * (i - start) as f32 * (stop - i) as f32;
            let metric = self.correlation[i] * weighting;
            if metric > best {
                best = metric;
                correction = i as i64 - self.window as i64 / 2;
            }
        }
        if correction != 0 {
            self.fragments[idx].input -= correction;
            self.fragments[idx].count = 0;
        }
        correction
    }
    fn blend(&mut self, output: &mut [Vec<T>], limit: usize) -> bool {
        let idx = self.current();
        let prev = &self.fragments[1 - idx];
        let f = &self.fragments[idx];
        let start = self.written.max(f.output);
        let stop = (prev.output + prev.count as i64).min(f.output + f.count as i64);
        assert!(start <= stop, "valid WSOLA overlap");
        for p in start
            ..start + (stop - start).min((limit - output[0].len()).min(i64::MAX as usize) as i64)
        {
            let a = (p - prev.output) as usize;
            let b = (p - f.output) as usize;
            for ch in 0..self.channels {
                let value = if f.input + p - start < 0 {
                    prev.data[ch][a]
                } else {
                    T::from_float(
                        prev.data[ch][a]
                            .as_float()
                            .mul_add(self.hann[a], f.data[ch][b].as_float() * self.hann[b]),
                    )
                };
                output[ch].push(value);
            }
            self.written += 1;
        }
        self.written == stop
    }
    fn flush_step(&mut self, output: &mut [Vec<T>], limit: usize) -> bool {
        if self.fragment == 0 {
            return true;
        }
        let idx = self.current();
        let f = &self.fragments[idx];
        if self.read == f.input + f.count as i64 && self.written == f.output + f.count as i64 {
            return true;
        }
        if f.input + (f.count as i64) < self.read {
            self.load();
            self.transform();
            if self.adjust() != 0 {
                self.load();
            }
        }
        let idx = self.current();
        let f = &self.fragments[idx];
        let overlap_end = f.output + (self.window / 2).min(f.count) as i64;
        while self.written < overlap_end {
            if !self.blend(output, limit) {
                return false;
            }
        }
        let f = &self.fragments[idx];
        if f.input + (f.count as i64) < self.read {
            self.advance();
            return false;
        }
        let start = self.written.max(overlap_end);
        let stop = f.output + f.count as i64;
        assert!(start <= stop, "valid WSOLA tail");
        let count = (stop - start).min((limit - output[0].len()).min(i64::MAX as usize) as i64);
        for p in start..start + count {
            for ch in 0..self.channels {
                output[ch].push(f.data[ch][(p - f.output) as usize]);
            }
            self.written += 1;
        }
        self.written == stop
    }
    fn apply(&mut self, input: &[&[T]], offset: &mut usize, output: &mut [Vec<T>], limit: usize) {
        loop {
            match self.state {
                State::Load | State::Reload => {
                    let target = self.fragments[self.current()].input + self.window as i64;
                    if !self.read_until(target, input, offset) {
                        break;
                    }
                    self.load();
                    self.transform();
                    if self.fragment == 0 {
                        self.advance();
                        continue;
                    }
                    self.state = if matches!(self.state, State::Reload) {
                        State::Overlap
                    } else {
                        State::Adjust
                    };
                }
                State::Adjust => {
                    self.state = if self.adjust() != 0 {
                        State::Reload
                    } else {
                        State::Overlap
                    };
                }
                State::Overlap => {
                    if !self.blend(output, limit) {
                        break;
                    }
                    self.advance();
                    self.state = State::Load;
                }
            }
        }
    }
    /// Preserve upstream audio-frame boundaries and the original output-frame capacity.
    pub(crate) fn push_frames(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<Vec<T>>> {
        assert!(
            !self.ended
                && input.len() == self.channels
                && input[0].len() <= 1048576
                && input.iter().all(|c| c.len() == input[0].len())
        );
        let frames = input[0].len();
        let target = (0.5 + frames as f64 / self.tempo) as usize;
        let mut offset = 0;
        let mut result = Vec::new();
        let mut dest = std::mem::take(&mut self.pending);
        let mut capacity = self.capacity;
        while offset < frames {
            if capacity == 0 {
                capacity = target;
            }
            self.apply(input, &mut offset, &mut dest, capacity);
            if dest[0].len() == capacity {
                if !dest[0].is_empty() {
                    result.push(std::mem::replace(
                        &mut dest,
                        vec![Vec::new(); self.channels],
                    ));
                }
                capacity = 0;
            }
        }
        if end {
            loop {
                if capacity == 0 {
                    capacity = 3 * self.window;
                }
                let done = self.flush_step(&mut dest, capacity);
                if !dest[0].is_empty() {
                    result.push(std::mem::replace(
                        &mut dest,
                        vec![Vec::new(); self.channels],
                    ));
                }
                capacity = 0;
                if done {
                    break;
                }
            }
            self.ended = true;
        }
        self.pending = dest;
        self.capacity = capacity;
        result
    }
    /// Flattened compatibility interface; frame-aware graphs use push_frames.
    pub(crate) fn push_packet(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        let packets = self.push_frames(input, end);
        let mut output = vec![Vec::new(); self.channels];
        for packet in packets {
            for (dst, src) in output.iter_mut().zip(packet) {
                dst.extend(src);
            }
        }
        output
    }
    pub(crate) fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        assert!(
            !self.ended
                && input.len() == self.channels
                && input[0].len() <= 65536
                && input.iter().all(|c| c.len() == input[0].len())
        );
        let mut output = vec![vec![]; self.channels];
        let mut offset = 0;
        self.apply(input, &mut offset, &mut output, usize::MAX);
        assert_eq!(offset, input[0].len());
        if end {
            while !self.flush_step(&mut output, usize::MAX) {}
            self.ended = true;
        }
        output
    }
}
