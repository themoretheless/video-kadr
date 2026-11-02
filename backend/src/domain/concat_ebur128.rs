//! Mono/stereo EBU meter for Concat Loudness's dynamic 192 kHz path.
//! SPDX-License-Identifier: LGPL-2.1-or-later
//! Port of FFmpeg libavfilter/ebur128.c, Copyright (c) 2011 Jan Kokemüller.
//! License: backend/licenses/FFmpeg-LGPL-2.1.txt. No warranty.
//! Original LGPL/MIT notices: backend/licenses/FFmpeg-ebur128-NOTICE.txt.
//! Not yet admitted to the production effect chain.
use anyhow::{ensure, Result};
pub(crate) struct Meter {
    fused: bool,
    channels: usize,
    tick: usize,
    needed: usize,
    short_frames: usize,
    at: usize,
    ring: Vec<Vec<f64>>,
    a: [f64; 5],
    b: [f64; 5],
    state: Vec<[f64; 5]>,
    peaks: Vec<f64>,
    boundaries: [f64; 1001],
    energies: [f64; 1000],
    blocks: [u64; 1000],
    shorts: [u64; 1000],
}
impl Meter {
    pub(crate) fn new(rate: u32, channels: usize) -> Result<Self> {
        Self::with_contraction(rate, channels, false)
    }
    /// Explicit Apple-clang contraction used by the audited FFmpeg binary.
    pub(crate) fn new_contracted(rate: u32, channels: usize) -> Result<Self> {
        Self::with_contraction(rate, channels, true)
    }
    fn with_contraction(rate: u32, channels: usize, fused: bool) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate) && (1..=2).contains(&channels),
            "unsupported EBU meter shape"
        );
        let k = (std::f64::consts::PI * 1681.974450955533 / f64::from(rate)).tan();
        let q = 0.7071752369554196;
        let vh = 10_f64.powf(3.999843853973347 / 20.);
        let vb = vh.powf(0.4996667741545416);
        let madd = |a: f64, b: f64, c: f64| if fused { a.mul_add(b, c) } else { a * b + c };
        let d = madd(k, k, 1. + k / q);
        let pb = [
            madd(k, k, vh + vb * k / q) / d,
            2. * madd(k, k, -vh) / d,
            madd(k, k, vh - vb * k / q) / d,
        ];
        let pa = [1., 2. * madd(k, k, -1.) / d, madd(k, k, 1. - k / q) / d];
        let k = (std::f64::consts::PI * 38.13547087602444 / f64::from(rate)).tan();
        let q = 0.5003270373238773;
        let ra = [
            1.,
            2. * madd(k, k, -1.) / madd(k, k, 1. + k / q),
            madd(k, k, 1. - k / q) / madd(k, k, 1. + k / q),
        ];
        let rb = [1., -2., 1.];
        fn convolution(p: [f64; 3], r: [f64; 3]) -> [f64; 5] {
            [
                p[0] * r[0],
                p[0] * r[1] + p[1] * r[0],
                p[0] * r[2] + p[1] * r[1] + p[2] * r[0],
                p[1] * r[2] + p[2] * r[1],
                p[2] * r[2],
            ]
        }
        let tick = (rate as usize + 5) / 10;
        let size = (rate as usize * 3).div_ceil(tick) * tick;
        Ok(Self {
            fused,
            channels,
            tick,
            needed: tick * 4,
            short_frames: 0,
            at: 0,
            ring: vec![vec![0.; size]; channels],
            a: if fused {
                [
                    1.,
                    pa[1] + ra[1],
                    (pa[1] * ra[1] + ra[2]) + pa[2],
                    pa[1].mul_add(ra[2], pa[2] * ra[1]),
                    pa[2] * ra[2],
                ]
            } else {
                convolution(pa, ra)
            },
            b: if fused {
                [
                    pb[0],
                    pb[0].mul_add(-2., pb[1]),
                    (pb[0] - 2. * pb[1]) + pb[2],
                    pb[1] - 2. * pb[2],
                    pb[2],
                ]
            } else {
                convolution(pb, rb)
            },
            state: vec![[0.; 5]; channels],
            peaks: vec![0.; channels],
            boundaries: std::array::from_fn(|i| 10_f64.powf((i as f64 / 10. - 70. + 0.691) / 10.)),
            energies: std::array::from_fn(|i| 10_f64.powf((i as f64 / 10. - 69.95 + 0.691) / 10.)),
            blocks: [0; 1000],
            shorts: [0; 1000],
        })
    }
    fn bin(&self, energy: f64) -> usize {
        let (mut low, mut high) = (0, 1000);
        while high - low > 1 {
            let mid = (low + high) / 2;
            if energy >= self.boundaries[mid] {
                low = mid
            } else {
                high = mid
            }
        }
        low
    }
    fn energy(&self, frames: usize) -> f64 {
        let mut sum = 0.;
        for channel in &self.ring {
            let mut cs = 0.;
            if self.at < frames {
                for &v in &channel[..self.at] {
                    cs = if self.fused {
                        v.mul_add(v, cs)
                    } else {
                        cs + v * v
                    };
                }
                for &v in &channel[channel.len() - (frames - self.at)..] {
                    cs = if self.fused {
                        v.mul_add(v, cs)
                    } else {
                        cs + v * v
                    };
                }
            } else {
                for &v in &channel[self.at - frames..self.at] {
                    cs = if self.fused {
                        v.mul_add(v, cs)
                    } else {
                        cs + v * v
                    };
                }
            }
            sum += cs;
        }
        sum / frames as f64
    }
    pub(crate) fn push(&mut self, input: &[&[f64]]) -> Result<()> {
        ensure!(input.len() == self.channels, "EBU channel mismatch");
        let frames = input[0].len();
        ensure!(
            input
                .iter()
                .all(|c| c.len() == frames && c.iter().all(|v| v.is_finite())),
            "invalid EBU input"
        );
        let mut offset = 0;
        while offset < frames {
            let n = (frames - offset).min(self.needed);
            for (c, data) in input.iter().enumerate() {
                let v = &mut self.state[c];
                for i in 0..n {
                    let sample = data[offset + i];
                    self.peaks[c] = self.peaks[c].max(sample.abs());
                    if self.fused {
                        v[0] = (-self.a[1]).mul_add(v[1], sample);
                        for j in 2..5 {
                            v[0] = (-self.a[j]).mul_add(v[j], v[0]);
                        }
                        let mut value = self.b[0].mul_add(v[0], self.b[1] * v[1]);
                        for j in 2..5 {
                            value = self.b[j].mul_add(v[j], value);
                        }
                        self.ring[c][self.at + i] = value;
                    } else {
                        v[0] = sample
                            - self.a[1] * v[1]
                            - self.a[2] * v[2]
                            - self.a[3] * v[3]
                            - self.a[4] * v[4];
                        self.ring[c][self.at + i] = self.b[0] * v[0]
                            + self.b[1] * v[1]
                            + self.b[2] * v[2]
                            + self.b[3] * v[3]
                            + self.b[4] * v[4];
                    }
                    v[4] = v[3];
                    v[3] = v[2];
                    v[2] = v[1];
                    v[1] = v[0];
                }
                for s in &mut v[1..] {
                    if s.abs() < f64::MIN_POSITIVE {
                        *s = 0.;
                    }
                }
            }
            self.at += n;
            self.short_frames += n;
            self.needed -= n;
            offset += n;
            if self.needed == 0 {
                let e = self.energy(self.tick * 4);
                if e >= self.boundaries[0] {
                    let b = self.bin(e);
                    self.blocks[b] += 1;
                }
                if self.short_frames == self.tick * 30 {
                    let e = self.energy(self.tick * 30);
                    if e >= self.boundaries[0] {
                        let b = self.bin(e);
                        self.shorts[b] += 1;
                    }
                    self.short_frames = self.tick * 20;
                }
                self.needed = self.tick;
                if self.at == self.ring[0].len() {
                    self.at = 0;
                }
            }
        }
        Ok(())
    }
    fn mean(&self, hist: &[u64; 1000]) -> Option<f64> {
        let count: u64 = hist.iter().sum();
        (count > 0).then(|| {
            hist.iter()
                .zip(self.energies)
                .map(|(&n, e)| n as f64 * e)
                .sum::<f64>()
                / count as f64
        })
    }
    fn start(&self, threshold: f64) -> usize {
        if threshold < self.boundaries[0] {
            0
        } else {
            let b = self.bin(threshold);
            b + usize::from(threshold > self.energies[b])
        }
    }
    pub(crate) fn integrated(&self) -> f64 {
        let Some(mean) = self.mean(&self.blocks) else {
            return f64::NEG_INFINITY;
        };
        let start = self.start(mean * 0.1);
        let mut energy = 0.;
        let mut count = 0;
        for i in start..1000 {
            energy += self.blocks[i] as f64 * self.energies[i];
            count += self.blocks[i];
        }
        if count == 0 {
            f64::NEG_INFINITY
        } else {
            10. * (energy / count as f64).log10() - 0.691
        }
    }
    pub(crate) fn relative_threshold(&self) -> f64 {
        self.mean(&self.blocks)
            .map_or(-70., |e| 10. * (e * 0.1).log10() - 0.691)
    }
    pub(crate) fn shortterm(&self) -> f64 {
        10. * self.energy(self.tick * 30).log10() - 0.691
    }
    pub(crate) fn peaks(&self) -> &[f64] {
        &self.peaks
    }
    pub(crate) fn range(&self) -> f64 {
        let Some(mean) = self.mean(&self.shorts) else {
            return 0.;
        };
        let start = self.start(mean * 0.01);
        let count: u64 = self.shorts[start..].iter().sum();
        if count == 0 {
            return 0.;
        }
        let low = ((count - 1) as f64 * 0.1 + 0.5) as u64;
        let high = ((count - 1) as f64 * 0.95 + 0.5) as u64;
        let percentile = |p| {
            let mut n = 0;
            for i in start..1000 {
                n += self.shorts[i];
                if n > p {
                    return 10. * self.energies[i].log10() - 0.691;
                }
            }
            unreachable!()
        };
        percentile(high) - percentile(low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    #[test]
    #[ignore = "requires independently compiled upstream EBU128_ORACLE"]
    fn upstream_meter_matches_dynamic_inputs() {
        let oracle = std::env::var("EBU128_ORACLE")
            .expect("set EBU128_ORACLE to upstream C meter executable");
        let mut comparisons = 0;
        for rate in [8000, 48000, 192000] {
            for channels in [1, 2] {
                for seconds in [0.2, 0.4, 3.0, 8.3] {
                    let n = (f64::from(rate) * seconds) as usize;
                    let data: Vec<Vec<f64>> = (0..channels)
                        .map(|c| {
                            (0..n)
                                .map(|i| {
                                    let t = i as f64 / f64::from(rate);
                                    let gain = if t < 1.2 {
                                        0.00001
                                    } else if t < 4.1 {
                                        0.4
                                    } else if t < 5.3 {
                                        0.
                                    } else {
                                        0.08
                                    };
                                    gain * (t * std::f64::consts::TAU * (317. + c as f64 * 680.))
                                        .sin()
                                })
                                .collect()
                        })
                        .collect();
                    for chunk in [137, 19200] {
                        let mut meter = Meter::new(rate, channels).unwrap();
                        for start in (0..n).step_by(chunk) {
                            let end = (start + chunk).min(n);
                            meter
                                .push(&data.iter().map(|c| &c[start..end]).collect::<Vec<_>>())
                                .unwrap();
                        }
                        let mut raw = tempfile::tempfile().unwrap();
                        for i in 0..n {
                            for c in &data {
                                raw.write_all(&c[i].to_le_bytes()).unwrap();
                            }
                        }
                        use std::io::{Seek, SeekFrom};
                        raw.seek(SeekFrom::Start(0)).unwrap();
                        let out = Command::new(&oracle)
                            .args([rate.to_string(), channels.to_string(), chunk.to_string()])
                            .stdin(Stdio::from(raw))
                            .output()
                            .unwrap();
                        assert!(out.status.success());
                        let expected: Vec<f64> = String::from_utf8(out.stdout)
                            .unwrap()
                            .split_whitespace()
                            .map(|v| v.parse().unwrap())
                            .collect();
                        let mut actual = vec![
                            meter.integrated(),
                            meter.relative_threshold(),
                            meter.shortterm(),
                            meter.range(),
                        ];
                        actual.extend_from_slice(meter.peaks());
                        assert_eq!(actual.len(), expected.len());
                        for (a, e) in actual.into_iter().zip(expected) {
                            assert!(a==e || (a-e).abs()<1e-9,"rate{rate} ch{channels} duration{seconds} chunk{chunk}: {a} != {e}");
                            comparisons += 1;
                        }
                    }
                }
            }
        }
        eprintln!("Upstream EBU meter: {comparisons} metrics matched");
    }
    #[test]
    fn silence_and_invalid_shape() {
        let mut m = Meter::new(192000, 2).unwrap();
        assert_eq!(m.integrated(), f64::NEG_INFINITY);
        assert_eq!(m.relative_threshold(), -70.);
        assert_eq!(m.range(), 0.);
        assert!(m.push(&[&[f64::NAN], &[0.]]).is_err());
        assert!(m.push(&[&[0.]]).is_err());
        assert_eq!(m.peaks(), &[0., 0.]);
        assert!(Meter::new(192000, 6).is_err());
    }
}
