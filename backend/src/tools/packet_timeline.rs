//! Adapts complete variable-frame DSP streams to bounded 48 kHz host reads.
//! Host placement trims output; source packets and DSP EOF are never padded.
use super::{packet_echo::floating64, packet_pitch::Block};
use anyhow::{ensure, Context, Result};
use std::{collections::VecDeque, ops::Range};
pub(crate) trait Source: Send {
    fn next_packet(&mut self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>>;
    fn reset(&mut self);
}
macro_rules! source {
    ($ty:ty) => {
        impl Source for $ty {
            fn next_packet(
                &mut self,
                checkpoint: &mut dyn FnMut() -> Result<()>,
            ) -> Result<Option<Block>> {
                <$ty>::next_packet(self, checkpoint)
            }
            fn reset(&mut self) {
                <$ty>::reset(self)
            }
        }
    };
}
source!(super::packet_pitch_frames::Reader);
source!(super::packet_voice::Reader);
pub(crate) struct Timeline<R> {
    source: R,
    bounds: Range<i64>,
    channels: usize,
    queue: Vec<VecDeque<f64>>,
    cursor: u64,
    ended: bool,
    failed: bool,
}
impl<R: Source> Timeline<R> {
    pub(crate) fn new(source: R, channels: usize, bounds: Range<i64>) -> Result<Self> {
        ensure!(
            (1..=16).contains(&channels),
            "invalid packet timeline channels"
        );
        let duration = bounds
            .end
            .checked_sub(bounds.start)
            .context("packet timeline bounds overflow")?;
        ensure!(
            (0..=48000 * 6 * 60 * 60).contains(&duration),
            "invalid packet timeline duration"
        );
        Ok(Self {
            source,
            bounds,
            channels,
            queue: vec![VecDeque::new(); channels],
            cursor: 0,
            ended: false,
            failed: false,
        })
    }
    pub(crate) fn reset(&mut self) {
        self.source.reset();
        for ch in &mut self.queue {
            ch.clear();
        }
        self.cursor = 0;
        self.ended = false;
        self.failed = false;
    }
    fn pump(&mut self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
        checkpoint()?;
        if let Some(packet) = self.source.next_packet(checkpoint)? {
            let data = floating64(packet);
            ensure!(
                data.len() == self.channels,
                "packet timeline channel mismatch"
            );
            let count = data[0].len();
            ensure!(
                count > 0
                    && count <= 1048576
                    && data
                        .iter()
                        .all(|ch| ch.len() == count && ch.iter().all(|v| v.is_finite())),
                "invalid packet timeline output"
            );
            ensure!(
                self.queue[0]
                    .len()
                    .checked_add(count)
                    .is_some_and(|n| n <= 1048576),
                "packet timeline queue exceeds budget"
            );
            for (dst, src) in self.queue.iter_mut().zip(data) {
                dst.extend(src);
            }
        } else {
            self.ended = true;
        }
        checkpoint()?;
        Ok(())
    }
    fn discard_to(
        &mut self,
        target: u64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        while self.cursor < target {
            checkpoint()?;
            if self.queue[0].is_empty() {
                if self.ended {
                    self.cursor = target;
                    break;
                }
                self.pump(checkpoint)?;
                continue;
            }
            let count = (target - self.cursor).min(self.queue[0].len() as u64) as usize;
            for ch in &mut self.queue {
                ch.drain(..count);
            }
            self.cursor += count as u64;
        }
        Ok(())
    }
    /// Returns host silence outside placement and after natural EOF. Backward
    /// seeks replay the entire chain from source-in, including audible latency.
    /// An error poisons the chain until reset; partial output is never returned.
    pub(crate) fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<f64>>> {
        ensure!(!self.failed, "packet timeline requires reset after failure");
        ensure!(frames <= 65536, "packet timeline block exceeds budget");
        let end = start
            .checked_add(frames as i64)
            .context("packet timeline clock overflow")?;
        self.failed = true;
        checkpoint()?;
        let mut output = vec![vec![0.; frames]; self.channels];
        let first = start.max(self.bounds.start);
        let last = end.min(self.bounds.end);
        if first < last {
            let target = first
                .checked_sub(self.bounds.start)
                .context("packet timeline age overflow")? as u64;
            if target < self.cursor {
                self.reset();
                self.failed = true;
            }
            self.discard_to(target, checkpoint)?;
            let mut at = (first - start) as usize;
            let stop = (last - start) as usize;
            while at < stop {
                checkpoint()?;
                if self.queue[0].is_empty() {
                    if self.ended {
                        self.cursor += (stop - at) as u64;
                        break;
                    }
                    self.pump(checkpoint)?;
                    continue;
                }
                let count = (stop - at).min(self.queue[0].len());
                for (dst, src) in output.iter_mut().zip(&mut self.queue) {
                    for sample in &mut dst[at..at + count] {
                        *sample = src.pop_front().unwrap();
                    }
                }
                at += count;
                self.cursor += count as u64;
            }
        }
        checkpoint()?;
        self.failed = false;
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Packets {
        data: Vec<Vec<f64>>,
        at: usize,
        invalid: bool,
    }
    impl Source for Packets {
        fn next_packet(&mut self, _: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
            if self.invalid {
                return Ok(Some(Block::F64(vec![vec![f64::NAN]])));
            }
            if self.at == self.data[0].len() {
                return Ok(None);
            }
            let end = (self.at + 7).min(self.data[0].len());
            let out = self
                .data
                .iter()
                .map(|ch| ch[self.at..end].to_vec())
                .collect();
            self.at = end;
            Ok(Some(Block::F64(out)))
        }
        fn reset(&mut self) {
            self.at = 0;
        }
    }
    fn make(invalid: bool) -> Timeline<Packets> {
        Timeline::new(
            Packets {
                data: vec![(0..31).map(|n| n as f64 / 32.).collect()],
                at: 0,
                invalid,
            },
            1,
            19..80,
        )
        .unwrap()
    }
    #[test]
    fn placement_seeks_eof_and_host_blocks() {
        let mut timeline = make(false);
        for (start, count) in [
            (0, 23),
            (23, 4),
            (27, 27),
            (54, 32),
            (30, 17),
            (19, 61),
            (49, 3),
            (-12, 20),
            (79, 4),
            (19, 0),
        ] {
            let result = timeline.render(start, count, &mut || Ok(())).unwrap();
            for (i, &v) in result[0].iter().enumerate() {
                let age = start + i as i64 - 19;
                assert_eq!(
                    v,
                    if (0..31).contains(&age) {
                        age as f64 / 32.
                    } else {
                        0.
                    }
                );
            }
        }
    }
    #[test]
    fn placement_trims_before_natural_eof() {
        let mut timeline = make(false);
        timeline.bounds.end = 25;
        assert_eq!(
            timeline.render(22, 7, &mut || Ok(())).unwrap()[0],
            [3. / 32., 4. / 32., 5. / 32., 0., 0., 0., 0.]
        );
    }
    #[test]
    fn cancellation_poison_and_reset() {
        let mut timeline = make(false);
        let mut calls = 0;
        assert!(timeline
            .render(42, 9, &mut || {
                calls += 1;
                ensure!(calls < 6, "cancelled");
                Ok(())
            })
            .is_err());
        assert!(timeline.render(19, 1, &mut || Ok(())).is_err());
        timeline.reset();
        assert_eq!(
            timeline.render(20, 1, &mut || Ok(())).unwrap()[0],
            [1. / 32.]
        );
        assert!(make(true).render(19, 1, &mut || Ok(())).is_err());
        assert!(timeline.render(i64::MAX, 1, &mut || Ok(())).is_err());
        assert!(timeline.render(0, 65537, &mut || Ok(())).is_err());
    }
}
