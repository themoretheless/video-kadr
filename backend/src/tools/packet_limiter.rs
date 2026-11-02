//! Default-attack Concat limiter stage; retain its audible delay and original EOF.
use super::{packet_echo::floating64, packet_pitch::Block};
use crate::domain::{concat_format_plan::Format, concat_limiter::LookaheadLimiter};
use anyhow::{ensure, Result};
pub(crate) struct Stage {
    core: LookaheadLimiter,
    channels: usize,
}
impl Stage {
    pub(crate) fn new(
        format: Format,
        rate: u32,
        channels: usize,
        limit: f64,
        release_ms: f64,
        auto_level: bool,
    ) -> Result<Self> {
        ensure!(
            format == Format::F64,
            "limiter requires negotiated packed double"
        );
        ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "invalid limiter audio shape"
        );
        ensure!(
            limit.is_finite()
                && (0.0625..=1.).contains(&limit)
                && release_ms.is_finite()
                && (1. ..=8000.).contains(&release_ms),
            "invalid limiter settings"
        );
        Ok(Self {
            core: LookaheadLimiter::with_release(rate, channels, limit, auto_level, release_ms),
            channels,
        })
    }
    pub(crate) fn reset(&mut self) {
        self.core.reset();
    }
    pub(crate) fn push_packet(&mut self, input: Block) -> Result<Block> {
        let mut data = floating64(input);
        ensure!(data.len() == self.channels, "limiter channel mismatch");
        let frames = data[0].len();
        ensure!(
            frames <= 1048576
                && data
                    .iter()
                    .all(|ch| ch.len() == frames && ch.iter().all(|v| v.is_finite())),
            "invalid limiter packet"
        );
        let mut frame = [0.; 16];
        for i in 0..frames {
            for ch in 0..self.channels {
                frame[ch] = data[ch][i];
            }
            self.core.process_frame(&mut frame[..self.channels]);
            for ch in 0..self.channels {
                data[ch][i] = frame[ch];
            }
        }
        Ok(Block::F64(data))
    }
}
