//! Measured, bidirectionally checked local photometric flow for visible identity.
//! It can disambiguate a known visible instance; it never performs recovery.
use crate::{object_matting::Mask, sam_tracking, Image};
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};
const R: i32 = 3;
const N: usize = 147;
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "SAM continuity cancelled");
    Ok(())
}
fn patch(image: &Image, x: i32, y: i32) -> Option<[f32; N]> {
    if y < R || y + R >= image.height as i32 {
        return None;
    }
    let mut values = [0f32; N];
    let mut mean = [0f32; 3];
    let mut index = 0;
    for dy in -R..=R {
        for dx in -R..=R {
            let xx = (x + dx).rem_euclid(image.width as i32) as usize;
            let yy = (y + dy) as usize;
            for c in 0..3 {
                let v = f32::from(image.rgba[(yy * image.width + xx) * 4 + c]);
                values[index] = v;
                mean[c] += v;
                index += 1;
            }
        }
    }
    for m in &mut mean {
        *m /= 49.;
    }
    let mut square = 0f32;
    for (i, v) in values.iter_mut().enumerate() {
        *v -= mean[i % 3];
        square += *v * *v;
    }
    if square / (N as f32) < 16. {
        return None;
    }
    let norm = square.sqrt();
    for v in &mut values {
        *v /= norm;
    }
    Some(values)
}
fn circular_delta(a: f32, b: f32, width: usize) -> f32 {
    (a - b + width as f32 / 2.).rem_euclid(width as f32) - width as f32 / 2.
}
fn locate(
    image: &Image,
    feature: &[f32; N],
    centre: [i32; 2],
    radius: i32,
    cancel: &AtomicBool,
) -> Result<Option<[i32; 2]>> {
    let mut ranked = vec![];
    for dy in -radius..=radius {
        check(cancel)?;
        for dx in -radius..=radius {
            let p = [
                (centre[0] + dx).rem_euclid(image.width as i32),
                centre[1] + dy,
            ];
            if let Some(candidate) = patch(image, p[0], p[1]) {
                let score = feature
                    .iter()
                    .zip(candidate)
                    .map(|(a, b)| a * b)
                    .sum::<f32>();
                ranked.push((score, p));
            }
        }
    }
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let Some(&(best, p)) = ranked.first() else {
        return Ok(None);
    };
    if best < 0.8 {
        return Ok(None);
    }
    let second = ranked
        .iter()
        .find(|(_, q)| {
            circular_delta(p[0] as f32, q[0] as f32, image.width).hypot((p[1] - q[1]) as f32) >= 6.
        })
        .map(|v| v.0)
        .unwrap_or(-1.);
    if 1. - second < 1e-4 || (1. - best).max(0.) / (1. - second) > 0.8 {
        return Ok(None);
    }
    Ok(Some(p))
}
fn feature_points(image: &Image, mask: &Mask, cancel: &AtomicBool) -> Result<Vec<[i32; 2]>> {
    let (mut left, mut right, mut top, mut bottom) = (mask.width, 0, mask.height, 0);
    for (i, v) in mask.bytes.iter().enumerate() {
        if *v >= 230 {
            left = left.min(i % mask.width);
            right = right.max(i % mask.width);
            top = top.min(i / mask.width);
            bottom = bottom.max(i / mask.width);
        }
    }
    if left == mask.width {
        return Ok(vec![]);
    }
    let mut cells: [Option<(f32, [i32; 2])>; 25] = [None; 25];
    let belongs = |x: i32, y: i32| {
        crate::mask_apply::sample(
            mask,
            (x as f32 + 0.5) / image.width as f32,
            (y as f32 + 0.5) / image.height as f32,
        ) >= 0.5
    };
    for y in (R..image.height as i32 - R).step_by(2) {
        check(cancel)?;
        for x in (0..image.width as i32).step_by(2) {
            if crate::mask_apply::sample(
                mask,
                (x as f32 + 0.5) / image.width as f32,
                (y as f32 + 0.5) / image.height as f32,
            ) < 0.9
                || ![-R, R].iter().all(|dy| {
                    [-R, R]
                        .iter()
                        .all(|dx| belongs((x + dx).rem_euclid(image.width as i32), y + dy))
                })
            {
                continue;
            }
            let pixel = |xx: i32, yy: i32, c: usize| {
                f32::from(
                    image.rgba[(yy as usize * image.width
                        + xx.rem_euclid(image.width as i32) as usize)
                        * 4
                        + c],
                )
            };
            let energy = (0..3)
                .map(|c| {
                    (pixel(x + 2, y, c) - pixel(x - 2, y, c)).abs()
                        + (pixel(x, y + 2, c) - pixel(x, y - 2, c)).abs()
                })
                .sum::<f32>();
            let mx = (x as usize * mask.width / image.width).clamp(left, right);
            let my = (y as usize * mask.height / image.height).clamp(top, bottom);
            let column = ((mx - left) * 5 / (right - left + 1)).min(4);
            let row = ((my - top) * 5 / (bottom - top + 1)).min(4);
            let cell = &mut cells[row * 5 + column];
            if cell.is_none_or(|(old, _)| energy > old) {
                *cell = Some((energy, [x, y]));
            }
        }
    }
    Ok(cells
        .into_iter()
        .flatten()
        .filter(|(energy, _)| *energy > 0.)
        .map(|(_, p)| p)
        .collect())
}
pub struct Continuity {
    previous: Mask,
    pub matches: Vec<[f32; 2]>,
    shift: [f32; 2],
    witness_span: [f32; 2],
}
enum MotionEvidence {
    Missing,
    Competing,
    Measured(Continuity),
}
impl Continuity {
    pub fn measure(
        previous: &Image,
        current: &Image,
        mask: &Mask,
        cancel: &AtomicBool,
    ) -> Result<Option<Self>> {
        let mut best = match Self::measure_single(previous, current, mask, [1., 1.], cancel)? {
            MotionEvidence::Missing => None,
            MotionEvidence::Competing => return Ok(None),
            MotionEvidence::Measured(flow) => Some(flow),
        };
        if best.as_ref().is_some_and(|flow| flow.matches.len() >= 6) {
            return Ok(best);
        }
        // Fine texture can disappear under resampling and modest viewpoint
        // changes. Seek independent coarse RGB evidence, retaining the same
        // NCC/uniqueness thresholds and full-resolution cycle error budget.
        for divisor in [2usize, 4] {
            check(cancel)?;
            let size = [current.width / divisor, current.height / divisor];
            if size.iter().any(|n| *n < 32) {
                continue;
            }
            let reduce = |image: &Image| -> Result<Image> {
                let rgba = image::RgbaImage::from_raw(
                    image.width as u32,
                    image.height as u32,
                    image.rgba.clone(),
                )
                .ok_or_else(|| anyhow::anyhow!("invalid continuity RGB"))?;
                let small = image::imageops::resize(
                    &rgba,
                    size[0] as u32,
                    size[1] as u32,
                    image::imageops::FilterType::Triangle,
                );
                Image::new(size[0], size[1], small.into_raw())
            };
            let evidence = Self::measure_single(
                &reduce(previous)?,
                &reduce(current)?,
                mask,
                [
                    current.width as f32 / size[0] as f32,
                    current.height as f32 / size[1] as f32,
                ],
                cancel,
            )?;
            let flow = match evidence {
                MotionEvidence::Missing => continue,
                MotionEvidence::Competing => return Ok(None),
                MotionEvidence::Measured(flow) => flow,
            };
            // Do not replace stronger fine evidence with a denser cluster
            // on one small part. A replacement must retain the number of
            // measured matches and both source-coordinate spans.
            if best.as_ref().is_none_or(|old| {
                flow.matches.len() >= old.matches.len()
                    && (0..2).all(|axis| flow.witness_span[axis] >= old.witness_span[axis])
            }) {
                best = Some(flow);
            }
        }
        Ok(best)
    }
    fn measure_single(
        previous: &Image,
        current: &Image,
        mask: &Mask,
        scale: [f32; 2],
        cancel: &AtomicBool,
    ) -> Result<MotionEvidence> {
        check(cancel)?;
        ensure!(
            previous.width == current.width && previous.height == current.height,
            "continuity frame shape changed"
        );
        let native_side = (current.width as f32 * scale[0]).min(current.height as f32 * scale[1]);
        let native_radius = (native_side / 4.).clamp(8., 64.);
        let radius = (native_radius / scale[0].max(scale[1])).floor().max(1.) as i32;
        let mut pairs = vec![];
        ensure!(
            mask.width > 0
                && mask.height > 0
                && mask
                    .width
                    .checked_mul(mask.height)
                    .is_some_and(|n| n == mask.bytes.len() && n <= 65536),
            "invalid continuity mask"
        );
        ensure!(
            previous.width > 0
                && previous.height > 0
                && previous.rgba.len() == previous.width * previous.height * 4
                && current.rgba.len() == current.width * current.height * 4,
            "invalid continuity image"
        );
        for old in feature_points(previous, mask, cancel)? {
            let Some(feature) = patch(previous, old[0], old[1]) else {
                continue;
            };
            let Some(found) = locate(current, &feature, old, radius, cancel)? else {
                continue;
            };
            let Some(back_feature) = patch(current, found[0], found[1]) else {
                continue;
            };
            let Some(back) = locate(previous, &back_feature, found, radius, cancel)? else {
                continue;
            };
            if (circular_delta(back[0] as f32, old[0] as f32, current.width) * scale[0])
                .hypot((back[1] - old[1]) as f32 * scale[1])
                > 3.
            {
                continue;
            }
            pairs.push((
                old,
                found,
                [
                    circular_delta(found[0] as f32, old[0] as f32, current.width),
                    (found[1] - old[1]) as f32,
                ],
            ));
        }
        if pairs.len() < 3 {
            return Ok(MotionEvidence::Missing);
        }
        // A majority is not identity. Two independently cycle-checked motion
        // clusters can represent indistinguishable subjects. Do not let a
        // coarser pyramid level silently choose between them.
        let distance =
            |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]) * scale[0]).hypot((a[1] - b[1]) * scale[1]);
        let supported: Vec<_> = pairs
            .iter()
            .filter(|a| pairs.iter().filter(|b| distance(a.2, b.2) <= 6.).count() >= 3)
            .collect();
        if supported
            .iter()
            .any(|a| supported.iter().any(|b| distance(a.2, b.2) > 12.))
        {
            return Ok(MotionEvidence::Competing);
        }
        let median = |axis: usize| {
            let mut v: Vec<_> = pairs.iter().map(|p| p.2[axis]).collect();
            v.sort_by(f32::total_cmp);
            v[v.len() / 2]
        };
        let shift = [median(0), median(1)];
        pairs.retain(|p| {
            ((p.2[0] - shift[0]) * scale[0]).hypot((p.2[1] - shift[1]) * scale[1]) <= 6.
        });
        if pairs.len() < 3 {
            return Ok(MotionEvidence::Missing);
        }
        let mut xs: Vec<_> = pairs
            .iter()
            .map(|p| (p.0[0] as f32 + 0.5) / previous.width as f32)
            .collect();
        xs.sort_by(f32::total_cmp);
        let largest_gap = xs
            .windows(2)
            .map(|p| p[1] - p[0])
            .fold(xs[0] + 1. - xs[xs.len() - 1], f32::max);
        let min_y = pairs.iter().map(|p| p.0[1]).min().unwrap();
        let max_y = pairs.iter().map(|p| p.0[1]).max().unwrap();
        let witness_span = [
            1. - largest_gap,
            (max_y - min_y) as f32 / previous.height as f32,
        ];
        let matches = pairs
            .iter()
            .map(|p| {
                [
                    (p.1[0] as f32 + 0.5) / current.width as f32,
                    (p.1[1] as f32 + 0.5) / current.height as f32,
                ]
            })
            .collect();
        Ok(MotionEvidence::Measured(Self {
            previous: mask.clone(),
            matches,
            shift: [
                shift[0] / current.width as f32,
                shift[1] / current.height as f32,
            ],
            witness_span,
        }))
    }
    pub fn support(&self, mask: &Mask) -> f32 {
        self.matches
            .iter()
            .filter(|p| crate::mask_apply::sample(mask, p[0], p[1]) >= 0.5)
            .count() as f32
            / self.matches.len() as f32
    }
    pub fn overlap(&self, mask: &Mask, cancel: &AtomicBool) -> Result<f32> {
        let mut shifted = self.previous.clone();
        for y in 0..mask.height {
            check(cancel)?;
            for x in 0..mask.width {
                let u = ((x as f32 + 0.5) / mask.width as f32 - self.shift[0]).rem_euclid(1.);
                let v = (y as f32 + 0.5) / mask.height as f32 - self.shift[1];
                shifted.bytes[y * mask.width + x] = if (0. ..1.).contains(&v) {
                    (crate::mask_apply::sample(&self.previous, u, v) * 255.).round() as u8
                } else {
                    0
                };
            }
        }
        sam_tracking::overlap(&shifted, mask, cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene(dx: i32, dy: i32, exposure: f32, twins: bool) -> (Image, Mask) {
        let mut rgba = vec![30; 64 * 64 * 4];
        let mut bytes = vec![0; 256 * 256];
        for y in 12..28 {
            for x in 16..32 {
                let xx = x + dx;
                let yy = y + dy;
                if !(0..64).contains(&xx) || !(0..64).contains(&yy) {
                    continue;
                }
                let value = ((x * 31 + y * 71 + x * y * 3) % 181 + 40) as f32;
                for c in 0..3 {
                    rgba[(yy as usize * 64 + xx as usize) * 4 + c] =
                        (value * exposure).round() as u8;
                }
                for sy in 0..4 {
                    for sx in 0..4 {
                        bytes[(yy as usize * 4 + sy) * 256 + xx as usize * 4 + sx] = 255;
                    }
                }
                if twins {
                    let tx = x - 4;
                    for c in 0..3 {
                        rgba[(y as usize * 64 + tx as usize) * 4 + c] =
                            (value * exposure).round() as u8;
                    }
                }
            }
        }
        for pixel in rgba.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        (
            Image::new(64, 64, rgba).unwrap(),
            Mask {
                width: 256,
                height: 256,
                bytes,
            },
        )
    }
    #[test]
    fn measured_flow_supports_translation_exposure_and_reverse_direction() {
        let cancel = AtomicBool::new(false);
        let (a, mask) = scene(0, 0, 1., false);
        let (b, moved) = scene(7, 5, 0.6, false);
        let flow = Continuity::measure(&a, &b, &mask, &cancel)
            .unwrap()
            .unwrap();
        assert!(flow.matches.len() >= 3);
        assert!(flow.support(&moved) >= 0.75);
        assert!(flow.overlap(&moved, &cancel).unwrap() > 0.9);
        let reverse = Continuity::measure(&b, &a, &moved, &cancel)
            .unwrap()
            .unwrap();
        assert!(reverse.overlap(&mask, &cancel).unwrap() > 0.9);
    }
    #[test]
    fn flat_occluded_and_ambiguous_flow_never_prove_visible_identity() {
        let cancel = AtomicBool::new(false);
        let (a, mask) = scene(0, 0, 1., false);
        let blank = Image::new(64, 64, vec![30; 64 * 64 * 4]).unwrap();
        assert!(Continuity::measure(&a, &blank, &mask, &cancel)
            .unwrap()
            .is_none());
        let (twins, _) = scene(16, 0, 1., true);
        assert!(Continuity::measure(&a, &twins, &mask, &cancel)
            .unwrap()
            .is_none());
        cancel.store(true, Ordering::Relaxed);
        assert!(Continuity::measure(&a, &a, &mask, &cancel).is_err());
    }
    #[test]
    fn coarse_visible_texture_survives_independent_sensor_noise() {
        let c = AtomicBool::new(false);
        let noisy = |im: Image, salt: u32| {
            let rgb =
                image::RgbaImage::from_raw(im.width as u32, im.height as u32, im.rgba).unwrap();
            let mut large =
                image::imageops::resize(&rgb, 256, 256, image::imageops::FilterType::Nearest)
                    .into_raw();
            for (i, p) in large.chunks_exact_mut(4).enumerate() {
                let mut h = (i as u32).wrapping_mul(374761393) ^ salt;
                h = (h ^ (h >> 13)).wrapping_mul(1274126177);
                let noise = ((h ^ (h >> 16)) % 81) as i32 - 40;
                for channel in &mut p[..3] {
                    *channel = (i32::from(*channel) + noise).clamp(0, 255) as u8;
                }
            }
            Image::new(256, 256, large).unwrap()
        };
        let (a, mask) = scene(0, 0, 1., false);
        let (b, target) = scene(7, 5, 0.8, false);
        let source = noisy(a, 9187);
        let flow = Continuity::measure(&source, &noisy(b, 16381), &mask, &c)
            .unwrap()
            .unwrap();
        assert!(flow.support(&target) >= 0.75);
        assert!(flow.overlap(&target, &c).unwrap() >= 0.9);
        let (twins, _) = scene(16, 0, 0.8, true);
        assert!(
            Continuity::measure(&source, &noisy(twins, 16381), &mask, &c)
                .unwrap()
                .is_none(),
            "independent sensor noise must not resolve two identical subjects"
        );
    }
    #[test]
    #[ignore = "requires explicit private calibrated RGB/mask continuity fixture"]
    fn actual_calibrated_continuity() {
        let folder =
            std::path::PathBuf::from(std::env::var("VIDEO_KADR_CONTINUITY_FIXTURE").unwrap());
        let load = |name: &str| {
            let im = image::open(folder.join(name)).unwrap().to_rgba8();
            Image::new(im.width() as usize, im.height() as usize, im.into_raw()).unwrap()
        };
        let mask = |name: &str| {
            let im = image::open(folder.join(name)).unwrap().to_luma8();
            Mask {
                width: im.width() as usize,
                height: im.height() as usize,
                bytes: im.into_raw(),
            }
        };
        let c = AtomicBool::new(false);
        let flow = Continuity::measure(
            &load("previous.png"),
            &load("current.png"),
            &mask("previous-mask.png"),
            &c,
        )
        .unwrap()
        .expect("measured visible source continuity");
        let reference = mask("current-mask.png");
        println!(
            "actual continuity matches={} support={} warpedIoU={}",
            flow.matches.len(),
            flow.support(&reference),
            flow.overlap(&reference, &c).unwrap()
        );
        assert!(flow.support(&reference) >= 0.75);
        assert!(flow.overlap(&reference, &c).unwrap() >= 0.5);
    }
}
