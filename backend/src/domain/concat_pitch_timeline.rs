//! Seekable variable-frame Pitch reader. Source decoding owns packet boundaries;
//! the output clock is always 48 kHz and is independent of source sample count.
use super::concat_pitch::{Graph, RateStream, Sample};
use anyhow::{ensure, Context, Result};
use std::{collections::VecDeque, ops::Range};

pub(crate) type Timeline<T> = GraphTimeline<T, T, <T as Sample>::Rate>;
pub(crate) type FloatTimeline<I, O> =
    GraphTimeline<I, O, super::concat_resample::FloatOutputResampler<I, O>>;
pub(crate) struct GraphTimeline<I: Sample, O: Sample, R> {
    pitch: Graph<I, O, R>,
    channels: usize,
    source: Range<i64>,
    source_next: i64,
    output_origin: i64,
    output_cursor: u64,
    queued: Vec<VecDeque<O>>,
    ended: bool,
    next: Option<i64>,
}
impl<I: Sample, O: Sample, R: RateStream<I, O>> GraphTimeline<I, O, R> {
    pub(crate) fn new(
        source_rate: u32,
        channels: usize,
        semitones: f32,
        source: Range<i64>,
        output_origin: i64,
    ) -> Result<Self> {
        Self::new_with_source_planar(
            source_rate,
            channels,
            semitones,
            source,
            output_origin,
            false,
        )
    }
    pub(crate) fn new_with_source_planar(
        source_rate: u32,
        channels: usize,
        semitones: f32,
        source: Range<i64>,
        output_origin: i64,
        source_planar: bool,
    ) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&source_rate),
            "invalid Pitch source rate"
        );
        ensure!((1..=16).contains(&channels), "invalid Pitch channel count");
        ensure!(
            semitones.is_finite() && (-12. ..=12.).contains(&semitones),
            "invalid Pitch semitones"
        );
        let count = source
            .end
            .checked_sub(source.start)
            .context("Pitch source bounds overflow")?;
        ensure!(
            count >= 0 && count <= i64::from(source_rate) * 6 * 60 * 60,
            "Pitch source exceeds six-hour budget"
        );
        Ok(Self {
            pitch: Graph::new_with_source_planar(source_rate, channels, semitones, source_planar),
            channels,
            source_next: source.start,
            source,
            output_origin,
            output_cursor: 0,
            queued: vec![VecDeque::new(); channels],
            ended: false,
            next: None,
        })
    }
    fn reset(&mut self) {
        self.pitch.reset();
        self.source_next = self.source.start;
        self.output_cursor = 0;
        for channel in &mut self.queued {
            channel.clear();
        }
        self.ended = false;
    }
    fn pump(
        &mut self,
        read: &mut dyn FnMut(i64, usize) -> Result<Vec<Vec<I>>>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        checkpoint()?;
        let data = if self.source_next == self.source.end {
            let empty = vec![&[][..]; self.channels];
            self.ended = true;
            self.pitch.push_packet(&empty, true)
        } else {
            let max_frames = (self.source.end - self.source_next).min(65536) as usize;
            // A partial first/last frame is legal. Never concatenate decoded frames:
            // both WSOLA destination capacity and integer conversion depend on them.
            let packet = read(self.source_next, max_frames)?;
            ensure!(
                packet.len() == self.channels,
                "Pitch source channel mismatch"
            );
            let frames = packet[0].len();
            ensure!(
                frames > 0 && frames <= max_frames && packet.iter().all(|ch| ch.len() == frames),
                "invalid Pitch source packet"
            );
            ensure!(
                packet.iter().flatten().all(|&sample| sample.valid()),
                "nonfinite Pitch source packet"
            );
            let refs: Vec<&[I]> = packet.iter().map(Vec::as_slice).collect();
            let data = self.pitch.push_packet(&refs, false);
            self.source_next += frames as i64;
            data
        };
        ensure!(
            data.len() == self.channels && data.iter().all(|ch| ch.len() == data[0].len()),
            "invalid Pitch output packet"
        );
        ensure!(
            self.queued[0]
                .len()
                .checked_add(data[0].len())
                .is_some_and(|n| n <= 1048576),
            "Pitch output queue exceeds budget"
        );
        for (dst, src) in self.queued.iter_mut().zip(data) {
            dst.extend(src);
        }
        checkpoint()?;
        Ok(())
    }
    /// `read` returns ONE decoded source frame, clipped to `max_frames` at the
    /// source end. It must preserve actual codec/demuxer boundaries on replay.
    /// Output before placement and after natural EOF is silence. No post-effect
    /// trimming is performed here; the host owns its output clip bounds.
    pub(crate) fn render(
        &mut self,
        start: i64,
        frames: usize,
        read: &mut dyn FnMut(i64, usize) -> Result<Vec<Vec<I>>>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<O>>> {
        ensure!(
            frames <= 65536,
            "Pitch timeline output block exceeds budget"
        );
        let end = start
            .checked_add(frames as i64)
            .context("Pitch output clock overflow")?;
        if self.next != Some(start) {
            self.reset();
        }
        // Any read, shape or cancellation failure invalidates recursive state.
        self.next = None;
        checkpoint()?;
        let mut output = vec![vec![O::silence(); frames]; self.channels];
        let leading = self
            .output_origin
            .saturating_sub(start)
            .clamp(0, frames as i64) as usize;
        if leading < frames {
            let target = start
                .max(self.output_origin)
                .checked_sub(self.output_origin)
                .context("Pitch output age overflow")? as u64;
            ensure!(target >= self.output_cursor, "Pitch output cursor mismatch");
            while self.output_cursor < target {
                if self.queued[0].is_empty() {
                    if self.ended {
                        self.output_cursor = target;
                        break;
                    }
                    self.pump(read, checkpoint)?;
                    continue;
                }
                let discard =
                    (target - self.output_cursor).min(self.queued[0].len() as u64) as usize;
                for channel in &mut self.queued {
                    channel.drain(..discard);
                }
                self.output_cursor += discard as u64;
                checkpoint()?;
            }
            let mut at = leading;
            while at < frames {
                if self.queued[0].is_empty() {
                    if self.ended {
                        self.output_cursor += (frames - at) as u64;
                        break;
                    }
                    self.pump(read, checkpoint)?;
                    continue;
                }
                let count = (frames - at).min(self.queued[0].len());
                for (dst, src) in output.iter_mut().zip(&mut self.queued) {
                    for sample in &mut dst[at..at + count] {
                        *sample = src.pop_front().unwrap();
                    }
                }
                at += count;
                self.output_cursor += count as u64;
                checkpoint()?;
            }
        }
        checkpoint()?;
        self.next = Some(end);
        Ok(output)
    }
    #[cfg(test)]
    pub(crate) fn queued_frames(&self) -> usize {
        self.queued[0].len()
    }
}
