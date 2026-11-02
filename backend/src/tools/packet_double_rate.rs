//! Double-to-double packet rate conversion, preserving a separate resampler EOF.
use super::{packet_pitch::Block, packet_timeline::Source};
use crate::domain::concat_resample::FloatOutputResampler;
use anyhow::{bail, ensure, Result};
pub(crate) struct Reader {
    source: Box<dyn Source>,
    rate: FloatOutputResampler<f64, f64>,
    channels: usize,
    ended: bool,
    failed: bool,
}
impl Reader {
    pub(crate) fn new(
        source: Box<dyn Source>,
        from: u32,
        to: u32,
        channels: usize,
    ) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&from)
                && (8000..=192000).contains(&to)
                && (1..=2).contains(&channels),
            "invalid double rate shape"
        );
        Ok(Self {
            source,
            rate: FloatOutputResampler::new(from, to, channels),
            channels,
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
            let data = match self.source.next_packet(check)? {
                Some(Block::F64(data)) => {
                    ensure!(data.len() == self.channels, "double rate channel mismatch");
                    let frames = data[0].len();
                    ensure!(
                        frames > 0
                            && frames <= 1048576
                            && data
                                .iter()
                                .all(|c| c.len() == frames && c.iter().all(|v| v.is_finite())),
                        "invalid double rate packet"
                    );
                    data
                }
                Some(_) => bail!("double rate requires F64 packets"),
                None => {
                    self.ended = true;
                    vec![Vec::new(); self.channels]
                }
            };
            check()?;
            let output = self.rate.push(
                &data.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                self.ended,
            );
            ensure!(
                output.len() == self.channels
                    && output
                        .iter()
                        .all(|c| c.len() == output[0].len() && c.iter().all(|v| v.is_finite())),
                "invalid double rate output"
            );
            if !output[0].is_empty() {
                return Ok(Some(Block::F64(output)));
            }
        }
    }
}
impl Source for Reader {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(!self.failed, "double rate requires reset after failure");
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
