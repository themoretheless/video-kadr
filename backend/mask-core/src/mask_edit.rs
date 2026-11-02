//! Source-coordinate correction strokes and three-pass feathering.
use crate::Mask;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    SmartBrush,
    Brush,
    SmartEraser,
    Eraser,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Stroke {
    pub tool: Tool,
    pub size: f64,
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionAttachment>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Correction {
    pub custom: bool,
    pub strokes: Vec<Stroke>,
    pub feather: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegionAttachment {
    pub source_id: String,
    pub mask_source_id: String,
    pub source_hash: String,
    pub mask_hash: String,
    pub mask_file_hash: String,
    pub encoder_hash: String,
    pub decoder_hash: String,
    pub runtime_hash: String,
    pub prompt_hash: String,
    pub points: Vec<[f64; 2]>,
}
impl RegionAttachment {
    pub fn validate(&self) -> Result<()> {
        use sha2::{Digest, Sha256};
        ensure!(
            !self.source_id.is_empty()
                && !self.mask_source_id.is_empty()
                && self.source_id != self.mask_source_id,
            "invalid region source IDs"
        );
        for hash in [
            &self.source_hash,
            &self.mask_hash,
            &self.mask_file_hash,
            &self.encoder_hash,
            &self.decoder_hash,
            &self.runtime_hash,
            &self.prompt_hash,
        ] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v)),
                "invalid region identity"
            );
        }
        ensure!(
            !self.points.is_empty()
                && self.points.len() <= 65536
                && self
                    .points
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
            "invalid region points"
        );
        ensure!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(&self.points)?)) == self.prompt_hash,
            "region point identity mismatch"
        );
        Ok(())
    }
}
impl Stroke {
    pub fn is_smart(&self) -> bool {
        matches!(self.tool, Tool::SmartBrush | Tool::SmartEraser)
    }
    /// Canonical source admission preserves paths just outside picture edges.
    pub fn tidy(mut self) -> Option<Self> {
        self.size = if self.size.is_finite() {
            self.size.clamp(0.005, 0.5)
        } else {
            0.005
        };
        self.at = self.at.filter(|v| v.is_finite()).map(|v| v.max(0.));
        self.points.retain(|p| p.iter().all(|v| v.is_finite()));
        for p in &mut self.points {
            for v in p {
                *v = v.clamp(-0.5, 1.5);
            }
        }
        (!self.points.is_empty()).then_some(self)
    }
}
fn valid(mask: &Mask) -> Result<()> {
    ensure!(
        mask.width > 0
            && mask.height > 0
            && mask
                .width
                .checked_mul(mask.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024 && n == mask.bytes.len()),
        "invalid correction mask"
    );
    Ok(())
}
struct Budget<'a> {
    cancel: &'a AtomicBool,
    left: u64,
}
impl Budget<'_> {
    fn use_pixels(&mut self, n: u64) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "mask correction cancelled"
        );
        self.left = self
            .left
            .checked_sub(n)
            .ok_or_else(|| anyhow::anyhow!("mask correction work budget exceeded"))?;
        Ok(())
    }
}
/// Regions are optional learned object answers, indexed by the original stroke.
/// A missing smart region uses the original automatic mask's confidence.
pub fn paint(
    auto: &Mask,
    strokes: &[Stroke],
    aspect: f32,
    regions: &[Option<&Mask>],
    cancel: &AtomicBool,
) -> Result<Mask> {
    valid(auto)?;
    ensure!(
        strokes.len() <= 1024 && (regions.is_empty() || regions.len() == strokes.len()),
        "invalid correction strokes/regions"
    );
    let mut total = 0usize;
    for (index, s) in strokes.iter().enumerate() {
        if let Some(region) = &s.region {
            region.validate()?;
            ensure!(
                s.is_smart()
                    && region.points == s.points
                    && regions.get(index).is_some_and(Option::is_some),
                "unbound learned region"
            );
            {
                use sha2::{Digest, Sha256};
                let pixels = &regions[index].unwrap().bytes;
                ensure!(
                    format!("{:x}", Sha256::digest(pixels)) == region.mask_hash,
                    "learned region mask binding mismatch"
                );
            }
        }
        total = total
            .checked_add(s.points.len())
            .ok_or_else(|| anyhow::anyhow!("too many correction points"))?;
        ensure!(
            total <= 65536
                && s.size.is_finite()
                && (0.005..=0.5).contains(&s.size)
                && s.at.is_none_or(|v| v.is_finite() && v >= 0.)
                && s.points
                    .iter()
                    .all(|p| p.iter().all(|v| v.is_finite() && (-0.5..=1.5).contains(v))),
            "invalid correction stroke"
        );
    }
    for m in regions.iter().flatten() {
        valid(m)?;
    }
    let mut budget = Budget {
        cancel,
        left: 128 * 1024 * 1024,
    };
    budget.use_pixels(0)?;
    let mut out = auto.clone();
    let aspect = if aspect.is_finite() && aspect > 0. {
        aspect
    } else {
        1.
    };
    for (i, s) in strokes.iter().enumerate() {
        if let Some(region) = regions.get(i).copied().flatten().filter(|_| s.is_smart()) {
            for y in 0..out.height {
                budget.use_pixels(out.width as u64)?;
                for x in 0..out.width {
                    if crate::sample(
                        region,
                        (x as f32 + 0.5) / out.width as f32,
                        (y as f32 + 0.5) / out.height as f32,
                    ) >= 0.5
                    {
                        out.bytes[y * out.width + x] =
                            if s.tool == Tool::SmartBrush { 255 } else { 0 };
                    }
                }
            }
        } else {
            let rx = (s.size as f32 * out.width as f32 / 2.).max(0.5);
            let ry = (s.size as f32 * aspect * out.height as f32 / 2.).max(0.5);
            ensure!(
                rx.is_finite() && ry.is_finite(),
                "correction aspect overflow"
            );
            let points: Vec<_> = s
                .points
                .iter()
                .map(|p| {
                    (
                        p[0] as f32 * out.width as f32,
                        p[1] as f32 * out.height as f32,
                    )
                })
                .collect();
            if let Some(first) = points.first() {
                disc(&mut out, auto, s.tool, *first, rx, ry, &mut budget)?;
            }
            for p in points.windows(2) {
                let (a, b) = (p[0], p[1]);
                let distance = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let steps = (distance / (rx.min(ry) * 0.25)).ceil().max(1.) as usize;
                for step in 1..=steps {
                    let t = step as f32 / steps as f32;
                    disc(
                        &mut out,
                        auto,
                        s.tool,
                        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t),
                        rx,
                        ry,
                        &mut budget,
                    )?;
                }
            }
        }
    }
    budget.use_pixels(0)?;
    Ok(out)
}
fn disc(
    out: &mut Mask,
    auto: &Mask,
    tool: Tool,
    (cx, cy): (f32, f32),
    rx: f32,
    ry: f32,
    budget: &mut Budget<'_>,
) -> Result<()> {
    budget.use_pixels(1)?;
    let x0 = ((cx - rx).floor() as i64).max(0);
    let x1 = ((cx + rx).ceil() as i64).min(out.width as i64 - 1);
    let y0 = ((cy - ry).floor() as i64).max(0);
    let y1 = ((cy + ry).ceil() as i64).min(out.height as i64 - 1);
    if x0 > x1 || y0 > y1 {
        return Ok(());
    }
    for y in y0..=y1 {
        budget.use_pixels((x1 - x0 + 1) as u64)?;
        let dy = (y as f32 + 0.5 - cy) / ry;
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5 - cx) / rx;
            if dx * dx + dy * dy > 1. {
                continue;
            }
            let i = y as usize * out.width + x as usize;
            let original = auto.bytes[i];
            match tool {
                Tool::Brush => out.bytes[i] = 255,
                Tool::Eraser => out.bytes[i] = 0,
                Tool::SmartBrush if original >= 38 => out.bytes[i] = 255,
                Tool::SmartEraser if original <= 217 => out.bytes[i] = 0,
                _ => {}
            }
        }
    }
    Ok(())
}
/// Three horizontal+vertical box passes, rounded at each byte boundary.
pub fn feather(mask: &Mask, radius: f32, cancel: &AtomicBool) -> Result<Mask> {
    valid(mask)?;
    ensure!(
        radius.is_finite() && (0.0..=65536.).contains(&radius),
        "invalid mask feather radius"
    );
    let mut budget = Budget {
        cancel,
        left: 512 * 1024 * 1024,
    };
    budget.use_pixels(0)?;
    let r = radius.round() as usize;
    let mut out = mask.clone();
    if r == 0 {
        return Ok(out);
    }
    let mut scratch = vec![0; mask.bytes.len()];
    let window = (2 * r + 1) as u32;
    for _ in 0..3 {
        for y in 0..mask.height {
            budget.use_pixels((mask.width + r + 1) as u64)?;
            let row = &out.bytes[y * mask.width..(y + 1) * mask.width];
            let mut sum = u32::from(row[0]) * r as u32;
            for offset in 0..=r {
                sum += u32::from(row[offset.min(mask.width - 1)]);
            }
            for x in 0..mask.width {
                scratch[y * mask.width + x] = ((sum + window / 2) / window) as u8;
                let leaving = row[x.saturating_sub(r)];
                let arriving = row[(x + r + 1).min(mask.width - 1)];
                sum = sum + u32::from(arriving) - u32::from(leaving);
            }
        }
        for x in 0..mask.width {
            budget.use_pixels((mask.height + r + 1) as u64)?;
            let at = |y: usize| scratch[y.min(mask.height - 1) * mask.width + x];
            let mut sum = u32::from(at(0)) * r as u32;
            for offset in 0..=r {
                sum += u32::from(at(offset));
            }
            for y in 0..mask.height {
                out.bytes[y * mask.width + x] = ((sum + window / 2) / window) as u8;
                sum = sum + u32::from(at(y + r + 1)) - u32::from(at(y.saturating_sub(r)));
            }
        }
    }
    budget.use_pixels(0)?;
    Ok(out)
}
/// Blank automatic masks retain the picture; explicit custom strokes may erase all.
pub fn resolve(
    auto: &Mask,
    custom: bool,
    strokes: &[Stroke],
    regions: &[Option<&Mask>],
    aspect: f32,
    softness: f64,
    cancel: &AtomicBool,
) -> Result<Option<Mask>> {
    valid(auto)?;
    ensure!(
        softness.is_finite() && (0.0..=0.1).contains(&softness),
        "invalid cutout feather"
    );
    ensure!(!cancel.load(Ordering::Relaxed), "mask correction cancelled");
    if (!custom || strokes.is_empty()) && auto.is_blank() {
        return Ok(None);
    }
    let painted = if custom && !strokes.is_empty() {
        paint(auto, strokes, aspect, regions, cancel)?
    } else {
        auto.clone()
    };
    Ok(Some(feather(
        &painted,
        softness as f32 * painted.width as f32,
        cancel,
    )?))
}
