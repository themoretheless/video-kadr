//! Typed execution of a planned Echo stage and its input conversion.
use super::packet_pitch::Block;
use crate::domain::{
    concat_echo_kernel::Echo, concat_format_plan::Format, concat_resample::FloatInput,
};
use anyhow::{bail, Result};
pub(crate) fn floating32(input: Block) -> Vec<Vec<f32>> {
    macro_rules! convert {
        ($data:expr) => {
            $data
                .into_iter()
                .map(|ch| ch.into_iter().map(FloatInput::normalized_float).collect())
                .collect()
        };
    }
    match input {
        Block::U8(v) => convert!(v),
        Block::S16(v) => convert!(v),
        Block::S32(v) => convert!(v),
        Block::F32(v) => v,
        Block::F64(v) => convert!(v),
    }
}
pub(crate) fn floating64(input: Block) -> Vec<Vec<f64>> {
    macro_rules! convert {
        ($data:expr) => {
            $data
                .into_iter()
                .map(|ch| ch.into_iter().map(FloatInput::normalized).collect())
                .collect()
        };
    }
    match input {
        Block::U8(v) => convert!(v),
        Block::S16(v) => convert!(v),
        Block::S32(v) => convert!(v),
        Block::F32(v) => convert!(v),
        Block::F64(v) => v,
    }
}
pub(crate) enum Stage {
    S16(Echo<i16>),
    S32(Echo<i32>),
    F32(Echo<f32>),
    F64(Echo<f64>),
}
impl Stage {
    pub(crate) fn new(
        format: Format,
        rate: u32,
        channels: usize,
        in_gain: f32,
        out_gain: f32,
        delays: &[f32],
        decays: &[f32],
    ) -> Result<Self> {
        Ok(match format {
            Format::S16P => Self::S16(Echo::new(
                rate, channels, in_gain, out_gain, delays, decays,
            )?),
            Format::S32P => Self::S32(Echo::new(
                rate, channels, in_gain, out_gain, delays, decays,
            )?),
            Format::F32P => Self::F32(Echo::new(
                rate, channels, in_gain, out_gain, delays, decays,
            )?),
            Format::F64P => Self::F64(Echo::new(
                rate, channels, in_gain, out_gain, delays, decays,
            )?),
            _ => bail!("Echo requires a negotiated planar S16/S32/F32/F64 format"),
        })
    }
    pub(crate) fn reset(&mut self) {
        match self {
            Self::S16(e) => e.reset(),
            Self::S32(e) => e.reset(),
            Self::F32(e) => e.reset(),
            Self::F64(e) => e.reset(),
        }
    }
    pub(crate) fn push_packet(&mut self, input: Block) -> Result<Block> {
        macro_rules! process {
            ($echo:expr,$data:expr,$variant:ident) => {{
                let data = $data;
                let refs: Vec<&[_]> = data.iter().map(Vec::as_slice).collect();
                Block::$variant($echo.push_packet(&refs)?)
            }};
        }
        Ok(match self {
            Self::S16(e) => match input {
                Block::U8(v) => process!(
                    e,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|v| (i16::from(v) - 128) << 8)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S16
                ),
                Block::S16(v) => process!(e, v, S16),
                Block::S32(v) => process!(
                    e,
                    v.into_iter()
                        .map(|ch| ch.into_iter().map(|v| (v >> 16) as i16).collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S16
                ),
                _ => bail!("floating-to-integer Echo input conversion is not implemented"),
            },
            Self::S32(e) => match input {
                Block::U8(v) => process!(
                    e,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|v| (i32::from(v) - 128) << 24)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S32
                ),
                Block::S16(v) => process!(
                    e,
                    v.into_iter()
                        .map(|ch| ch
                            .into_iter()
                            .map(|v| i32::from(v) << 16)
                            .collect::<Vec<_>>())
                        .collect::<Vec<_>>(),
                    S32
                ),
                Block::S32(v) => process!(e, v, S32),
                _ => bail!("floating-to-integer Echo input conversion is not implemented"),
            },
            Self::F32(e) => process!(e, floating32(input), F32),
            Self::F64(e) => process!(e, floating64(input), F64),
        })
    }
    pub(crate) fn finish_packet(&mut self) -> Option<Block> {
        match self {
            Self::S16(e) => e.finish_packet().map(Block::S16),
            Self::S32(e) => e.finish_packet().map(Block::S32),
            Self::F32(e) => e.finish_packet().map(Block::F32),
            Self::F64(e) => e.finish_packet().map(Block::F64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        concat_format_plan::{Filter, Plan},
        concat_pitch::Pitch,
    };
    use std::io::Write;
    trait Input: crate::domain::concat_pitch::Sample {
        const RAW: &'static str;
        const FORMAT: Format;
        fn sample(v: f64) -> Self;
        fn write(self, f: &mut std::fs::File);
        fn block(data: Vec<Vec<Self>>) -> Block;
    }
    macro_rules! input {
        ($ty:ty,$raw:literal,$fmt:ident,$variant:ident,$scale:literal,$bias:literal) => {
            impl Input for $ty {
                const RAW: &'static str = $raw;
                const FORMAT: Format = Format::$fmt;
                fn sample(v: f64) -> Self {
                    (v * $scale + $bias) as Self
                }
                fn write(self, f: &mut std::fs::File) {
                    f.write_all(&self.to_le_bytes()).unwrap();
                }
                fn block(data: Vec<Vec<Self>>) -> Block {
                    Block::$variant(data)
                }
            }
        };
    }
    input!(u8, "u8", U8, U8, 60., 128.);
    input!(i16, "s16le", S16, S16, 12000., 0.);
    input!(i32, "s32le", S32, S32, 1000000000., 0.);
    input!(f32, "f32le", F32, F32, 1., 0.);
    input!(f64, "f64le", F64, F64, 1., 0.);
    fn qualify<T: Input>() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/concat-pitch-pcm.json"))
                .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.pcm");
        let mut compared = 0usize;
        for (index, c) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let rate = c["rate"].as_u64().unwrap() as usize;
            let channels = c["channels"].as_u64().unwrap() as usize;
            let frames = rate * 3 / 8 + 211;
            let semi = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
            let source: Vec<Vec<T>> = (0..channels)
                .map(|ch| {
                    (0..frames)
                        .map(|i| {
                            let value = if c["signal"] == "steps" {
                                (if ch == 0 { 2. } else { -1.5 })
                                    * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. }
                            } else {
                                let h = ((i % 257 + ch * 1061) as u32)
                                    .wrapping_mul(1664525)
                                    .wrapping_add(1013904223);
                                (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096.
                            };
                            T::sample(value)
                        })
                        .collect()
                })
                .collect();
            let mut file = std::fs::File::create(&path).unwrap();
            for i in 0..frames {
                for ch in &source {
                    ch[i].write(&mut file);
                }
            }
            drop(file);
            let chain = format!("{},aecho=0.8:0.85:250:0.4", c["chain"].as_str().unwrap());
            let reference = std::process::Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    T::RAW,
                    "-ar",
                    &rate.to_string(),
                    "-ac",
                    &channels.to_string(),
                    "-i",
                ])
                .arg(&path)
                .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let plan = Plan::build(
                T::FORMAT,
                &[
                    Filter::Resample,
                    Filter::Pass,
                    Filter::Resample,
                    Filter::Tempo,
                    Filter::Echo,
                ],
                Format::F32,
            )
            .unwrap();
            assert_eq!(plan.stages[3].output, T::FORMAT);
            let mut pitch = Pitch::<T>::new(rate as u32, channels, semi);
            let mut echo = Stage::new(
                plan.stages[4].input,
                48000,
                channels,
                0.8,
                0.85,
                &[250.],
                &[0.4],
            )
            .unwrap();
            for pass in 0..3 {
                if pass > 0 {
                    pitch.reset();
                    echo.reset();
                }
                let mut actual = vec![Vec::new(); channels];
                let mut at = 0;
                let mut block = 0;
                while at < frames {
                    let count = if pass == 0 {
                        65536
                    } else {
                        [1, 255, 17, 1024, 4096][block % 5]
                    }
                    .min(frames - at);
                    let refs: Vec<&[T]> = source.iter().map(|ch| &ch[at..at + count]).collect();
                    let shifted = pitch.push(&refs, pass != 2 && at + count == frames);
                    let output = floating32(echo.push_packet(T::block(shifted)).unwrap());
                    for (dst, src) in actual.iter_mut().zip(output) {
                        dst.extend(src);
                    }
                    at += count;
                    block += 1;
                }
                if pass == 2 {
                    let empty = vec![&[][..]; channels];
                    let output = floating32(
                        echo.push_packet(T::block(pitch.push(&empty, true)))
                            .unwrap(),
                    );
                    for (dst, src) in actual.iter_mut().zip(output) {
                        dst.extend(src);
                    }
                }
                while let Some(packet) = echo.finish_packet() {
                    let output = floating32(packet);
                    for (dst, src) in actual.iter_mut().zip(output) {
                        dst.extend(src);
                    }
                }
                assert_eq!(
                    actual[0].len() * channels * 4,
                    reference.stdout.len(),
                    "{} case{index} pass{pass} length",
                    T::RAW
                );
                for (i, raw) in reference.stdout.chunks_exact(4).enumerate() {
                    let expected = f32::from_le_bytes(raw.try_into().unwrap());
                    let value = actual[i % channels][i / channels];
                    assert!(value.is_finite()&&(value-expected).abs()<=2e-7,"{} case{index} rate{rate} ch{channels} semi{semi} pass{pass} value{i}: {value} vs {expected}",T::RAW);
                }
            }
            compared += reference.stdout.len() / 4;
        }
        println!(
            "qualified {compared} {} Pitch/Echo/floating-output values plus reset/chunk/EOF replay",
            T::RAW
        );
    }
    #[test]
    #[ignore = "requires pinned FFmpeg typed transport qualification"]
    fn actual_original_pitch_echo_chains() {
        qualify::<u8>();
        qualify::<i16>();
        qualify::<i32>();
        qualify::<f32>();
        qualify::<f64>();
    }
    #[test]
    fn unsupported_formats_and_nonfinite_inputs_do_not_use_fallbacks() {
        assert!(Stage::new(Format::U8P, 48000, 1, 0.8, 0.85, &[250.], &[0.4]).is_err());
        let mut echo = Stage::new(Format::S16P, 48000, 1, 0.8, 0.85, &[250.], &[0.4]).unwrap();
        assert!(echo.push_packet(Block::F32(vec![vec![0.]])).is_err());
        let mut echo = Stage::new(Format::F32P, 48000, 1, 0.8, 0.85, &[250.], &[0.4]).unwrap();
        assert!(echo.push_packet(Block::F64(vec![vec![f64::NAN]])).is_err());
    }
}
