//! Write-time tangent normalization adapted from WeftCut (MIT; see docs/licenses).
//! Coordinates are explicit; evaluation never re-solves them after trim/split.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TangentMode {
    Auto,
    Free,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Continuity {
    Smooth,
    Broken,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tangent {
    pub x: f64,
    pub y: f64,
    pub mode: TangentMode,
}
impl Tangent {
    pub fn valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && (0.0..=1.0).contains(&self.x)
    }
}
/// Domain projection used during authoring. Segment classification is supplied
/// by the owning track; procedural and linear segments do not use handles.
#[derive(Debug, Clone, PartialEq)]
pub struct TangentKey<T> {
    pub t_us: i64,
    pub value: T,
    pub in_: Tangent,
    pub out: Tangent,
    pub continuity: Continuity,
    pub outgoing_spline: bool,
}
const IN_IDENTITY: (f64, f64) = (2.0 / 3.0, 2.0 / 3.0);
const OUT_IDENTITY: (f64, f64) = (1.0 / 3.0, 1.0 / 3.0);
fn clamp01(value: f64) -> f64 {
    if value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        value
    }
}
pub fn out_slope(out: Tangent, dt_next: f64, dv_next: f64) -> Option<f64> {
    if out.x == 0.0 || dt_next <= 0.0 {
        return None;
    }
    Some((out.y / out.x) * (dv_next / dt_next))
}

pub fn in_slope(in_: Tangent, dt_prev: f64, dv_prev: f64) -> Option<f64> {
    if in_.x == 1.0 || dt_prev <= 0.0 {
        return None;
    }
    Some(((1.0 - in_.y) / (1.0 - in_.x)) * (dv_prev / dt_prev))
}

pub fn in_y_for_slope(in_x: f64, m: f64, dt_prev: f64, dv_prev: f64) -> Option<f64> {
    if dv_prev == 0.0 || dt_prev <= 0.0 || in_x == 1.0 {
        return None;
    }
    Some(1.0 - (m * (1.0 - in_x) * dt_prev) / dv_prev)
}

pub fn out_y_for_slope(out_x: f64, m: f64, dt_next: f64, dv_next: f64) -> Option<f64> {
    if dv_next == 0.0 || dt_next <= 0.0 || out_x == 0.0 {
        return None;
    }
    Some((m * out_x * dt_next) / dv_next)
}

/// Monotone-clamped tangent (scalar per microsecond) at key `i` over the
/// scalar projections `s` and times `t`: 0 at an endpoint, a local extremum,
/// or when a neighbour delta is 0 — Blender "Auto Clamped", so an Auto key
/// never overshoots.
fn tangent_at(s: &[f64], t: &[i64], i: usize) -> f64 {
    if i == 0 || i + 1 >= s.len() {
        return 0.0;
    }
    let d_prev = s[i] - s[i - 1];
    let d_next = s[i + 1] - s[i];
    if d_prev == 0.0 || d_next == 0.0 || d_prev.signum() != d_next.signum() {
        return 0.0;
    }
    let dt = (i128::from(t[i + 1]) - i128::from(t[i - 1])) as f64;
    if dt <= 0.0 {
        return 0.0;
    }
    (s[i + 1] - s[i - 1]) / dt
}

/// Resolve every `Auto` side and every `Smooth` pair of `Free` sides over
/// SORTED keys — the twin of `src/shared/tangents.ts::solveAutoTangents`,
/// which is the one main's write normalization runs. `scalar` projects a value
/// onto the axis the slopes are measured on; `None` (a non-scalar `T`) sends
/// Auto sides to the identity coordinates and skips the Smooth rule. A side
/// adjacent to a non-Spline segment is left alone: the engine ignores it.
pub fn solve_auto_tangents<T: Clone>(
    keys: &[TangentKey<T>],
    scalar: Option<&dyn Fn(&T) -> f64>,
) -> Vec<TangentKey<T>> {
    let n = keys.len();
    let s: Option<Vec<f64>> = scalar.map(|f| keys.iter().map(|k| f(&k.value)).collect());
    let t: Vec<i64> = keys.iter().map(|k| k.t_us).collect();
    let mut out: Vec<TangentKey<T>> = keys.to_vec();
    for i in 0..n {
        let k = &keys[i];
        let out_spline = i + 1 < n && keys[i].outgoing_spline;
        let in_spline = i > 0 && keys[i - 1].outgoing_spline;

        // Auto out: this key's leaving handle from the monotone slope.
        if k.out.mode == TangentMode::Auto && out_spline {
            out[i].out = match &s {
                None => Tangent {
                    x: OUT_IDENTITY.0,
                    y: OUT_IDENTITY.1,
                    mode: TangentMode::Auto,
                },
                Some(s) => {
                    let m = tangent_at(s, &t, i);
                    let dt = (i128::from(t[i + 1]) - i128::from(t[i])) as f64;
                    let dv = s[i + 1] - s[i];
                    if dv == 0.0 || dt <= 0.0 {
                        Tangent {
                            x: OUT_IDENTITY.0,
                            y: OUT_IDENTITY.1,
                            mode: TangentMode::Auto,
                        }
                    } else {
                        Tangent {
                            x: 1.0 / 3.0,
                            y: clamp01(m * dt / (3.0 * dv)),
                            mode: TangentMode::Auto,
                        }
                    }
                }
            };
        }

        // Auto in: this key's arriving handle from the same slope.
        if k.in_.mode == TangentMode::Auto && in_spline {
            out[i].in_ = match &s {
                None => Tangent {
                    x: IN_IDENTITY.0,
                    y: IN_IDENTITY.1,
                    mode: TangentMode::Auto,
                },
                Some(s) => {
                    let m = tangent_at(s, &t, i);
                    let dt = (i128::from(t[i]) - i128::from(t[i - 1])) as f64;
                    let dv = s[i] - s[i - 1];
                    if dv == 0.0 || dt <= 0.0 {
                        Tangent {
                            x: IN_IDENTITY.0,
                            y: IN_IDENTITY.1,
                            mode: TangentMode::Auto,
                        }
                    } else {
                        Tangent {
                            x: 2.0 / 3.0,
                            y: clamp01(1.0 - m * dt / (3.0 * dv)),
                            mode: TangentMode::Auto,
                        }
                    }
                }
            };
        }

        // Smooth continuity over two Free sides: OUT WINS — the arriving handle
        // is re-aimed so its slope equals the leaving handle's (deterministic
        // because main cannot know which handle was dragged).
        if let Some(s) = &s {
            if k.continuity == Continuity::Smooth
                && k.in_.mode == TangentMode::Free
                && k.out.mode == TangentMode::Free
                && out_spline
                && in_spline
            {
                let dt_next = (i128::from(t[i + 1]) - i128::from(t[i])) as f64;
                let dv_next = s[i + 1] - s[i];
                let dt_prev = (i128::from(t[i]) - i128::from(t[i - 1])) as f64;
                let dv_prev = s[i] - s[i - 1];
                if let Some(y) = out_slope(k.out, dt_next, dv_next)
                    .and_then(|m| in_y_for_slope(k.in_.x, m, dt_prev, dv_prev))
                {
                    out[i].in_.y = y;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::bezier::TimingBezier;
    use serde_json::Value;

    fn key(value: &Value) -> TangentKey<f64> {
        let side = |name: &str| Tangent {
            x: value[name]["x"].as_f64().unwrap(),
            y: value[name]["y"].as_f64().unwrap(),
            mode: match value[name]["mode"].as_str().unwrap() {
                "Auto" => TangentMode::Auto,
                "Free" => TangentMode::Free,
                _ => panic!("unknown fixture mode"),
            },
        };
        TangentKey {
            t_us: value["t_us"].as_i64().unwrap(),
            value: value["value"].as_f64().unwrap(),
            in_: side("in"),
            out: side("out"),
            continuity: match value["continuity"].as_str().unwrap() {
                "Smooth" => Continuity::Smooth,
                "Broken" => Continuity::Broken,
                _ => panic!("unknown fixture continuity"),
            },
            outgoing_spline: value["segment"]["kind"] == "Spline",
        }
    }

    #[test]
    fn upstream_solve_goldens_match_and_are_idempotent() {
        let fixture: Value = serde_json::from_str(include_str!("tangents_golden.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 5);
        for case in cases {
            let input: Vec<_> = case["input"]["value"]
                .as_array()
                .unwrap()
                .iter()
                .map(key)
                .collect();
            let expected: Vec<_> = case["expect"]["value"]
                .as_array()
                .unwrap()
                .iter()
                .map(key)
                .collect();
            let resolved = solve_auto_tangents(&input, Some(&|v: &f64| *v));
            assert_eq!(resolved.len(), expected.len());
            for (actual, want) in resolved.iter().zip(&expected) {
                assert_eq!(actual.t_us, want.t_us);
                assert_eq!(actual.value, want.value);
                assert_eq!(actual.continuity, want.continuity);
                assert_eq!(actual.outgoing_spline, want.outgoing_spline);
                for (a, b) in [(actual.in_, want.in_), (actual.out, want.out)] {
                    assert_eq!(a.x, b.x, "{}", case["name"]);
                    assert_eq!(a.mode, b.mode);
                    // Same tolerance as the source Rust/TS golden harness.
                    assert!((a.y - b.y).abs() < 1e-9, "{}", case["name"]);
                }
            }
            assert_eq!(
                solve_auto_tangents(&resolved, Some(&|v: &f64| *v)),
                resolved
            );
        }
    }

    #[test]
    fn auto_curve_does_not_overshoot_and_non_scalar_is_identity() {
        let keys: Vec<_> = [(0, 0.0), (100, 2.0), (110, 3.0), (200, -1.0), (210, -1.0)]
            .into_iter()
            .map(|(t_us, value)| TangentKey {
                t_us,
                value,
                in_: Tangent {
                    x: 0.9,
                    y: -3.0,
                    mode: TangentMode::Auto,
                },
                out: Tangent {
                    x: 0.1,
                    y: 4.0,
                    mode: TangentMode::Auto,
                },
                continuity: Continuity::Smooth,
                outgoing_spline: true,
            })
            .collect();
        let resolved = solve_auto_tangents(&keys, Some(&|v: &f64| *v));
        for pair in resolved.windows(2) {
            let curve = TimingBezier::new(
                [pair[0].out.x, pair[0].out.y],
                [pair[1].in_.x, pair[1].in_.y],
            )
            .unwrap();
            for step in 0..=1000 {
                let progress = curve.sample(step as f64 / 1000.0).unwrap();
                assert!((-1e-12..=1.0 + 1e-12).contains(&progress));
            }
        }
        let identity = solve_auto_tangents(&keys, None);
        assert_eq!(identity[1].in_.y, 2.0 / 3.0);
        assert_eq!(identity[1].out.y, 1.0 / 3.0);
        assert_eq!(identity[0].in_, keys[0].in_);
        assert_eq!(identity.last().unwrap().out, keys.last().unwrap().out);
    }

    #[test]
    fn full_signed_time_span_does_not_overflow() {
        let keys: Vec<_> = [(i64::MIN, 0.0), (0, 1.0), (i64::MAX, 2.0)]
            .into_iter()
            .map(|(t_us, value)| TangentKey {
                t_us,
                value,
                in_: Tangent {
                    x: 2.0 / 3.0,
                    y: 2.0 / 3.0,
                    mode: TangentMode::Auto,
                },
                out: Tangent {
                    x: 1.0 / 3.0,
                    y: 1.0 / 3.0,
                    mode: TangentMode::Auto,
                },
                continuity: Continuity::Smooth,
                outgoing_spline: true,
            })
            .collect();
        let resolved = solve_auto_tangents(&keys, Some(&|v: &f64| *v));
        assert!((resolved[1].out.y - 1.0 / 3.0).abs() < 1e-15);
        assert!((resolved[1].in_.y - 2.0 / 3.0).abs() < 1e-15);
        assert_eq!(clamp01(-0.0).to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn continuity_uses_world_slope_and_degenerate_sides_stay_put() {
        let out = Tangent {
            x: 0.25,
            y: 0.75,
            mode: TangentMode::Free,
        };
        let m = out_slope(out, 200.0, 3.0).unwrap();
        let y = in_y_for_slope(0.8, m, 100.0, -2.0).unwrap();
        let arriving = Tangent {
            x: 0.8,
            y,
            mode: TangentMode::Free,
        };
        assert!((in_slope(arriving, 100.0, -2.0).unwrap() - m).abs() < 1e-15);
        assert!((out_y_for_slope(out.x, m, 200.0, 3.0).unwrap() - out.y).abs() < 1e-15);
        assert_eq!(in_y_for_slope(0.8, m, 100.0, 0.0), None);
        assert_eq!(out_slope(Tangent { x: 0.0, ..out }, 200.0, 3.0), None);
        assert_eq!(in_slope(Tangent { x: 1.0, ..arriving }, 100.0, -2.0), None);
        assert!(!Tangent { x: -0.1, ..out }.valid());
    }
}
