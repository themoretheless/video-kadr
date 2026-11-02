//! Typed finite feed-forward Echo, including original scalar arithmetic and EOF packets.
use anyhow::{ensure, Result};
pub(crate) trait Sample: Copy + Default {
    fn double(self) -> f64;
    fn accumulate(self, decay: f32, sum: f64) -> f64;
    fn clipped(value: f64) -> Self;
}
macro_rules! integer {
    ($ty:ty) => {
        impl Sample for $ty {
            fn double(self) -> f64 {
                f64::from(self)
            }
            fn accumulate(self, decay: f32, sum: f64) -> f64 {
                sum + f64::from(self as f32 * decay)
            }
            fn clipped(value: f64) -> Self {
                value.clamp(Self::MIN as f64, Self::MAX as f64) as Self
            }
        }
    };
}
integer!(i16);
integer!(i32);
impl Sample for f32 {
    fn double(self) -> f64 {
        f64::from(self)
    }
    fn accumulate(self, decay: f32, sum: f64) -> f64 {
        sum + f64::from(self * decay)
    }
    fn clipped(value: f64) -> Self {
        value.clamp(-1., 1.) as Self
    }
}
impl Sample for f64 {
    fn double(self) -> f64 {
        self
    }
    fn accumulate(self, decay: f32, sum: f64) -> f64 {
        self.mul_add(f64::from(decay), sum)
    }
    fn clipped(value: f64) -> Self {
        value.clamp(-1., 1.)
    }
}
pub(crate) fn mix<T: Sample>(
    input: T,
    delayed: impl Iterator<Item = (T, f32)>,
    in_gain: f32,
    out_gain: f32,
) -> T {
    let mut value = input.double() * f64::from(in_gain);
    for (sample, decay) in delayed {
        value = sample.accumulate(decay, value);
    }
    T::clipped(value * f64::from(out_gain))
}
pub(crate) struct Echo<T: Sample> {
    history: Vec<Vec<T>>,
    position: usize,
    delays: Vec<usize>,
    decays: Vec<f32>,
    in_gain: f32,
    out_gain: f32,
    flushing: bool,
    tail: usize,
}
impl<T: Sample> Echo<T> {
    pub(crate) fn new(
        rate: u32,
        channels: usize,
        in_gain: f32,
        out_gain: f32,
        delays_ms: &[f32],
        decays: &[f32],
    ) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "invalid Echo audio shape"
        );
        ensure!(
            in_gain.is_finite()
                && out_gain.is_finite()
                && (0. ..=1.).contains(&in_gain)
                && (0. ..=1.).contains(&out_gain),
            "invalid Echo gain"
        );
        ensure!(
            (1..=16).contains(&delays_ms.len()) && delays_ms.len() == decays.len(),
            "invalid Echo tap count"
        );
        ensure!(
            delays_ms
                .iter()
                .all(|v| v.is_finite() && *v > 0. && *v <= 90000.)
                && decays.iter().all(|v| v.is_finite() && *v > 0. && *v <= 1.),
            "invalid Echo tap"
        );
        // Original delay parsing and multiplication are float, division is double.
        let delays: Vec<usize> = delays_ms
            .iter()
            .map(|&v| (f64::from(v * rate as f32) / 1000.) as usize)
            .collect();
        let maximum = *delays.iter().max().unwrap();
        ensure!(
            maximum > 0
                && maximum
                    .checked_mul(channels)
                    .is_some_and(|n| n <= 8_388_608),
            "Echo delay state exceeds budget"
        );
        Ok(Self {
            history: vec![vec![T::default(); maximum]; channels],
            position: 0,
            delays,
            decays: decays.to_vec(),
            in_gain,
            out_gain,
            flushing: false,
            tail: maximum,
        })
    }
    pub(crate) fn reset(&mut self) {
        for ch in &mut self.history {
            ch.fill(T::default());
        }
        self.position = 0;
        self.flushing = false;
        self.tail = self.history[0].len();
    }
    fn process(&mut self, input: &[&[T]]) -> Vec<Vec<T>> {
        let frames = input[0].len();
        let maximum = self.history[0].len();
        let mut output = vec![vec![T::default(); frames]; self.history.len()];
        for frame in 0..frames {
            for ch in 0..self.history.len() {
                let sample = input[ch][frame];
                output[ch][frame] = mix(
                    sample,
                    self.delays
                        .iter()
                        .zip(&self.decays)
                        .map(|(&delay, &decay)| {
                            (
                                self.history[ch][(self.position + maximum - delay) % maximum],
                                decay,
                            )
                        }),
                    self.in_gain,
                    self.out_gain,
                );
                self.history[ch][self.position] = sample;
            }
            self.position = (self.position + 1) % maximum;
        }
        output
    }
    /// One input packet; the output preserves its frame boundary and count.
    pub(crate) fn push_packet(&mut self, input: &[&[T]]) -> Result<Vec<Vec<T>>> {
        ensure!(
            !self.flushing && input.len() == self.history.len(),
            "Echo input after EOF or channel mismatch"
        );
        let frames = input[0].len();
        ensure!(
            frames <= 1048576
                && input
                    .iter()
                    .all(|ch| ch.len() == frames && ch.iter().all(|v| v.double().is_finite())),
            "invalid Echo input packet"
        );
        Ok(self.process(input))
    }
    /// Original EOF extension is max(delay) samples, in packets of at most 2048.
    pub(crate) fn finish_packet(&mut self) -> Option<Vec<Vec<T>>> {
        self.flushing = true;
        if self.tail == 0 {
            return None;
        }
        let frames = self.tail.min(2048);
        self.tail -= frames;
        let zeros = vec![T::default(); frames];
        let input = vec![zeros.as_slice(); self.history.len()];
        Some(self.process(&input))
    }
    #[cfg(test)]
    pub(crate) fn retained_samples(&self) -> usize {
        self.history.iter().map(Vec::len).sum()
    }
}
