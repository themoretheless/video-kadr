//! Owned streaming Kaiser-windowed rational resampling for Concat pitch graphs.
use std::{collections::VecDeque, f64::consts::PI};
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn i0(x: f64) -> f64 {
    let q = x * x / 4.;
    let mut term = 1.;
    let mut sum = 1.;
    for k in 1..128 {
        term *= q / (k * k) as f64;
        let next = sum + term;
        if next == sum {
            break;
        }
        sum = next;
    }
    sum
}
pub(crate) trait Sample:
    Copy + Default + std::ops::Add<Output = Self> + std::ops::Sub<Output = Self>
{
    const DOUBLE: bool;
    fn finish(self) -> Self {
        self
    }
    fn interpolate(self, other: Self, fraction: u64, denominator: u64) -> Self {
        Self::from_double(
            ((other - self).double() * (1. / denominator as f64))
                .mul_add(fraction as f64, self.double()),
        )
    }
    fn from_double(value: f64) -> Self;
    fn double(self) -> f64;
    fn madd(self, coefficient: Self, sum: Self) -> Self;
}
impl Sample for f32 {
    const DOUBLE: bool = false;
    fn from_double(value: f64) -> Self {
        value as f32
    }
    fn double(self) -> f64 {
        f64::from(self)
    }
    fn madd(self, c: Self, s: Self) -> Self {
        self.mul_add(c, s)
    }
}
impl Sample for f64 {
    const DOUBLE: bool = true;
    fn from_double(value: f64) -> Self {
        value
    }
    fn double(self) -> f64 {
        self
    }
    fn madd(self, c: Self, s: Self) -> Self {
        self.mul_add(c, s)
    }
}
pub(crate) struct Resampler<T: Sample> {
    input_rate: u32,
    output_rate: u32,
    channels: usize,
    taps: usize,
    center: i64,
    phases: u64,
    denominator: u64,
    increment: u64,
    bank: Vec<Vec<T>>,
    input: Vec<VecDeque<T>>,
    first: i64,
    total: i64,
    next: u64,
    initialized: bool,
    ended: bool,
}
impl<T: Sample> Resampler<T> {
    pub(crate) fn new(input_rate: u32, output_rate: u32, channels: usize) -> Self {
        assert!(
            (8000..=192000).contains(&input_rate)
                && (8000..=192000).contains(&output_rate)
                && (1..=16).contains(&channels)
        );
        let factor = (f64::from(output_rate) * 0.97 / f64::from(input_rate)).min(1.);
        let taps = ((32. / factor).ceil() as usize + 1) & !1;
        let center = (taps as i64 - 1) / 2;
        let exact = u64::from(output_rate) / gcd(u64::from(input_rate), u64::from(output_rate));
        let phases = exact.min(1024);
        let mut bank = vec![vec![T::default(); taps]; phases as usize + 1];
        let mut norm = 0.;
        let count = if phases % 2 == 0 {
            phases / 2 + 1
        } else {
            phases
        };
        for phase in 0..count {
            let mut values = Vec::with_capacity(taps);
            let mut sine =
                (PI * phase as f64 / phases as f64).sin() * if center & 1 != 0 { 1. } else { -1. };
            for tap in 0..taps {
                let x = PI * ((tap as i64 - center) as f64 - phase as f64 / phases as f64) * factor;
                let sinc = if x == 0. {
                    1.
                } else if factor == 1. {
                    sine / x
                } else {
                    x.sin() / x
                };
                let w = 2. * x / (factor * taps as f64 * PI);
                let value = sinc * i0(9. * (-w).mul_add(w, 1.).max(0.).sqrt());
                values.push(value);
                sine = -sine;
                if phase == 0 {
                    norm += value;
                }
            }
            for tap in 0..taps {
                bank[phase as usize][tap] = T::from_double(values[tap] / norm);
            }
            if phases % 2 == 0 {
                for tap in 0..taps {
                    let v = bank[phase as usize][tap];
                    bank[(phases - phase) as usize][taps - 1 - tap] = v;
                }
            }
        }
        bank[phases as usize][0] = if taps % 8 == 0 {
            bank[0][taps - 1]
        } else {
            T::default()
        };
        for tap in 1..taps {
            bank[phases as usize][tap] = bank[0][tap - 1];
        }
        let common = gcd(u64::from(output_rate), u64::from(input_rate) * phases);
        let mut denominator = u64::from(output_rate) / common;
        let mut increment = u64::from(input_rate) * phases / common;
        while denominator < (1 << 20) && increment < (1 << 20) {
            denominator *= 2;
            increment *= 2;
        }
        Self {
            input_rate,
            output_rate,
            channels,
            taps,
            center,
            phases,
            denominator,
            increment,
            bank,
            input: (0..channels).map(|_| VecDeque::new()).collect(),
            first: 0,
            total: 0,
            next: 0,
            initialized: false,
            ended: false,
        }
    }
    #[cfg(test)]
    pub(crate) fn retained_frames(&self) -> (usize, usize) {
        (self.input[0].len(), self.taps)
    }
    pub(crate) fn reset(&mut self) {
        for channel in &mut self.input {
            channel.clear();
        }
        self.first = 0;
        self.total = 0;
        self.next = 0;
        self.initialized = false;
        self.ended = false;
    }
    fn position(&self) -> (i64, usize, u64) {
        let position = u128::from(self.next) * u128::from(self.increment);
        let whole = (position / u128::from(self.denominator)) as u64;
        (
            (whole / self.phases) as i64,
            (whole % self.phases) as usize,
            (position % u128::from(self.denominator)) as u64,
        )
    }
    fn sample(&self, ch: usize, mut index: i64) -> T {
        if index < 0 {
            index = -index;
        }
        if index >= self.total {
            index = 2 * self.total - 1 - index;
        }
        assert!(
            index >= self.first && index < self.total,
            "retained resampling window"
        );
        self.input[ch][(index - self.first) as usize]
    }
    fn dot(&self, ch: usize, start: i64, phase: usize, linear: bool) -> T {
        if linear {
            let mut sum = T::default();
            for tap in 0..self.taps {
                sum = self
                    .sample(ch, start + tap as i64)
                    .madd(self.bank[phase][tap], sum);
            }
            sum
        } else if T::DOUBLE {
            let mut lanes = [T::default(); 2];
            for tap in 0..self.taps {
                lanes[tap % 2] = self
                    .sample(ch, start + tap as i64)
                    .madd(self.bank[phase][tap], lanes[tap % 2]);
            }
            lanes[0] + lanes[1]
        } else {
            let prefix = if self.taps >= 8 {
                self.taps & !7
            } else {
                self.taps & !3
            };
            let mut lanes = [T::default(); 4];
            for tap in 0..prefix {
                lanes[tap % 4] = self
                    .sample(ch, start + tap as i64)
                    .madd(self.bank[phase][tap], lanes[tap % 4]);
            }
            let mut sum = (lanes[0] + lanes[1]) + (lanes[2] + lanes[3]);
            for tap in prefix..self.taps {
                sum = self
                    .sample(ch, start + tap as i64)
                    .madd(self.bank[phase][tap], sum);
            }
            sum
        }
    }
    fn drain(&mut self, limit: i64, output: &mut [Vec<T>]) {
        if !self.initialized {
            if limit < (self.taps + 1) as i64 {
                return;
            }
            self.initialized = true;
        }
        loop {
            let (base, phase, fraction) = self.position();
            let start = base - self.center;
            if start + self.taps as i64 > limit {
                break;
            }
            let linear = self.increment % self.denominator != 0;
            for ch in 0..self.channels {
                let a = self.dot(ch, start, phase, linear);
                let value = if linear {
                    let b = self.dot(ch, start, phase + 1, true);
                    a.interpolate(b, fraction, self.denominator)
                } else {
                    a
                };
                output[ch].push(value.finish());
            }
            self.next += 1;
            let keep = (self.position().0 - self.center).max(0).min(self.total);
            while self.first < keep {
                for channel in &mut self.input {
                    channel.pop_front();
                }
                self.first += 1;
            }
        }
    }
    /// Input chunks are bounded; EOF is explicit and may be supplied as an empty chunk.
    pub(crate) fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        assert!(!self.ended && input.len() == self.channels);
        let frames = input[0].len();
        assert!(frames <= 65536 && input.iter().all(|ch| ch.len() == frames));
        if self.input_rate == self.output_rate {
            self.total += frames as i64;
            self.ended = end;
            return input.iter().map(|ch| ch.to_vec()).collect();
        }
        for (stored, source) in self.input.iter_mut().zip(input) {
            stored.extend(source.iter().copied());
        }
        self.total += frames as i64;
        let mut output = vec![vec![]; self.channels];
        self.drain(self.total, &mut output);
        if end {
            let reflection = (self.input[0].len().min(self.taps) + 1) / 2;
            self.drain(self.total + reflection as i64, &mut output);
            self.ended = true;
        }
        output
    }
}

/// Integer filter transport uses the original default float resampling interior.
/// Identity-rate conversion must preserve integer samples without a float round trip.
pub(crate) trait IntegerSample: Copy + Default {
    fn scaled(self) -> f32;
    fn from_scaled(value: f32) -> Self;
    fn from_vector(value: f32, channels: usize) -> Self {
        let _ = channels;
        Self::from_scaled(value)
    }
}
impl IntegerSample for i16 {
    fn from_vector(value: f32, channels: usize) -> Self {
        // Original AArch64 SWR: mono uses SQRSHRN, packed multichannel
        // selects the upper halfword after FCVTZS. Scalar tails use lrintf.
        let fixed = (value * 2147483648.) as i32;
        let bias = if channels == 1 { 32768 } else { 0 };
        ((i64::from(fixed) + bias) >> 16).clamp(-32768, 32767) as i16
    }
    fn scaled(self) -> f32 {
        f32::from(self) * (1. / 32768.)
    }
    fn from_scaled(value: f32) -> Self {
        (value * 32768.).round_ties_even() as i16
    }
}
impl IntegerSample for i32 {
    fn scaled(self) -> f32 {
        self as f32 * (1. / 2147483648.)
    }
    fn from_scaled(value: f32) -> Self {
        (value * 2147483648.).round_ties_even() as i32
    }
}
pub(crate) struct IntegerResampler<T: IntegerSample> {
    inner: Resampler<f32>,
    identity: bool,
    channels: usize,
    output_planar: bool,
    ended: bool,
    sample: std::marker::PhantomData<T>,
}
impl<T: IntegerSample> IntegerResampler<T> {
    #[cfg(test)]
    pub(crate) fn retained_frames(&self) -> (usize, usize) {
        self.inner.retained_frames()
    }
    pub(crate) fn new(from: u32, to: u32, channels: usize) -> Self {
        Self {
            inner: Resampler::new(from, to, channels),
            identity: from == to,
            channels,
            output_planar: false,
            ended: false,
            sample: std::marker::PhantomData,
        }
    }
    pub(crate) fn set_output_planar(&mut self, planar: bool) {
        self.output_planar = planar;
    }
    pub(crate) fn reset(&mut self) {
        self.inner.reset();
        self.ended = false;
    }
    /// One decoded source frame. Quantization depends on emitted frame boundaries;
    /// callers must preserve source packets instead of passing arbitrary host chunks.
    pub(crate) fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        assert!(
            !self.ended
                && input.len() == self.channels
                && input[0].len() <= 1048576
                && input.iter().all(|ch| ch.len() == input[0].len())
        );
        self.ended = end;
        if self.identity {
            return input.iter().map(|ch| ch.to_vec()).collect();
        }
        let converted: Vec<Vec<f32>> = input
            .iter()
            .map(|ch| ch.iter().map(|&v| v.scaled()).collect())
            .collect();
        let refs: Vec<&[f32]> = converted.iter().map(Vec::as_slice).collect();
        let mut normal = vec![Vec::new(); self.channels];
        for at in (0..input[0].len()).step_by(65536) {
            let stop = (at + 65536).min(input[0].len());
            let block: Vec<&[f32]> = refs.iter().map(|ch| &ch[at..stop]).collect();
            let data = self.inner.push(&block, false);
            for (dst, src) in normal.iter_mut().zip(data) {
                dst.extend(src);
            }
        }
        let mut output = self.convert_packet(normal);
        if end {
            let empty = vec![&[][..]; self.channels];
            let tail = self.inner.push(&empty, true);
            let tail = self.convert_packet(tail);
            for (dst, src) in output.iter_mut().zip(tail) {
                dst.extend(src);
            }
        }
        output
    }
    fn convert_packet(&self, packet: Vec<Vec<f32>>) -> Vec<Vec<T>> {
        // swri_audio_convert vectorizes len & ~15 samples per channel.
        // Preserve each emitted frame, including its separate EOF tail.
        let vector_len = packet[0].len() & !15;
        packet
            .into_iter()
            .map(|ch| {
                ch.into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if i < vector_len {
                            T::from_vector(v, if self.output_planar { 1 } else { self.channels })
                        } else {
                            T::from_scaled(v)
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// Fixed-point S16P interior selected by the original U8 resampling graph.
#[derive(Clone, Copy, Default)]
struct Fixed16(i32);
impl std::ops::Add for Fixed16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}
impl std::ops::Sub for Fixed16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}
impl Sample for Fixed16 {
    const DOUBLE: bool = false;
    fn from_double(value: f64) -> Self {
        Self(
            ((value * 32768.) as f32)
                .round_ties_even()
                .clamp(-32768., 32767.) as i32,
        )
    }
    fn double(self) -> f64 {
        f64::from(self.0)
    }
    fn madd(self, coefficient: Self, sum: Self) -> Self {
        Self(self.0.wrapping_mul(coefficient.0).wrapping_add(sum.0))
    }
    fn interpolate(self, other: Self, fraction: u64, denominator: u64) -> Self {
        let delta = i64::from(other.0.wrapping_sub(self.0));
        let adjustment = delta * fraction as i64 / denominator as i64;
        Self(self.0.wrapping_add(adjustment as i32))
    }
    fn finish(self) -> Self {
        Self((self.0.wrapping_add(16384) >> 15).clamp(-32768, 32767))
    }
}
pub(crate) struct U8Resampler {
    inner: Resampler<Fixed16>,
    channels: usize,
}
impl U8Resampler {
    pub(crate) fn new(from: u32, to: u32, channels: usize) -> Self {
        Self {
            inner: Resampler::new(from, to, channels),
            channels,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.inner.reset();
    }
    pub(crate) fn push(&mut self, input: &[&[u8]], end: bool) -> Vec<Vec<u8>> {
        assert!(
            input.len() == self.channels
                && input[0].len() <= 1048576
                && input.iter().all(|ch| ch.len() == input[0].len())
        );
        let mut output = vec![Vec::new(); self.channels];
        if input[0].is_empty() {
            let empty = vec![&[][..]; self.channels];
            let data = self.inner.push(&empty, end);
            for (dst, src) in output.iter_mut().zip(data) {
                dst.extend(src.into_iter().map(|v| ((v.0 >> 8) + 128) as u8));
            }
            return output;
        }
        for at in (0..input[0].len()).step_by(65536) {
            let stop = (at + 65536).min(input[0].len());
            let data: Vec<Vec<Fixed16>> = input
                .iter()
                .map(|ch| {
                    ch[at..stop]
                        .iter()
                        .map(|&v| Fixed16((i32::from(v) - 128) << 8))
                        .collect()
                })
                .collect();
            let refs: Vec<&[Fixed16]> = data.iter().map(Vec::as_slice).collect();
            let data = self.inner.push(&refs, end && stop == input[0].len());
            for (dst, src) in output.iter_mut().zip(data) {
                dst.extend(src.into_iter().map(|v| ((v.0 >> 8) + 128) as u8));
            }
        }
        output
    }
}

/// Source normalization for a resampler whose negotiated output is floating.
/// The input format determines the SWR interior: <=32-bit uses FLTP, double DBLP.
pub(crate) trait FloatInput: Copy + Send {
    const DOUBLE: bool;
    fn normalized(self) -> f64;
    fn normalized_float(self) -> f32;
}
impl FloatInput for u8 {
    const DOUBLE: bool = false;
    fn normalized(self) -> f64 {
        (f64::from(self) - 128.) / 128.
    }
    fn normalized_float(self) -> f32 {
        (f32::from(self) - 128.) / 128.
    }
}
impl FloatInput for i16 {
    const DOUBLE: bool = false;
    fn normalized(self) -> f64 {
        f64::from(self) / 32768.
    }
    fn normalized_float(self) -> f32 {
        f32::from(self) / 32768.
    }
}
impl FloatInput for i32 {
    const DOUBLE: bool = false;
    fn normalized(self) -> f64 {
        f64::from(self) / 2147483648.
    }
    fn normalized_float(self) -> f32 {
        self as f32 / 2147483648.
    }
}
impl FloatInput for f32 {
    const DOUBLE: bool = false;
    fn normalized(self) -> f64 {
        f64::from(self)
    }
    fn normalized_float(self) -> f32 {
        self
    }
}
impl FloatInput for f64 {
    const DOUBLE: bool = true;
    fn normalized(self) -> f64 {
        self
    }
    fn normalized_float(self) -> f32 {
        self as f32
    }
}
pub(crate) trait FloatOutput: Copy + Send {
    fn from_float(value: f32) -> Self;
    fn from_double(value: f64) -> Self;
}
impl FloatOutput for f32 {
    fn from_float(value: f32) -> Self {
        value
    }
    fn from_double(value: f64) -> Self {
        value as f32
    }
}
impl FloatOutput for f64 {
    fn from_float(value: f32) -> Self {
        f64::from(value)
    }
    fn from_double(value: f64) -> Self {
        value
    }
}
enum FloatInterior {
    Single(Resampler<f32>),
    Double(Resampler<f64>),
}
pub(crate) struct FloatOutputResampler<I, O> {
    inner: FloatInterior,
    channels: usize,
    identity: bool,
    ended: bool,
    sample: std::marker::PhantomData<(I, O)>,
}
impl<I: FloatInput, O: FloatOutput> FloatOutputResampler<I, O> {
    pub(crate) fn new(from: u32, to: u32, channels: usize) -> Self {
        Self {
            inner: if I::DOUBLE {
                FloatInterior::Double(Resampler::new(from, to, channels))
            } else {
                FloatInterior::Single(Resampler::new(from, to, channels))
            },
            channels,
            identity: from == to,
            ended: false,
            sample: std::marker::PhantomData,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.ended = false;
        match &mut self.inner {
            FloatInterior::Single(r) => r.reset(),
            FloatInterior::Double(r) => r.reset(),
        }
    }
    pub(crate) fn push(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<O>> {
        assert!(
            !self.ended
                && input.len() == self.channels
                && input[0].len() <= 1048576
                && input.iter().all(|ch| ch.len() == input[0].len())
        );
        self.ended = end;
        if self.identity {
            // SWR's full_convert bypass preserves S32 low bits on direct DBL output.
            return input
                .iter()
                .map(|ch| ch.iter().map(|&v| O::from_double(v.normalized())).collect())
                .collect();
        }
        let frames = input[0].len();
        let mut output = vec![Vec::new(); self.channels];
        // Empty EOF is still a real upstream flush frame.
        for at in (0..frames.max(1)).step_by(65536) {
            let stop = (at + 65536).min(frames);
            match &mut self.inner {
                FloatInterior::Single(r) => {
                    let data: Vec<Vec<f32>> = input
                        .iter()
                        .map(|ch| ch[at..stop].iter().map(|&v| v.normalized_float()).collect())
                        .collect();
                    let refs: Vec<&[f32]> = data.iter().map(Vec::as_slice).collect();
                    for (dst, src) in output.iter_mut().zip(r.push(&refs, end && stop == frames)) {
                        dst.extend(src.into_iter().map(O::from_float));
                    }
                }
                FloatInterior::Double(r) => {
                    let data: Vec<Vec<f64>> = input
                        .iter()
                        .map(|ch| ch[at..stop].iter().map(|&v| v.normalized()).collect())
                        .collect();
                    let refs: Vec<&[f64]> = data.iter().map(Vec::as_slice).collect();
                    for (dst, src) in output.iter_mut().zip(r.push(&refs, end && stop == frames)) {
                        dst.extend(src.into_iter().map(O::from_double));
                    }
                }
            }
        }
        output
    }
}
