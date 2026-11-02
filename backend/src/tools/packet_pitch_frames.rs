//! Sequential decoded-source Pitch stream with exact output audio-frame boundaries.
use super::{
    packet_pcm::{Decoded, Frames, Scalar},
    packet_pitch::{format_plan, Block},
};
use crate::domain::{
    concat_format_plan::{Filter, Format, Plan},
    concat_pitch::{Graph, RateStream, Sample},
    concat_resample::{FloatInput, FloatOutputResampler},
};
use anyhow::{bail, ensure, Result};
use std::collections::VecDeque;
trait Emit: Sample + Send + 'static {
    fn block(data: Vec<Vec<Self>>) -> Block;
}
macro_rules! emit {($($ty:ty:$variant:ident),+)=>{$(impl Emit for $ty {fn block(data:Vec<Vec<Self>>)->Block{Block::$variant(data)}})+};}
emit!(u8:U8,i16:S16,i32:S32,f32:F32,f64:F64);
trait Read: Send {
    fn next_packet(&mut self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>>;
    fn reset(&mut self);
}
struct Stream<I: Scalar + Sample, O: Sample, R> {
    source: Frames<I>,
    graph: Graph<I, O, R>,
    at: u64,
    pending: VecDeque<Vec<Vec<O>>>,
    queued: usize,
    ended: bool,
    failed: bool,
}
impl<I: Scalar + Sample, O: Sample, R: RateStream<I, O>> Stream<I, O, R> {
    fn new(source: Frames<I>, semitones: f64) -> Result<Self> {
        ensure!(
            semitones.is_finite() && (-12. ..=12.).contains(&semitones),
            "invalid framed Pitch semitones"
        );
        let spec = source.spec();
        Ok(Self {
            graph: Graph::new_with_derived_source_planar(
                spec.rate,
                spec.channels,
                semitones,
                spec.source_planar,
            ),
            at: spec.start,
            source,
            pending: VecDeque::new(),
            queued: 0,
            ended: false,
            failed: false,
        })
    }
    fn read(&mut self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<Option<Vec<Vec<O>>>> {
        checkpoint()?;
        loop {
            if let Some(packet) = self.pending.pop_front() {
                self.queued -= packet[0].len();
                checkpoint()?;
                return Ok(Some(packet));
            }
            if self.ended {
                return Ok(None);
            }
            let output = if self.at == self.source.end() {
                self.ended = true;
                self.graph
                    .push_packet_frames(&vec![&[][..]; self.source.spec().channels], true)
            } else {
                let input = self.source.read_packet(
                    self.at as i64,
                    (self.source.end() - self.at).min(65536) as usize,
                )?;
                self.at += input[0].len() as u64;
                let refs: Vec<&[I]> = input.iter().map(Vec::as_slice).collect();
                self.graph.push_packet_frames(&refs, false)
            };
            for packet in output {
                ensure!(
                    packet.len() == self.source.spec().channels
                        && !packet[0].is_empty()
                        && packet
                            .iter()
                            .all(|ch| ch.len() == packet[0].len() && ch.iter().all(|&s| s.valid())),
                    "invalid framed Pitch output"
                );
                self.queued = self
                    .queued
                    .checked_add(packet[0].len())
                    .filter(|&n| n <= 1048576)
                    .ok_or_else(|| anyhow::anyhow!("framed Pitch output queue exceeds budget"))?;
                self.pending.push_back(packet);
            }
            checkpoint()?;
        }
    }
}
impl<I: Scalar + Sample + Send, O: Emit, R: RateStream<I, O> + Send> Read for Stream<I, O, R>
where
    I::Rate: Send,
{
    fn next_packet(&mut self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(!self.failed, "framed Pitch requires reset after failure");
        self.failed = true;
        let packet = self.read(checkpoint)?;
        self.failed = false;
        Ok(packet.map(O::block))
    }
    fn reset(&mut self) {
        self.graph.reset();
        self.at = self.source.spec().start;
        self.pending.clear();
        self.queued = 0;
        self.ended = false;
        self.failed = false;
    }
}
fn select<I: Scalar + Sample + FloatInput + Emit>(
    source: Frames<I>,
    native: Format,
    output: Format,
    semitones: f64,
) -> Result<Box<dyn Read>>
where
    I::Rate: Send + 'static,
{
    if output == native {
        return Ok(Box::new(Stream::<I, I, I::Rate>::new(source, semitones)?));
    }
    Ok(match output {
        Format::F32 => Box::new(Stream::<I, f32, FloatOutputResampler<I, f32>>::new(
            source, semitones,
        )?),
        Format::F64 => Box::new(Stream::<I, f64, FloatOutputResampler<I, f64>>::new(
            source, semitones,
        )?),
        _ => bail!("negotiated integer framed Pitch conversion is not implemented"),
    })
}
pub(crate) struct Reader {
    pub plan: Plan,
    source: Box<dyn Read>,
}
impl Reader {
    pub(crate) fn new(
        source: Decoded,
        following: &[Filter],
        sink: Format,
        semitones: f32,
    ) -> Result<Self> {
        Self::new_derived(source, following, sink, f64::from(semitones))
    }
    pub(crate) fn new_derived(
        source: Decoded,
        following: &[Filter],
        sink: Format,
        semitones: f64,
    ) -> Result<Self> {
        let plan = format_plan(&source, following, sink)?;
        let output = plan.stages[3].output;
        let source = match source {
            Decoded::U8(s) => select(s, Format::U8, output, semitones)?,
            Decoded::S16(s) => select(s, Format::S16, output, semitones)?,
            Decoded::S32(s) => select(s, Format::S32, output, semitones)?,
            Decoded::F32(s) => select(s, Format::F32, output, semitones)?,
            Decoded::F64(s) => select(s, Format::F64, output, semitones)?,
        };
        Ok(Self { plan, source })
    }
    /// Natural EOF is None. No placement padding or frame coalescing is performed.
    pub(crate) fn next_packet(
        &mut self,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Option<Block>> {
        self.source.next_packet(checkpoint)
    }
    /// Reset the complete downstream graph too: after an error this stream will
    /// not silently resume with missing or duplicated audio frames.
    pub(crate) fn reset(&mut self) {
        self.source.reset();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn movable_to_mixer_workers() {
        fn send<T: Send>() {}
        send::<super::Reader>();
    }
}
