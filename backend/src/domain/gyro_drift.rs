//! Observable gyro-bias and heading correction. No zero-yaw assumption is made.
//! A constant yaw rate cannot be separated from sensor bias using gravity alone.
use super::{insta360_gyro::SensorSample, spherical_video::Orientation};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationaryInterval {
    pub start_seconds: f64,
    pub end_seconds: f64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeadingAnchor {
    pub time_seconds: f64,
    pub heading_degrees: f64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct DriftCorrection {
    pub bias_radians_per_second: Option<[f64; 3]>,
    /// User-declared stationary windows, not automatically inferred from small IMU values.
    pub stationary_intervals: Vec<StationaryInterval>,
    /// Observed world headings; periodic angles are unwrapped near the integrated path.
    pub heading_anchors: Vec<HeadingAnchor>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftReport {
    pub source: String,
    pub bias_radians_per_second: [f64; 3],
    pub calibration_samples: usize,
    pub heading_anchors: usize,
    pub visual_frames: usize,
    pub visual_pairs: usize,
    pub visual_rejected_pairs: usize,
    pub corrected_heading_degrees: f64,
    pub warnings: Vec<String>,
}
impl Default for DriftReport {
    fn default() -> Self {
        Self {
            source: "none".into(),
            bias_radians_per_second: [0.; 3],
            calibration_samples: 0,
            heading_anchors: 0,
            visual_frames: 0,
            visual_pairs: 0,
            visual_rejected_pairs: 0,
            corrected_heading_degrees: 0.,
            warnings: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct HeadingOffset {
    pub time_seconds: f64,
    pub radians: f64,
}
impl DriftCorrection {
    pub fn validate(&self, duration: f64) -> Result<()> {
        ensure!(
            self.stationary_intervals.len() <= 16 && self.heading_anchors.len() <= 256,
            "too many drift calibration observations"
        );
        ensure!(
            self.bias_radians_per_second.is_none() || self.stationary_intervals.is_empty(),
            "choose explicit bias or declared stationary windows"
        );
        if let Some(b) = self.bias_radians_per_second {
            ensure!(
                b.iter().all(|v| v.is_finite()) && norm(b) <= 0.2,
                "invalid gyro bias (rad/s)"
            );
        }
        let mut previous = -1.;
        let mut total = 0.;
        for w in &self.stationary_intervals {
            ensure!(
                w.start_seconds.is_finite()
                    && w.end_seconds.is_finite()
                    && w.start_seconds >= 0.
                    && w.start_seconds >= previous
                    && w.end_seconds <= duration
                    && w.end_seconds - w.start_seconds >= 0.5,
                "invalid declared stationary interval"
            );
            previous = w.end_seconds;
            total += w.end_seconds - w.start_seconds;
        }
        ensure!(total <= 60., "stationary calibration exceeds 60 seconds");
        ensure!(
            self.heading_anchors.is_empty() || self.heading_anchors.len() >= 2,
            "heading drift correction needs at least two observations"
        );
        let mut previous = -1.;
        for a in &self.heading_anchors {
            ensure!(
                a.time_seconds.is_finite()
                    && a.time_seconds >= 0.
                    && a.time_seconds <= duration
                    && a.time_seconds > previous
                    && a.heading_degrees.is_finite()
                    && a.heading_degrees.abs() <= 360000.,
                "invalid observed heading anchor"
            );
            previous = a.time_seconds;
        }
        Ok(())
    }
}
fn norm(v: [f64; 3]) -> f64 {
    v.iter().map(|v| v * v).sum::<f64>().sqrt()
}
/// All admitted samples must pass a declared still-window variance test. Slow pan
/// is NEVER automatically relabelled as bias by this function or its caller.
pub fn stationary_bias(
    samples: &[SensorSample],
    windows: &[StationaryInterval],
) -> Result<([f64; 3], usize)> {
    ensure!(!windows.is_empty(), "no declared stationary intervals");
    let mut weighted = [0.; 3];
    let mut duration = 0.;
    let mut count = 0;
    for w in windows {
        let selected: Vec<_> = samples
            .iter()
            .filter(|s| s.time_seconds >= w.start_seconds && s.time_seconds <= w.end_seconds)
            .collect();
        ensure!(
            selected.len() >= 25,
            "stationary calibration has too few samples"
        );
        let span = selected.last().unwrap().time_seconds - selected[0].time_seconds;
        ensure!(span >= 0.45, "stationary calibration is too short");
        let mut gyro = [0.; 3];
        let mut acc = [0.; 3];
        let mut weight = 0.;
        for pair in selected.windows(2) {
            let dt = pair[1].time_seconds - pair[0].time_seconds;
            ensure!(dt > 0. && dt <= 0.1, "invalid stationary sample clock");
            weight += dt;
            for k in 0..3 {
                gyro[k] +=
                    (pair[0].radians_per_second[k] + pair[1].radians_per_second[k]) * 0.5 * dt;
                acc[k] += (pair[0].acceleration_g[k] + pair[1].acceleration_g[k]) * 0.5 * dt;
            }
        }
        gyro = gyro.map(|v| v / weight);
        acc = acc.map(|v| v / weight);
        let variance = |get: fn(&SensorSample) -> [f64; 3], mean: [f64; 3]| {
            selected
                .iter()
                .map(|s| (0..3).map(|k| (get(s)[k] - mean[k]).powi(2)).sum::<f64>())
                .sum::<f64>()
                / selected.len() as f64
        };
        ensure!(
            (0.95..=1.05).contains(&norm(acc))
                && variance(|s| s.acceleration_g, acc).sqrt() < 0.015,
            "declared stationary acceleration is not stable gravity"
        );
        ensure!(
            norm(gyro) <= 0.05 && variance(|s| s.radians_per_second, gyro).sqrt() < 0.003,
            "declared stationary gyro is moving or unstable"
        );
        for k in 0..3 {
            weighted[k] += gyro[k] * weight;
        }
        duration += weight;
        count += selected.len();
    }
    ensure!(duration > 0., "empty stationary calibration");
    Ok((weighted.map(|v| v / duration), count))
}
fn mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}
fn wrap(v: f64) -> f64 {
    (v + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}
fn heading(q: [f64; 4]) -> Result<f64> {
    let x = 2. * (q[1] * q[3] + q[0] * q[2]);
    let z = 1. - 2. * (q[1] * q[1] + q[2] * q[2]);
    ensure!(
        x * x + z * z > 1e-8,
        "heading is unobservable at the camera pole"
    );
    Ok(x.atan2(z))
}
pub fn orientation_at(track: &[Orientation], t: f64) -> Result<[f64; 4]> {
    ensure!(
        !track.is_empty(),
        "heading correction requires an orientation track"
    );
    let i = track.partition_point(|s| s.time_seconds <= t);
    if i == 0 {
        return Ok(track[0].quaternion);
    }
    if i == track.len() {
        return Ok(track[i - 1].quaternion);
    }
    let (a, b) = (track[i - 1], track[i]);
    let f = (t - a.time_seconds) / (b.time_seconds - a.time_seconds);
    let sign = if a
        .quaternion
        .iter()
        .zip(b.quaternion)
        .map(|(a, b)| a * b)
        .sum::<f64>()
        < 0.
    {
        -1.
    } else {
        1.
    };
    let dot = a
        .quaternion
        .iter()
        .zip(b.quaternion)
        .map(|(a, b)| a * b * sign)
        .sum::<f64>()
        .clamp(-1., 1.);
    let q = if dot > 0.9995 {
        std::array::from_fn(|k| a.quaternion[k] * (1. - f) + b.quaternion[k] * sign * f)
    } else {
        let angle = dot.acos();
        std::array::from_fn(|k| {
            (a.quaternion[k] * ((1. - f) * angle).sin()
                + b.quaternion[k] * sign * (f * angle).sin())
                / angle.sin()
        })
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    Ok(q.map(|v| v / n))
}
pub fn observed_heading_offsets(
    track: &[Orientation],
    anchors: &[HeadingAnchor],
) -> Result<Vec<HeadingOffset>> {
    let mut offsets: Vec<HeadingOffset> = vec![];
    for a in anchors {
        let measured = heading(orientation_at(track, a.time_seconds)?)?;
        let raw = wrap(a.heading_degrees.to_radians() - measured);
        let correction = if let Some(p) = offsets.last() {
            p.radians + wrap(raw - p.radians)
        } else {
            raw
        };
        offsets.push(HeadingOffset {
            time_seconds: a.time_seconds,
            radians: correction,
        });
    }
    Ok(offsets)
}
/// World-Y correction preserves the measured gravity/pitch/roll and intentional
/// motion between independently observed anchors. Ends hold; no unbounded extrapolation.
pub fn apply_heading_offsets(track: &mut [Orientation], offsets: &[HeadingOffset]) -> Result<()> {
    if offsets.is_empty() {
        return Ok(());
    }
    ensure!(
        offsets.len() >= 2
            && offsets
                .iter()
                .all(|o| o.time_seconds.is_finite() && o.radians.is_finite())
            && offsets
                .windows(2)
                .all(|w| w[1].time_seconds > w[0].time_seconds),
        "invalid heading correction clock"
    );
    for s in track {
        let i = offsets.partition_point(|o| o.time_seconds <= s.time_seconds);
        let angle = if i == 0 {
            offsets[0].radians
        } else if i == offsets.len() {
            offsets[i - 1].radians
        } else {
            let (a, b) = (offsets[i - 1], offsets[i]);
            a.radians
                + (b.radians - a.radians) * (s.time_seconds - a.time_seconds)
                    / (b.time_seconds - a.time_seconds)
        };
        let half = angle * 0.5;
        s.quaternion = mul([half.cos(), 0., half.sin(), 0.], s.quaternion);
        let n = s.quaternion.iter().map(|v| v * v).sum::<f64>().sqrt();
        ensure!(n.is_finite() && n > 1e-12, "invalid corrected orientation");
        s.quaternion = s.quaternion.map(|v| v / n);
    }
    Ok(())
}
/// Evaluate the actual product Y(delta(t))*Q(t), retaining every observed
/// heading knot. Additional knots bound the noncommuting rotation curvature;
/// merely rotating a two-key compressed gyro track would lose interior anchors.
pub fn corrected_heading_track(
    track: &[Orientation],
    offsets: &[HeadingOffset],
) -> Result<Vec<Orientation>> {
    if offsets.is_empty() {
        return Ok(track.to_vec());
    }
    ensure!(
        track.len() >= 2,
        "heading correction needs an orientation interval"
    );
    ensure!(
        offsets.len() >= 2
            && offsets
                .iter()
                .all(|o| o.time_seconds.is_finite() && o.radians.is_finite())
            && offsets
                .windows(2)
                .all(|w| w[1].time_seconds > w[0].time_seconds),
        "invalid observed heading offsets"
    );
    let mut times: Vec<_> =
        track
            .iter()
            .map(|s| s.time_seconds)
            .chain(offsets.iter().map(|o| o.time_seconds).filter(|t| {
                *t >= track[0].time_seconds && *t <= track[track.len() - 1].time_seconds
            }))
            .collect();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let angle_at = |time: f64| {
        let i = offsets.partition_point(|o| o.time_seconds <= time);
        if i == 0 {
            offsets[0].radians
        } else if i == offsets.len() {
            offsets[i - 1].radians
        } else {
            let (a, b) = (offsets[i - 1], offsets[i]);
            a.radians
                + (b.radians - a.radians) * (time - a.time_seconds)
                    / (b.time_seconds - a.time_seconds)
        }
    };
    let mut output = vec![];
    for pair in times.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let qa = orientation_at(track, a)?;
        let qb = orientation_at(track, b)?;
        let inverse_a = [qa[0], -qa[1], -qa[2], -qa[3]];
        let relative = mul(qb, inverse_a);
        let angle = 2. * relative[0].abs().clamp(-1., 1.).acos();
        let axis = relative[1].hypot(relative[3]);
        let vector = relative[1..].iter().map(|v| v * v).sum::<f64>().sqrt();
        let noncommuting = if vector > 1e-12 { axis / vector } else { 0. };
        let delta = (angle_at(b) - angle_at(a)).abs();
        let epsilon = 0.0001f64.to_radians();
        let product = angle * delta * noncommuting;
        let steps = if product <= epsilon {
            1
        } else {
            ((product / (4. * epsilon)).sqrt().ceil() as usize)
                .max((angle / 0.1).ceil() as usize)
                .max((delta / 0.1).ceil() as usize)
        };
        ensure!(
            steps <= 1000000
                && output
                    .len()
                    .checked_add(steps)
                    .is_some_and(|n| n <= 1000000),
            "heading correction exceeds motion work budget"
        );
        for i in 0..steps {
            let t = a + (b - a) * i as f64 / steps as f64;
            let q = orientation_at(track, t)?;
            let h = angle_at(t) * 0.5;
            output.push(Orientation {
                time_seconds: t,
                quaternion: mul([h.cos(), 0., h.sin(), 0.], q),
            });
        }
    }
    let t = *times.last().unwrap();
    let h = angle_at(t) * 0.5;
    output.push(Orientation {
        time_seconds: t,
        quaternion: mul([h.cos(), 0., h.sin(), 0.], orientation_at(track, t)?),
    });
    Ok(output)
}
/// Metadata output diagnostics deliberately never call a low angular rate a
/// stationary camera. Explicit bias remains in original calibrated sensor axes.
pub fn calibrate_bias(
    samples: &[SensorSample],
    config: &DriftCorrection,
    capture_windows: &[StationaryInterval],
) -> Result<DriftReport> {
    let mut report = DriftReport::default();
    if let Some(b) = config.bias_radians_per_second {
        report.source = "explicit_bias".into();
        report.bias_radians_per_second = b;
    } else if !capture_windows.is_empty() {
        let (b, n) = stationary_bias(samples, capture_windows)?;
        report.source = "declared_stationary".into();
        report.bias_radians_per_second = b;
        report.calibration_samples = n;
        report.warnings.push("Stationary intervals are caller observations; IMU-only constant yaw is not independently observable.".into());
    }
    Ok(report)
}
pub fn bias_corrected(samples: &[SensorSample], bias: [f64; 3]) -> Vec<SensorSample> {
    samples
        .iter()
        .map(|s| SensorSample {
            radians_per_second: std::array::from_fn(|k| s.radians_per_second[k] - bias[k]),
            ..*s
        })
        .collect()
}
pub fn apply_observed_headings(
    track: &mut Vec<Orientation>,
    config: &DriftCorrection,
    report: &mut DriftReport,
) -> Result<()> {
    if config.heading_anchors.is_empty() {
        return Ok(());
    }
    let offsets = observed_heading_offsets(track, &config.heading_anchors)?;
    report.corrected_heading_degrees = offsets
        .last()
        .context("missing heading correction")?
        .radians
        .to_degrees();
    report.heading_anchors = offsets.len();
    if report.source == "none" {
        report.source = "observed_heading".into()
    } else {
        report.source.push_str("+observed_heading")
    };
    *track = corrected_heading_track(track, &offsets)?;
    Ok(())
}
