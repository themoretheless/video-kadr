//! Native Filmcraft Lightning, source RNG/bolt/distance/glow arithmetic.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Lightning {
    pub start: [Option<f64>; 2],
    pub end: [Option<f64>; 2],
    pub segments: f32,
    pub amplitude: f32,
    pub detail: f32,
    pub detail_amplitude: f32,
    pub branching: f32,
    pub speed: f32,
    pub width: f32,
    pub core: f32,
    pub outside: [f32; 4],
    pub inside: [f32; 4],
    pub seed: f32,
    pub mode: u32,
}
impl Default for Lightning {
    fn default() -> Self {
        Self {
            start: [None, None],
            end: [None, None],
            segments: 7.,
            amplitude: 10.,
            detail: 2.,
            detail_amplitude: 0.3,
            branching: 0.3,
            speed: 10.,
            width: 10.,
            core: 0.4,
            outside: [0.35, 0.3, 1., 1.],
            inside: [1.; 4],
            seed: 0.,
            mode: 1,
        }
    }
}
struct Point {
    x: f64,
    y: f64,
}
fn point(p: [Option<f64>; 2], fraction: f64, w: usize, h: usize, scale: f32) -> Point {
    Point {
        x: p[0].map_or(w as f64 * fraction, |v| v * scale as f64),
        y: p[1].map_or(h as f64 * fraction, |v| v * scale as f64),
    }
}
struct WorkingImage {
    w: usize,
    h: usize,
    px: Vec<f32>,
}
fn unpremul(p: [f32; 4]) -> [f32; 3] {
    if p[3] <= 1e-6 {
        [0.; 3]
    } else {
        [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
    }
}
impl Lightning {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start
                .iter()
                .chain(self.end.iter())
                .flatten()
                .all(|v| v.is_finite()),
            "invalid Lightning point"
        );
        ensure!(
            self.inside
                .iter()
                .chain(self.outside.iter())
                .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "invalid Lightning color"
        );
        ensure!(self.mode <= 4, "invalid Lightning blend");
        for (v, lo, hi) in [
            (self.segments, 1., 100.),
            (self.amplitude, 0., 100.),
            (self.detail, 0., 10.),
            (self.detail_amplitude, 0., 1.),
            (self.branching, 0., 1.),
            (self.speed, 0., 100.),
            (self.width, 0., 200.),
            (self.core, 0., 1.),
            (self.seed, 0., 10000.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "invalid Lightning scalar"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        seconds: f64,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            seconds.is_finite() && scale.is_finite() && scale > 0.,
            "invalid Lightning context"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let img = &mut WorkingImage {
            w,
            h,
            px: frame.pixels().iter().flatten().copied().collect(),
        };

        let s = point(self.start, 0.1, img.w, img.h, scale);
        let en = point(self.end, 0.9, img.w, img.h, scale);
        let segs = self.segments.clamp(1.0, 100.0) as usize;
        let dist = ((en.x - s.x).hypot(en.y - s.y)) as f32;
        let amp = self.amplitude / 100.0 * dist * 0.5;
        let levels = self.detail.clamp(0.0, 6.0) as usize;
        let damp = self.detail_amplitude;
        let branching = self.branching;
        let speed = self.speed as f64;
        let width = (self.width * scale).max(0.5);
        let core = self.core;
        let outc = lin(self.outside);
        let inc = lin(self.inside);
        let mode = self.mode;
        let strike = (seconds * speed / 10.0).floor() as u64;
        let mut rng = Rng(self.seed.max(0.0) as u64 * 7919 + strike);
        let main = bolt(
            (s.x as f32, s.y as f32),
            (en.x as f32, en.y as f32),
            segs,
            amp,
            levels,
            damp,
            &mut rng,
        );
        let mut lines: Vec<(Vec<(f32, f32)>, f32)> = vec![(main.clone(), width)];
        for i in 1..main.len().saturating_sub(1) {
            checkpoint()?;
            if rng.next() < branching * 0.25 {
                let p = main[i];
                let (dx, dy) = (main[i + 1].0 - main[i - 1].0, main[i + 1].1 - main[i - 1].1);
                let a = dy.atan2(dx) + rng.signed() * 0.6;
                let l = dist * (0.15 + rng.next() * 0.25);
                let q = (p.0 + a.cos() * l, p.1 + a.sin() * l);
                lines.push((
                    bolt(p, q, 4, amp * 0.4, levels.min(3), damp, &mut rng),
                    width * 0.6,
                ));
            }
        }
        let (w, h) = (img.w, img.h);
        let mut dmap = vec![f32::INFINITY; w * h];
        for (pts, lw) in &lines {
            for seg in pts.windows(2) {
                checkpoint()?;
                let (a, b) = (seg[0], seg[1]);
                let r = lw * 3.0;
                let (x0, x1) = (
                    (a.0.min(b.0) - r).floor().max(0.0) as usize,
                    ((a.0.max(b.0) + r).ceil().max(0.0) as usize).min(w),
                );
                let (y0, y1) = (
                    (a.1.min(b.1) - r).floor().max(0.0) as usize,
                    ((a.1.max(b.1) + r).ceil().max(0.0) as usize).min(h),
                );
                let (ex, ey) = (b.0 - a.0, b.1 - a.1);
                let l2 = (ex * ex + ey * ey).max(1e-6);
                for y in y0..y1 {
                    checkpoint()?;
                    for x in x0..x1 {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let t = (((px - a.0) * ex + (py - a.1) * ey) / l2).clamp(0.0, 1.0);
                        let d =
                            ((px - a.0 - ex * t).powi(2) + (py - a.1 - ey * t).powi(2)).sqrt() / lw;
                        let i = y * w + x;
                        if d < dmap[i] {
                            dmap[i] = d;
                        }
                    }
                }
            }
        }
        img.px.chunks_mut(4).zip(dmap.iter()).for_each(|(p, &d)| {
            if !d.is_finite() {
                return;
            }
            let glow = (-(d * 1.6)).exp() * 0.9;
            let c = 1.0 - smoothstep(core * 0.5, core * 0.5 + 0.2, d);
            let col = lerp3(outc.map(|v| v * glow), inc, c);
            let a = glow.max(c).min(1.0);
            let base = unpremul([p[0], p[1], p[2], p[3]]);
            let o = match mode {
                0 => {
                    let mut q = [p[0], p[1], p[2], p[3]];
                    over(&mut q, [col[0] * a, col[1] * a, col[2] * a, a]);
                    p.copy_from_slice(&q);
                    return;
                }
                m => blend_simple(m, base, col),
            };
            let na = p[3] + (1.0 - p[3]) * a;
            let mixed = lerp3(base, o, a);
            p.copy_from_slice(&[mixed[0] * na, mixed[1] * na, mixed[2] * na, na]);
        });

        let mut output = frame.clone();
        let mut index = 0;
        output.map_premultiplied(
            &mut |_| {
                let p = [
                    img.px[index],
                    img.px[index + 1],
                    img.px[index + 2],
                    img.px[index + 3],
                ];
                index += 4;
                p
            },
            checkpoint,
        )?;
        *frame = output;
        Ok(())
    }
}
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn lin(c: [f32; 4]) -> [f32; 3] {
    std::array::from_fn(|i| {
        let v = c[i];
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}
/// Blend `src` (straight linear RGB) onto `base` (straight linear RGB) with one of
/// [`filmcraft_project::effect::SIMPLE_BLEND`] (Normal, Add, Screen, Multiply, Overlay).
#[inline]
fn blend_simple(mode: u32, base: [f32; 3], src: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for k in 0..3 {
        let (b, s) = (base[k], src[k]);
        o[k] = match mode {
            1 => b + s,
            2 => 1.0 - (1.0 - b.min(1.0)) * (1.0 - s.min(1.0)),
            3 => b * s,
            4 => {
                if b < 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b.min(1.0)) * (1.0 - s.min(1.0))
                }
            }
            _ => s,
        };
    }
    o
}

/// Premultiplied `src` over premultiplied `dst`.
#[inline]
fn over(dst: &mut [f32], src: [f32; 4]) {
    let k = 1.0 - src[3];
    for c in 0..4 {
        dst[c] = src[c] + dst[c] * k;
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

/// A bolt between two points: midpoint displacement, `levels` of detail.
fn bolt(
    a: (f32, f32),
    b: (f32, f32),
    segs: usize,
    amp: f32,
    levels: usize,
    detail_amp: f32,
    rng: &mut Rng,
) -> Vec<(f32, f32)> {
    let mut pts = vec![a];
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / len, dx / len);
    for i in 1..segs {
        let t = i as f32 / segs as f32;
        let o = rng.signed() * amp;
        pts.push((a.0 + dx * t + nx * o, a.1 + dy * t + ny * o));
    }
    pts.push(b);
    let mut a2 = amp / segs as f32 * 2.0 * detail_amp.max(0.05) * 4.0;
    for _ in 0..levels {
        let mut np = Vec::with_capacity(pts.len() * 2);
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let (ex, ey) = (q.0 - p.0, q.1 - p.1);
            let l = (ex * ex + ey * ey).sqrt().max(1e-3);
            let o = rng.signed() * a2.min(l * 0.5);
            np.push(p);
            np.push((
                (p.0 + q.0) / 2.0 - ey / l * o,
                (p.1 + q.1) / 2.0 + ex / l * o,
            ));
        }
        if let Some(&last) = pts.last() {
            np.push(last);
        }
        pts = np;
        a2 *= 0.5;
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: Lightning,
        seconds: f64,
        pixel_scale: f32,
        bits: Vec<u32>,
    }
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[test]
    fn original_lightning_working_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-lightning.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 360);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, c.seconds, c.pixel_scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Lightning case{index}"
            );
        }
    }
}
