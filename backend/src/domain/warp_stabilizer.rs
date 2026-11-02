//! Filmcraft Warp Stabilizer path correction math. MIT attribution: docs/licenses/filmcraft-MIT.txt.
//! Tracking, project dispatch and framing integration remain separate.
use std::ops::{Add, Mul, Sub};
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
        Vec2::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
}
impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

/// Affine transform `[a c e; b d f; 0 0 1]` mapping (x, y) → (a x + c y + e, b x + d y + f).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    pub fn translate(x: f64, y: f64) -> Self {
        Affine {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Affine {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }
    /// Rotation by `deg` degrees, clockwise on screen (y down), matching Premiere's Rotation.
    pub fn rotate_deg(deg: f64) -> Self {
        let (s, c) = deg.to_radians().sin_cos();
        Affine {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }
    /// `self * o`: apply `o` first, then `self`.
    pub fn motion(position: Vec2, scale: Vec2, rotation_deg: f64, anchor: Vec2) -> Affine {
        Self::translate(position.x, position.y)
            .then_apply(&Self::rotate_deg(rotation_deg))
            .then_apply(&Self::scale(scale.x, scale.y))
            .then_apply(&Self::translate(-anchor.x, -anchor.y))
    }
    pub fn then_apply(&self, o: &Affine) -> Affine {
        // result = self ∘ o
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }
    pub fn apply(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }
    pub fn inverse(&self) -> Option<Affine> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let a = self.d * inv;
        let b = -self.b * inv;
        let c = -self.c * inv;
        let d = self.a * inv;
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }
}

#[derive(Clone, Debug)]
pub struct StabilizerPath {
    pub w: usize,
    pub h: usize,
    pub steps: Vec<Affine>,
    pub fps: f64,
}
/// Similarity parameters (tx, ty, angle, log-scale) of an affine.
fn decompose(m: &Affine) -> [f64; 4] {
    let s = (m.a * m.a + m.b * m.b).sqrt().max(1e-9);
    [m.e, m.f, m.b.atan2(m.a), s.ln()]
}
fn compose(p: [f64; 4]) -> Affine {
    let s = p[3].exp();
    let (sn, cs) = p[2].sin_cos();
    Affine {
        a: s * cs,
        b: s * sn,
        c: -s * sn,
        d: s * cs,
        e: p[0],
        f: p[1],
    }
}

/// Per-frame correction transforms (current frame px → stabilised px, analysis pixels).
pub fn corrections(
    path: &StabilizerPath,
    smooth_frames: f64,
    no_motion: bool,
    preserve_scale: bool,
) -> Vec<Affine> {
    let n = path.steps.len() + 1;
    // cumulative: frame 0 → frame k, about the frame centre for stable angle/scale decomposition
    let (cxp, cyp) = (path.w as f64 / 2.0, path.h as f64 / 2.0);
    let to_c = Affine::translate(-cxp, -cyp);
    let from_c = Affine::translate(cxp, cyp);
    let mut cum = vec![Affine::IDENTITY; n];
    for k in 1..n {
        cum[k] = path.steps[k - 1].then_apply(&cum[k - 1]);
    }
    let params: Vec<[f64; 4]> = cum
        .iter()
        .map(|c| decompose(&to_c.then_apply(c).then_apply(&from_c)))
        .collect();
    // unwrap angles
    let mut params = params;
    for k in 1..n {
        let d = params[k][2] - params[k - 1][2];
        let wrap = (d / std::f64::consts::TAU).round() * std::f64::consts::TAU;
        params[k][2] -= wrap;
    }
    let smoothed: Vec<[f64; 4]> = if no_motion {
        let mean = params
            .iter()
            .fold([0.0; 4], |a, p| {
                [a[0] + p[0], a[1] + p[1], a[2] + p[2], a[3] + p[3]]
            })
            .map(|v| v / n as f64);
        vec![mean; n]
    } else {
        let sigma = smooth_frames.max(0.01);
        let r = (sigma * 3.0).ceil() as isize;
        (0..n as isize)
            .map(|k| {
                let mut acc = [0.0; 4];
                let mut ws = 0.0;
                for j in -r..=r {
                    let i = (k + j).clamp(0, n as isize - 1) as usize;
                    let w = (-(j as f64).powi(2) / (2.0 * sigma * sigma)).exp();
                    for c in 0..4 {
                        acc[c] += params[i][c] * w;
                    }
                    ws += w;
                }
                acc.map(|v| v / ws)
            })
            .collect()
    };
    (0..n)
        .map(|k| {
            let mut sp = smoothed[k];
            if preserve_scale {
                sp[3] = params[k][3];
            }
            // in centred coordinates: smoothed ∘ raw⁻¹
            let raw = compose(params[k]);
            let sm = compose(sp);
            let corr = raw
                .inverse()
                .map(|ri| sm.then_apply(&ri))
                .unwrap_or(Affine::IDENTITY);
            from_c.then_apply(&corr).then_apply(&to_c)
        })
        .collect()
}

/// Smallest zoom about the centre (≥ 1) that hides the borders a correction exposes.
pub fn cover_scale(corr: &Affine, w: f64, h: f64, max: f64) -> f64 {
    let Some(inv) = corr.inverse() else {
        return 1.0;
    };
    let (cx, cy) = (w / 2.0, h / 2.0);
    let covered = |z: f64| {
        [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
            .iter()
            .all(|&(x, y)| {
                let q = inv.apply(Vec2::new(cx + (x - cx) / z, cy + (y - cy) / z));
                q.x >= -0.5 && q.y >= -0.5 && q.x <= w + 0.5 && q.y <= h + 0.5
            })
    };
    if covered(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (1.0, max.max(1.0));
    if !covered(hi) {
        return hi;
    }
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if covered(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_path_and_border_zoom() {
        let path = StabilizerPath {
            w: 100,
            h: 80,
            steps: vec![Affine::IDENTITY; 4],
            fps: 24.,
        };
        for fixed in [false, true] {
            for preserve in [false, true] {
                assert_eq!(
                    corrections(&path, 2., fixed, preserve),
                    vec![Affine::IDENTITY; 5]
                );
            }
        }
        assert_eq!(cover_scale(&Affine::IDENTITY, 100., 80., 4.), 1.);
        let moved = Affine::translate(10., 0.);
        let zoom = cover_scale(&moved, 100., 80., 4.);
        assert!((zoom - 50. / 40.5).abs() < 1e-8);
        assert_eq!(cover_scale(&moved, 100., 80., 1.1), 1.1);
    }
    #[test]
    fn fixed_translation_path_removes_motion_and_preserves_dimensions() {
        let path = StabilizerPath {
            w: 100,
            h: 80,
            steps: vec![Affine::translate(2., 0.); 2],
            fps: 24.,
        };
        let correction = corrections(&path, 1., true, false);
        assert_eq!(correction.len(), 3);
        for (m, expected) in correction.iter().zip([2., 0., -2.]) {
            assert!((m.e - expected).abs() < 1e-12);
            assert_eq!(m.f, 0.);
        }
    }
}

#[cfg(test)]
mod upstream_oracle_tests {
    use super::*;
    #[test]
    fn original_correction_and_cover_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-stabilizer.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 48);
        for case in cases {
            let path = StabilizerPath {
                w: 160,
                h: 90,
                fps: 24.,
                steps: case["stepBits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        let bits: [u64; 6] = serde_json::from_value(v.clone()).unwrap();
                        let [a, b, c, d, e, f] = bits.map(f64::from_bits);
                        Affine { a, b, c, d, e, f }
                    })
                    .collect(),
            };
            let actual = corrections(
                &path,
                case["smooth"].as_f64().unwrap(),
                case["fixed"].as_bool().unwrap(),
                case["preserve"].as_bool().unwrap(),
            );
            for (index, m) in actual.iter().enumerate() {
                let expected: [u64; 6] =
                    serde_json::from_value(case["bits"][index].clone()).unwrap();
                assert_eq!(
                    [m.a, m.b, m.c, m.d, m.e, m.f].map(f64::to_bits),
                    expected,
                    "case {case} frame {index}"
                );
                assert_eq!(
                    cover_scale(m, 160., 90., 4.).to_bits(),
                    case["coverBits"][index].as_u64().unwrap()
                );
            }
        }
    }
}

/// Analyse the source clip using Filmcraft's frame count, analysis scale and
/// eight-pixel tracking inset. The caller owns media revision/cache lifetime.
/// `source_frame` supplies display-luma analysis frames at the requested scale.
pub fn analyse_clip<E>(
    now: f64,
    duration: f64,
    frame_rate: f64,
    source_width: u32,
    method: super::motion_tracker::TrackMethod,
    detailed: bool,
    source_frame: &mut dyn FnMut(f64, f32) -> Result<Option<super::motion_tracker::Gray>, E>,
    checkpoint: &mut dyn FnMut() -> Result<(), E>,
) -> Result<Option<StabilizerPath>, E> {
    use super::motion_tracker::{track_step, Prepared};
    checkpoint()?;
    if !now.is_finite() || !duration.is_finite() || !frame_rate.is_finite() || duration < 0. {
        return Ok(None);
    }
    let fps = frame_rate.max(1.);
    let n = ((duration * fps).round() as usize).clamp(1, 20_000);
    let scale = ((if detailed { 960. } else { 480. }) / source_width.max(1) as f32).min(1.);
    // Keep only adjacent pyramids. The fixed region comes from the first
    // available frame, exactly as in the source's collected-frame analysis.
    let mut previous: Option<Prepared> = None;
    let mut dimensions = None;
    let mut region = [Vec2::ZERO; 4];
    let mut steps = Vec::with_capacity(n.saturating_sub(1));
    for k in 0..n {
        checkpoint()?;
        let current = source_frame(k as f64 / fps - now, scale)?.map(Prepared::new);
        if dimensions.is_none() {
            if let Some(frame) = current.as_ref() {
                let (w, h) = (frame.base().w, frame.base().h);
                dimensions = Some((w, h));
                let m = 8.;
                region = [
                    Vec2::new(m, m),
                    Vec2::new(w as f64 - m, m),
                    Vec2::new(w as f64 - m, h as f64 - m),
                    Vec2::new(m, h as f64 - m),
                ];
            }
        }
        if k > 0 {
            checkpoint()?;
            let step = match (&previous, &current) {
                (Some(a), Some(b)) if a.base().w == b.base().w => track_step(a, b, &region, method)
                    .map(|s| s.transform)
                    .unwrap_or(Affine::IDENTITY),
                _ => Affine::IDENTITY,
            };
            steps.push(step);
        }
        previous = current;
    }
    checkpoint()?;
    let Some((w, h)) = dimensions else {
        return Ok(None);
    };
    Ok(Some(StabilizerPath { w, h, steps, fps }))
}

#[cfg(test)]
mod analysis_tests {
    use super::super::motion_tracker::{Gray, TrackMethod};
    use super::*;
    #[test]
    fn streaming_analysis_cancels_between_frames() {
        let calls = std::cell::Cell::new(0usize);
        let result = analyse_clip(
            0.,
            100.,
            60.,
            3840,
            TrackMethod::Position,
            true,
            &mut |_, scale| {
                assert_eq!(scale, 0.25);
                calls.set(calls.get() + 1);
                Ok(Some(Gray::new(32, 24, vec![0.; 768])))
            },
            &mut || {
                if calls.get() >= 2 {
                    Err("cancelled")
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(calls.get(), 2);
    }
    #[test]
    fn source_requests_missing_frames_and_cancel() {
        let mut requests = vec![];
        let path = analyse_clip(
            0.25,
            0.125,
            24.,
            1920,
            TrackMethod::Position,
            false,
            &mut |dt, scale| {
                requests.push((dt, scale));
                Ok::<_, &'static str>(if requests.len() == 2 {
                    Some(Gray::new(32, 24, vec![0.; 768]))
                } else {
                    None
                })
            },
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(path.steps, vec![Affine::IDENTITY; 2]);
        assert_eq!((path.w, path.h, path.fps), (32, 24, 24.));
        assert_eq!(
            requests,
            vec![
                (-0.25, 0.25),
                (1. / 24. - 0.25, 0.25),
                (2. / 24. - 0.25, 0.25)
            ]
        );
        let mut called = false;
        let result = analyse_clip(
            0.,
            1.,
            24.,
            1920,
            TrackMethod::Position,
            true,
            &mut |_, _| {
                called = true;
                Ok(None)
            },
            &mut || Err("cancelled"),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert!(!called);
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WarpStabilizer {
    pub result: u32,
    pub smoothness: f32,
    pub method: u32,
    pub preserve_scale: bool,
    pub framing: u32,
    pub max_scale: f32,
    pub action_safe: f32,
    pub additional_scale: f32,
    pub detailed: bool,
    pub crop_less: f32,
}
impl Default for WarpStabilizer {
    fn default() -> Self {
        Self {
            result: 0,
            smoothness: 50.,
            method: 1,
            preserve_scale: false,
            framing: 2,
            max_scale: 150.,
            action_safe: 0.,
            additional_scale: 100.,
            detailed: false,
            crop_less: 50.,
        }
    }
}
#[derive(Clone, Debug)]
pub struct StabilizedFrame {
    pub transform: Affine,
    /// Equal left/right and top/bottom normalized crop amounts, for Crop framing.
    pub crop: Option<[f32; 2]>,
    pub synthesize_edges: bool,
}
impl WarpStabilizer {
    pub fn is_valid(&self) -> bool {
        self.result <= 1
            && self.method <= 3
            && self.framing <= 3
            && [
                (self.smoothness, 0., 1000.),
                (self.max_scale, 100., 1000.),
                (self.action_safe, 0., 100.),
                (self.additional_scale, 50., 400.),
                (self.crop_less, 0., 100.),
            ]
            .into_iter()
            .all(|(v, lo, hi)| v.is_finite() && (lo..=hi).contains(&v))
    }
    /// Upstream Perspective/Subspace choices also use the similarity model.
    pub fn tracking_method(&self) -> super::motion_tracker::TrackMethod {
        if self.method == 0 {
            super::motion_tracker::TrackMethod::Position
        } else {
            super::motion_tracker::TrackMethod::PositionScaleRotation
        }
    }
    pub fn frame_plan(
        &self,
        path: &StabilizerPath,
        seconds: f64,
        width: usize,
        height: usize,
    ) -> Option<StabilizedFrame> {
        if !self.is_valid()
            || !seconds.is_finite()
            || path.w == 0
            || path.h == 0
            || width == 0
            || height == 0
            || !path.fps.is_finite()
            || path.fps <= 0.
        {
            return None;
        }
        let n = path.steps.len() + 1;
        let k = ((seconds * path.fps).round().max(0.) as usize).min(n - 1);
        let smooth = self.smoothness as f64 / 100. * path.fps;
        let crop_less = self.crop_less as f64 / 100.;
        let corr = corrections(
            path,
            smooth * (0.5 + crop_less),
            self.result == 1,
            self.preserve_scale,
        );
        let (aw, ah) = (path.w as f64, path.h as f64);
        let max_scale = (self.max_scale as f64 / 100.).max(1.);
        let safe = 1. + self.action_safe as f64 / 100.;
        let extra = self.additional_scale as f64 / 100.;
        let zoom = if self.framing == 2 {
            corr.iter()
                .map(|c| cover_scale(c, aw, ah, max_scale))
                .fold(1., f64::max)
                * safe
        } else {
            1.
        } * extra;
        let s = width as f64 / aw;
        let (lw, lh) = (width as f64, height as f64);
        let z = Affine::translate(lw / 2., lh / 2.)
            .then_apply(&Affine::scale(zoom, zoom))
            .then_apply(&Affine::translate(-lw / 2., -lh / 2.));
        let transform = z
            .then_apply(&Affine::scale(s, s))
            .then_apply(&corr[k])
            .then_apply(&Affine::scale(1. / s, 1. / s));
        let crop = if self.framing == 1 {
            let worst = corr
                .iter()
                .map(|c| cover_scale(c, aw, ah, 4.))
                .fold(1., f64::max);
            let (mx, my) = (lw * (1. - 1. / worst) / 2., lh * (1. - 1. / worst) / 2.);
            Some([(mx / lw) as f32, (my / lh) as f32])
        } else {
            None
        };
        Some(StabilizedFrame {
            transform,
            crop,
            synthesize_edges: self.framing == 3,
        })
    }
}
#[cfg(test)]
mod plan_tests {
    use super::*;
    #[test]
    fn defaults_validation_and_all_framings() {
        let effect: WarpStabilizer = serde_json::from_str("{}").unwrap();
        assert_eq!(effect, WarpStabilizer::default());
        assert!(effect.is_valid());
        let path = StabilizerPath {
            w: 100,
            h: 80,
            steps: vec![Affine::IDENTITY; 3],
            fps: 24.,
        };
        for framing in 0..4 {
            let e = WarpStabilizer {
                framing,
                ..effect.clone()
            };
            let plan = e.frame_plan(&path, 0.5, 200, 160).unwrap();
            assert_eq!(plan.transform, Affine::IDENTITY);
            assert_eq!(plan.crop, if framing == 1 { Some([0., 0.]) } else { None });
            assert_eq!(plan.synthesize_edges, framing == 3);
        }
        for method in 1..4 {
            assert_eq!(
                WarpStabilizer {
                    method,
                    ..effect.clone()
                }
                .tracking_method(),
                super::super::motion_tracker::TrackMethod::PositionScaleRotation
            );
        }
        assert!(!WarpStabilizer {
            smoothness: f32::NAN,
            ..effect.clone()
        }
        .is_valid());
        assert!(!WarpStabilizer {
            additional_scale: 49.,
            ..effect
        }
        .is_valid());
    }
}
