//! Frame-level Concat Loudness at the original dynamic 192 kHz clock.
//! SPDX-License-Identifier: LGPL-2.1-or-later
//! Port of FFmpeg af_loudnorm.c, Copyright (c) 2016 Kyle Swanson.
//! License: backend/licenses/FFmpeg-LGPL-2.1.txt; no warranty.
//! Caller supplies a first 3-second frame, then 100ms frames (short at EOF).
//! Not yet admitted to production; source-rate scheduling is a separate layer.
use super::{concat_ebur128::Meter, concat_loudness_gain::Gain, concat_loudness_limiter::Limiter};
use anyhow::{ensure, Result};
enum Mode {
    First,
    Dynamic,
    Linear(f64),
    Ended,
}
pub(crate) struct Loudness {
    channels: usize,
    target: f64,
    input: Box<Meter>,
    output: Box<Meter>,
    limiter: Limiter,
    gain: Option<Gain>,
    ring: Vec<f64>,
    read: usize,
    write: usize,
    mode: Mode,
}
impl Loudness {
    pub(crate) fn new(channels: usize, target: f64) -> Result<Self> {
        ensure!(
            target.is_finite() && (-30. ..=-12.).contains(&target),
            "invalid Loudness target"
        );
        let target: f64 = format!("{target:.1}").parse()?;
        Ok(Self {
            channels,
            target,
            input: Box::new(Meter::new_contracted(192000, channels)?),
            output: Box::new(Meter::new_contracted(192000, channels)?),
            limiter: Limiter::new(channels, 10_f64.powf(-1.5 / 20.))?,
            gain: None,
            ring: vec![0.; 576000 * channels],
            read: 0,
            write: 0,
            mode: Mode::First,
        })
    }
    fn measure(meter: &mut Meter, channels: usize, data: &[f64]) -> Result<()> {
        let planar: Vec<Vec<f64>> = (0..channels)
            .map(|c| data.iter().skip(c).step_by(channels).copied().collect())
            .collect();
        meter.push(&planar.iter().map(Vec::as_slice).collect::<Vec<_>>())
    }
    pub(crate) fn frame(&mut self, data: &[f64]) -> Result<Vec<f64>> {
        ensure!(!matches!(self.mode, Mode::Ended), "Loudness already ended");
        ensure!(
            !data.is_empty()
                && data.len() % self.channels == 0
                && data.iter().all(|v| v.is_finite()),
            "invalid Loudness frame"
        );
        let frames = data.len() / self.channels;
        ensure!(
            frames
                <= if matches!(self.mode, Mode::First) {
                    576000
                } else {
                    19200
                },
            "oversized Loudness frame"
        );
        Self::measure(&mut self.input, self.channels, data)?;
        if matches!(self.mode, Mode::First) && frames < 576000 {
            let offset = 10_f64.powf((self.target - self.input.integrated()) / 20.);
            let peak = self.input.peaks().iter().copied().fold(0_f64, f64::max);
            let ceiling = 10_f64.powf(-1.5 / 20.);
            self.mode = Mode::Linear(if peak * offset < ceiling {
                offset
            } else {
                ceiling / peak
            });
        }
        let result = match self.mode {
            Mode::First => {
                self.ring.copy_from_slice(data);
                let gain = Gain::new(self.target, self.input.shortterm())?;
                let initial = if self.input.shortterm() <= -70. {
                    1.
                } else {
                    10_f64.powf((self.target - self.input.shortterm()) / 20.)
                };
                for i in 0..self.limiter.buffer.len() {
                    self.limiter.buffer[i] = self.ring[i] * initial;
                }
                self.read = self.limiter.buffer.len();
                self.gain = Some(gain);
                self.mode = Mode::Dynamic;
                self.limiter.process(19200, true)?
            }
            Mode::Dynamic => {
                let (gain, next) = self.gain.as_ref().unwrap().pair();
                for n in 0..frames {
                    for c in 0..self.channels {
                        self.ring[self.write + c] = data[n * self.channels + c];
                        self.limiter.buffer[self.limiter.index + c] = self.ring[self.read + c]
                            * (gain + (n as f64 / frames as f64) * (next - gain));
                    }
                    self.write = (self.write + self.channels) % self.ring.len();
                    self.read = (self.read + self.channels) % self.ring.len();
                    self.limiter.index =
                        (self.limiter.index + self.channels) % self.limiter.buffer.len();
                }
                self.limiter.index = (self.limiter.index + (19200 - frames) * self.channels)
                    % self.limiter.buffer.len();
                let out = self.limiter.process(frames, false)?;
                Self::measure(&mut self.output, self.channels, &out)?;
                self.gain.as_mut().unwrap().update(
                    self.input.integrated(),
                    self.input.shortterm(),
                    self.input.relative_threshold(),
                    self.output.shortterm(),
                )?;
                return Ok(out);
            }
            Mode::Linear(offset) => data.iter().map(|v| v * offset).collect(),
            Mode::Ended => unreachable!(),
        };
        // Original short silent clips can produce NaN via 0 * infinity; do not
        // disguise this source behavior as production-qualified finite audio.
        if result.iter().all(|v| v.is_finite()) {
            Self::measure(&mut self.output, self.channels, &result)?;
        }
        Ok(result)
    }
    pub(crate) fn finish(&mut self) -> Result<Vec<f64>> {
        if !matches!(self.mode, Mode::Dynamic) {
            self.mode = Mode::Ended;
            return Ok(Vec::new());
        }
        let offset = 21120 * self.channels;
        self.read = (self.read + self.ring.len() - offset) % self.ring.len();
        let mut data = Vec::with_capacity(556800 * self.channels);
        for _ in 0..556800 {
            data.extend_from_slice(&self.ring[self.read..self.read + self.channels]);
            self.read = (self.read + self.channels) % self.ring.len();
        }
        Self::measure(&mut self.input, self.channels, &data)?;
        let gain = self.gain.as_ref().unwrap().pair().0;
        self.limiter.index = 0;
        for (dest, src) in self.limiter.buffer.iter_mut().zip(&data) {
            *dest = *src * gain;
        }
        let mut source = self.limiter.buffer.len();
        let mut out = Vec::with_capacity(data.len());
        for _ in 0..29 {
            out.extend(self.limiter.process(19200, false)?);
            for _ in 0..19200 {
                for c in 0..self.channels {
                    self.limiter.buffer[self.limiter.index + c] = if source < data.len() {
                        data[source + c] * gain
                    } else {
                        0.
                    };
                }
                if source < data.len() {
                    source += self.channels;
                }
                self.limiter.index =
                    (self.limiter.index + self.channels) % self.limiter.buffer.len();
            }
        }
        Self::measure(&mut self.output, self.channels, &out)?;
        self.mode = Mode::Ended;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires original FFmpeg executable via LOUDNESS_FFMPEG"]
    fn original_full_frames_and_eof() {
        use std::{
            io::{BufWriter, Seek, SeekFrom, Write},
            process::{Command, Stdio},
        };
        let ffmpeg = std::env::var("LOUDNESS_FFMPEG").expect("set LOUDNESS_FFMPEG");
        let mut values = 0;
        let mut failed_cases = 0;
        for channels in [1, 2] {
            for target in [-30., -23., -12.] {
                for seconds in [0.2, 0.4, 2.99, 3., 3.0005, 3.1, 4.3] {
                    let frames = (seconds * 192000.) as usize;
                    let data: Vec<f64> = (0..frames)
                        .flat_map(|i| {
                            (0..channels).map(move |c| {
                                let t = i as f64 / 192000.;
                                let gain = if t < 0.3 {
                                    0.02
                                } else if t < 1.7 {
                                    0.7
                                } else if t < 3.3 {
                                    0.08
                                } else {
                                    1.2
                                };
                                gain * (t * std::f64::consts::TAU * (317. + c as f64 * 680.)).sin()
                            })
                        })
                        .collect();
                    let mut core = Loudness::new(channels, target).unwrap();
                    let mut actual = Vec::new();
                    let first = frames.min(576000) * channels;
                    actual.extend(core.frame(&data[..first]).unwrap());
                    for block in data[first..].chunks(19200 * channels) {
                        actual.extend(core.frame(block).unwrap());
                    }
                    actual.extend(core.finish().unwrap());
                    assert!(core.finish().unwrap().is_empty());
                    assert!(core.frame(&[0.; 2][..channels]).is_err());
                    let mut raw = tempfile::tempfile().unwrap();
                    {
                        let mut writer = BufWriter::new(&mut raw);
                        for v in &data {
                            writer.write_all(&v.to_le_bytes()).unwrap();
                        }
                        writer.flush().unwrap();
                    }
                    raw.seek(SeekFrom::Start(0)).unwrap();
                    let filter = format!("loudnorm=I={target:.1}:TP=-1.5:LRA=11");
                    let output = Command::new(&ffmpeg)
                        .args([
                            "-v",
                            "error",
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
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(
                        actual.len() * 8,
                        output.stdout.len(),
                        "duration{seconds} channels{channels}"
                    );
                    let mut max = 0_f64;
                    for (i, (a, bytes)) in
                        actual.iter().zip(output.stdout.chunks_exact(8)).enumerate()
                    {
                        let e = f64::from_le_bytes(bytes.try_into().unwrap());
                        max = max.max((a - e).abs());
                        if !((a - e).abs() < 2e-12 || (a.is_nan() && e.is_nan())) && i == 0 {
                            eprintln!("first sample mismatch: {a} != {e}");
                        }
                    }
                    if max >= 2e-12 {
                        failed_cases += 1;
                        let reference_first =
                            f64::from_le_bytes(output.stdout[8..16].try_into().unwrap());
                        eprintln!(
                            "Failed gain diagnostic: native={} original={} input_shortterm={}",
                            actual[1] / data[1],
                            reference_first / data[1],
                            core.input.shortterm()
                        );
                    }
                    values += actual.len();
                    eprintln!("Loudness full target{target} channels{channels} duration{seconds}: {} values max{max}",actual.len());
                }
            }
        }
        eprintln!(
            "Loudness full frames: {values} original doubles compared, {failed_cases} failed cases"
        );
        assert_eq!(failed_cases, 0, "full-frame sample tolerance remains 2e-12");
    }
}
