//! Per-source Pitch bridge: actual decoded scalar/packets to seekable 48 kHz output.
//! Keep typed output until downstream graph format negotiation is resolved.
use super::packet_pcm::{Decoded, Frames, Scalar};
use crate::domain::{concat_pitch::Sample, concat_pitch_timeline::Timeline};
use anyhow::Result;

pub(crate) struct Stream<T: Scalar + Sample> {
    source: Frames<T>,
    timeline: Timeline<T>,
}
impl<T: Scalar + Sample> Stream<T> {
    fn new(source: Frames<T>, semitones: f32, origin: i64) -> Result<Self> {
        let spec = source.spec();
        let timeline = Timeline::new_with_source_planar(
            spec.rate,
            spec.channels,
            semitones,
            spec.start as i64..source.end() as i64,
            origin,
            spec.source_planar,
        )?;
        Ok(Self { source, timeline })
    }
    fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<T>>> {
        let source = &mut self.source;
        self.timeline.render(
            start,
            frames,
            &mut |at, max| source.read_packet(at, max),
            checkpoint,
        )
    }
}
macro_rules! bridge {
    ($($variant:ident : $scalar:ty),+ $(,)?) => {
        pub(crate) enum Reader { $($variant(Stream<$scalar>)),+ }
        pub(crate) enum Block { $($variant(Vec<Vec<$scalar>>)),+ }
        impl Block {
            pub(crate) fn frames(&self)->usize {match self {$(Self::$variant(data)=>data.first().map_or(0,Vec::len)),+}}
        }
        impl Reader {
            pub(crate) fn new(source: Decoded, semitones: f32, origin: i64) -> Result<Self> {
                Ok(match source { $(Decoded::$variant(source) => Self::$variant(Stream::new(source,semitones,origin)?)),+ })
            }
            /// Output placement is measured in 48 kHz samples; source coordinates
            /// remain in their original sample rate and decoded scalar type.
            pub(crate) fn render(&mut self, start: i64, frames: usize, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<Block> {
                Ok(match self { $(Self::$variant(stream) => Block::$variant(stream.render(start,frames,checkpoint)?)),+ })
            }
        }
    }
}
bridge!(U8:u8, S16:i16, S32:i32, F32:f32, F64:f64);

use crate::domain::{
    concat_pitch_timeline::FloatTimeline,
    concat_resample::{FloatInput, FloatOutput},
};
trait FloatRead<O>: Send {
    fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<O>>>;
}
struct FloatStream<I: Scalar + Sample, O: Sample> {
    source: Frames<I>,
    timeline: FloatTimeline<I, O>,
}
impl<I: Scalar + Sample + FloatInput, O: Sample + FloatOutput> FloatStream<I, O> {
    fn new(source: Frames<I>, semitones: f32, origin: i64) -> Result<Self> {
        let spec = source.spec();
        let timeline = FloatTimeline::new_with_source_planar(
            spec.rate,
            spec.channels,
            semitones,
            spec.start as i64..source.end() as i64,
            origin,
            spec.source_planar,
        )?;
        Ok(Self { source, timeline })
    }
}
impl<I: Scalar + Sample + FloatInput, O: Sample + FloatOutput> FloatRead<O> for FloatStream<I, O>
where
    I::Rate: Send,
{
    fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<O>>> {
        let source = &mut self.source;
        self.timeline.render(
            start,
            frames,
            &mut |at, max| source.read_packet(at, max),
            checkpoint,
        )
    }
}
/// A downstream-negotiated floating boundary before WSOLA. This differs from
/// normalizing a completed integer Pitch block: alignment/blending use O.
pub(crate) struct FloatReader<O> {
    source: Box<dyn FloatRead<O>>,
}
impl<O: Sample + FloatOutput + 'static> FloatReader<O> {
    pub(crate) fn new(source: Decoded, semitones: f32, origin: i64) -> Result<Self> {
        let source: Box<dyn FloatRead<O>> = match source {
            Decoded::U8(s) => Box::new(FloatStream::<_, O>::new(s, semitones, origin)?),
            Decoded::S16(s) => Box::new(FloatStream::<_, O>::new(s, semitones, origin)?),
            Decoded::S32(s) => Box::new(FloatStream::<_, O>::new(s, semitones, origin)?),
            Decoded::F32(s) => Box::new(FloatStream::<_, O>::new(s, semitones, origin)?),
            Decoded::F64(s) => Box::new(FloatStream::<_, O>::new(s, semitones, origin)?),
        };
        Ok(Self { source })
    }
    pub(crate) fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<O>>> {
        self.source.render(start, frames, checkpoint)
    }
}

use crate::domain::concat_format_plan::{Filter, Format, Plan};
enum Selected {
    Native(Reader),
    Single(FloatReader<f32>),
    Double(FloatReader<f64>),
}
/// Selects the Pitch endpoint using downstream capabilities. It does not run
/// those following effects; the host consumes the typed block and remaining plan.
pub(crate) struct PlannedReader {
    pub plan: Plan,
    source: Selected,
}
pub(crate) fn format_plan(source: &Decoded, following: &[Filter], sink: Format) -> Result<Plan> {
    let (scalar, planar) = match source {
        Decoded::U8(s) => (Format::U8, s.spec().source_planar),
        Decoded::S16(s) => (Format::S16, s.spec().source_planar),
        Decoded::S32(s) => (Format::S32, s.spec().source_planar),
        Decoded::F32(s) => (Format::F32, s.spec().source_planar),
        Decoded::F64(s) => (Format::F64, s.spec().source_planar),
    };
    let input = scalar.layout(planar);
    let mut filters = vec![
        Filter::Resample,
        Filter::Pass,
        Filter::Resample,
        Filter::Tempo,
    ];
    filters.extend_from_slice(following);
    let plan = Plan::build(input, &filters, sink)?;
    anyhow::ensure!(
        plan.stages[0].input == input && plan.stages[0].output == input,
        "unsupported Pitch normalizer format transition"
    );
    Ok(plan)
}
impl PlannedReader {
    pub(crate) fn new(
        source: Decoded,
        following: &[Filter],
        sink: Format,
        semitones: f32,
        origin: i64,
    ) -> Result<Self> {
        let plan = format_plan(&source, following, sink)?;
        let scalar = plan.stages[0].input.packed();
        let output = plan.stages[3].output;
        let source = if output == scalar {
            Selected::Native(Reader::new(source, semitones, origin)?)
        } else {
            match output {
                Format::F32 => Selected::Single(FloatReader::new(source, semitones, origin)?),
                Format::F64 => Selected::Double(FloatReader::new(source, semitones, origin)?),
                _ => anyhow::bail!("negotiated integer Pitch conversion is not implemented"),
            }
        };
        Ok(Self { plan, source })
    }
    pub(crate) fn render(
        &mut self,
        start: i64,
        frames: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Block> {
        match &mut self.source {
            Selected::Native(source) => source.render(start, frames, checkpoint),
            Selected::Single(source) => Ok(Block::F32(source.render(start, frames, checkpoint)?)),
            Selected::Double(source) => Ok(Block::F64(source.render(start, frames, checkpoint)?)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readers_can_move_to_existing_blocking_mixer_workers() {
        fn send<T: Send>() {}
        send::<Reader>();
        send::<PlannedReader>();
        send::<FloatReader<f32>>();
        send::<FloatReader<f64>>();
    }
}
