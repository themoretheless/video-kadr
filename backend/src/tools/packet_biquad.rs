//! Execution of negotiated planar biquads while preserving complete frame boundaries.
use super::{
    packet_echo::{floating32, floating64},
    packet_pitch::Block,
};
use crate::domain::{
    concat_biquad_stream::{Design, Filter},
    concat_format_plan::Format,
};
use anyhow::{bail, Result};
pub(crate) enum Stage {
    S16(Filter<i16>),
    S32(Filter<i32>),
    F32(Filter<f32>),
    F64(Filter<f64>),
}
impl Stage {
    pub(crate) fn new(
        format: Format,
        design: Design,
        rate: u32,
        channels: usize,
        frequency: f64,
        width: f64,
        gain: f64,
    ) -> Result<Self> {
        Ok(match format {
            Format::S16P => Self::S16(Filter::new(design, rate, channels, frequency, width, gain)?),
            Format::S32P => Self::S32(Filter::new(design, rate, channels, frequency, width, gain)?),
            Format::F32P => Self::F32(Filter::new(design, rate, channels, frequency, width, gain)?),
            Format::F64P => Self::F64(Filter::new(design, rate, channels, frequency, width, gain)?),
            _ => bail!("biquad requires negotiated planar S16/S32/F32/F64"),
        })
    }
    pub(crate) fn reset(&mut self) {
        match self {
            Self::S16(f) => f.reset(),
            Self::S32(f) => f.reset(),
            Self::F32(f) => f.reset(),
            Self::F64(f) => f.reset(),
        }
    }
    pub(crate) fn push_packet(&mut self, input: Block) -> Result<Block> {
        macro_rules! process {
            ($filter:expr,$data:expr,$variant:ident) => {{
                let data = $data;
                let refs: Vec<&[_]> = data.iter().map(Vec::as_slice).collect();
                Block::$variant($filter.push_packet(&refs)?)
            }};
        }
        Ok(match self {
            Self::S16(f) => match input {
                Block::S16(v) => process!(f, v, S16),
                Block::U8(v) => process!(
                    f,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|s| (i16::from(s) - 128) << 8)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S16
                ),
                Block::S32(v) => process!(
                    f,
                    v.into_iter()
                        .map(|ch| ch.into_iter().map(|s| (s >> 16) as i16).collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S16
                ),
                _ => bail!("floating-to-integer biquad conversion is not implemented"),
            },
            Self::S32(f) => match input {
                Block::S32(v) => process!(f, v, S32),
                Block::U8(v) => process!(
                    f,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|s| (i32::from(s) - 128) << 24)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S32
                ),
                Block::S16(v) => process!(
                    f,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|s| i32::from(s) << 16)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S32
                ),
                _ => bail!("floating-to-integer biquad conversion is not implemented"),
            },
            Self::F32(f) => process!(f, floating32(input), F32),
            Self::F64(f) => process!(f, floating64(input), F64),
        })
    }
}
