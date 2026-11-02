//! Independent source-local instance witness. SAM class similarity alone is not
//! instance identity. Texture recovery requires unique RGB patch correspondences
//! and an affine-consistent transferred authored support, not just one helmet.
use crate::{object_matting::Mask, Image};
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone)]
struct Point {
    x: f32,
    y: f32,
    bits: [u64; 4],
}
#[derive(Clone)]
pub struct Instance {
    points: Vec<Point>,
    mask: Mask,
    size: [usize; 2],
    centre: f32,
    bounds: [f32; 4],
    uniform: Option<[f32; 3]>,
}
struct Proof {
    matrix: [f32; 6],
    pairs: Vec<(usize, usize)>,
}
fn check(c: &AtomicBool) -> Result<()> {
    ensure!(!c.load(Ordering::Relaxed), "instance evidence cancelled");
    Ok(())
}
fn sample(mask: &Mask, u: f32, v: f32) -> bool {
    (0. ..1.).contains(&v) && crate::mask_apply::sample(mask, u.rem_euclid(1.), v) >= 0.5
}
fn bounds(mask: &Mask) -> ([f32; 4], f32) {
    let mut columns = vec![false; mask.width];
    let (mut top, mut bottom) = (mask.height, 0);
    for (i, v) in mask.bytes.iter().enumerate() {
        if *v >= 128 {
            columns[i % mask.width] = true;
            top = top.min(i / mask.width);
            bottom = bottom.max(i / mask.width);
        }
    }
    if top == mask.height {
        return ([0., 1., 0., 1.], 0.5);
    }
    let (mut run, mut largest, mut end) = (0usize, 0usize, 0usize);
    for i in 0..2 * mask.width {
        if columns[i % mask.width] {
            run = 0;
        } else {
            run = (run + 1).min(mask.width);
            if run > largest {
                largest = run;
                end = i + 1;
            }
        }
    }
    let start = if largest == 0 { 0 } else { end % mask.width };
    let width = mask.width - largest;
    let centre = (start as f32 + width as f32 / 2.) / mask.width as f32;
    (
        [
            start as f32 / mask.width as f32,
            (start + width) as f32 / mask.width as f32,
            top as f32 / mask.height as f32,
            (bottom + 1) as f32 / mask.height as f32,
        ],
        centre,
    )
}
fn unwrap(x: f32, centre: f32, width: f32) -> f32 {
    centre + (x - centre + width / 2.).rem_euclid(width) - width / 2.
}
fn uniform(image: &Image, mask: &Mask) -> Option<[f32; 3]> {
    let mut bins = vec![(0usize, [0f32; 3]); 4096];
    let mut total = 0usize;
    let mut saturated = 0usize;
    for y in 0..image.height {
        for x in 0..image.width {
            if crate::mask_apply::sample(
                mask,
                (x as f32 + 0.5) / image.width as f32,
                (y as f32 + 0.5) / image.height as f32,
            ) < 0.9
            {
                continue;
            }
            total += 1;
            let p = &image.rgba[(y * image.width + x) * 4..][..3];
            let hi = *p.iter().max().unwrap() as f32;
            let lo = *p.iter().min().unwrap() as f32;
            if hi < 32. || (hi - lo) / hi < 0.65 {
                continue;
            }
            saturated += 1;
            let sum = p.iter().map(|v| f32::from(*v)).sum::<f32>();
            let c = [p[0] as f32 / sum, p[1] as f32 / sum, p[2] as f32 / sum];
            let index = ((c[0] * 16.).floor() as usize).min(15) * 256
                + ((c[1] * 16.).floor() as usize).min(15) * 16
                + ((c[2] * 16.).floor() as usize).min(15);
            bins[index].0 += 1;
            for j in 0..3 {
                bins[index].1[j] += c[j];
            }
        }
    }
    if total < 16 || saturated as f32 / (total as f32) < 0.9 {
        return None;
    }
    let best = bins.into_iter().max_by_key(|b| b.0)?;
    if best.0 as f32 / (total as f32) < 0.9 {
        return None;
    }
    Some(best.1.map(|v| v / best.0 as f32))
}
const CIRCLE: [(i32, i32); 16] = [
    (0, -3),
    (1, -3),
    (2, -2),
    (3, -1),
    (3, 0),
    (3, 1),
    (2, 2),
    (1, 3),
    (0, 3),
    (-1, 3),
    (-2, 2),
    (-3, 1),
    (-3, 0),
    (-3, -1),
    (-2, -2),
    (-1, -3),
];
fn corner(gray: &[f32], w: usize, h: usize, x: i32, y: i32) -> f32 {
    if y < 9 || y + 9 >= h as i32 {
        return 0.;
    }
    let at =
        |dx: i32, dy: i32| gray[(y + dy) as usize * w + (x + dx).rem_euclid(w as i32) as usize];
    let centre = at(0, 0);
    let quick = [0, 4, 8, 12].map(|i| at(CIRCLE[i].0, CIRCLE[i].1) - centre);
    if quick.iter().filter(|v| **v > 12.).count() < 3
        && quick.iter().filter(|v| **v < -12.).count() < 3
    {
        return 0.;
    }
    let ring = CIRCLE.map(|(dx, dy)| at(dx, dy) - centre);
    let mut score = 0f32;
    for start in 0..16 {
        let mut high = f32::INFINITY;
        let mut low = f32::INFINITY;
        for n in 0..9 {
            high = high.min(ring[(start + n) % 16]);
            low = low.min(-ring[(start + n) % 16]);
        }
        score = score.max(high).max(low);
    }
    if score >= 12. {
        score
    } else {
        0.
    }
}
fn bits(rgba: &[u8], gray: &[f32], w: usize, x: i32, y: i32) -> [u64; 4] {
    let mut mx = 0f32;
    let mut my = 0f32;
    for dy in -8i32..=8 {
        for dx in -8i32..=8 {
            if dx * dx + dy * dy > 64 {
                continue;
            }
            let value = gray[(y + dy) as usize * w + (x + dx).rem_euclid(w as i32) as usize];
            mx += dx as f32 * value;
            my += dy as f32 * value;
        }
    }
    let angle = my.atan2(mx);
    let (s, c) = angle.sin_cos();
    let mut state = 0x6a09e667u32;
    let mut out = [0u64; 4];
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        ((state % 17) as i32 - 8) as f32
    };
    let mut coordinate = || loop {
        let x = next();
        let y = next();
        if x * x + y * y <= 64. {
            break (x, y);
        }
    };
    for i in 0..256 {
        let (ax, ay) = coordinate();
        let (bx, by) = coordinate();
        let point = |a: f32, b: f32| {
            let xx = x + (a * c - b * s).round() as i32;
            let yy = y + (a * s + b * c).round() as i32;
            let yy = yy.clamp(y - 8, y + 8);
            rgba[(yy as usize * w + xx.rem_euclid(w as i32) as usize) * 4 + i % 3]
        };
        if point(ax, ay) < point(bx, by) {
            out[i / 64] |= 1u64 << (i % 64);
        }
    }
    out
}
impl Instance {
    pub fn new(image: &Image, mask: &Mask, cancel: &AtomicBool) -> Result<Self> {
        check(cancel)?;
        ensure!(
            mask.width > 0
                && mask.height > 0
                && mask.width.checked_mul(mask.height) == Some(mask.bytes.len()),
            "invalid instance support"
        );
        let (b, centre) = bounds(mask);
        let mut points = vec![];
        let base =
            image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.rgba.clone())
                .ok_or_else(|| anyhow::anyhow!("invalid instance RGB"))?;
        let base_scale = (1024. / image.width.max(image.height) as f32).min(1.);
        for factor in [1., 0.75, 0.5, 0.35, 0.25, 0.175] {
            check(cancel)?;
            let scale = base_scale * factor;
            let (w, h) = (
                (image.width as f32 * scale).round() as usize,
                (image.height as f32 * scale).round() as usize,
            );
            if w < 24 || h < 24 {
                continue;
            }
            let scaled = image::imageops::resize(
                &base,
                w as u32,
                h as u32,
                image::imageops::FilterType::Triangle,
            );
            let rgba = scaled.as_raw();
            let gray: Vec<_> = rgba
                .chunks_exact(4)
                .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                .collect();
            let mut cells: Vec<Vec<(f32, i32, i32)>> = (0..16).map(|_| vec![]).collect();
            for y in 9..h as i32 - 9 {
                check(cancel)?;
                for x in 0..w as i32 {
                    let u = (x as f32 + 0.5) / w as f32;
                    let v = (y as f32 + 0.5) / h as f32;
                    if !sample(mask, u, v) {
                        continue;
                    }
                    let score = corner(&gray, w, h, x, y);
                    if score == 0. {
                        continue;
                    }
                    let support = [-8, 0, 8]
                        .iter()
                        .flat_map(|dy| [-8, 0, 8].iter().map(move |dx| (*dx, *dy)))
                        .filter(|(dx, dy)| {
                            sample(mask, (x + dx) as f32 / w as f32, (y + dy) as f32 / h as f32)
                        })
                        .count();
                    if support < 7 {
                        continue;
                    }
                    let uu = unwrap(u, centre, 1.);
                    let col = (((uu - b[0]) / (b[1] - b[0]).max(1e-6) * 4.).floor() as i32)
                        .clamp(0, 3) as usize;
                    let row = (((v - b[2]) / (b[3] - b[2]).max(1e-6) * 4.).floor() as i32)
                        .clamp(0, 3) as usize;
                    cells[row * 4 + col].push((score, x, y));
                }
            }
            for mut cell in cells {
                cell.sort_by(|a, b| b.0.total_cmp(&a.0));
                let mut chosen: Vec<[i32; 2]> = vec![];
                for (_, x, y) in cell {
                    if chosen
                        .iter()
                        .any(|p| ((p[0] - x).pow(2) + (p[1] - y).pow(2)) < 25)
                    {
                        continue;
                    }
                    chosen.push([x, y]);
                    points.push(Point {
                        x: unwrap(
                            (x as f32 + 0.5) / scale,
                            centre * image.width as f32,
                            image.width as f32,
                        ),
                        y: (y as f32 + 0.5) / scale,
                        bits: bits(rgba, &gray, w, x, y),
                    });
                    if chosen.len() == 3 {
                        break;
                    }
                }
            }
        }
        Ok(Self {
            points,
            mask: mask.clone(),
            size: [image.width, image.height],
            centre,
            bounds: b,
            uniform: uniform(image, mask),
        })
    }
    fn shape_similarity(&self, other: &Self) -> f32 {
        let mut both = 0usize;
        let mut union = 0usize;
        for y in 0..24 {
            for x in 0..24 {
                let cell = |v: &Self| {
                    sample(
                        &v.mask,
                        v.bounds[0] + (x as f32 + 0.5) / 24. * (v.bounds[1] - v.bounds[0]),
                        v.bounds[2] + (y as f32 + 0.5) / 24. * (v.bounds[3] - v.bounds[2]),
                    )
                };
                let (a, b) = (cell(self), cell(other));
                both += usize::from(a && b);
                union += usize::from(a || b);
            }
        }
        both as f32 / union.max(1) as f32
    }
    fn low_texture_match(&self, other: &Self) -> bool {
        if self.points.len() >= 6 || other.points.len() >= 6 {
            return false;
        }
        let (Some(a), Some(b)) = (self.uniform, other.uniform) else {
            return false;
        };
        let aspect = |v: &Self| (v.bounds[1] - v.bounds[0]) / (v.bounds[3] - v.bounds[2]).max(1e-6);
        a.iter().zip(b).all(|(a, b)| (*a - b).abs() <= 0.035)
            && (aspect(self) / aspect(other)).ln().abs() <= 1.25f32.ln()
            && self.shape_similarity(other) >= 0.8
    }
    pub fn matches(&self, other: &Self, cancel: &AtomicBool) -> Result<bool> {
        Ok(self.low_texture_match(other) || self.proof(other, cancel)?.is_some())
    }
    /// Keep only new-view features tied to the immutable original, not unmatched
    /// background patches accidentally included by a propagated SAM plane.
    pub fn retain_original_witness(&self, other: &mut Self, cancel: &AtomicBool) -> Result<bool> {
        if self.low_texture_match(other) {
            return Ok(true);
        }
        let Some(proof) = self.proof(other, cancel)? else {
            return Ok(false);
        };
        let keep: std::collections::BTreeSet<_> = proof.pairs.iter().map(|p| p.1).collect();
        other.points = other
            .points
            .iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, p)| p.clone())
            .collect();
        Ok(true)
    }
    fn proof(&self, other: &Self, cancel: &AtomicBool) -> Result<Option<Proof>> {
        if self.points.len() < 6 || other.points.len() < 6 {
            return Ok(None);
        }
        let distance = |a: &Point, b: &Point| {
            a.bits
                .iter()
                .zip(b.bits)
                .map(|(a, b)| (*a ^ b).count_ones())
                .sum::<u32>()
        };
        let mut pairs = vec![];
        for (i, a) in self.points.iter().enumerate() {
            check(cancel)?;
            let mut best = (u32::MAX, 0);
            let mut second = u32::MAX;
            for (j, b) in other.points.iter().enumerate() {
                let d = distance(a, b);
                if d < best.0 {
                    second = best.0;
                    best = (d, j);
                } else {
                    second = second.min(d);
                }
            }
            if best.0 > 64 || second == 0 || best.0 as f32 > second as f32 * 0.75 {
                continue;
            }
            let b = &other.points[best.1];
            let reverse = self
                .points
                .iter()
                .enumerate()
                .map(|(k, p)| (distance(p, b), k))
                .min();
            if reverse.is_some_and(|v| v.1 == i) {
                pairs.push((i, best.1, best.0));
            }
        }
        #[cfg(test)]
        if std::env::var_os("VIDEO_KADR_INSTANCE_TRACE").is_some() {
            eprintln!(
                "instance mutual sourceFeatures={} currentFeatures={} ratioMatches={}",
                self.points.len(),
                other.points.len(),
                pairs.len()
            );
        }
        pairs.sort_by_key(|p| p.2);
        if pairs.len() < 6 {
            return Ok(None);
        }
        pairs.truncate(64);
        let tolerance = (0.025
            * ((other.bounds[1] - other.bounds[0]) * other.size[0] as f32)
                .hypot((other.bounds[3] - other.bounds[2]) * other.size[1] as f32))
        .clamp(3., 8.);
        let mut winner = None;
        let mut count = 0;
        let mut random = 0xbb67ae85u32;
        for iteration in 0..256 {
            check(cancel)?;
            let mut pick = || {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                (random as usize) % pairs.len()
            };
            let indices = if iteration == 0 {
                [0, 1, 2]
            } else {
                [pick(), pick(), pick()]
            };
            if indices[0] == indices[1] || indices[0] == indices[2] || indices[1] == indices[2] {
                continue;
            }
            let source = indices.map(|i| &self.points[pairs[i].0]);
            let target = indices.map(|i| &other.points[pairs[i].1]);
            let Some(matrix) = affine(source, target) else {
                continue;
            };
            let det = matrix[0] * matrix[4] - matrix[1] * matrix[3];
            if !(0.04..=25.).contains(&det) {
                continue;
            }
            let inliers: Vec<_> = pairs
                .iter()
                .filter(|p| {
                    let a = &self.points[p.0];
                    let b = &other.points[p.1];
                    (matrix[0] * a.x + matrix[1] * a.y + matrix[2] - b.x)
                        .hypot(matrix[3] * a.x + matrix[4] * a.y + matrix[5] - b.y)
                        <= tolerance
                })
                .map(|p| (p.0, p.1))
                .collect();
            if inliers.len() > count && inliers.len() >= 6 {
                count = inliers.len();
                winner = Some(Proof {
                    matrix,
                    pairs: inliers,
                });
            }
        }
        let Some(proof) = winner else { return Ok(None) };
        if proof.pairs.len() * 2 < pairs.len() {
            return Ok(None);
        }
        let mut distances: Vec<_> = proof
            .pairs
            .iter()
            .map(|(i, j)| distance(&self.points[*i], &other.points[*j]))
            .collect();
        distances.sort_unstable();
        if distances[distances.len() / 2] > 48 {
            return Ok(None);
        }
        #[cfg(test)]
        if std::env::var_os("VIDEO_KADR_INSTANCE_TRACE").is_some() {
            eprintln!("instance witness sourceFeatures={} currentFeatures={} ratioMatches={} affineInliers={}",self.points.len(),other.points.len(),pairs.len(),proof.pairs.len());
        }

        let (mut left, mut right, mut top, mut bottom) = (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        );
        for (i, _) in &proof.pairs {
            let p = &self.points[*i];
            left = left.min(p.x);
            right = right.max(p.x);
            top = top.min(p.y);
            bottom = bottom.max(p.y);
        }
        if right - left < ((self.bounds[1] - self.bounds[0]) * self.size[0] as f32 * 0.2).max(4.)
            || bottom - top
                < ((self.bounds[3] - self.bounds[2]) * self.size[1] as f32 * 0.2).max(4.)
        {
            return Ok(None);
        }
        let (mut both, mut total, mut expected) = (0usize, 0usize, 0usize);
        let m = proof.matrix;
        let det = m[0] * m[4] - m[1] * m[3];
        for y in 0..other.mask.height {
            check(cancel)?;
            for x in 0..other.mask.width {
                let px = unwrap(
                    (x as f32 + 0.5) / other.mask.width as f32 * other.size[0] as f32,
                    other.centre * other.size[0] as f32,
                    other.size[0] as f32,
                ) - m[2];
                let py = (y as f32 + 0.5) / other.mask.height as f32 * other.size[1] as f32 - m[5];
                let sx = (m[4] * px - m[1] * py) / det;
                let sy = (-m[3] * px + m[0] * py) / det;
                let a = sample(
                    &self.mask,
                    sx / self.size[0] as f32,
                    sy / self.size[1] as f32,
                );
                let b = other.mask.bytes[y * other.mask.width + x] >= 128;
                both += usize::from(a && b);
                total += usize::from(b);
                expected += usize::from(a);
            }
        }
        #[cfg(test)]
        if std::env::var_os("VIDEO_KADR_INSTANCE_TRACE").is_some() {
            eprintln!("instance returned support sourceArea={} currentArea={total} intersection={both} expected={expected}",self.mask.bytes.iter().filter(|v|**v>=128).count());
        }
        let precision = both as f32 / total.max(1) as f32;
        let iou = both as f32 / (total + expected - both).max(1) as f32;
        if precision < 0.75 || iou < 0.35 {
            return Ok(None);
        }
        Ok(Some(proof))
    }
}
fn affine(a: [&Point; 3], b: [&Point; 3]) -> Option<[f32; 6]> {
    let mut m = [[0f32; 5]; 3];
    for i in 0..3 {
        m[i] = [a[i].x, a[i].y, 1., b[i].x, b[i].y];
    }
    for col in 0..3 {
        let pivot = (col..3).max_by(|a, b| m[*a][col].abs().total_cmp(&m[*b][col].abs()))?;
        if m[pivot][col].abs() < 1e-4 {
            return None;
        }
        m.swap(col, pivot);
        let divisor = m[col][col];
        for x in col..5 {
            m[col][x] /= divisor;
        }
        for row in 0..3 {
            if row == col {
                continue;
            }
            let factor = m[row][col];
            for x in col..5 {
                m[row][x] -= factor * m[col][x];
            }
        }
    }
    Some([m[0][3], m[1][3], m[2][3], m[0][4], m[1][4], m[2][4]])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene(dx: i32, exposure: f32, scramble: bool) -> (Image, Mask) {
        let mut rgba = vec![25; 128 * 96 * 4];
        let mut mask = vec![0; 65536];
        for y in 20..80 {
            for x in 30..90 {
                let xx = x + dx;
                if !(0..128).contains(&xx) {
                    continue;
                }
                for c in 0..3 {
                    let salt = if scramble { 7919 } else { 0 };
                    let v = ((x * 107 + y * 67 + x * y * 11 + c * 83 + salt) % 191 + 40) as f32;
                    rgba[(y as usize * 128 + xx as usize) * 4 + c as usize] =
                        (v * exposure).round().min(255.) as u8;
                }
            }
        }
        for y in 0..256 {
            for x in 0..256 {
                let xx = (x as f32 + 0.5) / 256. * 128.;
                let yy = (y as f32 + 0.5) / 256. * 96.;
                if xx >= (30 + dx) as f32 && xx < (90 + dx) as f32 && yy >= 20. && yy < 80. {
                    mask[y * 256 + x] = 255;
                }
            }
        }
        for p in rgba.chunks_exact_mut(4) {
            p[3] = 255;
        }
        (
            Image::new(128, 96, rgba).unwrap(),
            Mask {
                width: 256,
                height: 256,
                bytes: mask,
            },
        )
    }
    #[test]
    fn local_texture_tracks_gain_and_translation_but_rejects_same_palette_other_instance() {
        let c = AtomicBool::new(false);
        let (a, m) = scene(0, 1., false);
        let id = Instance::new(&a, &m, &c).unwrap();
        let (b, n) = scene(17, 0.6, false);
        let yes = Instance::new(&b, &n, &c).unwrap();
        assert!(id.matches(&yes, &c).unwrap());
        let (b, n) = scene(17, 1., true);
        let no = Instance::new(&b, &n, &c).unwrap();
        assert!(!id.matches(&no, &c).unwrap());
    }
    #[test]
    fn matching_points_cannot_endorse_returned_background_scope() {
        let c = AtomicBool::new(false);
        let (a, m) = scene(0, 1., false);
        let id = Instance::new(&a, &m, &c).unwrap();
        let mut expanded = m.clone();
        expanded.bytes.fill(255);
        let wrong = Instance::new(&a, &expanded, &c).unwrap();
        assert!(!id.matches(&wrong, &c).unwrap());
        let mut wrong = wrong;
        assert!(!id.retain_original_witness(&mut wrong, &c).unwrap());
    }
    #[test]
    fn neutral_uniform_surfaces_cannot_bypass_instance_evidence() {
        let c = AtomicBool::new(false);
        let gray = Image::new(64, 64, vec![100; 64 * 64 * 4]).unwrap();
        let mask = Mask {
            width: 256,
            height: 256,
            bytes: vec![255; 65536],
        };
        let id = Instance::new(&gray, &mask, &c).unwrap();
        assert!(id.uniform.is_none());
        assert!(!id.matches(&id, &c).unwrap());
    }
}
