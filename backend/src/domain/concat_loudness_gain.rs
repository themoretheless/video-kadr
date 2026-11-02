//! Gain history for Concat Loudness's default dynamic mode.
//! SPDX-License-Identifier: LGPL-2.1-or-later
//! Port of FFmpeg af_loudnorm.c, Copyright (c) 2016 Kyle Swanson.
//! License: backend/licenses/FFmpeg-LGPL-2.1.txt; no warranty.
//! Not yet connected to the production normalizer.
use anyhow::{ensure, Result};
pub(crate) struct Gain {
    target: f64,
    delta: [f64; 30],
    weights: [f64; 21],
    previous: f64,
    index: usize,
    above: bool,
}
impl Gain {
    pub(crate) fn new(target: f64, shortterm: f64) -> Result<Self> {
        ensure!(
            target.is_finite() && (-30. ..=-12.).contains(&target),
            "invalid Loudness target"
        );
        ensure!(
            shortterm.is_finite() || shortterm == f64::NEG_INFINITY,
            "invalid initial loudness"
        );
        // Original default measured threshold = -70, measured I = 0.
        let above = shortterm >= -70.;
        let envelope = if shortterm <= -70. {
            0.
        } else {
            target - shortterm
        };
        let initial = 10_f64.powf(envelope / 20.);
        let sigma = 3.5;
        let c1 = 1. / (sigma * (2. * std::f64::consts::PI).sqrt());
        let c2 = 2. * sigma * sigma;
        let mut weights = std::array::from_fn(|i| {
            let x = i as f64 - 10.;
            c1 * (-(x.powf(2.) / c2)).exp()
        });
        let adjust = 1. / weights.iter().sum::<f64>();
        for weight in &mut weights {
            *weight *= adjust;
        }
        Ok(Self {
            target,
            delta: [initial; 30],
            weights,
            previous: initial,
            index: 0,
            above,
        })
    }
    fn gaussian(&self, index: usize) -> f64 {
        // The source's strict >0 branch maps index10 to30, equivalent to0.
        let start = if index > 10 { index - 10 } else { index + 20 };
        let mut result = 0.;
        for i in 0..21 {
            result += self.delta[(start + i) % 30] * self.weights[i];
        }
        result
    }
    pub(crate) fn pair(&self) -> (f64, f64) {
        (
            self.gaussian((self.index + 10) % 30),
            self.gaussian((self.index + 11) % 30),
        )
    }
    pub(crate) fn update(
        &mut self,
        global: f64,
        shortterm: f64,
        relative: f64,
        output_shortterm: f64,
    ) -> Result<()> {
        ensure!(
            [global, shortterm, relative, output_shortterm]
                .iter()
                .all(|v| v.is_finite() || *v == f64::NEG_INFINITY),
            "invalid loudness measurement"
        );
        if !self.above {
            if shortterm > -70. {
                self.previous *= 1.0058;
            }
            if output_shortterm >= self.target {
                self.above = true;
            }
        }
        self.delta[self.index] = if shortterm < relative || shortterm <= -70. || !self.above {
            self.previous
        } else {
            let difference = shortterm - global;
            let envelope = if difference.abs() < 5.5 {
                difference
            } else {
                5.5 * if difference < 0. { -1. } else { 1. }
            };
            10_f64.powf((envelope + (self.target - shortterm)) / 20.)
        };
        self.previous = self.delta[self.index];
        self.index = (self.index + 1) % 30;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires independently compiled LOUDNESS_GAIN_ORACLE"]
    fn original_history_and_gaussian() {
        use std::{
            io::{Seek, SeekFrom, Write},
            process::{Command, Stdio},
        };
        let oracle = std::env::var("LOUDNESS_GAIN_ORACLE").expect("set LOUDNESS_GAIN_ORACLE");
        let mut compared = 0;
        for target in [-30., -23., -12.] {
            for initial in [f64::NEG_INFINITY, -70., -69.999, -35.25, -8.] {
                let mut gain = Gain::new(target, initial).unwrap();
                let mut input = tempfile::tempfile().unwrap();
                writeln!(input, "{target} {initial}").unwrap();
                let mut actual = Vec::new();
                for n in 0..240 {
                    let global = -65. + ((n * 37) % 600) as f64 / 10.;
                    let short = if n % 17 == 0 {
                        f64::NEG_INFINITY
                    } else {
                        -80. + ((n * 23) % 800) as f64 / 10.
                    };
                    let threshold = -70. + ((n * 7) % 250) as f64 / 10.;
                    let out = if n < 70 { -80. } else { -20. };
                    writeln!(input, "{global:.17} {short:.17} {threshold:.17} {out:.17}").unwrap();
                    let (a, b) = gain.pair();
                    actual.extend([a, b]);
                    gain.update(global, short, threshold, out).unwrap();
                }
                input.seek(SeekFrom::Start(0)).unwrap();
                let output = Command::new(&oracle)
                    .stdin(Stdio::from(input))
                    .output()
                    .unwrap();
                assert!(output.status.success());
                let expected: Vec<f64> = String::from_utf8(output.stdout)
                    .unwrap()
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
                assert_eq!(actual.len(), expected.len());
                for (i, (a, e)) in actual.into_iter().zip(expected).enumerate() {
                    assert!(
                        (a - e).abs() <= 2e-12 * e.abs().max(1.),
                        "target{target} initial{initial} value{i}: {a} != {e}"
                    );
                    compared += 1;
                }
            }
        }
        eprintln!("Original Loudnorm gain: {compared} smoothed gains matched");
    }
    #[test]
    fn threshold_hold_recovery_and_wrap() {
        let mut g = Gain::new(-23., f64::NEG_INFINITY).unwrap();
        for _ in 0..60 {
            g.update(
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
                -70.,
                f64::NEG_INFINITY,
            )
            .unwrap();
        }
        let (a, b) = g.pair();
        assert!((a - 1.).abs() < 1e-14 && (b - 1.).abs() < 1e-14);
        for _ in 0..60 {
            g.update(-30., -30., -40., -23.).unwrap();
        }
        let expected = 10_f64.powf(7. / 20.);
        let (a, b) = g.pair();
        assert!((a - expected).abs() < 1e-14 && (b - expected).abs() < 1e-14);
        assert!(g.update(f64::NAN, -30., -40., -23.).is_err());
    }
}
