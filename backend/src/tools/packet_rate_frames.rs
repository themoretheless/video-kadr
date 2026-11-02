//! Original scalar -> packed-double source resampling for Loudness's 192 kHz clock.
//! Keep source packets and SWR EOF separate; no host f32 conforming or PCM padding.
use super::{
    packet_pcm::{Decoded, Frames, Scalar},
    packet_pitch::Block,
    packet_timeline::Source,
};
use crate::domain::concat_resample::{FloatInput, FloatOutputResampler};
use anyhow::{ensure, Result};
struct Stream<I: Scalar + FloatInput> {
    source: Frames<I>,
    rate: FloatOutputResampler<I, f64>,
    at: u64,
    ended: bool,
    failed: bool,
}
impl<I: Scalar + FloatInput> Stream<I> {
    fn new(source: Frames<I>, output_rate: u32) -> Result<Self> {
        let spec = source.spec();
        ensure!(
            (8000..=192000).contains(&spec.rate)
                && (8000..=192000).contains(&output_rate)
                && (1..=16).contains(&spec.channels),
            "invalid source rate conversion shape"
        );
        Ok(Self {
            rate: FloatOutputResampler::new(spec.rate, output_rate, spec.channels),
            at: spec.start,
            source,
            ended: false,
            failed: false,
        })
    }
    fn read(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        loop {
            check()?;
            if self.ended {
                return Ok(None);
            }
            let data = if self.at == self.source.end() {
                self.ended = true;
                self.rate
                    .push(&vec![&[][..]; self.source.spec().channels], true)
            } else {
                let input = self.source.read_packet(
                    self.at as i64,
                    (self.source.end() - self.at).min(65536) as usize,
                )?;
                ensure!(!input[0].is_empty(), "empty decoded source rate packet");
                check()?;
                self.at += input[0].len() as u64;
                let refs = input.iter().map(Vec::as_slice).collect::<Vec<_>>();
                self.rate.push(&refs, false)
            };
            ensure!(
                data.len() == self.source.spec().channels,
                "source rate output channel mismatch"
            );
            let frames = data[0].len();
            ensure!(
                frames <= 2097152
                    && data
                        .iter()
                        .all(|ch| ch.len() == frames && ch.iter().all(|v| v.is_finite())),
                "invalid source rate output packet"
            );
            if frames > 0 {
                return Ok(Some(Block::F64(data)));
            }
        }
    }
}
impl<I: Scalar + FloatInput + Send> Source for Stream<I> {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(
            !self.failed,
            "source rate reader requires reset after failure"
        );
        self.failed = true;
        let packet = self.read(check)?;
        check()?;
        self.failed = false;
        Ok(packet)
    }
    fn reset(&mut self) {
        self.rate.reset();
        self.at = self.source.spec().start;
        self.ended = false;
        self.failed = false;
    }
}
pub(crate) fn reader(source: Decoded, output_rate: u32) -> Result<Box<dyn Source>> {
    Ok(match source {
        Decoded::U8(s) => Box::new(Stream::new(s, output_rate)?),
        Decoded::S16(s) => Box::new(Stream::new(s, output_rate)?),
        Decoded::S32(s) => Box::new(Stream::new(s, output_rate)?),
        Decoded::F32(s) => Box::new(Stream::new(s, output_rate)?),
        Decoded::F64(s) => Box::new(Stream::new(s, output_rate)?),
    })
}
