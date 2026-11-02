//! Bounded arbitrary-packet scheduling for the 192 kHz Loudness core.
//! Source must already negotiate packed-double 192 kHz; no implicit f32 conversion.
use super::{packet_pitch::Block, packet_timeline::Source};
use crate::domain::concat_loudness::Loudness;
use anyhow::{bail, ensure, Result};
pub(crate) struct Reader {
    source: Box<dyn Source>,
    core: Loudness,
    channels: usize,
    target: f64,
    held: Vec<Vec<f64>>,
    at: usize,
    staging: Vec<f64>,
    first: bool,
    eof: bool,
    ended: bool,
    failed: bool,
}
impl Reader {
    pub(crate) fn new(
        source: Box<dyn Source>,
        rate: u32,
        channels: usize,
        target: f64,
    ) -> Result<Self> {
        ensure!(rate == 192000, "Loudness requires negotiated 192 kHz input");
        let core = Loudness::new(channels, target)?;
        Ok(Self {
            source,
            core,
            channels,
            target,
            held: vec![Vec::new(); channels],
            at: 0,
            staging: Vec::new(),
            first: true,
            eof: false,
            ended: false,
            failed: false,
        })
    }
    fn output(&self, data: Vec<f64>) -> Result<Option<Block>> {
        if data.is_empty() {
            return Ok(None);
        }
        ensure!(
            data.len() % self.channels == 0 && data.iter().all(|v| v.is_finite()),
            "Loudness produced nonfinite audio (including original short-silence behavior)"
        );
        let mut planar = vec![Vec::with_capacity(data.len() / self.channels); self.channels];
        for frame in data.chunks_exact(self.channels) {
            for c in 0..self.channels {
                planar[c].push(frame[c]);
            }
        }
        Ok(Some(Block::F64(planar)))
    }
    fn read(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        check()?;
        if self.ended {
            return Ok(None);
        }
        let needed = if self.first { 576000 } else { 19200 };
        while self.staging.len() / self.channels < needed && !self.eof {
            check()?;
            if self.at == self.held[0].len() {
                match self.source.next_packet(check)? {
                    Some(Block::F64(data)) => {
                        ensure!(data.len() == self.channels, "Loudness channel mismatch");
                        let n = data[0].len();
                        ensure!(
                            n > 0
                                && n <= 2097152
                                && data
                                    .iter()
                                    .all(|c| c.len() == n && c.iter().all(|v| v.is_finite())),
                            "invalid Loudness source packet"
                        );
                        self.held = data;
                        self.at = 0;
                    }
                    Some(_) => bail!("Loudness requires double source packets"),
                    None => {
                        self.eof = true;
                        break;
                    }
                }
                check()?;
            }
            let n = (needed - self.staging.len() / self.channels).min(self.held[0].len() - self.at);
            for i in self.at..self.at + n {
                for c in 0..self.channels {
                    self.staging.push(self.held[c][i]);
                }
            }
            self.at += n;
        }
        check()?;
        if !self.staging.is_empty() {
            let data = self.core.frame(&self.staging)?;
            self.staging.clear();
            self.first = false;
            return self.output(data);
        }
        ensure!(self.eof, "Loudness scheduler made no progress");
        let data = self.core.finish()?;
        self.ended = true;
        self.output(data)
    }
}
impl Source for Reader {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(!self.failed, "Loudness requires reset after failure");
        self.failed = true;
        let result = self.read(check)?;
        check()?;
        self.failed = false;
        Ok(result)
    }
    fn reset(&mut self) {
        self.source.reset();
        self.core =
            Loudness::new(self.channels, self.target).expect("validated Loudness configuration");
        self.held = vec![Vec::new(); self.channels];
        self.at = 0;
        self.staging.clear();
        self.first = true;
        self.eof = false;
        self.ended = false;
        self.failed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Input {
        data: Vec<Vec<f64>>,
        chunk: usize,
        at: usize,
    }
    impl Source for Input {
        fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
            check()?;
            if self.at == self.data[0].len() {
                return Ok(None);
            }
            let end = (self.at + self.chunk).min(self.data[0].len());
            let data = self.data.iter().map(|c| c[self.at..end].to_vec()).collect();
            self.at = end;
            Ok(Some(Block::F64(data)))
        }
        fn reset(&mut self) {
            self.at = 0;
        }
    }
    fn drain(reader: &mut Reader) -> Result<Vec<Vec<f64>>> {
        let mut out = vec![Vec::new(); reader.channels];
        while let Some(block) = reader.next_packet(&mut || Ok(()))? {
            let Block::F64(data) = block else {
                panic!("wrong output type")
            };
            for (out, data) in out.iter_mut().zip(data) {
                out.extend(data);
            }
        }
        Ok(out)
    }
    #[test]
    fn arbitrary_packets_reset_cancel_and_nonfinite_source_behavior() {
        for frames in [1000, 576000, 595337] {
            let data: Vec<Vec<f64>> = (0..2)
                .map(|c| {
                    (0..frames)
                        .map(|i| 0.4 * (i as f64 * (0.013 + c as f64 * 0.007)).sin())
                        .collect()
                })
                .collect();
            let mut core = Loudness::new(2, -23.).unwrap();
            let mut expected = Vec::new();
            let packed: Vec<f64> = (0..frames).flat_map(|i| [data[0][i], data[1][i]]).collect();
            let first = frames.min(576000) * 2;
            expected.extend(core.frame(&packed[..first]).unwrap());
            for block in packed[first..].chunks(38400) {
                expected.extend(core.frame(block).unwrap());
            }
            expected.extend(core.finish().unwrap());
            for chunk in [137, 65535, 2097152] {
                let input = Input {
                    data: data.clone(),
                    chunk,
                    at: 0,
                };
                let mut reader = Reader::new(Box::new(input), 192000, 2, -23.).unwrap();
                for _ in 0..2 {
                    let actual = drain(&mut reader).unwrap();
                    assert_eq!(actual[0].len(), frames);
                    for i in 0..frames {
                        assert_eq!(actual[0][i], expected[2 * i]);
                        assert_eq!(actual[1][i], expected[2 * i + 1]);
                    }
                    assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                    reader.reset();
                }
                let mut calls = 0;
                assert!(reader
                    .next_packet(&mut || {
                        calls += 1;
                        ensure!(calls < 4, "cancelled");
                        Ok(())
                    })
                    .is_err());
                assert!(reader.next_packet(&mut || Ok(())).is_err());
                reader.reset();
                assert_eq!(drain(&mut reader).unwrap()[0].len(), frames);
            }
        }
        let input = Input {
            data: vec![vec![0.; 1000]],
            chunk: 137,
            at: 0,
        };
        let mut reader = Reader::new(Box::new(input), 192000, 1, -23.).unwrap();
        assert!(drain(&mut reader).is_err());
        assert!(reader.next_packet(&mut || Ok(())).is_err());
        let input = Input {
            data: vec![vec![]],
            chunk: 137,
            at: 0,
        };
        let mut reader = Reader::new(Box::new(input), 192000, 1, -23.).unwrap();
        assert!(drain(&mut reader).unwrap()[0].is_empty());
    }
    #[test]
    #[ignore = "requires original FFmpeg via LOUDNESS_FFMPEG"]
    fn original_loudness_downsample_packets() {
        use std::{
            io::{BufWriter, Seek, SeekFrom, Write},
            process::{Command, Stdio},
        };
        let ffmpeg = std::env::var("LOUDNESS_FFMPEG").expect("set LOUDNESS_FFMPEG");
        let mut compared = 0;
        for channels in [1, 2] {
            for target in [-30., -23., -12.] {
                for frames in [38400, 576000, 595337] {
                    let data: Vec<Vec<f64>> = (0..channels)
                        .map(|c| {
                            (0..frames)
                                .map(|i| {
                                    let t = i as f64 / 192000.;
                                    (if t < 1.2 { 0.05 } else { 0.7 })
                                        * (t * std::f64::consts::TAU * (317. + c as f64 * 680.))
                                            .sin()
                                })
                                .collect()
                        })
                        .collect();
                    let input = Input {
                        data: data.clone(),
                        chunk: 65535,
                        at: 0,
                    };
                    let loudness = Reader::new(Box::new(input), 192000, channels, target).unwrap();
                    let mut reader = super::super::packet_double_rate::Reader::new(
                        Box::new(loudness),
                        192000,
                        48000,
                        channels,
                    )
                    .unwrap();
                    let mut raw = tempfile::tempfile().unwrap();
                    {
                        let mut w = BufWriter::new(&mut raw);
                        for i in 0..frames {
                            for c in &data {
                                w.write_all(&c[i].to_le_bytes()).unwrap();
                            }
                        }
                        w.flush().unwrap();
                    }
                    raw.seek(SeekFrom::Start(0)).unwrap();
                    let filter = format!(
                        "loudnorm=I={target:.1}:TP=-1.5:LRA=11,aresample=48000:osf=dbl,ashowinfo"
                    );
                    let reference = Command::new(&ffmpeg)
                        .args([
                            "-v",
                            "info",
                            "-f",
                            "f64le",
                            "-ar",
                            "192000",
                            "-ac",
                            &channels.to_string(),
                            "-i",
                            "pipe:0",
                            "-af",
                            &filter,
                            "-c:a",
                            "pcm_f64le",
                            "-f",
                            "f64le",
                            "pipe:1",
                        ])
                        .stdin(Stdio::from(raw))
                        .output()
                        .unwrap();
                    assert!(
                        reference.status.success(),
                        "{}",
                        String::from_utf8_lossy(&reference.stderr)
                    );
                    let stderr = String::from_utf8(reference.stderr).unwrap();
                    let packet_frames: Vec<usize> = stderr
                        .lines()
                        .filter(|l| l.contains("Parsed_ashowinfo") && l.contains("nb_samples:"))
                        .map(|l| {
                            assert!(l.contains("fmt:dbl") && l.contains("rate:48000"));
                            l.split("nb_samples:")
                                .nth(1)
                                .unwrap()
                                .split_whitespace()
                                .next()
                                .unwrap()
                                .parse()
                                .unwrap()
                        })
                        .collect();
                    assert!(!packet_frames.is_empty());
                    for _ in 0..2 {
                        let mut output = Vec::new();
                        let mut packets = Vec::new();
                        while let Some(block) = reader.next_packet(&mut || Ok(())).unwrap() {
                            let Block::F64(data) = block else {
                                panic!("wrong type")
                            };
                            packets.push(data[0].len());
                            for i in 0..data[0].len() {
                                for c in &data {
                                    output.push(c[i]);
                                }
                            }
                        }
                        assert_eq!(
                            packets, packet_frames,
                            "ch{channels} target{target} frames{frames}"
                        );
                        assert_eq!(output.len() * 8, reference.stdout.len());
                        for (i, (a, bytes)) in output
                            .iter()
                            .zip(reference.stdout.chunks_exact(8))
                            .enumerate()
                        {
                            let e = f64::from_le_bytes(bytes.try_into().unwrap());
                            assert!(
                                (a - e).abs() < 2e-12,
                                "ch{channels} target{target} frames{frames} value{i}: {a} != {e}"
                            );
                        }
                        compared += output.len();
                        reader.reset();
                    }
                    let mut calls = 0;
                    assert!(reader
                        .next_packet(&mut || {
                            calls += 1;
                            ensure!(calls < 6, "cancel");
                            Ok(())
                        })
                        .is_err());
                    assert!(reader.next_packet(&mut || Ok(())).is_err());
                    reader.reset();
                    assert!(reader.next_packet(&mut || Ok(())).unwrap().is_some());
                }
            }
        }
        eprintln!(
            "Loudness downsample: {compared} original values and exact packet lengths matched"
        );
    }
}
