//! Immutable source-owned neural appearance identity over SlimSAM encoder features.
//! SAM is not a dedicated instance-ReID network: ambiguous identities are refused.
use crate::{object_matting::Mask, slim_sam::Embedding, Image};
use anyhow::{ensure, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
const CHANNELS: usize = 256;
const GRID: usize = 64;
#[derive(Clone)]
pub struct Identity {
    feature: Vec<f32>,
    color: [f32; 48],
    area: f32,
    extent: f32,
    views: Vec<Identity>,
    aspect: f32,
    instance: Option<crate::sam_instance::Instance>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Seed,
    Tracked,
    Reacquired,
    Occluded,
    Ambiguous,
}
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "SAM reidentification cancelled"
    );
    Ok(())
}
fn unit(v: &mut [f32]) -> bool {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm < 1e-6 {
        return false;
    }
    for x in v {
        *x /= norm;
    }
    true
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x * y)
        .sum::<f32>()
        .clamp(-1., 1.)
}
fn shape(embedding: &Embedding, image: &Image, mask: Option<&Mask>) -> Result<()> {
    ensure!(
        embedding.image.len() == CHANNELS * GRID * GRID
            && embedding.image.iter().all(|x| x.is_finite())
            && embedding.scaled.iter().all(|v| *v > 0 && *v <= 1024),
        "invalid neural identity embedding"
    );
    ensure!(
        image.width > 0
            && image.height > 0
            && image
                .width
                .checked_mul(image.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024 && image.rgba.len() == n * 4),
        "invalid identity image"
    );
    if let Some(mask) = mask {
        ensure!(
            mask.width > 0
                && mask.height > 0
                && mask
                    .width
                    .checked_mul(mask.height)
                    .is_some_and(|n| n <= 65536 && mask.bytes.len() == n),
            "invalid identity mask"
        );
    }
    Ok(())
}
fn weight(mask: &Mask, u: f32, v: f32) -> f32 {
    let x = (u * mask.width as f32)
        .floor()
        .clamp(0., (mask.width - 1) as f32) as usize;
    let y = (v * mask.height as f32)
        .floor()
        .clamp(0., (mask.height - 1) as f32) as usize;
    f32::from(mask.bytes[y * mask.width + x].saturating_sub(127)) / 128.
}
fn global(embedding: &Embedding) -> Vec<f32> {
    let mut out = vec![0.; CHANNELS];
    let mut count = 0.0f32;
    for y in 0..GRID {
        for x in 0..GRID {
            if (x as f32 + 0.5) * 16. >= embedding.scaled[0] as f32
                || (y as f32 + 0.5) * 16. >= embedding.scaled[1] as f32
            {
                continue;
            }
            count += 1.;
            for c in 0..CHANNELS {
                out[c] += embedding.image[c * GRID * GRID + y * GRID + x];
            }
        }
    }
    for v in &mut out {
        *v /= count.max(1.);
    }
    out
}
fn support_aspect(mask: &Mask) -> f32 {
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
        return 1.;
    }
    let (mut gap, mut largest) = (0usize, 0usize);
    for i in 0..2 * mask.width {
        if columns[i % mask.width] {
            gap = 0;
        } else {
            gap = (gap + 1).min(mask.width);
            largest = largest.max(gap);
        }
    }
    let width = (mask.width - largest).max(1) as f32 / mask.width as f32;
    let height = (bottom - top + 1) as f32 / mask.height as f32;
    width / height
}
fn descriptor(
    embedding: &Embedding,
    image: &Image,
    mask: &Mask,
    cancel: &AtomicBool,
) -> Result<Option<Identity>> {
    shape(embedding, image, Some(mask))?;
    let mean = global(embedding);
    let mut feature = vec![0.; CHANNELS];
    let mut mass = 0.;
    for y in 0..GRID {
        check(cancel)?;
        for x in 0..GRID {
            let u = (x as f32 + 0.5) * 16. / embedding.scaled[0] as f32;
            let v = (y as f32 + 0.5) * 16. / embedding.scaled[1] as f32;
            if u >= 1. || v >= 1. {
                continue;
            }
            let w = weight(mask, u, v);
            mass += w;
            for c in 0..CHANNELS {
                feature[c] += (embedding.image[c * GRID * GRID + y * GRID + x] - mean[c]) * w;
            }
        }
    }
    if mass < 2. || !unit(&mut feature) {
        return Ok(None);
    }
    let mut color = [0.; 48];
    let mut area = 0.;
    for y in 0..image.height {
        check(cancel)?;
        for x in 0..image.width {
            let w = weight(
                mask,
                (x as f32 + 0.5) / image.width as f32,
                (y as f32 + 0.5) / image.height as f32,
            );
            area += w;
            for c in 0..3 {
                // Chromaticity retains hue under exposure changes. Linear binning
                // avoids a hard histogram discontinuity from quantisation noise.
                let pixel = &image.rgba[(y * image.width + x) * 4..][..3];
                let sum = pixel.iter().map(|v| f32::from(*v)).sum::<f32>().max(24.);
                let position = f32::from(pixel[c]) / sum * 15.;
                let bin = position.floor().min(15.) as usize;
                let fraction = position - bin as f32;
                color[c * 16 + bin] += w * (1. - fraction);
                color[c * 16 + (bin + 1).min(15)] += w * fraction;
            }
        }
    }
    if area < 4. || !unit(&mut color) {
        return Ok(None);
    }
    Ok(Some(Identity {
        feature,
        color,
        area: area / (image.width * image.height) as f32,
        extent: mask.bytes.iter().filter(|v| **v >= 128).count() as f32 / mask.bytes.len() as f32,
        views: vec![],
        aspect: support_aspect(mask),
        instance: None,
    }))
}
impl Identity {
    pub fn from_seed(
        embedding: &Embedding,
        image: &Image,
        seed: &Mask,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        let mut identity = descriptor(embedding, image, seed, cancel)?.ok_or_else(|| {
            anyhow::anyhow!("selected target has insufficient distinct neural appearance")
        })?;
        identity.instance = Some(crate::sam_instance::Instance::new(image, seed, cancel)?);
        Ok(identity)
    }
    /// Global source-coordinate proposals; positional embeddings are not used as identity.
    pub fn proposals(
        &self,
        embedding: &Embedding,
        image: &Image,
        cancel: &AtomicBool,
    ) -> Result<Vec<[f64; 2]>> {
        shape(embedding, image, None)?;
        let mean = global(embedding);
        let mut candidates = vec![];
        for y in 0..GRID {
            check(cancel)?;
            for x in 0..GRID {
                let u = (x as f32 + 0.5) * 16. / embedding.scaled[0] as f32;
                let v = (y as f32 + 0.5) * 16. / embedding.scaled[1] as f32;
                if u >= 1. || v >= 1. {
                    continue;
                }
                let mut f: Vec<_> = (0..CHANNELS)
                    .map(|c| embedding.image[c * GRID * GRID + y * GRID + x] - mean[c])
                    .collect();
                if unit(&mut f) {
                    let score = std::iter::once(self)
                        .chain(self.views.iter())
                        .map(|view| cosine(&view.feature, &f))
                        .fold(-1f32, f32::max);
                    candidates.push((score, [u as f64, v as f64]));
                }
            }
        }
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut out: Vec<[f64; 2]> = vec![];
        for (score, p) in candidates {
            if score < 0.35 {
                break;
            }
            if out.iter().all(|q| {
                ((p[0] - q[0]) * image.width as f64).hypot((p[1] - q[1]) * image.height as f64)
                    > (self.area as f64 * (image.width * image.height) as f64).sqrt() * 0.75
            }) {
                out.push(p);
                if out.len() == 4 {
                    break;
                }
            }
        }
        Ok(out)
    }
    pub fn similarity(
        &self,
        embedding: &Embedding,
        image: &Image,
        mask: &Mask,
        cancel: &AtomicBool,
    ) -> Result<Option<f32>> {
        let Some(other) = descriptor(embedding, image, mask, cancel)? else {
            return Ok(None);
        };
        #[cfg(test)]
        if std::env::var_os("VIDEO_KADR_INSTANCE_TRACE").is_some() {
            eprintln!("SAM source identity primary(neural,color,areaRatio,score)={:?} views={:?} returnedAspect={}",(cosine(&self.feature,&other.feature),cosine(&self.color,&other.color),other.area/self.area,self.compare(&other)),self.views.iter().map(|v|(cosine(&v.feature,&other.feature),cosine(&v.color,&other.color),other.area/v.area,v.compare(&other))).collect::<Vec<_>>(),other.aspect);
        }
        Ok(std::iter::once(self)
            .chain(self.views.iter())
            .filter_map(|view| view.compare(&other))
            .max_by(f32::total_cmp))
    }
    fn compare(&self, other: &Identity) -> Option<f32> {
        let ratio = other.area / self.area;
        if !(0.12..=8.).contains(&ratio) {
            return None;
        }
        let neural = cosine(&self.feature, &other.feature);
        let color = cosine(&self.color, &other.color);
        if neural < 0.55 || color < 0.65 {
            return None;
        }
        Some(neural * 0.8 + color * 0.2 - 0.12 * ratio.ln().abs().min(2.))
    }
    pub(crate) fn accepts_instance(
        &self,
        image: &Image,
        mask: &Mask,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        let current = crate::sam_instance::Instance::new(image, mask, cancel)?;
        for view in std::iter::once(self).chain(self.views.iter()) {
            if let Some(known) = &view.instance {
                if known.matches(&current, cancel)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn scope_error(&self, mask: &Mask) -> f32 {
        let area =
            mask.bytes.iter().filter(|v| **v >= 128).count() as f32 / mask.bytes.len() as f32;
        let size = (area.max(1e-8) / self.extent.max(1e-8))
            .ln()
            .abs()
            .min(2f32.ln());
        size + 0.5
            * (support_aspect(mask).max(1e-8) / self.aspect.max(1e-8))
                .ln()
                .abs()
    }
    /// Caller must establish source/hash provenance and unique contiguous identity
    /// agreement before enrollment. Recovered targets never call this method.
    pub fn enroll_admitted_view(
        &mut self,
        embedding: &Embedding,
        image: &Image,
        mask: &Mask,
        cancel: &AtomicBool,
    ) -> Result<Option<usize>> {
        check(cancel)?;
        let mut view = Self::from_seed(embedding, image, mask, cancel)?;
        match (&self.instance, &mut view.instance) {
            (Some(original), Some(current)) => {
                if !original.retain_original_witness(current, cancel)? {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        }
        if std::iter::once(&*self).chain(self.views.iter()).any(|old| {
            cosine(&old.feature, &view.feature) > 0.985 && cosine(&old.color, &view.color) > 0.985
        }) {
            return Ok(None);
        }
        if self.views.len() < 3 {
            self.views.push(view);
            return Ok(Some(self.views.len() - 1));
        }
        // Farthest-point coverage keeps a later profile/back view instead of
        // exhausting the budget on the first three almost-frontal samples.
        // This runs only in the provenance-checked contiguous preparation pass.
        let mut pool = self.views.clone();
        pool.push(view);
        let distance = |a: &Identity, b: &Identity| {
            1. - (0.8 * cosine(&a.feature, &b.feature) + 0.2 * cosine(&a.color, &b.color))
        };
        let mut chosen = vec![];
        for _ in 0..3 {
            let mut best = None;
            let mut best_distance = -1f32;
            for (i, candidate) in pool.iter().enumerate() {
                if chosen.contains(&i) {
                    continue;
                }
                let mut nearest = distance(self, candidate);
                for &j in &chosen {
                    nearest = nearest.min(distance(&pool[j], candidate));
                }
                if nearest > best_distance + 1e-6 {
                    best = Some(i);
                    best_distance = nearest;
                }
            }
            chosen.push(best.unwrap());
        }
        if !chosen.contains(&3) {
            return Ok(None);
        }
        let slot = (0..3).find(|i| !chosen.contains(i)).unwrap();
        self.views[slot] = pool.pop().unwrap();
        Ok(Some(slot))
    }
}
/// Distinct candidate instances must have a clear identity margin. Duplicate SAM
/// hypotheses for the same object are deduplicated by mask IoU, not confidence.
fn several_instances(mask: &Mask) -> bool {
    let total = mask.bytes.iter().filter(|v| **v >= 128).count();
    if total < 8 {
        return false;
    }
    let mut seen = vec![false; mask.bytes.len()];
    let mut substantial = 0;
    for start in 0..mask.bytes.len() {
        if seen[start] || mask.bytes[start] < 128 {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let mut size = 0;
        while let Some(i) = stack.pop() {
            size += 1;
            let x = i % mask.width;
            let y = i / mask.width;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let yy = y as isize + dy;
                    if yy < 0 || yy >= mask.height as isize {
                        continue;
                    }
                    let xx = (x as isize + dx).rem_euclid(mask.width as isize) as usize;
                    let j = yy as usize * mask.width + xx;
                    if !seen[j] && mask.bytes[j] >= 128 {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if size * 5 >= total {
            substantial += 1;
            if substantial >= 2 {
                return true;
            }
        }
    }
    false
}
pub fn select(
    identity: &Identity,
    embedding: &Embedding,
    image: &Image,
    candidates: Vec<Mask>,
    cancel: &AtomicBool,
) -> Result<(State, Option<Mask>)> {
    let mut scored = vec![];
    for mask in candidates {
        if let Some(score) = identity.similarity(embedding, image, &mask, cancel)? {
            scored.push((score, mask));
        }
    }
    select_scored(identity, None, scored, cancel)
}
fn containment(a: &Mask, b: &Mask) -> f32 {
    let (mut aa, mut bb, mut both) = (0usize, 0usize, 0usize);
    for (&a, &b) in a.bytes.iter().zip(&b.bytes) {
        aa += usize::from(a >= 128);
        bb += usize::from(b >= 128);
        both += usize::from(a >= 128 && b >= 128);
    }
    both as f32 / aa.min(bb).max(1) as f32
}
pub(crate) fn select_scored(
    identity: &Identity,
    previous: Option<&Mask>,
    candidates: Vec<(f32, Mask)>,
    cancel: &AtomicBool,
) -> Result<(State, Option<Mask>)> {
    select_scored_inner(identity, previous, None, candidates, cancel)
}
pub(crate) fn select_scored_visible(
    identity: &Identity,
    previous: Option<&Mask>,
    continuity: Option<&crate::sam_continuity::Continuity>,
    candidates: Vec<(f32, Mask)>,
    cancel: &AtomicBool,
) -> Result<(State, Option<Mask>)> {
    select_scored_inner(identity, previous, continuity, candidates, cancel)
}
fn select_scored_inner(
    identity: &Identity,
    previous: Option<&Mask>,
    continuity: Option<&crate::sam_continuity::Continuity>,
    candidates: Vec<(f32, Mask)>,
    cancel: &AtomicBool,
) -> Result<(State, Option<Mask>)> {
    #[cfg(test)]
    if let Ok(folder) = std::env::var("VIDEO_KADR_REID_DIAGNOSTIC") {
        static CALL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let call = CALL.fetch_add(1, Ordering::Relaxed);
        std::fs::create_dir_all(&folder)?;
        let mut details = vec![];
        for (i, (score, mask)) in candidates.iter().enumerate() {
            image::save_buffer(
                std::path::Path::new(&folder).join(format!("call-{call:02}-candidate-{i:02}.png")),
                &mask.bytes,
                256,
                256,
                image::ColorType::L8,
            )?;
            details.push(serde_json::json!({"index":i,"score":score,"severalInstances":several_instances(mask),"area":mask.bytes.iter().filter(|v|**v>=128).count()}));
        }
        std::fs::write(
            std::path::Path::new(&folder).join(format!("call-{call:02}.json")),
            serde_json::to_vec_pretty(&details)?,
        )?;
    }
    let mut ranked = vec![];
    let mut ambiguous_score = None::<f32>;
    for (score, mask) in candidates {
        check(cancel)?;
        ensure!(
            score.is_finite()
                && mask.width == 256
                && mask.height == 256
                && mask.bytes.len() == 65536,
            "invalid neural identity candidate"
        );
        if score >= 0.72 {
            if several_instances(&mask) {
                ambiguous_score = Some(ambiguous_score.unwrap_or(0.).max(score));
            } else {
                ranked.push((score, mask));
            }
        }
    }
    // Full and partial hypotheses are grouped around a source-consistent
    // semantic support, rather than an oversized background-connected mask.
    let extent_error = |mask: &Mask| identity.scope_error(mask);
    ranked.sort_by(|a, b| {
        extent_error(&a.1)
            .total_cmp(&extent_error(&b.1))
            .then_with(|| {
                b.1.bytes
                    .iter()
                    .filter(|v| **v >= 128)
                    .count()
                    .cmp(&a.1.bytes.iter().filter(|v| **v >= 128).count())
            })
    });
    let mut distinct: Vec<(f32, Mask, Mask)> = vec![];
    for (score, mask) in ranked {
        let mut duplicate = false;
        if distinct
            .iter()
            .filter(|(_, cover, _)| containment(cover, &mask) >= 0.75)
            .count()
            > 1
        {
            // A connected SAM spill joining two separately established instances
            // cannot erase the competing identity or boost only its first group.
            ambiguous_score = Some(ambiguous_score.unwrap_or(0.).max(score));
            continue;
        }
        for (best, cover, selected) in &mut distinct {
            if containment(cover, &mask) >= 0.75 {
                // Learned score admits the instance; semantic support chooses
                // its requested extent, so a high-scoring shirt does not replace
                // a person, or a whole body replace an authored face.
                let temporal = |candidate: &Mask| -> Result<f32> {
                    Ok(if let Some(prior) = previous {
                        0.25 * (1. - crate::sam_tracking::overlap(prior, candidate, cancel)?)
                    } else {
                        0.
                    })
                };
                if extent_error(&mask) + temporal(&mask)?
                    < extent_error(selected) + temporal(selected)?
                {
                    *selected = mask.clone();
                    *best = score;
                }
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            distinct.push((score, mask.clone(), mask));
        }
    }
    distinct.sort_by(|a, b| b.0.total_cmp(&a.0));
    check(cancel)?;
    if distinct.is_empty() {
        return Ok((
            if ambiguous_score.is_some() {
                State::Ambiguous
            } else {
                State::Occluded
            },
            None,
        ));
    }
    if distinct.len() > 1 || ambiguous_score.is_some_and(|score| distinct[0].0 - score < 0.08) {
        if let Some(flow) = continuity {
            // Different SAM hypotheses can split the SAME visible owner into a
            // shirt, sleeve and head. Actual bidirectional source-RGB tracks
            // establish ownership; only measured SAM planes are combined.
            let mut owned = vec![];
            for (_, _, mask) in &distinct {
                let overlap = flow.overlap(mask, cancel)?;
                if flow.support(mask) * flow.matches.len() as f32 >= 1.99 && overlap >= 0.2 {
                    owned.push((overlap, mask.clone()));
                }
            }
            owned.sort_by(|a, b| b.0.total_cmp(&a.0));
            if let Some((mut overlap, mut owner)) = owned.first().cloned() {
                let mut _parts = 1;
                for (_, mask) in owned.into_iter().skip(1) {
                    let mut union = owner.clone();
                    for (a, b) in union.bytes.iter_mut().zip(mask.bytes) {
                        *a = (*a).max(b);
                    }
                    let agreement = flow.overlap(&union, cancel)?;
                    if flow.support(&union) > flow.support(&owner) && agreement > overlap {
                        owner = union;
                        overlap = agreement;
                        _parts += 1;
                    }
                }
                if flow.support(&owner) >= 0.75 && overlap >= 0.45 {
                    #[cfg(test)]
                    eprintln!(
                        "visible measured owner fragments={_parts} support={} warpedIoU={overlap}",
                        flow.support(&owner)
                    );
                    return Ok((State::Tracked, Some(owner)));
                }
            }
        }
        return Ok((State::Ambiguous, None));
    }
    Ok((State::Reacquired, Some(distinct.remove(0).2)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        target: Option<(usize, usize)>,
        twin: bool,
    ) -> (Embedding, Image, Mask, Option<Mask>) {
        let mut features = vec![0.; 256 * 64 * 64];
        let mut rgba = vec![0; 64 * 64 * 4];
        let mut bytes = vec![0; 256 * 256];
        let mut second = bytes.clone();
        for y in 0..64 {
            for x in 0..64 {
                let object =
                    target.is_some_and(|(tx, ty)| x >= tx && x < tx + 12 && y >= ty && y < ty + 12);
                let other = twin && x >= 44 && x < 56 && y >= 40 && y < 52;
                features[y * 64 + x] = if object || other { 2. } else { 0. };
                features[4096 + y * 64 + x] = if object || other { 0. } else { 1. };
                rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].copy_from_slice(if object || other {
                    &[220, 30, 40, 255]
                } else {
                    &[20, 100, 200, 255]
                });
                for yy in 0..4 {
                    for xx in 0..4 {
                        if object {
                            bytes[(y * 4 + yy) * 256 + x * 4 + xx] = 255;
                        }
                        if other {
                            second[(y * 4 + yy) * 256 + x * 4 + xx] = 255;
                        }
                    }
                }
            }
        }
        (
            Embedding {
                image: features,
                position: vec![],
                scaled: [1024, 1024],
            },
            Image::new(64, 64, rgba).unwrap(),
            Mask {
                width: 256,
                height: 256,
                bytes,
            },
            twin.then_some(Mask {
                width: 256,
                height: 256,
                bytes: second,
            }),
        )
    }
    #[test]
    fn immutable_neural_identity_reacquires_after_occlusion_at_different_position() {
        let cancel = AtomicBool::new(false);
        let (a, image, seed, _) = fixture(Some((6, 7)), false);
        let id = Identity::from_seed(&a, &image, &seed, &cancel).unwrap();
        let (blank, missing, _, _) = fixture(None, false);
        assert!(id.proposals(&blank, &missing, &cancel).unwrap().is_empty());
        assert!(select(&id, &blank, &missing, vec![], &cancel)
            .unwrap()
            .1
            .is_none());
        let (moved, newimage, target, _) = fixture(Some((38, 30)), false);
        let p = id.proposals(&moved, &newimage, &cancel).unwrap();
        assert!(p
            .iter()
            .any(|p| p[0] > 0.59 && p[0] < 0.79 && p[1] > 0.46 && p[1] < 0.66));
        let (state, accepted) =
            select(&id, &moved, &newimage, vec![target.clone()], &cancel).unwrap();
        assert_eq!(state, State::Reacquired);
        assert_eq!(accepted.unwrap(), target);
    }
    #[test]
    fn lookalike_identity_is_ambiguous_and_color_distractor_is_rejected() {
        let cancel = AtomicBool::new(false);
        let (a, image, seed, _) = fixture(Some((6, 7)), false);
        let id = Identity::from_seed(&a, &image, &seed, &cancel).unwrap();
        let (twins, twinimage, one, two) = fixture(Some((6, 7)), true);
        let (state, mask) =
            select(&id, &twins, &twinimage, vec![one, two.unwrap()], &cancel).unwrap();
        assert_eq!(state, State::Ambiguous);
        assert!(mask.is_none());
        let (embedding, mut changed, target, _) = fixture(Some((38, 30)), false);
        for y in 30..42 {
            for x in 38..50 {
                changed.rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3]
                    .copy_from_slice(&[20, 230, 10]);
            }
        }
        assert!(id
            .similarity(&embedding, &changed, &target, &cancel)
            .unwrap()
            .is_none());
    }
    #[test]
    fn illumination_change_preserves_identity_without_accepting_green_distractor() {
        let cancel = AtomicBool::new(false);
        let (embedding, image, seed, _) = fixture(Some((6, 7)), false);
        let id = Identity::from_seed(&embedding, &image, &seed, &cancel).unwrap();
        let (moved, mut darker, mask, _) = fixture(Some((38, 30)), false);
        for byte in darker.rgba.chunks_exact_mut(4) {
            for channel in &mut byte[..3] {
                *channel = (*channel as f32 * 0.5).round() as u8;
            }
        }
        assert!(id
            .similarity(&moved, &darker, &mask, &cancel)
            .unwrap()
            .is_some());
    }
    #[test]
    fn bounded_frozen_views_cover_pose_variation_and_do_not_replace_seed() {
        let cancel = AtomicBool::new(false);
        let make = |angle: f32| {
            let (mut e, image, mask, _) = fixture(Some((6, 7)), false);
            e.image.fill(0.);
            for y in 7..19 {
                for x in 6..18 {
                    e.image[y * 64 + x] = angle.to_radians().cos();
                    e.image[8192 + y * 64 + x] = angle.to_radians().sin();
                }
            }
            (e, image, mask)
        };
        let (e, image, mask) = make(0.);
        let mut bank = Identity::from_seed(&e, &image, &mask, &cancel).unwrap();
        let immutable = bank.feature.clone();
        for angle in [35., 70., 105.] {
            let (e, image, mask) = make(angle);
            assert!(
                bank.similarity(&e, &image, &mask, &cancel)
                    .unwrap()
                    .unwrap()
                    >= 0.72
            );
            assert!(bank
                .enroll_admitted_view(&e, &image, &mask, &cancel)
                .unwrap()
                .is_some());
        }
        let (e, image, mask) = make(140.);
        assert!(
            bank.similarity(&e, &image, &mask, &cancel)
                .unwrap()
                .unwrap()
                >= 0.72
        );
        assert!(bank
            .enroll_admitted_view(&e, &image, &mask, &cancel)
            .unwrap()
            .is_some());
        assert_eq!(bank.feature, immutable);
        assert_eq!(bank.views.len(), 3);
        bank.views.clear();
        assert!(bank
            .similarity(&e, &image, &mask, &cancel)
            .unwrap()
            .is_none());
    }
    #[test]
    fn close_same_instance_hypotheses_preserve_authored_extent() {
        let cancel = AtomicBool::new(false);
        let (e, image, full, _) = fixture(Some((6, 7)), false);
        let mut small = full.clone();
        for y in 0..256 {
            for x in 0..256 {
                if x >= 12 * 4 {
                    small.bytes[y * 256 + x] = 0;
                }
            }
        }
        let whole = Identity::from_seed(&e, &image, &full, &cancel).unwrap();
        let (_, selected) = select_scored(
            &whole,
            None,
            vec![(0.90, full.clone()), (0.91, small.clone())],
            &cancel,
        )
        .unwrap();
        assert_eq!(selected.unwrap(), full);
        let part = Identity::from_seed(&e, &image, &small, &cancel).unwrap();
        let (_, selected) = select_scored(
            &part,
            None,
            vec![(0.91, full), (0.90, small.clone())],
            &cancel,
        )
        .unwrap();
        assert_eq!(selected.unwrap(), small);
    }
    #[test]
    fn connected_background_spill_cannot_merge_two_known_instances() {
        let cancel = AtomicBool::new(false);
        let (e, image, one, two) = fixture(Some((6, 7)), true);
        let two = two.unwrap();
        let id = Identity::from_seed(&e, &image, &one, &cancel).unwrap();
        let mut bridge = one.clone();
        for (i, v) in two.bytes.iter().enumerate() {
            bridge.bytes[i] = bridge.bytes[i].max(*v);
        }
        for step in 0..=160 {
            let x = 48 + step * 152 / 160;
            let y = 52 + step * 132 / 160;
            for dy in -2isize..=2 {
                for dx in -2isize..=2 {
                    bridge.bytes[((y as isize + dy) as usize) * 256 + (x as isize + dx) as usize] =
                        255;
                }
            }
        }
        assert!(!several_instances(&bridge));
        let (state, mask) = select_scored(
            &id,
            None,
            vec![(0.99, bridge), (0.90, one), (0.90, two)],
            &cancel,
        )
        .unwrap();
        assert_eq!(state, State::Ambiguous);
        assert!(mask.is_none());
    }
    #[test]
    fn semantic_support_survives_scale_growth_without_selecting_an_old_size_arm() {
        let cancel = AtomicBool::new(false);
        let (e, image, seed, _) = fixture(Some((6, 7)), false);
        let id = Identity::from_seed(&e, &image, &seed, &cancel).unwrap();
        let mut whole = seed.clone();
        whole.bytes.fill(0);
        let mut arm = whole.clone();
        for y in 28..124 {
            for x in 24..120 {
                whole.bytes[y * 256 + x] = 255;
                if x < 44 {
                    arm.bytes[y * 256 + x] = 255;
                }
            }
        }
        let (_, chosen) =
            select_scored(&id, None, vec![(0.90, whole.clone()), (0.95, arm)], &cancel).unwrap();
        assert_eq!(chosen.unwrap(), whole);
    }
    #[test]
    fn visible_flow_can_follow_known_twin_but_global_recovery_refuses_choice() {
        let cancel = AtomicBool::new(false);
        let (e, mut image, one, _) = fixture(Some((6, 7)), false);
        for y in 7..19 {
            for x in 6..18 {
                image.rgba[(y * 64 + x) * 4] = ((x * 23 + y * 17 + x * y * 3) % 100 + 140) as u8;
            }
        }
        let id = Identity::from_seed(&e, &image, &one, &cancel).unwrap();
        let (_, mut current, one, two) = fixture(Some((6, 7)), true);
        let two = two.unwrap();
        for y in 7..19 {
            for x in 6..18 {
                let pixel = &image.rgba[(y * 64 + x) * 4..(y * 64 + x + 1) * 4];
                current.rgba[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].copy_from_slice(pixel);
                let other = ((y - 7 + 40) * 64 + x - 6 + 44) * 4;
                current.rgba[other..other + 4].copy_from_slice(pixel);
            }
        }
        let flow = crate::sam_continuity::Continuity::measure(&image, &current, &one, &cancel)
            .unwrap()
            .unwrap();
        let mut left = one.clone();
        let mut right = one.clone();
        for y in 0..256 {
            for x in 0..256 {
                if x < 48 {
                    right.bytes[y * 256 + x] = 0;
                } else {
                    left.bytes[y * 256 + x] = 0;
                }
            }
        }
        let (state, owner) = select_scored_visible(
            &id,
            Some(&one),
            Some(&flow),
            vec![(0.9, left), (0.9, right)],
            &cancel,
        )
        .unwrap();
        assert_eq!(state, State::Tracked);
        assert_eq!(owner.unwrap(), one);
        let candidates = vec![(0.9, one.clone()), (0.9, two.clone())];
        let (state, mask) =
            select_scored_visible(&id, Some(&one), Some(&flow), candidates.clone(), &cancel)
                .unwrap();
        assert_eq!(state, State::Tracked);
        assert_eq!(mask.unwrap(), one);
        let (state, mask) = select_scored(&id, None, candidates, &cancel).unwrap();
        assert_eq!(state, State::Ambiguous);
        assert!(mask.is_none());
    }
    #[test]
    fn bad_masks_and_cancellation_do_not_change_identity() {
        let cancel = AtomicBool::new(false);
        let (a, image, mut seed, _) = fixture(Some((6, 7)), false);
        seed.bytes.truncate(1);
        assert!(Identity::from_seed(&a, &image, &seed, &cancel).is_err());
        let (a, image, seed, _) = fixture(Some((6, 7)), false);
        let id = Identity::from_seed(&a, &image, &seed, &cancel).unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert!(id.proposals(&a, &image, &cancel).is_err());
    }
    #[test]
    #[ignore = "requires pinned SlimSAM models and host ORT"]
    fn actual_pinned_encoder_decoder_reacquires_a_returning_target() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let make = |center: i32| {
            Image::new(
                128,
                96,
                (0..128 * 96)
                    .flat_map(|i| {
                        let x = i % 128;
                        let y = i / 128;
                        if (x - center).pow(2) + (y - 48).pow(2) < 18 * 18 {
                            [220, 25, 45, 255]
                        } else {
                            [22, 130, 180, 255]
                        }
                    })
                    .collect(),
            )
            .unwrap()
        };
        let source = make(36);
        let embedding = brush.embed(&source, &cancel).unwrap();
        let seed = brush
            .region(&embedding, &[[36. / 128., 0.5]], &cancel)
            .unwrap();
        let identity = Identity::from_seed(&embedding, &source, &seed, &cancel).unwrap();
        let blank = Image::new(128, 96, [22, 130, 180, 255].repeat(128 * 96)).unwrap();
        let blank_embedding = brush.embed(&blank, &cancel).unwrap();
        let (missing, mask) = brush
            .reidentify(&blank_embedding, &blank, &identity, None, &cancel)
            .unwrap();
        assert_eq!(missing, State::Occluded);
        assert!(mask.is_none());
        let mut distractor = make(91);
        for p in distractor.rgba.chunks_exact_mut(4) {
            if p[0] == 220 {
                p[..3].copy_from_slice(&[20, 230, 10]);
            }
        }
        let distractor_embedding = brush.embed(&distractor, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&distractor_embedding, &distractor, &identity, None, &cancel)
            .unwrap();
        assert_eq!(state, State::Occluded);
        assert!(mask.is_none());
        let twin = Image::new(
            128,
            96,
            (0..128 * 96)
                .flat_map(|i| {
                    let x: i32 = i % 128;
                    let y: i32 = i / 128;
                    if (x - 36).pow(2) + (y - 48).pow(2) < 18 * 18
                        || (x - 91).pow(2) + (y - 48).pow(2) < 18 * 18
                    {
                        [220, 25, 45, 255]
                    } else {
                        [22, 130, 180, 255]
                    }
                })
                .collect(),
        )
        .unwrap();
        let twin_embedding = brush.embed(&twin, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&twin_embedding, &twin, &identity, None, &cancel)
            .unwrap();
        assert_eq!(
            state,
            State::Ambiguous,
            "identical neural instances must not be silently selected"
        );
        assert!(mask.is_none());
        let moved = make(91);
        let embedding = brush.embed(&moved, &cancel).unwrap();
        let (state, reacquired) = brush
            .reidentify(&embedding, &moved, &identity, None, &cancel)
            .unwrap();
        assert_eq!(state, State::Reacquired);
        let mask = reacquired.expect("learned returning target");
        let (mut mass, mut cx) = (0., 0.);
        for y in 0..256 {
            for x in 0..256 {
                if mask.bytes[y * 256 + x] >= 128 {
                    mass += 1.;
                    cx += x as f64;
                }
            }
        }
        assert!(mass > 100.);
        assert!((cx / mass / 256. - 91. / 128.).abs() < 0.08);
        let seam = Image::new(
            128,
            96,
            (0..128 * 96)
                .flat_map(|i| {
                    let x: i32 = i % 128;
                    let y: i32 = i / 128;
                    let dx = (x - 125).abs().min(128 - (x - 125).abs());
                    if dx * dx + (y - 48).pow(2) < 18 * 18 {
                        [220, 25, 45, 255]
                    } else {
                        [22, 130, 180, 255]
                    }
                })
                .collect(),
        )
        .unwrap();
        let seam_embedding = brush.embed(&seam, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&seam_embedding, &seam, &identity, None, &cancel)
            .unwrap();
        assert_eq!(state, State::Reacquired);
        let mask = mask.unwrap();
        assert!(mask
            .bytes
            .chunks_exact(256)
            .any(|row| row[..8].iter().any(|v| *v >= 128)));
        assert!(mask
            .bytes
            .chunks_exact(256)
            .any(|row| row[248..].iter().any(|v| *v >= 128)));
    }
    #[test]
    #[ignore = "requires explicit private real-image fixture and pinned SAM runtime"]
    fn actual_neural_reidentification_on_private_photographic_material() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let decoded = image::open(std::env::var("VIDEO_KADR_REID_REAL_IMAGE").unwrap())
            .unwrap()
            .to_rgba8();
        let source = Image::new(
            decoded.width() as usize,
            decoded.height() as usize,
            decoded.into_raw(),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let embedding = brush.embed(&source, &cancel).unwrap();
        let seed = brush.region(&embedding, &[[0.65, 0.55]], &cancel).unwrap();
        let identity = Identity::from_seed(&embedding, &source, &seed, &cancel).unwrap();
        let shift = source.width / 3;
        let mut rgba = vec![0; source.rgba.len()];
        for y in 0..source.height {
            for x in 0..source.width {
                let a = (y * source.width + (x + shift) % source.width) * 4;
                let b = (y * source.width + x) * 4;
                rgba[b..b + 4].copy_from_slice(&source.rgba[a..a + 4]);
            }
        }
        let moved = Image::new(source.width, source.height, rgba).unwrap();
        let embedding = brush.embed(&moved, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&embedding, &moved, &identity, None, &cancel)
            .unwrap();
        assert_eq!(state, State::Reacquired);
        let mask = mask.unwrap();
        let mut expected = seed.clone();
        for y in 0..256 {
            for x in 0..256 {
                let u =
                    ((x as f32 + 0.5) / 256. + shift as f32 / source.width as f32).rem_euclid(1.);
                expected.bytes[y * 256 + x] =
                    (crate::mask_apply::sample(&seed, u, (y as f32 + 0.5) / 256.) * 255. + 0.5)
                        as u8;
            }
        }
        let overlap = crate::sam_tracking::overlap(&mask, &expected, &cancel).unwrap();
        let predicted = mask.bytes.iter().filter(|v| **v >= 128).count();
        let expected_area = expected.bytes.iter().filter(|v| **v >= 128).count();
        let common = mask
            .bytes
            .iter()
            .zip(&expected.bytes)
            .filter(|(a, b)| **a >= 128 && **b >= 128)
            .count();
        println!("photographic source-owned target recovery IoU={overlap}; predicted={predicted}; expected={expected_area}; intersection={common}");
        if let Ok(folder) = std::env::var("VIDEO_KADR_REID_REAL_ARTIFACTS") {
            std::fs::create_dir_all(&folder).unwrap();
            for (name, bytes) in [
                ("seed.png", &seed.bytes),
                ("expected.png", &expected.bytes),
                ("actual.png", &mask.bytes),
            ] {
                image::save_buffer(
                    Path::new(&folder).join(name),
                    bytes,
                    256,
                    256,
                    image::ColorType::L8,
                )
                .unwrap();
            }
        }

        assert!(overlap > 0.5);
    }
    #[test]
    #[ignore = "requires explicit private real-video frames and pinned SAM runtime"]
    fn actual_real_video_pose_and_exposure() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let load = |name: &str| {
            let p = Path::new(&std::env::var("VIDEO_KADR_REID_POSE_DIR").unwrap()).join(name);
            let im = image::open(p).unwrap().to_rgba8();
            let (w, h) = (im.width() as usize, im.height() as usize);
            let bytes = im.into_raw();
            let mut rolled = bytes.clone();
            for y in 0..h {
                for x in 0..w {
                    let a = (y * w + (x + w / 2) % w) * 4;
                    let b = (y * w + x) * 4;
                    rolled[b..b + 4].copy_from_slice(&bytes[a..a + 4]);
                }
            }
            Image::new(w, h, rolled).unwrap()
        };
        let cancel = AtomicBool::new(false);
        let source = load("pose-first.png");
        let moved = load("pose-last.png");
        let embedding = brush.embed(&source, &cancel).unwrap();
        let seed = brush.region(&embedding, &[[0.46, 0.35]], &cancel).unwrap();
        let identity = Identity::from_seed(&embedding, &source, &seed, &cancel).unwrap();
        let reference_embedding = brush.embed(&moved, &cancel).unwrap();
        let reference = brush
            .region(&reference_embedding, &[[0.46, 0.35]], &cancel)
            .unwrap();
        for exposure in [1.0f32, 0.55, 1.35] {
            let mut changed = Image::new(moved.width, moved.height, moved.rgba.clone()).unwrap();
            for pixel in changed.rgba.chunks_exact_mut(4) {
                for c in &mut pixel[..3] {
                    *c = (f32::from(*c) * exposure).round().clamp(0., 255.) as u8;
                }
            }
            let embedding = brush.embed(&changed, &cancel).unwrap();
            let (state, mask) = brush
                .reidentify(&embedding, &changed, &identity, None, &cancel)
                .unwrap();
            let iou = mask
                .as_ref()
                .map(|m| crate::sam_tracking::overlap(m, &reference, &cancel).unwrap())
                .unwrap_or(0.);
            println!("real video pose exposure={exposure} state={state:?} reference IoU={iou}");
            assert_eq!(state, State::Reacquired);
            assert!(iou > 0.5);
        }
    }
    #[test]
    #[ignore = "requires explicit private calibrated real-video sequence and pinned SAM"]
    fn actual_real_video_multiview_bank_pose_and_gap() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let folder = std::env::var("VIDEO_KADR_REID_SEQUENCE").unwrap();
        let load = |index: usize| {
            let im = image::open(Path::new(&folder).join(format!("pose-{index:02}.png")))
                .unwrap()
                .to_rgba8();
            let (w, h) = (im.width() as usize, im.height() as usize);
            let bytes = im.into_raw();
            let mut out = bytes.clone();
            for y in 0..h {
                for x in 0..w {
                    let a = (y * w + (x + w / 2) % w) * 4;
                    let b = (y * w + x) * 4;
                    out[b..b + 4].copy_from_slice(&bytes[a..a + 4]);
                }
            }
            Image::new(w, h, out).unwrap()
        };
        let cancel = AtomicBool::new(false);
        let source = load(0);
        let e = brush.embed(&source, &cancel).unwrap();
        let seed = brush.region(&e, &[[0.49, 0.70]], &cancel).unwrap();
        let mut bank = Identity::from_seed(&e, &source, &seed, &cancel).unwrap();
        let mut previous = seed;
        let mut previous_image = source.clone();
        for index in 1..=3 {
            let image = load(index);
            let e = brush.embed(&image, &cancel).unwrap();
            let saved = brush.region(&e, &[[0.49, 0.70]], &cancel).unwrap();
            let (state, found) = brush
                .reidentify_visible(
                    &e,
                    &image,
                    &bank,
                    Some((&previous_image, &previous)),
                    &cancel,
                )
                .unwrap();
            let agreement = found
                .as_ref()
                .map(|m| crate::sam_tracking::overlap(m, &saved, &cancel).unwrap())
                .unwrap_or(0.);
            println!("saved real frame={index} state={state:?} agreement={agreement}");
            if let Ok(folder) = std::env::var("VIDEO_KADR_REID_DIAGNOSTIC") {
                std::fs::create_dir_all(&folder).unwrap();
                for (name, mask) in [
                    ("previous", Some(&previous)),
                    ("reference", Some(&saved)),
                    ("actual", found.as_ref()),
                ] {
                    if let Some(mask) = mask {
                        image::save_buffer(
                            Path::new(&folder).join(format!("office-{index}-{name}.png")),
                            &mask.bytes,
                            256,
                            256,
                            image::ColorType::L8,
                        )
                        .unwrap();
                    }
                }
            }
            if state == State::Ambiguous {
                // A hash-valid propagated shirt/arm plane is not identity truth.
                // This real frame has competing hypotheses; freeze before it.
                assert_eq!(index, 3);
                assert!(found.is_none());
                break;
            }
            assert_eq!(state, State::Tracked);
            assert!(agreement >= 0.5);
            bank.enroll_admitted_view(&e, &image, &saved, &cancel)
                .unwrap();
            previous = saved;
            previous_image = image;
        }
        let image = load(16);
        let e = brush.embed(&image, &cancel).unwrap();
        let reference = brush.region(&e, &[[0.49, 0.70]], &cancel).unwrap();
        let (state, found) = brush.reidentify(&e, &image, &bank, None, &cancel).unwrap();
        let iou = found
            .as_ref()
            .map(|m| crate::sam_tracking::overlap(m, &reference, &cancel).unwrap())
            .unwrap_or(0.);
        println!(
            "real4sec bank views={} recovery={state:?} IoU={iou}",
            bank.views.len()
        );
        assert_eq!(state, State::Reacquired);
        assert!(iou > 0.5);
    }
    #[test]
    #[ignore = "requires explicit private photographic fixture and pinned SAM"]
    fn actual_photographic_twins_and_missing_identity_are_refused() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let decoded = image::open(std::env::var("VIDEO_KADR_REID_REAL_IMAGE").unwrap())
            .unwrap()
            .to_rgba8();
        let source = Image::new(
            decoded.width() as usize,
            decoded.height() as usize,
            decoded.into_raw(),
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        let e = brush.embed(&source, &cancel).unwrap();
        let seed = brush.region(&e, &[[0.65, 0.55]], &cancel).unwrap();
        let identity = Identity::from_seed(&e, &source, &seed, &cancel).unwrap();
        let mut twins = vec![];
        for row in source.rgba.chunks_exact(source.width * 4) {
            twins.extend_from_slice(row);
            twins.extend_from_slice(row);
        }
        let twins = Image::new(source.width * 2, source.height, twins).unwrap();
        let e = brush.embed(&twins, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&e, &twins, &identity, None, &cancel)
            .unwrap();
        println!("photographic twin state={state:?}");
        assert_eq!(state, State::Ambiguous);
        assert!(mask.is_none());
        let blank = Image::new(source.width, source.height, vec![128; source.rgba.len()]).unwrap();
        let e = brush.embed(&blank, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&e, &blank, &identity, None, &cancel)
            .unwrap();
        println!("photographic missing target state={state:?}");
        assert_eq!(state, State::Occluded);
        assert!(mask.is_none());
    }
    #[test]
    #[ignore = "requires explicit private front-back-close subject video and pinned SAM"]
    fn actual_front_back_close_person_identity() {
        front_back_close_person_identity(false);
    }
    #[test]
    #[ignore = "quality gate: requires private strong-pose footage; refusals are not recovery"]
    fn actual_front_back_close_person_identity_coverage() {
        front_back_close_person_identity(true);
    }
    fn front_back_close_person_identity(require_coverage: bool) {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut brush = crate::slim_sam::Brush::load(
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap()),
            Path::new(&std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap()),
        )
        .unwrap();
        let folder = std::env::var("VIDEO_KADR_REID_HARD_POSE").unwrap();
        let load = |i: usize| {
            let im = image::open(Path::new(&folder).join(format!("frame-{:02}.png", i + 1)))
                .unwrap()
                .to_rgba8();
            Image::new(im.width() as usize, im.height() as usize, im.into_raw()).unwrap()
        };
        fn person_mask(
            brush: &mut crate::slim_sam::Brush,
            e: &Embedding,
            points: &[[f64; 2]],
            cancel: &AtomicBool,
        ) -> Mask {
            let candidates = brush.candidates(e, points, cancel).unwrap();
            // Explicit author selection of the whole-person SAM hypothesis. The
            // default top-confidence hypothesis can be an arm or shirt panel.
            candidates
                .into_iter()
                .filter(|(_, m)| m.bytes.iter().filter(|v| **v >= 128).count() < 65536 * 4 / 5)
                .max_by_key(|(_, m)| m.bytes.iter().filter(|v| **v >= 128).count())
                .unwrap()
                .1
        }
        let cancel = AtomicBool::new(false);
        let source = load(0);
        let e = brush.embed(&source, &cancel).unwrap();
        // Explicit source-owned hand-authored whole-person contour. SlimSAM's
        // top hypotheses here select a shirt/arm rather than the full person;
        // that is an invalid seed for a front-to-back PERSON identity test.
        let polygon: [[f64; 2]; 19] = [
            [306., 115.],
            [333., 110.],
            [366., 120.],
            [371., 147.],
            [358., 161.],
            [365., 191.],
            [395., 200.],
            [418., 222.],
            [427., 277.],
            [431., 330.],
            [420., 360.],
            [280., 360.],
            [282., 322.],
            [273., 270.],
            [284., 214.],
            [311., 191.],
            [310., 166.],
            [300., 148.],
            [300., 128.],
        ];
        let seed = Mask {
            width: 256,
            height: 256,
            bytes: (0..65536)
                .map(|i| {
                    let (x, y) = (
                        ((i % 256) as f64 + 0.5) / 256. * 640.,
                        ((i / 256) as f64 + 0.5) / 256. * 360.,
                    );
                    let mut inside = false;
                    for j in 0..polygon.len() {
                        let a = polygon[j];
                        let b = polygon[(j + 1) % polygon.len()];
                        if (a[1] > y) != (b[1] > y)
                            && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0]
                        {
                            inside = !inside;
                        }
                    }
                    if inside {
                        255
                    } else {
                        0
                    }
                })
                .collect(),
        };
        let mut overlay = source.rgba.clone();
        for y in 0..source.height {
            for x in 0..source.width {
                if seed.bytes[(y * 256 / source.height) * 256 + x * 256 / source.width] >= 128 {
                    let i = (y * source.width + x) * 4;
                    overlay[i] = overlay[i] / 2 + 127;
                    overlay[i + 1] /= 2;
                    overlay[i + 2] /= 2;
                }
            }
        }
        image::save_buffer(
            Path::new(&folder).join("whole-person-seed-overlay.png"),
            &overlay,
            source.width as u32,
            source.height as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
        image::save_buffer(
            Path::new(&folder).join("whole-person-seed.png"),
            &seed.bytes,
            256,
            256,
            image::ColorType::L8,
        )
        .unwrap();
        let mut bank = Identity::from_seed(&e, &source, &seed, &cancel).unwrap();
        let mut previous = seed;
        let mut previous_image = source.clone();
        let mut retained_views = vec![];
        for index in 1..=7 {
            let image = load(index);
            let e = brush.embed(&image, &cancel).unwrap();
            let centre = [0.53, 0.53, 0.53, 0.53, 0.51, 0.48, 0.49, 0.56][index];
            let saved = person_mask(&mut brush, &e, &[[centre, 0.4], [centre, 0.75]], &cancel);
            let (state, found) = brush
                .reidentify_visible(
                    &e,
                    &image,
                    &bank,
                    Some((&previous_image, &previous)),
                    &cancel,
                )
                .unwrap();
            let iou = found
                .as_ref()
                .map(|m| crate::sam_tracking::overlap(m, &saved, &cancel).unwrap())
                .unwrap_or(0.);
            image::save_buffer(
                Path::new(&folder).join(format!("whole-person-reference-{index}.png")),
                &saved.bytes,
                256,
                256,
                image::ColorType::L8,
            )
            .unwrap();
            let score = bank.similarity(&e, &image, &saved, &cancel).unwrap();
            println!("hard pose contiguous index={index} state={state:?} agreement={iou} score={score:?} views={}",bank.views.len());
            if state != State::Tracked || iou < 0.5 {
                println!("freeze bank before unqualified source view index={index}");
                break;
            }
            if let Some(slot) = bank
                .enroll_admitted_view(&e, &image, found.as_ref().unwrap(), &cancel)
                .unwrap()
            {
                if slot < retained_views.len() {
                    retained_views[slot] = index;
                } else {
                    retained_views.push(index);
                }
            }
            println!("retained source view indices={retained_views:?}, source times=indices*0.5s");
            previous = saved;
            previous_image = image;
        }
        let mut failures = vec![];
        for index in [8usize, 20] {
            let image = load(index);
            let e = brush.embed(&image, &cancel).unwrap();
            let points = if index == 8 {
                vec![[0.56, 0.4], [0.56, 0.75]]
            } else {
                vec![[0.70, 0.4], [0.85, 0.85]]
            };
            let reference = person_mask(&mut brush, &e, &points, &cancel);
            image::save_buffer(
                Path::new(&folder).join(format!("heldout-reference-{index}.png")),
                &reference.bytes,
                256,
                256,
                image::ColorType::L8,
            )
            .unwrap();
            println!(
                "heldout proposed queries index={index} {:?}",
                bank.proposals(&e, &image, &cancel).unwrap()
            );
            let (state, found) = brush.reidentify(&e, &image, &bank, None, &cancel).unwrap();
            let iou = found
                .as_ref()
                .map(|m| crate::sam_tracking::overlap(m, &reference, &cancel).unwrap())
                .unwrap_or(0.);
            println!(
                "hard pose heldout index={index} state={state:?} agreement={iou} score={:?}",
                bank.similarity(&e, &image, &reference, &cancel).unwrap()
            );
            if let Some(mask) = &found {
                image::save_buffer(
                    Path::new(&folder).join(format!("heldout-actual-{index}.png")),
                    &mask.bytes,
                    256,
                    256,
                    image::ColorType::L8,
                )
                .unwrap();
            }
            if state == State::Reacquired {
                if iou <= 0.5 {
                    failures.push(format!("WRONG identity index{index} IoU={iou}"));
                }
            } else {
                assert!(found.is_none());
                println!("hard pose positive coverage OPEN index={index} state={state:?}");
                if require_coverage
                    || std::env::var_os("VIDEO_KADR_REQUIRE_HARD_COVERAGE").is_some()
                {
                    failures.push(format!("unqualified positive index{index} state={state:?}"));
                }
            }
        }
        // Remove the selected foreground person while retaining the different
        // red-helmet person on the left: an appearance distractor, not a twin
        // made by duplicating the same target photograph.
        let mut negative = load(20);
        for y in 0..negative.height {
            for x in negative.width / 2..negative.width {
                let i = (y * negative.width + x) * 4;
                negative.rgba[i..i + 4].copy_from_slice(&[20, 20, 20, 255]);
            }
        }
        let e = brush.embed(&negative, &cancel).unwrap();
        let (state, mask) = brush
            .reidentify(&e, &negative, &bank, None, &cancel)
            .unwrap();
        println!("real background helmet with foreground removed state={state:?}");
        if !matches!(state, State::Occluded | State::Ambiguous) || mask.is_some() {
            failures.push(format!("different-person distractor state={state:?}"));
        }
        assert!(
            failures.is_empty(),
            "hard pose qualification incomplete: {failures:?}"
        );
    }
    use std::path::Path;
}
