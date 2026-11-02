//! Image-observed residual world yaw after IMU pre-warp. Consensus must cover
//! separated background longitudes; foreground motion and ambiguous texture
//! cannot silently become a heading observation.
use super::spherical_optical_flow::{Correspondence, Flow, Mode, Stitching};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::f64::consts::{PI, TAU};
#[derive(Clone, Debug)]
pub struct WorldFrame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[f32; 4]>,
}
impl WorldFrame {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.width >= 32
                && self.height >= 16
                && self.width <= 1024
                && self.height <= 512
                && self.pixels.len() == self.width * self.height,
            "invalid visual drift frame"
        );
        Ok(())
    }
    pub fn sample(&self, u: f64, v: f64) -> Option<[f32; 4]> {
        if !u.is_finite() || !v.is_finite() || !(0. ..=1.).contains(&v) {
            return None;
        }
        let x = u.rem_euclid(1.) * self.width as f64 - 0.5;
        let y = (v * self.height as f64 - 0.5).clamp(0., (self.height - 1) as f64);
        let (ix, iy) = (x.floor() as isize, y.floor() as usize);
        let (tx, ty) = ((x - x.floor()) as f32, (y - y.floor()) as f32);
        let at = |dx: isize, dy: usize| {
            self.pixels[(iy + dy).min(self.height - 1) * self.width
                + (ix + dx).rem_euclid(self.width as isize) as usize]
        };
        let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
        Some(std::array::from_fn(|k| {
            (a[k] * (1. - tx) + b[k] * tx) * (1. - ty) + (c[k] * (1. - tx) + d[k] * tx) * ty
        }))
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YawObservation {
    pub radians: f64,
    pub inliers: usize,
    pub matches: usize,
    pub longitude_sectors: usize,
    pub latitude_bands: usize,
    pub residual_degrees: f64,
}
fn wrap(v: f64) -> f64 {
    (v + PI).rem_euclid(TAU) - PI
}
fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
/// `to-from` is the apparent background shift in IMU-world images. A positive
/// shift means overestimated gyro yaw; correction has the opposite sign.
pub fn yaw_consensus(matches: &[Correspondence]) -> Option<YawObservation> {
    let points: Vec<_> = matches
        .iter()
        .filter_map(|m| {
            if !m.from.iter().chain(&m.to).all(|v| v.is_finite())
                || m.confidence < 0.25
                || !(0.18..=0.82).contains(&m.from[1])
                || !(0.18..=0.82).contains(&m.to[1])
                || ((m.to[1] - m.from[1]) * PI).abs() > 2.5f64.to_radians()
            {
                return None;
            }
            let shift = wrap((m.to[0] - m.from[0]) * TAU);
            (shift.abs() <= 9f64.to_radians()).then_some((shift, m.confidence, m.from))
        })
        .collect();
    if points.len() < 30 {
        return None;
    }
    let centre = median(&mut points.iter().map(|p| p.0).collect::<Vec<_>>());
    let mad = median(
        &mut points
            .iter()
            .map(|p| wrap(p.0 - centre).abs())
            .collect::<Vec<_>>(),
    );
    let threshold = (mad * 3.).clamp(0.22f64.to_radians(), 0.8f64.to_radians());
    let inliers: Vec<_> = points
        .iter()
        .filter(|p| wrap(p.0 - centre).abs() <= threshold)
        .collect();
    if inliers.len() < 30 || inliers.len() * 100 < points.len() * 60 {
        return None;
    }
    let mut sectors = [false; 12];
    let mut bands = [false; 3];
    let (mut weighted, mut weights) = (0., 0.);
    for p in &inliers {
        sectors[((p.2[0].rem_euclid(1.) * 12.) as usize).min(11)] = true;
        bands[(((p.2[1] - 0.18) / 0.64 * 3.) as usize).min(2)] = true;
        weighted += p.0 * p.1;
        weights += p.1;
    }
    let longitude_sectors = sectors.iter().filter(|v| **v).count();
    let latitude_bands = bands.iter().filter(|v| **v).count();
    if longitude_sectors < 6 || latitude_bands < 2 {
        return None;
    }
    let mean = weighted / weights;
    let residual = (inliers
        .iter()
        .map(|p| wrap(p.0 - mean).powi(2) * p.1)
        .sum::<f64>()
        / weights)
        .sqrt();
    if residual > 0.5f64.to_radians() {
        return None;
    }
    Some(YawObservation {
        radians: mean,
        inliers: inliers.len(),
        matches: points.len(),
        longitude_sectors,
        latitude_bands,
        residual_degrees: residual.to_degrees(),
    })
}
#[path = "visual_gyro_drift_essential.rs"]
mod essential;

/// Translation-aware calibrated background observation, with strict cheirality
/// and scene-parallax checks. Pure rotation and planar degeneracy decline.
pub fn essential_yaw(
    matches: &[Correspondence],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<YawObservation>> {
    essential::observe(matches, checkpoint)
}

pub fn observe(
    a: &WorldFrame,
    b: &WorldFrame,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<YawObservation>> {
    a.validate()?;
    b.validate()?;
    let flow = Flow::prepare_sparse(
        |i, u, v| {
            if i == 0 {
                a.sample(u, v)
            } else {
                b.sample(u, v)
            }
        },
        Stitching {
            mode: Mode::OpticalFlow,
            search_radius: 6,
            strength: 1.,
            occlusion_strength:0.,
        },
        checkpoint,
    )?;
    let matches = flow.correspondences();
    if let Some(o) = spherical_rotation_yaw(&matches).or_else(|| yaw_consensus(&matches)) {
        return Ok(Some(o));
    }
    essential_yaw(&matches, checkpoint)
}

fn ray(p: [f64; 2]) -> [f64; 3] {
    let lon = (p[0] - 0.5) * TAU;
    let lat = (0.5 - p[1]) * PI;
    let (s, c) = lat.sin_cos();
    [c * lon.sin(), s, c * lon.cos()]
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit3(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot3(v, v).sqrt();
    (n > 1e-8).then(|| v.map(|v| v / n))
}
fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}
fn qrotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let c = cross3([q[1], q[2], q[3]], v);
    let cc = cross3([q[1], q[2], q[3]], c);
    std::array::from_fn(|k| v[k] + 2. * (q[0] * c[k] + cc[k]))
}
fn matrix_quat(m: [[f64; 3]; 3]) -> Option<[f64; 4]> {
    let mut q = if m[0][0] + m[1][1] + m[2][2] > 0. {
        let s = (1. + m[0][0] + m[1][1] + m[2][2]).sqrt() * 2.;
        [
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1. + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.;
        [
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1. + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1. + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        ]
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !n.is_finite() || n < 1e-8 {
        return None;
    }
    q = q.map(|v| v / n);
    if q[0] < 0. {
        q = q.map(|v| -v)
    }
    Some(q)
}
fn triad(a: [f64; 3], b: [f64; 3]) -> Option<[[f64; 3]; 3]> {
    let z = unit3(cross3(a, b))?;
    let y = cross3(z, a);
    Some(std::array::from_fn(|i| [a[i], y[i], z[i]]))
}
fn from_pair(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> Option<[f64; 4]> {
    if dot3(a, b).abs() > 0.94 || dot3(c, d).abs() > 0.94 {
        return None;
    }
    let x = triad(a, b)?;
    let y = triad(c, d)?;
    matrix_quat(std::array::from_fn(|i| {
        std::array::from_fn(|j| (0..3).map(|k| y[i][k] * x[j][k]).sum())
    }))
}
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let c0 = cross3(a[1], a[2]);
    let c1 = cross3(a[2], a[0]);
    let c2 = cross3(a[0], a[1]);
    let det = dot3(a[0], c0);
    if !det.is_finite() || det.abs() < 1e-5 {
        return None;
    }
    Some(std::array::from_fn(|i| {
        (c0[i] * b[0] + c1[i] * b[1] + c2[i] * b[2]) / det
    }))
}
/// A residual tilt in the accelerometer fusion is not a reason to invent yaw or
/// reject otherwise rigid background motion. Fit the full spherical rotation,
/// then observe only its world-Y twist; gravity itself is left unchanged.
pub fn spherical_rotation_yaw(matches: &[Correspondence]) -> Option<YawObservation> {
    let points: Vec<_> = matches
        .iter()
        .filter_map(|m| {
            if m.confidence < 0.25
                || !m.from.iter().chain(&m.to).all(|v| v.is_finite())
                || !(0.12..=0.88).contains(&m.from[1])
                || !(0.12..=0.88).contains(&m.to[1])
            {
                return None;
            }
            let (a, b) = (ray(m.from), ray(m.to));
            (dot3(a, b) > 15f64.to_radians().cos()).then_some((a, b, m.confidence, m.from))
        })
        .collect();
    if points.len() < 30 {
        return None;
    }
    let limit = 0.65f64.to_radians().cos();
    let mut best: Option<([f64; 4], f64, usize)> = None;
    for h in 0..128 {
        let i = (h * 37) % points.len();
        let j = (h * 97 + points.len() / 2 + 1) % points.len();
        let (a, b) = (&points[i], &points[j]);
        let Some(q) = from_pair(a.0, b.0, a.1, b.1) else {
            continue;
        };
        if q[0] < 7.5f64.to_radians().cos() {
            continue;
        }
        let mut score = 0.;
        let mut count = 0;
        for p in &points {
            if dot3(qrotate(q, p.0), p.1) >= limit {
                score += p.2;
                count += 1;
            }
        }
        if best.is_none_or(|(_, s, _)| score > s) {
            best = Some((q, score, count));
        }
    }
    let (mut q, _, _) = best?;
    for _ in 0..5 {
        let mut a = [[0.; 3]; 3];
        let mut rhs = [0.; 3];
        let mut weight = 0.;
        for p in &points {
            let v = qrotate(q, p.0);
            if dot3(v, p.1) < limit {
                continue;
            }
            weight += p.2;
            let error = cross3(v, p.1);
            for i in 0..3 {
                rhs[i] += p.2 * error[i];
                for j in 0..3 {
                    a[i][j] += p.2 * ((if i == j { 1. } else { 0. }) - v[i] * v[j]);
                }
            }
        }
        if weight <= 0. {
            return None;
        }
        for i in 0..3 {
            rhs[i] /= weight;
            for j in 0..3 {
                a[i][j] /= weight;
            }
        }
        let delta = solve3(a, rhs)?;
        let angle = dot3(delta, delta).sqrt();
        if angle < 1e-10 {
            break;
        }
        if angle > 0.1 {
            return None;
        }
        let k = (angle * 0.5).sin() / angle;
        q = qmul(
            [
                (angle * 0.5).cos(),
                delta[0] * k,
                delta[1] * k,
                delta[2] * k,
            ],
            q,
        );
    }
    let inliers: Vec<_> = points
        .iter()
        .filter(|p| dot3(qrotate(q, p.0), p.1) >= limit)
        .collect();
    if inliers.len() < 30 || inliers.len() * 100 < points.len() * 60 {
        return None;
    }
    let mut sectors = [false; 12];
    let mut bands = [false; 3];
    let mut energy = 0.;
    let mut weight = 0.;
    for p in &inliers {
        sectors[((p.3[0].rem_euclid(1.) * 12.) as usize).min(11)] = true;
        bands[(((p.3[1] - 0.12) / 0.76 * 3.) as usize).min(2)] = true;
        let error = dot3(qrotate(q, p.0), p.1).clamp(-1., 1.).acos();
        energy += error * error * p.2;
        weight += p.2;
    }
    let longitude_sectors = sectors.iter().filter(|v| **v).count();
    let latitude_bands = bands.iter().filter(|v| **v).count();
    if longitude_sectors < 6 || latitude_bands < 2 {
        return None;
    }
    let residual = (energy / weight).sqrt().to_degrees();
    if residual > 0.5 {
        return None;
    }
    let radians = 2. * q[2].atan2(q[0]);
    if radians.abs() > 9f64.to_radians() {
        return None;
    }
    Some(YawObservation {
        radians,
        inliers: inliers.len(),
        matches: points.len(),
        longitude_sectors,
        latitude_bands,
        residual_degrees: residual,
    })
}
