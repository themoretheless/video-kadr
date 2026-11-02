//! Bounded, image-derived dual-lens overlap flow. No learned/vendor model.
//! Coordinates are a fixed equirectangular analysis grid; forward/backward
//! consistency and distinct texture matches admit symmetric seam warps only.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[path = "spherical_seam_cut.rs"]
mod seam_cut;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Feather,
    OpticalFlow,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Stitching {
    pub mode: Mode,
    pub search_radius: u8,
    pub strength: f64,
    /// Experimental view ownership. Zero preserves measured-flow blending.
    pub occlusion_strength: f64,
}
impl Default for Stitching {
    fn default() -> Self {
        Self {
            mode: Mode::Feather,
            search_radius: 6,
            strength: 1.,
            occlusion_strength: 0.,
        }
    }
}
impl Stitching {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=12).contains(&self.search_radius)
                && self.strength.is_finite()
                && (0. ..=1.).contains(&self.strength)
                && self.occlusion_strength.is_finite()
                && (0. ..=1.).contains(&self.occlusion_strength),
            "invalid optical-flow stitching parameters"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default)]
struct Pixel {
    luma: f32,
    valid: bool,
    color: [f32; 3],
    priority: f64,
    opaque: bool,
}
#[derive(Clone, Copy, Debug, Default)]
struct Vector {
    dx: f64,
    dy: f64,
    confidence: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Correspondence {
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub confidence: f64,
}
pub struct Flow {
    width: usize,
    height: usize,
    cols: usize,
    rows: usize,
    step: usize,
    vectors: Vec<Vector>,
    midpoint_vectors: Vec<Vector>,
    ownership: Vec<f32>,
    pub admitted: usize,
    pub candidates: usize,
}
const WIDTH: usize = 256;
const HEIGHT: usize = 128;
const STEP: usize = 4;
const PATCH: i32 = 2;
fn index(width: usize, height: usize, x: i32, y: i32) -> Option<usize> {
    (y >= 0 && y < height as i32).then(|| y as usize * width + x.rem_euclid(width as i32) as usize)
}
fn observation(p: [f32; 4], priority: f64) -> Pixel {
    if p.iter().any(|v| !v.is_finite()) || p[3] <= 0.9 {
        return Pixel::default();
    }
    let luma = (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]) / p[3];
    if !luma.is_finite() {
        return Pixel::default();
    }
    Pixel {
        luma,
        valid: true,
        color: std::array::from_fn(|k| p[k] / p[3]),
        priority: if priority.is_finite() {
            priority.max(0.)
        } else {
            0.
        },
        opaque: p[3] >= 0.999,
    }
}
struct Patch {
    values: [f32; 25],
    energy: f64,
}
fn patch(image: &[Pixel], w: usize, h: usize, x: i32, y: i32) -> Option<Patch> {
    let mut values = [0.; 25];
    let mut n = 0;
    for dy in -PATCH..=PATCH {
        for dx in -PATCH..=PATCH {
            let p = image[index(w, h, x + dx, y + dy)?];
            if !p.valid {
                return None;
            }
            values[n] = p.luma;
            n += 1;
        }
    }
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / 25.;
    let mut energy = 0.;
    for value in &mut values {
        *value -= mean as f32;
        energy += (*value as f64).powi(2);
    }
    // A locally flat/noisy overlap cannot constrain optical flow.
    (energy.is_finite() && energy > 0.0004).then_some(Patch { values, energy })
}
fn patch_fractional(image: &[Pixel], w: usize, h: usize, x: f64, y: f64) -> Option<Patch> {
    let mut values = [0.; 25];
    let mut at = 0;
    for dy in -PATCH..=PATCH {
        for dx in -PATCH..=PATCH {
            let sx = x + dx as f64;
            let sy = y + dy as f64;
            let ix = sx.floor() as i32;
            let iy = sy.floor() as i32;
            let tx = (sx - sx.floor()) as f32;
            let ty = (sy - sy.floor()) as f32;
            let samples = [
                image[index(w, h, ix, iy)?],
                image[index(w, h, ix + 1, iy)?],
                image[index(w, h, ix, iy + 1)?],
                image[index(w, h, ix + 1, iy + 1)?],
            ];
            if samples.iter().any(|p| !p.valid) {
                return None;
            }
            values[at] = (samples[0].luma * (1. - tx) + samples[1].luma * tx) * (1. - ty)
                + (samples[2].luma * (1. - tx) + samples[3].luma * tx) * ty;
            at += 1;
        }
    }
    let mean = values.iter().map(|v| *v as f64).sum::<f64>() / 25.;
    let mut energy = 0.;
    for value in &mut values {
        *value -= mean as f32;
        energy += (*value as f64).powi(2);
    }
    (energy.is_finite() && energy > 0.0004).then_some(Patch { values, energy })
}
fn score(a: &Patch, b: &Patch) -> f64 {
    a.values
        .iter()
        .zip(b.values)
        .map(|(a, b)| *a as f64 * b as f64)
        .sum::<f64>()
        / (a.energy * b.energy).sqrt()
}
fn matching(
    reference: &Patch,
    source: &[Pixel],
    target: &[Pixel],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    radius: i32,
) -> Option<(f64, f64, f64)> {
    matching_seed(reference, source, target, w, h, x, y, radius, [0, 0])
}
fn matching_seed(
    reference: &Patch,
    source: &[Pixel],
    target: &[Pixel],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    radius: i32,
    seed: [i32; 2],
) -> Option<(f64, f64, f64)> {
    let mut matches = Vec::with_capacity(((radius * 2 + 1).pow(2)) as usize);
    for oy in -radius..=radius {
        for ox in -radius..=radius {
            let (dx, dy) = (seed[0] + ox, seed[1] + oy);
            if let Some(candidate) = patch(target, w, h, x + dx, y + dy) {
                matches.push((dx, dy, score(reference, &candidate)));
            }
        }
    }
    matches.sort_by(|a, b| {
        b.2.total_cmp(&a.2)
            .then_with(|| (a.0 * a.0 + a.1 * a.1).cmp(&(b.0 * b.0 + b.1 * b.1)))
    });
    let &(dx, dy, best) = matches.first()?;
    if best < 0.80 {
        return None;
    }
    // Nearby pixels represent the same match peak; a separate competing peak is ambiguous.
    let alternative = matches
        .iter()
        .find(|p| (p.0 - dx).pow(2) + (p.1 - dy).pow(2) > 4)
        .map(|p| p.2)
        .unwrap_or(-1.);
    if best - alternative < 0.035 {
        return None;
    }
    let refine = |axis_x: bool| {
        let eval = |offset: i32| {
            patch(
                target,
                w,
                h,
                x + dx + if axis_x { offset } else { 0 },
                y + dy + if axis_x { 0 } else { offset },
            )
            .map(|p| score(reference, &p))
        };
        let (Some(left), Some(right)) = (eval(-1), eval(1)) else {
            return 0.;
        };
        let den = left - 2. * best + right;
        if den >= -1e-6 {
            0.
        } else {
            (0.5 * (left - right) / den).clamp(-0.5, 0.5)
        }
    };
    let fixed_target = patch(target, w, h, x + dx, y + dy)?;
    // Inverse-compositional objective interpolates the reference once instead
    // of interpolating an already-resampled target twice. The latter produces
    // systematic small-motion bias even with a continuously optimized NCC peak.
    let evaluate = |px: f64, py: f64| {
        patch_fractional(
            source,
            w,
            h,
            x as f64 - (px - dx as f64),
            y as f64 - (py - dy as f64),
        )
        .map(|p| score(&p, &fixed_target))
    };
    let mut sub_x = dx as f64 + refine(true);
    let mut sub_y = dy as f64 + refine(false);
    let mut sub_score = evaluate(sub_x, sub_y).unwrap_or(best);
    if sub_score < best {
        sub_x = dx as f64;
        sub_y = dy as f64;
        sub_score = best;
    }
    for step in [0.25, 0.125, 0.0625, 0.03125] {
        let origin = (sub_x, sub_y);
        for oy in -1..=1 {
            for ox in -1..=1 {
                if ox == 0 && oy == 0 {
                    continue;
                }
                let px = origin.0 + ox as f64 * step;
                let py = origin.1 + oy as f64 * step;
                if (px - dx as f64).abs() > 0.75 || (py - dy as f64).abs() > 0.75 {
                    continue;
                }
                if let Some(value) = evaluate(px, py) {
                    if value > sub_score {
                        sub_score = value;
                        sub_x = px;
                        sub_y = py;
                    }
                }
            }
        }
    }
    Some((
        sub_x,
        sub_y,
        ((sub_score - 0.8) / 0.2).clamp(0., 1.) * ((sub_score - alternative) / 0.12).clamp(0., 1.),
    ))
}
fn downsample(image: &[Pixel], w: usize, h: usize, scale: usize) -> Vec<Pixel> {
    let mut out = vec![Pixel::default(); (w / scale) * (h / scale)];
    for y in 0..h / scale {
        for x in 0..w / scale {
            let mut value = 0.;
            let mut valid = true;
            for dy in 0..scale {
                for dx in 0..scale {
                    let p = image[(y * scale + dy) * w + x * scale + dx];
                    value += p.luma;
                    valid &= p.valid;
                }
            }
            out[y * (w / scale) + x] = Pixel {
                luma: value / (scale * scale) as f32,
                valid,
                ..Default::default()
            };
        }
    }
    out
}
struct Pyramid {
    scale: usize,
    images: [Vec<Pixel>; 2],
}
fn matching_pyramid(
    reference: &Patch,
    images: &[Vec<Pixel>; 2],
    pyramids: &[Pyramid],
    reverse: bool,
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    radius: i32,
) -> Option<(f64, f64, f64)> {
    let (a, b) = if reverse { (1, 0) } else { (0, 1) };
    if let Some(m) = matching(reference, &images[a], &images[b], w, h, x, y, radius) {
        return Some(m);
    }
    for level in pyramids {
        let scale = level.scale;
        let (cw, ch) = (w / scale, h / scale);
        let (cx, cy) = (x.div_euclid(scale as i32), y.div_euclid(scale as i32));
        let Some(coarse) = patch(&level.images[a], cw, ch, cx, cy) else {
            continue;
        };
        let Some((dx, dy, c)) = matching(
            &coarse,
            &level.images[a],
            &level.images[b],
            cw,
            ch,
            cx,
            cy,
            radius,
        ) else {
            continue;
        };
        if c < 0.5 || (dx.abs().max(dy.abs()) * scale as f64) <= radius as f64 + 2. {
            continue;
        }
        if let Some(m) = matching_seed(
            reference,
            &images[a],
            &images[b],
            w,
            h,
            x,
            y,
            2,
            [
                (dx * scale as f64).round() as i32,
                (dy * scale as f64).round() as i32,
            ],
        ) {
            if m.0.abs().max(m.1.abs()) > radius as f64 + 0.75 {
                return Some(m);
            }
        }
    }
    None
}
/// Apply the globally selected lens only to unresolved residual disagreement.
/// Supported midpoint flow, missing/budget-limited ownership, agreeing samples
/// and partial coverage retain the calibrated blend and premultiplied alpha.
pub fn seam_weights(
    pixels: [[f32; 4]; 2],
    weights: [f64; 2],
    strength: f64,
    alignment_confidence: f64,
    owner: Option<f64>,
) -> [f64; 2] {
    if strength <= 0.
        || weights.iter().any(|w| !w.is_finite() || *w <= 0.)
        || pixels
            .iter()
            .any(|p| p.iter().any(|v| !v.is_finite()) || p[3] < 0.999)
    {
        return weights;
    }
    if alignment_confidence > 0. {
        return weights;
    }
    let difference = (0..3)
        .map(|k| (pixels[0][k] / pixels[0][3] - pixels[1][k] / pixels[1][3]).abs() as f64)
        .fold(0., f64::max);
    let smooth = |x: f64| {
        let x = x.clamp(0., 1.);
        x * x * (3. - 2. * x)
    };
    let amount = smooth((difference - 0.025) / 0.075) * strength.clamp(0., 1.);
    if amount == 0. {
        return weights;
    }
    let total = weights[0] + weights[1];
    let geometric = weights[0] / total;
    let Some(owner) = owner.filter(|v| v.is_finite()) else {
        return weights;
    };
    let owner = owner.clamp(0., 1.);
    let left = geometric + (owner - geometric) * amount;
    [left * total, (1. - left) * total]
}
impl Flow {
    /// The sampler returns premultiplied linear RGBA per lens at normalized grid
    /// coordinates. Missing coverage or transparent pixels are excluded from flow.
    pub fn prepare(
        mut sample: impl FnMut(usize, f64, f64) -> Option<[f32; 4]>,
        config: Stitching,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        config.validate()?;
        Self::prepare_size(
            WIDTH,
            HEIGHT,
            STEP,
            &mut |l, u, v| sample(l, u, v).map(|p| (p, 1.)),
            config,
            false,
            checkpoint,
        )
    }
    /// Sparse temporal/background observations retain subpixel matching while
    /// limiting a long clip's visual drift analysis to a quarter of seam nodes.
    pub fn prepare_sparse(
        mut sample: impl FnMut(usize, f64, f64) -> Option<[f32; 4]>,
        config: Stitching,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        config.validate()?;
        Self::prepare_size(
            WIDTH,
            HEIGHT,
            8,
            &mut |l, u, v| sample(l, u, v).map(|p| (p, 1.)),
            config,
            false,
            checkpoint,
        )
    }
    /// Native seam path: the priority is uncapped optical-axis coverage. The
    /// second sampling pass evaluates the actual midpoint warp through the same
    /// source-row/rolling-shutter projection hook as the final output.
    pub fn prepare_with_priority(
        mut sample: impl FnMut(usize, f64, f64) -> Option<([f32; 4], f64)>,
        config: Stitching,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        config.validate()?;
        Self::prepare_size(
            WIDTH,
            HEIGHT,
            STEP,
            &mut sample,
            config,
            config.occlusion_strength > 0.,
            checkpoint,
        )
    }
    fn prepare_size(
        w: usize,
        h: usize,
        step: usize,
        sample: &mut dyn FnMut(usize, f64, f64) -> Option<([f32; 4], f64)>,
        config: Stitching,
        compute_seam: bool,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        ensure!(
            w >= 32 && h >= 16 && w <= WIDTH && h <= HEIGHT && step >= 1,
            "invalid optical-flow grid"
        );
        let mut images = [vec![Pixel::default(); w * h], vec![Pixel::default(); w * h]];
        for y in 0..h {
            checkpoint()?;
            for x in 0..w {
                for (lens, image) in images.iter_mut().enumerate() {
                    if let Some((p, priority)) = sample(
                        lens,
                        (x as f64 + 0.5) / w as f64,
                        (y as f64 + 0.5) / h as f64,
                    ) {
                        image[y * w + x] = observation(p, priority);
                    }
                }
            }
        }
        // Seam-only coarse hypotheses must still pass fine-resolution NCC and
        // the usual reverse check. Sparse temporal drift retains its old range.
        let pyramids: Vec<_> = if step == STEP {
            [2, 4]
                .into_iter()
                .map(|scale| Pyramid {
                    scale,
                    images: [
                        downsample(&images[0], w, h, scale),
                        downsample(&images[1], w, h, scale),
                    ],
                })
                .collect()
        } else {
            Vec::new()
        };
        let cols = w.div_ceil(step);
        let rows = h.div_ceil(step);
        let mut vectors = vec![Vector::default(); cols * rows];
        let (mut admitted, mut candidates) = (0, 0);
        for row in 0..rows {
            checkpoint()?;
            for col in 0..cols {
                let (x, y) = ((col * step) as i32, (row * step) as i32);
                if !(h as f64 * 0.06..=h as f64 * 0.94).contains(&(y as f64)) {
                    continue;
                }
                let Some(a) = patch(&images[0], w, h, x, y) else {
                    continue;
                };
                if step != STEP && patch(&images[1], w, h, x, y).is_none() {
                    continue;
                }
                candidates += 1;
                let Some((dx, dy, forward)) = matching_pyramid(
                    &a,
                    &images,
                    &pyramids,
                    false,
                    w,
                    h,
                    x,
                    y,
                    config.search_radius as i32,
                ) else {
                    continue;
                };
                let Some(b) = patch(
                    &images[1],
                    w,
                    h,
                    x + dx.round() as i32,
                    y + dy.round() as i32,
                ) else {
                    continue;
                };
                let Some((bx, by, backward)) = matching_pyramid(
                    &b,
                    &images,
                    &pyramids,
                    true,
                    w,
                    h,
                    x + dx.round() as i32,
                    y + dy.round() as i32,
                    config.search_radius as i32,
                ) else {
                    continue;
                };
                if (dx + bx).powi(2) + (dy + by).powi(2) > 1. {
                    continue;
                }
                if forward.min(backward) < 0.15 {
                    continue;
                }
                // Reject a wildly different surrounding motion at interpolation time; retain
                // independently supported nodes here so depth discontinuities are not blurred.
                vectors[row * cols + col] = Vector {
                    dx,
                    dy,
                    confidence: forward.min(backward),
                };
                admitted += 1;
            }
        }
        let mut result = Self {
            width: w,
            height: h,
            cols,
            rows,
            step,
            vectors,
            midpoint_vectors: Vec::new(),
            ownership: Vec::new(),
            admitted,
            candidates,
        };
        if step == STEP {
            result.midpoint_vectors = result.build_midpoint_field(checkpoint)?;
        }
        if compute_seam {
            result.prepare_ownership(&images, sample, config, checkpoint)?;
        }
        Ok(result)
    }
    /// Measured matches remain indexed by source A for temporal consumers.
    /// Rendering is indexed by the virtual midpoint, so splat each independent
    /// anchor at A+delta/2 into a separate confidence-consistent field. Conflicting
    /// depths leave a hole rather than blending foreground and background motion.
    fn build_midpoint_field(
        &self,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vector>> {
        let mut values = vec![Vector::default(); self.vectors.len()];
        let mut coverage = vec![0f64; values.len()];
        let mut conflict = vec![false; values.len()];
        for (i, p) in self.vectors.iter().enumerate() {
            if i % self.cols == 0 {
                checkpoint()?;
            }
            if p.confidence < 0.15 {
                continue;
            }
            let gx = (i % self.cols) as f64 + p.dx / (2. * self.step as f64);
            let gy = (i / self.cols) as f64 + p.dy / (2. * self.step as f64);
            let (ix, iy) = (gx.floor() as i64, gy.floor() as i64);
            let (tx, ty) = (gx - gx.floor(), gy - gy.floor());
            for (dx, dy, w) in [
                (0, 0, (1. - tx) * (1. - ty)),
                (1, 0, tx * (1. - ty)),
                (0, 1, (1. - tx) * ty),
                (1, 1, tx * ty),
            ] {
                let row = iy + dy;
                if row < 0 || row >= self.rows as i64 || w < 1e-9 {
                    continue;
                }
                let at = row as usize * self.cols + (ix + dx).rem_euclid(self.cols as i64) as usize;
                if conflict[at] {
                    continue;
                }
                let v = &mut values[at];
                if v.confidence > 0.
                    && (v.dx / v.confidence - p.dx).hypot(v.dy / v.confidence - p.dy) > 2.
                {
                    conflict[at] = true;
                    *v = Vector::default();
                    continue;
                }
                v.dx += p.dx * p.confidence * w;
                v.dy += p.dy * p.confidence * w;
                v.confidence += p.confidence * w;
                coverage[at] += w;
            }
        }
        for (i, v) in values.iter_mut().enumerate() {
            if v.confidence > 1e-9 {
                v.dx /= v.confidence;
                v.dy /= v.confidence;
                v.confidence = (v.confidence / coverage[i].max(1.)).clamp(0., 1.);
            }
        }
        // Rasterize locally coherent source-grid triangles at their actual
        // irregular midpoints. Three measured anchors avoid interpreting the
        // midpoint-map Jacobian as lost confidence, while rejected/folded or
        // conflicting depth triangles retain the conservative sparse splat.
        let mut triangulated = vec![false; values.len()];
        for row in 0..self.rows - 1 {
            checkpoint()?;
            for col in 0..self.cols {
                for corners in [
                    [(col, row), (col + 1, row), (col, row + 1)],
                    [(col + 1, row), (col + 1, row + 1), (col, row + 1)],
                ] {
                    let anchors = corners.map(|(x, y)| self.vectors[y * self.cols + x % self.cols]);
                    if anchors.iter().any(|p| p.confidence < 0.15)
                        || anchors
                            .iter()
                            .any(|p| (p.dx - anchors[0].dx).hypot(p.dy - anchors[0].dy) > 2.)
                    {
                        continue;
                    }
                    let q: [[f64; 2]; 3] = std::array::from_fn(|i| {
                        [
                            corners[i].0 as f64 * self.step as f64 + anchors[i].dx * 0.5,
                            corners[i].1 as f64 * self.step as f64 + anchors[i].dy * 0.5,
                        ]
                    });
                    let den = (q[1][1] - q[2][1]) * (q[0][0] - q[2][0])
                        + (q[2][0] - q[1][0]) * (q[0][1] - q[2][1]);
                    if den.abs() < (self.step * self.step) as f64 * 0.1 {
                        continue;
                    }
                    let lo_x = (q.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min)
                        / self.step as f64)
                        .floor() as i64;
                    let hi_x = (q.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max)
                        / self.step as f64)
                        .ceil() as i64;
                    let lo_y = ((q.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)
                        / self.step as f64)
                        .floor() as i64)
                        .max(0);
                    let hi_y = ((q.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max)
                        / self.step as f64)
                        .ceil() as i64)
                        .min(self.rows as i64 - 1);
                    for y in lo_y..=hi_y {
                        for x in lo_x..=hi_x {
                            let (px, py) =
                                (x as f64 * self.step as f64, y as f64 * self.step as f64);
                            let wa = ((q[1][1] - q[2][1]) * (px - q[2][0])
                                + (q[2][0] - q[1][0]) * (py - q[2][1]))
                                / den;
                            let wb = ((q[2][1] - q[0][1]) * (px - q[2][0])
                                + (q[0][0] - q[2][0]) * (py - q[2][1]))
                                / den;
                            let weights = [wa, wb, 1. - wa - wb];
                            if weights.iter().any(|v| *v < -1e-7) {
                                continue;
                            }
                            let at =
                                y as usize * self.cols + x.rem_euclid(self.cols as i64) as usize;
                            if conflict[at] {
                                continue;
                            }
                            let value = Vector {
                                dx: (0..3).map(|i| anchors[i].dx * weights[i]).sum(),
                                dy: (0..3).map(|i| anchors[i].dy * weights[i]).sum(),
                                confidence: (0..3)
                                    .map(|i| anchors[i].confidence * weights[i])
                                    .sum::<f64>()
                                    .clamp(0., 1.),
                            };
                            if triangulated[at]
                                && (values[at].dx - value.dx).hypot(values[at].dy - value.dy) > 2.
                            {
                                conflict[at] = true;
                                values[at] = Vector::default();
                                continue;
                            }
                            values[at] = value;
                            triangulated[at] = true;
                        }
                    }
                }
            }
        }
        Ok(values)
    }
    fn prepare_ownership(
        &mut self,
        original: &[Vec<Pixel>; 2],
        sample: &mut dyn FnMut(usize, f64, f64) -> Option<([f32; 4], f64)>,
        config: Stitching,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let mut images = [
            vec![seam_cut::Observation::default(); self.width * self.height],
            vec![seam_cut::Observation::default(); self.width * self.height],
        ];
        for y in 0..self.height {
            checkpoint()?;
            for x in 0..self.width {
                let (u, v) = (
                    (x as f64 + 0.5) / self.width as f64,
                    (y as f64 + 0.5) / self.height as f64,
                );
                let displacement = self.displacement(u, v);
                for lens in 0..2 {
                    let p = if let Some((delta, c)) = displacement {
                        let factor = if lens == 0 { -0.5 } else { 0.5 } * config.strength * c;
                        sample(
                            lens,
                            (u + delta[0] * factor).rem_euclid(1.),
                            (v + delta[1] * factor).clamp(0., 1.),
                        )
                        .map(|(p, q)| observation(p, q))
                        .unwrap_or(original[lens][y * self.width + x])
                    } else {
                        original[lens][y * self.width + x]
                    };
                    images[lens][y * self.width + x] = seam_cut::Observation {
                        color: p.color,
                        priority: p.priority,
                        valid: p.valid && p.opaque,
                    };
                }
            }
        }
        self.ownership =
            seam_cut::solve(&images, self.width, self.height, checkpoint)?.unwrap_or_default();
        Ok(())
    }
    /// Bilinear ownership of lens0 from the globally optimized binary cut.
    /// A budget fallback returns no ownership and retains the calibrated blend.
    pub fn ownership(&self, u: f64, v: f64) -> Option<f64> {
        if self.ownership.is_empty() || !u.is_finite() || !v.is_finite() || !(0. ..=1.).contains(&v)
        {
            return None;
        }
        let x = u.rem_euclid(1.) * self.width as f64 - 0.5;
        let y = (v * self.height as f64 - 0.5).clamp(0., self.height as f64 - 1.);
        let (ix, iy) = (x.floor() as i32, y.floor() as usize);
        let (tx, ty) = (x - x.floor(), y - y.floor());
        let at = |dx: i32, dy: usize| {
            self.ownership[(iy + dy).min(self.height - 1) * self.width
                + (ix + dx).rem_euclid(self.width as i32) as usize] as f64
        };
        Some(
            (at(0, 0) * (1. - tx) + at(1, 0) * tx) * (1. - ty)
                + (at(0, 1) * (1. - tx) + at(1, 1) * tx) * ty,
        )
    }
    /// Independently admitted grid matches for robust spherical motion fitting.
    pub fn correspondences(&self) -> Vec<Correspondence> {
        self.vectors
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                if p.confidence < 0.15 {
                    return None;
                }
                let x = (i % self.cols) * self.step;
                let y = (i / self.cols) * self.step;
                let from = [
                    (x as f64 + 0.5) / self.width as f64,
                    (y as f64 + 0.5) / self.height as f64,
                ];
                Some(Correspondence {
                    from,
                    to: [
                        (from[0] + p.dx / self.width as f64).rem_euclid(1.),
                        from[1] + p.dy / self.height as f64,
                    ],
                    confidence: p.confidence,
                })
            })
            .collect()
    }
    /// Normalized A→B displacement plus confidence. Missing neighbouring nodes
    /// smoothly fade the warp to the original calibrated blend; inconsistent
    /// supported vectors at depth/occlusion boundaries never get averaged.
    pub fn displacement(&self, u: f64, v: f64) -> Option<([f64; 2], f64)> {
        if !u.is_finite() || !v.is_finite() || !(0. ..=1.).contains(&v) {
            return None;
        }
        let x = (u.rem_euclid(1.) * self.width as f64 - 0.5) / self.step as f64;
        let y = (v * self.height as f64 - 0.5) / self.step as f64;
        if y < 0. || y >= self.rows as f64 - 1. {
            return None;
        }
        let ix = x.floor() as i64;
        let iy = y.floor() as usize;
        let tx = x - x.floor();
        let ty = y - y.floor();
        let node = |cx: i64, cy: usize| {
            let field = if self.midpoint_vectors.is_empty() {
                &self.vectors
            } else {
                &self.midpoint_vectors
            };
            field[cy * self.cols + cx.rem_euclid(self.cols as i64) as usize]
        };
        let points = [
            node(ix, iy),
            node(ix + 1, iy),
            node(ix, iy + 1),
            node(ix + 1, iy + 1),
        ];
        let reference = points.iter().find(|p| p.confidence >= 0.15)?;
        if points
            .iter()
            .filter(|p| p.confidence >= 0.15)
            .any(|p| (p.dx - reference.dx).hypot(p.dy - reference.dy) > 2.)
        {
            return None;
        }
        let weights = [
            (1. - tx) * (1. - ty),
            tx * (1. - ty),
            (1. - tx) * ty,
            tx * ty,
        ];
        let confidence = points
            .iter()
            .zip(weights)
            .map(|(p, w)| p.confidence * w)
            .sum::<f64>();
        if confidence < 1e-6 {
            return None;
        }
        let dx = points
            .iter()
            .zip(weights)
            .map(|(p, w)| p.dx * p.confidence * w)
            .sum::<f64>()
            / confidence;
        let dy = points
            .iter()
            .zip(weights)
            .map(|(p, w)| p.dy * p.confidence * w)
            .sum::<f64>()
            / confidence;
        Some((
            [dx / self.width as f64, dy / self.height as f64],
            confidence,
        ))
    }
}
