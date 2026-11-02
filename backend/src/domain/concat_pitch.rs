//! Owned Concat Pitch graph with explicit 48 kHz output and source packet framing.
use super::{concat_resample, concat_tempo};
pub(crate) trait RateStream<T: concat_tempo::Sample, U: concat_tempo::Sample = T> {
    fn create(from: u32, to: u32, channels: usize) -> Self;
    fn reset(&mut self);
    fn set_output_planar(&mut self, _planar: bool) {}
    fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<U>>;
}
impl<T: concat_resample::Sample + concat_tempo::Sample> RateStream<T>
    for concat_resample::Resampler<T>
{
    fn create(from: u32, to: u32, channels: usize) -> Self {
        Self::new(from, to, channels)
    }
    fn reset(&mut self) {
        concat_resample::Resampler::reset(self);
    }
    fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        assert!(!input.is_empty() && input[0].len() <= 1048576);
        if input[0].is_empty() {
            return concat_resample::Resampler::push(self, input, end);
        }
        let mut output = vec![Vec::new(); input.len()];
        for at in (0..input[0].len()).step_by(65536) {
            let stop = (at + 65536).min(input[0].len());
            let refs: Vec<&[T]> = input.iter().map(|ch| &ch[at..stop]).collect();
            let data = concat_resample::Resampler::push(self, &refs, end && stop == input[0].len());
            for (dst, src) in output.iter_mut().zip(data) {
                dst.extend(src);
            }
        }
        output
    }
}
impl<T: concat_resample::IntegerSample + concat_tempo::Sample> RateStream<T>
    for concat_resample::IntegerResampler<T>
{
    fn create(from: u32, to: u32, channels: usize) -> Self {
        Self::new(from, to, channels)
    }
    fn set_output_planar(&mut self, planar: bool) {
        concat_resample::IntegerResampler::set_output_planar(self, planar);
    }
    fn reset(&mut self) {
        concat_resample::IntegerResampler::reset(self);
    }
    fn push(&mut self, input: &[&[T]], end: bool) -> Vec<Vec<T>> {
        concat_resample::IntegerResampler::push(self, input, end)
    }
}
pub(crate) trait Sample: concat_tempo::Sample {
    fn valid(self) -> bool {
        true
    }
    fn silence() -> Self {
        Self::default()
    }
    type Rate: RateStream<Self>;
}
impl Sample for f32 {
    fn valid(self) -> bool {
        self.is_finite()
    }
    type Rate = concat_resample::Resampler<f32>;
}
impl Sample for f64 {
    fn valid(self) -> bool {
        self.is_finite()
    }
    type Rate = concat_resample::Resampler<f64>;
}
impl Sample for i16 {
    type Rate = concat_resample::IntegerResampler<i16>;
}
impl Sample for i32 {
    type Rate = concat_resample::IntegerResampler<i32>;
}
pub(crate) type Pitch<T> = Graph<T, T, <T as Sample>::Rate>;
pub(crate) type FloatPitch<I, O> = Graph<I, O, concat_resample::FloatOutputResampler<I, O>>;
pub(crate) struct Graph<I: Sample, O: Sample, R> {
    normalize: I::Rate,
    shift: R,
    tempo: concat_tempo::Tempo<O>,
    channels: usize,
    packet_frames: usize,
    input: Vec<Vec<I>>,
    ended: bool,
}
impl<I: Sample, O: Sample, R: RateStream<I, O>> Graph<I, O, R> {
    /// Raw PCM profile: the source demuxer rounds sample_rate/10 down to a power of two.
    pub(crate) fn new(input_rate: u32, channels: usize, semitones: f32) -> Self {
        Self::with_packet_size(
            input_rate,
            channels,
            semitones,
            1usize << (input_rate / 10).ilog2(),
        )
    }
    pub(crate) fn new_with_source_planar(
        input_rate: u32,
        channels: usize,
        semitones: f32,
        source_planar: bool,
    ) -> Self {
        let mut graph = Self::new(input_rate, channels, semitones);
        graph.normalize.set_output_planar(source_planar);
        graph
    }
    pub(crate) fn with_packet_size(
        input_rate: u32,
        channels: usize,
        semitones: f32,
        packet_frames: usize,
    ) -> Self {
        Self::with_derived_packet_size(input_rate, channels, f64::from(semitones), packet_frames)
    }
    /// Preserve source-derived shifts until the original decimal rate/tempo formatting.
    pub(crate) fn new_with_derived_source_planar(
        input_rate: u32,
        channels: usize,
        semitones: f64,
        source_planar: bool,
    ) -> Self {
        let mut graph = Self::with_derived_packet_size(
            input_rate,
            channels,
            semitones,
            1usize << (input_rate / 10).ilog2(),
        );
        graph.normalize.set_output_planar(source_planar);
        graph
    }
    fn with_derived_packet_size(
        input_rate: u32,
        channels: usize,
        semitones: f64,
        packet_frames: usize,
    ) -> Self {
        assert!(
            semitones.is_finite()
                && (-12. ..=12.).contains(&semitones)
                && (1..=65536).contains(&packet_frames)
        );
        let ratio = 2f64.powf(semitones / 12.);
        let shifted_rate = format!("{:.6}", 48000. * ratio)
            .parse::<f64>()
            .unwrap()
            .round_ties_even() as u32;
        let tempo = format!("{:.8}", 1. / ratio).parse::<f64>().unwrap();
        Self {
            normalize: I::Rate::create(input_rate, 48000, channels),
            shift: R::create(shifted_rate, 48000, channels),
            tempo: concat_tempo::Tempo::new(48000, channels, tempo),
            channels,
            packet_frames,
            input: (0..channels)
                .map(|_| Vec::with_capacity(packet_frames))
                .collect(),
            ended: false,
        }
    }
    pub(crate) fn output_rate(&self) -> u32 {
        48000
    }
    pub(crate) fn reset(&mut self) {
        self.normalize.reset();
        self.shift.reset();
        self.tempo.reset();
        for ch in &mut self.input {
            ch.clear();
        }
        self.ended = false;
    }
    fn flatten(channels: usize, packets: Vec<Vec<Vec<O>>>) -> Vec<Vec<O>> {
        let mut output = vec![Vec::new(); channels];
        for packet in packets {
            for (dst, src) in output.iter_mut().zip(packet) {
                dst.extend(src);
            }
        }
        output
    }
    fn feed_tempo(&mut self, data: &[Vec<O>], output: &mut Vec<Vec<Vec<O>>>) {
        if data[0].is_empty() {
            return;
        }
        let input: Vec<&[O]> = data.iter().map(Vec::as_slice).collect();
        output.extend(self.tempo.push_frames(&input, false));
    }
    fn feed_shift(&mut self, data: &[Vec<I>], output: &mut Vec<Vec<Vec<O>>>) {
        if data[0].is_empty() {
            return;
        }
        // A resampled source frame can exceed the primitive input block limit.
        // Keep it one transport frame: integer SIMD tails depend on this boundary.
        let input: Vec<&[I]> = data.iter().map(Vec::as_slice).collect();
        let shifted = self.shift.push(&input, false);
        self.feed_tempo(&shifted, output);
    }
    fn process_packet(&mut self, input: &[&[I]], output: &mut Vec<Vec<Vec<O>>>) {
        let normalized = self.normalize.push(input, false);
        self.feed_shift(&normalized, output);
    }
    fn finish(&mut self, output: &mut Vec<Vec<Vec<O>>>) {
        let empty: Vec<&[I]> = vec![&[][..]; self.channels];
        let normalized = self.normalize.push(&empty, true);
        self.feed_shift(&normalized, output);
        let shifted = self.shift.push(&empty, true);
        self.feed_tempo(&shifted, output);
        let tempo_empty: Vec<&[O]> = vec![&[][..]; self.channels];
        output.extend(self.tempo.push_frames(&tempo_empty, true));
        self.ended = true;
    }
    pub(crate) fn push_packet(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<O>> {
        let packets = self.push_packet_frames(input, end);
        Self::flatten(self.channels, packets)
    }
    pub(crate) fn push(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<O>> {
        let packets = self.push_frames(input, end);
        Self::flatten(self.channels, packets)
    }
    /// A decoded source frame, preserving codec/demuxer boundaries explicitly.
    pub(crate) fn push_packet_frames(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<Vec<O>>> {
        assert!(!self.ended && self.input[0].is_empty());
        let mut output = Vec::new();
        if !input[0].is_empty() {
            self.process_packet(input, &mut output);
        }
        if end {
            self.finish(&mut output);
        }
        output
    }
    /// Arbitrary host chunks regrouped on the configured canonical source-packet grid.
    pub(crate) fn push_frames(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<Vec<O>>> {
        assert!(
            !self.ended
                && input.len() == self.channels
                && input[0].len() <= 65536
                && input.iter().all(|ch| ch.len() == input[0].len())
        );
        let mut output = Vec::new();
        let mut at = 0;
        while at < input[0].len() {
            let count = (self.packet_frames - self.input[0].len()).min(input[0].len() - at);
            for ch in 0..self.channels {
                self.input[ch].extend_from_slice(&input[ch][at..at + count]);
            }
            at += count;
            if self.input[0].len() == self.packet_frames {
                let mut packet = std::mem::take(&mut self.input);
                let refs: Vec<&[I]> = packet.iter().map(Vec::as_slice).collect();
                self.process_packet(&refs, &mut output);
                for ch in &mut packet {
                    ch.clear();
                }
                self.input = packet;
            }
        }
        if end {
            if !self.input[0].is_empty() {
                let mut packet = std::mem::take(&mut self.input);
                let refs: Vec<&[I]> = packet.iter().map(Vec::as_slice).collect();
                self.process_packet(&refs, &mut output);
                for ch in &mut packet {
                    ch.clear();
                }
                self.input = packet;
            }
            self.finish(&mut output);
        }
        output
    }
}

impl RateStream<u8> for concat_resample::U8Resampler {
    fn create(from: u32, to: u32, channels: usize) -> Self {
        Self::new(from, to, channels)
    }
    fn reset(&mut self) {
        concat_resample::U8Resampler::reset(self);
    }
    fn push(&mut self, input: &[&[u8]], end: bool) -> Vec<Vec<u8>> {
        concat_resample::U8Resampler::push(self, input, end)
    }
}
impl Sample for u8 {
    fn silence() -> Self {
        128
    }
    type Rate = concat_resample::U8Resampler;
}

impl<I: Sample + concat_resample::FloatInput, O: Sample + concat_resample::FloatOutput>
    RateStream<I, O> for concat_resample::FloatOutputResampler<I, O>
{
    fn create(from: u32, to: u32, channels: usize) -> Self {
        Self::new(from, to, channels)
    }
    fn reset(&mut self) {
        concat_resample::FloatOutputResampler::reset(self);
    }
    fn push(&mut self, input: &[&[I]], end: bool) -> Vec<Vec<O>> {
        concat_resample::FloatOutputResampler::push(self, input, end)
    }
}
