//! WeftCut timing Bézier primitive. Tangent authoring and continuity belong
//! to the editor; this evaluator consumes resolved unit control points.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingBezier {
    p1: [f64; 2],
    p2: [f64; 2],
}

impl TimingBezier {
    pub fn new(p1: [f64; 2], p2: [f64; 2]) -> Option<Self> {
        if !p1.into_iter().chain(p2).all(f64::is_finite)
            || !(0.0..=1.0).contains(&p1[0])
            || !(0.0..=1.0).contains(&p2[0])
        {
            return None;
        }
        let value = Self { p1, p2 };
        value
            .coefficients(1)
            .into_iter()
            .all(f64::is_finite)
            .then_some(value)
    }

    fn coefficients(self, axis: usize) -> [f64; 3] {
        let c = 3.0 * self.p1[axis];
        let b = 3.0 * (self.p2[axis] - self.p1[axis]) - c;
        [1.0 - c - b, b, c]
    }

    fn coordinate(self, axis: usize, parameter: f64) -> f64 {
        let [a, b, c] = self.coefficients(axis);
        ((a * parameter + b) * parameter + c) * parameter
    }

    fn parameter(self, progress: f64) -> f64 {
        let [a, b, c] = self.coefficients(0);
        let mut parameter = progress;
        for _ in 0..8 {
            let error = self.coordinate(0, parameter) - progress;
            if error.abs() < 1e-7 {
                return parameter;
            }
            let derivative = (3.0 * a * parameter + 2.0 * b) * parameter + c;
            if derivative.abs() < 1e-6 {
                break;
            }
            parameter -= error / derivative;
        }
        let (mut low, mut high) = (0.0, 1.0);
        parameter = progress;
        while low < high {
            let at = self.coordinate(0, parameter);
            if (at - progress).abs() < 1e-7 {
                break;
            }
            if progress > at {
                low = parameter;
            } else {
                high = parameter;
            }
            let next = (high - low) * 0.5 + low;
            if next == parameter {
                break;
            }
            parameter = next;
        }
        parameter
    }

    pub fn sample(self, progress: f64) -> Option<f64> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return None;
        }
        if progress == 0.0 || progress == 1.0 {
            return Some(progress);
        }
        Some(self.coordinate(1, self.parameter(progress)))
    }

    /// Compiler-owned progress expression. Registers 0..2 stay available for
    /// the audio pan grid; inversion uses only 3..9 and resets them each call.
    pub fn ffmpeg_expression(self, progress: &str) -> String {
        let polynomial = |axis| {
            let [a, b, c] = self.coefficients(axis);
            format!("((({a:.17e})*ld(3)+({b:.17e}))*ld(3)+({c:.17e}))*ld(3)")
        };
        let x = polynomial(0);
        let y = polynomial(1);
        let [a, b, c] = self.coefficients(0);
        let derivative = format!("((3*({a:.17e})*ld(3)+2*({b:.17e}))*ld(3)+({c:.17e}))");
        format!(
            "(st(4,({progress}));st(3,ld(4));st(5,0);st(6,({x})-ld(4));st(7,{derivative});\
             while(lt(ld(5),8)*gte(abs(ld(6)),1e-7)*gte(abs(ld(7)),1e-6),\
               st(3,ld(3)-ld(6)/ld(7));st(5,ld(5)+1);st(6,({x})-ld(4));st(7,{derivative}));\
             if(lt(ld(5),8)*lt(abs(ld(6)),1e-7),0,\
               st(3,ld(4));st(8,0);st(9,1);st(5,0);\
               while(lt(ld(5),64)*gte(abs(({x})-ld(4)),1e-7),\
                 if(gt(ld(4),({x})),st(8,ld(3)),st(9,ld(3)));\
                 st(3,(ld(9)-ld(8))*0.5+ld(8));st(5,ld(5)+1)));\
             if(eq(ld(4),0),0,if(eq(ld(4),1),1,({y}))))"
        )
    }

    /// Exact full-segment extrema in the parameter domain; X is monotone.
    pub fn bounds(self) -> (f64, f64) {
        self.parameter_bounds(0.0, 1.0)
    }

    /// Includes the solver's X tolerance as well as mathematical extrema.
    pub fn bounds_interval(self, start: f64, end: f64) -> Option<(f64, f64)> {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end > 1.0 || start > end {
            return None;
        }
        if start == end && (start == 0.0 || start == 1.0) {
            return Some((start, start));
        }
        let invert = |x: f64| {
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..64 {
                let mid = (lo + hi) * 0.5;
                if self.coordinate(0, mid) < x {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) * 0.5
        };
        let lo = invert((start - 1e-7).max(0.0));
        let hi = invert((end + 1e-7).min(1.0));
        Some(self.parameter_bounds(lo, hi))
    }

    fn parameter_bounds(self, start: f64, end: f64) -> (f64, f64) {
        let [a, b, c] = self.coefficients(1);
        let scale = a.abs().max(b.abs()).max(c.abs());
        let (a, b, c) = (3.0 * (a / scale), 2.0 * (b / scale), c / scale);
        let left = self.coordinate(1, start);
        let right = self.coordinate(1, end);
        let mut bounds = (left.min(right), left.max(right));
        let mut include = |root: f64| {
            if root > start && root < end {
                let value = self.coordinate(1, root);
                bounds.0 = bounds.0.min(value);
                bounds.1 = bounds.1.max(value);
            }
        };
        if a == 0.0 {
            if b != 0.0 {
                include(-c / b);
            }
        } else {
            let discriminant = b * b - 4.0 * a * c;
            if discriminant >= 0.0 {
                let q = -0.5 * (b + discriminant.sqrt().copysign(b));
                if q == 0.0 {
                    include(-b / (2.0 * a));
                } else {
                    include(q / a);
                    include(c / q);
                }
            }
        }
        bounds
    }

    /// Continue uses tangent direction; zero-length sides fall back to the
    /// opposite handle, then the diagonal. Vertical handles have zero slope.
    pub fn endpoint_slope(self, before: bool) -> f64 {
        let mut direction = if before {
            self.p1
        } else {
            [1.0 - self.p2[0], 1.0 - self.p2[1]]
        };
        if direction == [0.0, 0.0] {
            direction = if before {
                self.p2
            } else {
                [1.0 - self.p1[0], 1.0 - self.p1[1]]
            };
        }
        if direction == [0.0, 0.0] {
            direction = [1.0, 1.0];
        }
        if direction[0] <= 0.0 {
            0.0
        } else {
            direction[1] / direction[0]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_bounds_include_inverse_tolerance_and_interior_extrema() {
        for points in [
            [[0.1, -2.0], [0.9, 3.0]],
            [[0.0, 0.0], [0.0, 1.0]],
            [[1.0, 0.0], [0.0, 1.0]],
        ] {
            let curve = TimingBezier::new(points[0], points[1]).unwrap();
            for (start, end) in [(0.0, 0.1), (0.2, 0.4), (0.3, 0.8), (0.8, 1.0), (0.5, 0.5)] {
                let (lo, hi) = curve.bounds_interval(start, end).unwrap();
                for index in 0..=10000 {
                    let value = curve
                        .sample(start + (end - start) * index as f64 / 10000.0)
                        .unwrap();
                    assert!(
                        value >= lo - 1e-12 && value <= hi + 1e-12,
                        "{points:?} {start}..{end} {value} outside {lo}..{hi}"
                    );
                }
            }
        }
    }

    #[test]
    fn ffmpeg_inversion_matches_native_for_arbitrary_and_degenerate_curves() {
        for points in [
            [[0.25, 0.1], [0.25, 1.0]],
            [[0.42, 0.0], [1.0, 1.0]],
            [[0.0, 0.0], [0.0, 1.0]],
            [[1.0, 0.0], [0.0, 1.0]],
            [[0.1, -2.0], [0.9, 3.0]],
        ] {
            let curve = TimingBezier::new(points[0], points[1]).unwrap();
            let expression = curve.ffmpeg_expression("t");
            let output = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("aevalsrc=exprs='{expression}':s=1000:d=1.001"),
                    "-c:a",
                    "pcm_f64le",
                    "-f",
                    "f64le",
                    "pipe:1",
                ])
                .output()
                .expect("FFmpeg required for arbitrary Bézier export qualification");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len(), 1001 * 8);
            for (index, bytes) in output.stdout.chunks_exact(8).enumerate() {
                let actual = f64::from_le_bytes(bytes.try_into().unwrap());
                let expected = curve.sample(index as f64 / 1000.0).unwrap();
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "{points:?} at={index} actual={actual} expected={expected}"
                );
            }
        }
    }

    #[test]
    fn timing_inverse_matches_exact_polynomials_and_css_curve() {
        let quadratic = TimingBezier::new([1.0 / 3.0, 0.0], [2.0 / 3.0, 1.0 / 3.0]).unwrap();
        for index in 0..=1000 {
            let x = index as f64 / 1000.0;
            assert!((quadratic.sample(x).unwrap() - x * x).abs() < 1e-12);
        }
        let css = TimingBezier::new([0.25, 0.1], [0.25, 1.0]).unwrap();
        assert!((css.sample(0.5).unwrap() - 0.802403387695).abs() < 1e-7);
        for p in [
            [[0.0, 0.0], [0.0, 1.0]],
            [[1.0, 0.0], [0.0, 1.0]],
            [[1.0, -2.0], [1.0, 3.0]],
        ] {
            let curve = TimingBezier::new(p[0], p[1]).unwrap();
            for index in 1..1000 {
                let x = index as f64 / 1000.0;
                assert!((curve.coordinate(0, curve.parameter(x)) - x).abs() < 1e-7);
            }
        }
    }

    #[test]
    fn bounds_and_continue_tangents_include_overshoot_and_degenerate_handles() {
        for p in [
            [[0.1, -2.0], [0.9, 3.0]],
            [[0.0, 2.0], [1.0, -1.0]],
            [[0.5, 0.0], [0.5, 1.0]],
        ] {
            let curve = TimingBezier::new(p[0], p[1]).unwrap();
            let (lo, hi) = curve.bounds();
            for index in 0..=10000 {
                let y = curve.coordinate(1, index as f64 / 10000.0);
                assert!(y >= lo - 1e-12 && y <= hi + 1e-12);
            }
        }
        let curve = TimingBezier::new([0.0, 0.0], [0.5, 1.0]).unwrap();
        assert_eq!(curve.endpoint_slope(true), 2.0);
        let identity = TimingBezier::new([0.0, 0.0], [0.0, 0.0]).unwrap();
        assert_eq!(identity.endpoint_slope(true), 1.0);
        let vertical = TimingBezier::new([0.0, 1.0], [1.0, 0.0]).unwrap();
        assert_eq!(vertical.endpoint_slope(true), 0.0);
        assert_eq!(vertical.endpoint_slope(false), 0.0);
        assert!(TimingBezier::new([-0.1, 0.0], [1.0, 1.0]).is_none());
        assert!(TimingBezier::new([0.0, f64::NAN], [1.0, 1.0]).is_none());
        let huge = TimingBezier::new([0.25, 1e200], [0.75, -1e200]).unwrap();
        let (lo, hi) = huge.bounds();
        assert!(lo < -1e199 && hi > 1e199);
    }
}
