//! Source-negotiated Robot stages, retaining each filter's scalar boundary.
use super::{
    packet_echo::{floating32, floating64},
    packet_pitch::Block,
};
use crate::domain::{
    concat_format_plan::Format,
    concat_robot::{Flanger, Phaser},
};
use anyhow::{bail, ensure, Result};
pub(crate) enum Stage {
    Phaser {
        core: Phaser,
        format: Format,
        channels: usize,
    },
    Flanger {
        core: Flanger,
        channels: usize,
    },
}
impl Stage {
    pub(crate) fn phaser(format: Format, rate: u32, channels: usize, speed: f64) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate)
                && (1..=16).contains(&channels)
                && speed.is_finite()
                && (0.56..=2.).contains(&speed),
            "invalid source Phaser settings"
        );
        ensure!(
            matches!(
                format.packed(),
                Format::S16 | Format::S32 | Format::F32 | Format::F64
            ),
            "invalid Phaser precision"
        );
        Ok(Self::Phaser {
            core: Phaser::new(rate, channels, speed),
            format,
            channels,
        })
    }
    pub(crate) fn flanger(format: Format, rate: u32, channels: usize) -> Result<Self> {
        ensure!(
            format == Format::F64P
                && (8000..=192000).contains(&rate)
                && (1..=16).contains(&channels),
            "invalid source Flanger format"
        );
        Ok(Self::Flanger {
            core: Flanger::new(rate, channels),
            channels,
        })
    }
    pub(crate) fn push_packet(&mut self, input: Block) -> Result<Block> {
        match self {
            Self::Phaser {
                core,
                format,
                channels,
            } => {
                macro_rules! process {
                    ($data:expr,$variant:ident,$scalar:ty) => {{
                        let mut data = $data;
                        ensure!(data.len() == *channels, "Phaser channel mismatch");
                        let frames = data[0].len();
                        ensure!(
                            frames <= 1048576
                                && data.iter().all(|ch| ch.len() == frames
                                    && ch.iter().all(|s| (*s as f64).is_finite())),
                            "invalid Phaser packet"
                        );
                        let mut frame = [0.; 16];
                        for i in 0..frames {
                            for ch in 0..*channels {
                                frame[ch] = data[ch][i] as f64;
                            }
                            core.process_frame(&mut frame[..*channels]);
                            for ch in 0..*channels {
                                ensure!(frame[ch].is_finite(), "nonfinite Phaser output");
                                data[ch][i] = frame[ch] as $scalar;
                            }
                        }
                        Ok(Block::$variant(data))
                    }};
                }
                match format.packed() {
                    Format::F64 => process!(floating64(input), F64, f64),
                    Format::F32 => process!(floating32(input), F32, f32),
                    Format::S16 => match input {
                        Block::S16(v) => process!(v, S16, i16),
                        _ => bail!("Phaser S16 conversion needs compilation"),
                    },
                    Format::S32 => match input {
                        Block::S32(v) => process!(v, S32, i32),
                        _ => bail!("Phaser S32 conversion needs compilation"),
                    },
                    _ => bail!("invalid Phaser precision"),
                }
            }
            Self::Flanger { core, channels } => {
                let mut data = floating64(input);
                ensure!(data.len() == *channels, "Flanger channel mismatch");
                let frames = data[0].len();
                ensure!(
                    frames <= 1048576
                        && data
                            .iter()
                            .all(|ch| ch.len() == frames && ch.iter().all(|s| s.is_finite())),
                    "invalid Flanger packet"
                );
                let mut frame = [0.; 16];
                for i in 0..frames {
                    for ch in 0..*channels {
                        frame[ch] = data[ch][i];
                    }
                    core.process_frame(&mut frame[..*channels]);
                    for ch in 0..*channels {
                        ensure!(frame[ch].is_finite(), "nonfinite Flanger output");
                        data[ch][i] = frame[ch];
                    }
                }
                Ok(Block::F64(data))
            }
        }
    }
}
