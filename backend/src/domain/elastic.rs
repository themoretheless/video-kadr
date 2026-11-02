//! Parameterized WeftCut/Penner Elastic primitives for keyframe segments.
//! Segment owners must retain exact key values at boundaries; the formulas
//! deliberately retain Penner's small residual strictly inside the segment.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElasticDirection {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Elastic {
    direction: ElasticDirection,
    amplitude: f64,
    period: f64,
}

impl Elastic {
    pub fn new(direction: ElasticDirection, amplitude: f64, period: f64) -> Option<Self> {
        if !amplitude.is_finite() || amplitude < 1.0 || !period.is_finite() || period <= 0.0 {
            return None;
        }
        // Reject parameters whose angular frequency cannot be represented.
        if !(std::f64::consts::TAU / period).is_finite() {
            return None;
        }
        Some(Self {
            direction,
            amplitude,
            period,
        })
    }

    pub fn sample(self, progress: f64) -> f64 {
        let phase = self.period / std::f64::consts::TAU * (1.0 / self.amplitude).asin();
        let out = |t: f64| {
            self.amplitude
                * 2.0_f64.powf(-10.0 * t)
                * ((t - phase) * std::f64::consts::TAU / self.period).sin()
                + 1.0
        };
        // In is not the reflection of Out when amplitude > 1.
        let inward = |t: f64| {
            -(self.amplitude
                * 2.0_f64.powf(10.0 * (t - 1.0))
                * ((t - 1.0 - phase) * std::f64::consts::TAU / self.period).sin())
        };
        match self.direction {
            ElasticDirection::In => inward(progress),
            ElasticDirection::Out => out(progress),
            ElasticDirection::InOut if progress < 0.5 => 0.5 * inward(2.0 * progress),
            ElasticDirection::InOut => 0.5 + 0.5 * out(2.0 * progress - 1.0),
        }
    }

    /// Bounds of the closed segment interval, including exact endpoint keys
    /// and one-sided Penner residues. Cost is independent of oscillation count.
    pub fn bounds(self, start: f64, end: f64) -> Option<(f64, f64)> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end > 1.0 || start > end {
            return None;
        }
        let mut bounds = match self.direction {
            ElasticDirection::In | ElasticDirection::Out => self.branch_bounds(start, end),
            ElasticDirection::InOut => {
                let mut bounds = (f64::INFINITY, f64::NEG_INFINITY);
                if start < 0.5 {
                    let branch = Self {
                        direction: ElasticDirection::In,
                        ..self
                    };
                    let (lo, hi) = branch.branch_bounds(2.0 * start, 2.0 * end.min(0.5));
                    bounds = (0.5 * lo, 0.5 * hi);
                }
                if end >= 0.5 {
                    let branch = Self {
                        direction: ElasticDirection::Out,
                        ..self
                    };
                    let (lo, hi) =
                        branch.branch_bounds(2.0 * start.max(0.5) - 1.0, 2.0 * end - 1.0);
                    bounds = (bounds.0.min(0.5 + 0.5 * lo), bounds.1.max(0.5 + 0.5 * hi));
                }
                bounds
            }
        };
        if start == 0.0 {
            bounds.0 = bounds.0.min(0.0);
            bounds.1 = bounds.1.max(0.0);
        }
        if end == 1.0 {
            bounds.0 = bounds.0.min(1.0);
            bounds.1 = bounds.1.max(1.0);
        }
        (bounds.0.is_finite() && bounds.1.is_finite()).then_some(bounds)
    }

    fn branch_bounds(self, start: f64, end: f64) -> (f64, f64) {
        let a = self.sample(start);
        let b = self.sample(end);
        let mut bounds = (a.min(b), a.max(b));
        let omega = std::f64::consts::TAU / self.period;
        let inward = self.direction == ElasticDirection::In;
        let k = if inward {
            10.0 * std::f64::consts::LN_2
        } else {
            -10.0 * std::f64::consts::LN_2
        };
        let shift = if inward { 1.0 } else { 0.0 };
        let phase = (1.0 / self.amplitude).asin();
        // d(exp(k*t)*sin(theta))/dt = exp(k*t)*(k*sin(theta)+omega*cos(theta)).
        let root = (-omega).atan2(k);
        let first = (((start - shift) * omega - phase - root) / std::f64::consts::PI).ceil();
        let last = (((end - shift) * omega - phase - root) / std::f64::consts::PI).floor();
        // Consecutive stationary points alternate sign; their magnitudes grow
        // (In) or decay (Out) monotonically. Only the first/last pair can win.
        for cycle in [first, first + 1.0, last - 1.0, last] {
            if cycle < first || cycle > last {
                continue;
            }
            let at = shift + (root + phase + cycle * std::f64::consts::PI) / omega;
            if at >= start && at <= end {
                let value = self.sample(at);
                bounds.0 = bounds.0.min(value);
                bounds.1 = bounds.1.max(value);
            }
        }
        bounds
    }

    /// The input is a compiler-owned FFmpeg expression, never user text.
    pub fn ffmpeg_expression(self, progress: &str) -> String {
        let amplitude = format!("{:.17}", self.amplitude);
        let period = format!("{:.17e}", self.period);
        let phase = format!("(({period})/(2*PI)*asin(1/({amplitude})))");
        let out = |t: &str| {
            format!("(({amplitude})*pow(2,-10*({t}))*sin((({t})-({phase}))*2*PI/({period}))+1)")
        };
        let inward = |t: &str| {
            format!("(-({amplitude})*pow(2,10*(({t})-1))*sin((({t})-1-({phase}))*2*PI/({period})))")
        };
        match self.direction {
            ElasticDirection::In => inward(progress),
            ElasticDirection::Out => out(progress),
            ElasticDirection::InOut => format!(
                "if(lt(({progress}),0.5),0.5*({}),0.5+0.5*({}))",
                inward(&format!("2*({progress})")),
                out(&format!("2*({progress})-1"))
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_bounds_cover_overshoots_and_partial_intervals() {
        for direction in [
            ElasticDirection::In,
            ElasticDirection::Out,
            ElasticDirection::InOut,
        ] {
            for (amplitude, period) in [(1.0, 0.3), (2.0, 0.17), (4.0, 0.011), (1.5, 3.0)] {
                let elastic = Elastic::new(direction, amplitude, period).unwrap();
                for (start, end) in [(0.0, 1.0), (0.1, 0.4), (0.3, 0.8), (0.6, 0.9), (0.5, 0.5)] {
                    let (lo, hi) = elastic.bounds(start, end).unwrap();
                    let mut sampled = (f64::INFINITY, f64::NEG_INFINITY);
                    for n in 0..=100000 {
                        let at = start + (end - start) * n as f64 / 100000.0;
                        let value = elastic.sample(at);
                        assert!(value >= lo-1e-12 && value <= hi+1e-12, "{direction:?} a={amplitude} p={period} at={at} {value} outside {lo}..{hi}");
                        sampled.0 = sampled.0.min(value);
                        sampled.1 = sampled.1.max(value);
                    }
                    if start == 0.0 {
                        sampled.0 = sampled.0.min(0.0);
                        sampled.1 = sampled.1.max(0.0);
                    }
                    if end == 1.0 {
                        sampled.0 = sampled.0.min(1.0);
                        sampled.1 = sampled.1.max(1.0);
                    }
                    assert!(sampled.0 - lo < 0.0001 && hi - sampled.1 < 0.0001, "{direction:?} a={amplitude} p={period} interval={start}..{end} sampled={sampled:?} bounds={lo}..{hi}");
                }
            }
        }
        assert!(
            Elastic::new(ElasticDirection::Out, 1.0, 0.3)
                .unwrap()
                .bounds(0.0, 1.0)
                .unwrap()
                .1
                > 1.3
        );
        let fast = Elastic::new(ElasticDirection::Out, 2.0, 1e-9).unwrap();
        let (lo, hi) = fast.bounds(0.0, 1.0).unwrap();
        assert!(lo < -0.9 && hi > 2.9);
        assert!(fast.bounds(-0.1, 0.5).is_none());
        assert!(fast.bounds(0.9, 0.1).is_none());
    }

    #[test]
    fn pinned_source_values_and_parameter_validation() {
        let out = Elastic::new(ElasticDirection::Out, 1.0, 0.3).unwrap();
        for (t, expected) in [(0.1, 1.25), (0.25, 0.9116116523516815), (0.5, 1.015625)] {
            assert!((out.sample(t) - expected).abs() < 1e-12);
        }
        let inward = Elastic::new(ElasticDirection::In, 1.0, 0.3).unwrap();
        assert!((inward.sample(0.25) + 0.005524271728019903).abs() < 1e-12);
        let a2_in = Elastic::new(ElasticDirection::In, 2.0, 0.3).unwrap();
        let a2_out = Elastic::new(ElasticDirection::Out, 2.0, 0.3).unwrap();
        assert!((a2_in.sample(0.5) - (1.0 - a2_out.sample(0.5))).abs() > 0.01);
        for (a, p) in [
            (0.9, 0.3),
            (f64::NAN, 0.3),
            (1.0, 0.0),
            (1.0, -1.0),
            (1.0, f64::INFINITY),
        ] {
            assert!(Elastic::new(ElasticDirection::Out, a, p).is_none());
        }
    }

    #[test]
    fn parameterized_directions_match_real_ffmpeg() {
        for direction in [
            ElasticDirection::In,
            ElasticDirection::Out,
            ElasticDirection::InOut,
        ] {
            for (amplitude, period) in [(1.0, 0.3), (2.0, 0.17), (1.5, 0.6)] {
                let elastic = Elastic::new(direction, amplitude, period).unwrap();
                let expression = elastic.ffmpeg_expression("t");
                let output = std::process::Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        &format!("aevalsrc=exprs='{expression}':s=1000:d=1"),
                        "-c:a",
                        "pcm_f64le",
                        "-f",
                        "f64le",
                        "pipe:1",
                    ])
                    .output()
                    .expect("FFmpeg required for Elastic qualification");
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout.len(), 1000 * 8);
                for (tick, bytes) in output.stdout.chunks_exact(8).enumerate() {
                    let actual = f64::from_le_bytes(bytes.try_into().unwrap());
                    assert!(
                        (actual - elastic.sample(tick as f64 / 1000.0)).abs() < 1e-12,
                        "{direction:?} a={amplitude} p={period} t={tick}"
                    );
                }
            }
        }
    }
}
