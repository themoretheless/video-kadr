//! Ordered Pitch stage over an existing packet source. Never coalesces transport frames.
use super::{packet_pitch::Block, packet_timeline::Source};
use crate::domain::{
    concat_format_plan::Format,
    concat_pitch::{Graph, RateStream, Sample},
    concat_resample::{FloatInput, FloatOutputResampler},
};
use anyhow::{bail, ensure, Result};
use std::collections::VecDeque;
trait Transport: Sample + FloatInput + Send + 'static {
    fn take(block: Block) -> Result<Vec<Vec<Self>>>;
    fn block(data: Vec<Vec<Self>>) -> Block;
}
macro_rules! transport {($($ty:ty:$tag:ident),+)=>{$(impl Transport for $ty {
    fn take(block:Block)->Result<Vec<Vec<Self>>>{match block {Block::$tag(data)=>Ok(data),_=>bail!("ordered Pitch input scalar mismatch")}}
    fn block(data:Vec<Vec<Self>>)->Block {Block::$tag(data)}
})+};}
transport!(u8:U8,i16:S16,i32:S32,f32:F32,f64:F64);
trait Process: Send {
    fn push(&mut self, block: Option<Block>) -> Result<Vec<Block>>;
    fn reset(&mut self);
}
struct Filter<I: Sample, O: Sample, R: RateStream<I, O>> {
    graph: Graph<I, O, R>,
    channels: usize,
}
impl<I: Transport, O: Transport, R: RateStream<I, O> + Send> Process for Filter<I, O, R>
where
    I::Rate: Send,
{
    fn push(&mut self, block: Option<Block>) -> Result<Vec<Block>> {
        let end = block.is_none();
        let input = if let Some(block) = block {
            I::take(block)?
        } else {
            vec![Vec::new(); self.channels]
        };
        ensure!(
            input.len() == self.channels,
            "ordered Pitch channel mismatch"
        );
        let count = input[0].len();
        ensure!(
            count <= 1048576
                && (end || count > 0)
                && input
                    .iter()
                    .all(|ch| ch.len() == count && ch.iter().all(|&v| v.valid())),
            "invalid ordered Pitch packet"
        );
        let refs = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Ok(self
            .graph
            .push_packet_frames(&refs, end)
            .into_iter()
            .map(O::block)
            .collect())
    }
    fn reset(&mut self) {
        self.graph.reset();
    }
}
fn select<I: Transport>(
    input: Format,
    output: Format,
    channels: usize,
    pitch: f64,
) -> Result<Box<dyn Process>>
where
    I::Rate: Send + 'static,
{
    if input == output {
        return Ok(Box::new(Filter::<I, I, I::Rate> {
            graph: Graph::new_with_derived_source_planar(48000, channels, pitch, false),
            channels,
        }));
    }
    Ok(match output {
        Format::F32 => Box::new(Filter::<I, f32, FloatOutputResampler<I, f32>> {
            graph: Graph::new_with_derived_source_planar(48000, channels, pitch, false),
            channels,
        }),
        Format::F64 => Box::new(Filter::<I, f64, FloatOutputResampler<I, f64>> {
            graph: Graph::new_with_derived_source_planar(48000, channels, pitch, false),
            channels,
        }),
        _ => bail!("ordered Pitch integer scalar transition is not implemented"),
    })
}
pub(crate) struct Node {
    source: Box<dyn Source>,
    filter: Box<dyn Process>,
    pending: VecDeque<Block>,
    queued: usize,
    ended: bool,
    failed: bool,
}
impl Node {
    pub(crate) fn new(
        source: Box<dyn Source>,
        input: Format,
        output: Format,
        channels: usize,
        pitch: f32,
    ) -> Result<Self> {
        Self::new_derived(source, input, output, channels, f64::from(pitch))
    }
    pub(crate) fn new_derived(
        source: Box<dyn Source>,
        input: Format,
        output: Format,
        channels: usize,
        pitch: f64,
    ) -> Result<Self> {
        ensure!(
            input == input.packed() && output == output.packed(),
            "ordered Pitch requires negotiated packed formats"
        );
        ensure!(
            (1..=16).contains(&channels) && pitch.is_finite() && (-12. ..=12.).contains(&pitch),
            "invalid ordered Pitch settings"
        );
        let filter = match input {
            Format::U8 => select::<u8>(input, output, channels, pitch)?,
            Format::S16 => select::<i16>(input, output, channels, pitch)?,
            Format::S32 => select::<i32>(input, output, channels, pitch)?,
            Format::F32 => select::<f32>(input, output, channels, pitch)?,
            Format::F64 => select::<f64>(input, output, channels, pitch)?,
            _ => bail!("unsupported ordered Pitch scalar"),
        };
        Ok(Self {
            source,
            filter,
            pending: VecDeque::new(),
            queued: 0,
            ended: false,
            failed: false,
        })
    }
    fn read(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        loop {
            check()?;
            if let Some(packet) = self.pending.pop_front() {
                self.queued -= packet.frames();
                return Ok(Some(packet));
            }
            if self.ended {
                return Ok(None);
            }
            let packet = self.source.next_packet(check)?;
            self.ended = packet.is_none();
            for packet in self.filter.push(packet)? {
                let count = packet.frames();
                ensure!(count > 0, "empty ordered Pitch output frame");
                self.queued = self
                    .queued
                    .checked_add(count)
                    .filter(|n| *n <= 1048576)
                    .ok_or_else(|| anyhow::anyhow!("ordered Pitch output queue exceeds budget"))?;
                self.pending.push_back(packet);
            }
        }
    }
}
impl Source for Node {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(!self.failed, "ordered Pitch requires reset after failure");
        self.failed = true;
        let result = self.read(check)?;
        check()?;
        self.failed = false;
        Ok(result)
    }
    fn reset(&mut self) {
        self.source.reset();
        self.filter.reset();
        self.pending.clear();
        self.queued = 0;
        self.ended = false;
        self.failed = false;
    }
}
