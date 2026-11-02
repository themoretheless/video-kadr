//! Negotiated scalar packets -> double rate conversion, without an f32 bridge.
use super::{packet_pcm::Scalar, packet_pitch::Block, packet_timeline::Source};
use crate::domain::{
    concat_format_plan::Format,
    concat_resample::{FloatInput, FloatOutputResampler},
};
use anyhow::{bail, ensure, Result};
trait Input: Scalar + FloatInput + Send + 'static {
    fn take(block: Block) -> Result<Vec<Vec<Self>>>;
}
macro_rules! input {($($ty:ty:$tag:ident),+)=>{$(impl Input for $ty {fn take(block:Block)->Result<Vec<Vec<Self>>>{match block {Block::$tag(v)=>Ok(v),_=>bail!("typed rate source scalar mismatch")}}})+};}
input!(u8:U8,i16:S16,i32:S32,f32:F32,f64:F64);
struct Reader<T: Input> {
    source: Box<dyn Source>,
    rate: FloatOutputResampler<T, f64>,
    channels: usize,
    from: u32,
    to: u32,
    ended: bool,
    failed: bool,
}
impl<T: Input> Reader<T> {
    fn read(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        loop {
            check()?;
            if self.ended {
                return Ok(None);
            }
            let input = match self.source.next_packet(check)? {
                Some(block) => T::take(block)?,
                None => {
                    self.ended = true;
                    vec![Vec::new(); self.channels]
                }
            };
            ensure!(input.len() == self.channels, "typed rate channel mismatch");
            let frames = input[0].len();
            ensure!(
                (self.ended || frames > 0)
                    && frames <= 1048576
                    && input
                        .iter()
                        .all(|c| c.len() == frames && c.iter().all(|&v| v.valid())),
                "invalid typed rate input packet"
            );
            ensure!(
                (frames as u64 * self.to as u64).div_ceil(self.from as u64) + 4096 <= 2097152,
                "typed rate output packet exceeds bound"
            );
            check()?;
            let output = self.rate.push(
                &input.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                self.ended,
            );
            ensure!(
                output.len() == self.channels
                    && output.iter().all(|c| c.len() == output[0].len()
                        && c.len() <= 2097152
                        && c.iter().all(|v| v.is_finite())),
                "invalid typed rate output"
            );
            if !output[0].is_empty() {
                return Ok(Some(Block::F64(output)));
            }
        }
    }
}
impl<T: Input> Source for Reader<T> {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(
            !self.failed,
            "typed rate reader requires reset after failure"
        );
        self.failed = true;
        let output = self.read(check)?;
        check()?;
        self.failed = false;
        Ok(output)
    }
    fn reset(&mut self) {
        self.source.reset();
        self.rate.reset();
        self.ended = false;
        self.failed = false;
    }
}
fn select<T: Input>(
    source: Box<dyn Source>,
    from: u32,
    to: u32,
    channels: usize,
) -> Box<dyn Source> {
    Box::new(Reader::<T> {
        source,
        rate: FloatOutputResampler::new(from, to, channels),
        channels,
        from,
        to,
        ended: false,
        failed: false,
    })
}
pub(crate) fn reader(
    source: Box<dyn Source>,
    format: Format,
    from: u32,
    to: u32,
    channels: usize,
) -> Result<Box<dyn Source>> {
    ensure!(
        (8000..=192000).contains(&from)
            && (8000..=192000).contains(&to)
            && (1..=2).contains(&channels),
        "invalid typed rate shape"
    );
    Ok(match format.packed() {
        Format::U8 => select::<u8>(source, from, to, channels),
        Format::S16 => select::<i16>(source, from, to, channels),
        Format::S32 => select::<i32>(source, from, to, channels),
        Format::F32 => select::<f32>(source, from, to, channels),
        Format::F64 => select::<f64>(source, from, to, channels),
        _ => bail!("unsupported typed rate source"),
    })
}
