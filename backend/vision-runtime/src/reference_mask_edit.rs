// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The probability picture: how much of each pixel is the subject.

/// A mask with every value under this is nobody's: the model found
/// nothing, and the picture should show as shot rather than vanish.
const BLANK_BELOW: u8 = 10;

/// An eight-bit picture of probabilities, `0` background and `255` subject,
/// covering the whole source picture whatever its shape.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Mask {
    /// A mask of one value throughout.
    pub fn filled(width: u32, height: u32, value: u8) -> Mask {
        Mask {
            width: width.max(1),
            height: height.max(1),
            data: vec![value; width.max(1) as usize * height.max(1) as usize],
        }
    }

    /// Wraps a buffer of exactly `width * height` bytes.
    pub fn from_bytes(width: u32, height: u32, data: Vec<u8>) -> Option<Mask> {
        (width > 0 && height > 0 && data.len() == width as usize * height as usize).then_some(
            Mask {
                width,
                height,
                data,
            },
        )
    }

    /// Pixels across.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Pixels down.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The bytes, row-major from the top-left.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// The bytes, to write into.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// True when the model found nothing: no pixel is more than a few
    /// percent likely to be the subject.
    pub fn is_blank(&self) -> bool {
        self.data.iter().all(|&v| v < BLANK_BELOW)
    }

    /// The mask shrunk by `factor` in each direction, each new pixel the
    /// mean of the block it replaces. A large model's answer stored at a
    /// size the renderer can read quickly.
    pub fn shrunk(&self, factor: u32) -> Mask {
        let factor = factor.max(1);
        if factor == 1 {
            return self.clone();
        }
        let width = (self.width / factor).max(1);
        let height = (self.height / factor).max(1);
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let mut sum = 0u32;
                for dy in 0..factor {
                    for dx in 0..factor {
                        sum += u32::from(
                            self.at(i64::from(x * factor + dx), i64::from(y * factor + dy)),
                        );
                    }
                }
                data.push((sum / (factor * factor)) as u8);
            }
        }
        Mask {
            width,
            height,
            data,
        }
    }

    /// The value at a pixel, clamped to the edge.
    pub fn at(&self, x: i64, y: i64) -> u8 {
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        self.data[y * self.width as usize + x]
    }

    /// The value at a fraction of the picture, bilinear between the four
    /// pixels around it, in `0..=1`. Outside the picture is the edge.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let x = u.clamp(0.0, 1.0) * self.width as f32 - 0.5;
        let y = v.clamp(0.0, 1.0) * self.height as f32 - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let (x0, y0) = (x0 as i64, y0 as i64);
        let top = f32::from(self.at(x0, y0)) * (1.0 - fx) + f32::from(self.at(x0 + 1, y0)) * fx;
        let bottom =
            f32::from(self.at(x0, y0 + 1)) * (1.0 - fx) + f32::from(self.at(x0 + 1, y0 + 1)) * fx;
        (top * (1.0 - fy) + bottom * fy) / 255.0
    }

    /// The mask softened: three box blurs of `radius` pixels, which is a
    /// gaussian to the eye at a fraction of the cost. Zero is the mask
    /// itself.
    pub fn blurred(&self, radius: f32) -> Mask {
        let radius = radius.round().max(0.0) as usize;
        if radius == 0 {
            return self.clone();
        }
        let mut out = self.clone();
        let mut scratch = vec![0u8; self.data.len()];
        for _ in 0..3 {
            blur_rows(&out.data, &mut scratch, self.width as usize, radius);
            blur_columns(
                &scratch,
                &mut out.data,
                self.width as usize,
                self.height as usize,
                radius,
            );
        }
        out
    }
}

/// One horizontal box blur, every row.
fn blur_rows(source: &[u8], out: &mut [u8], width: usize, radius: usize) {
    let window = (2 * radius + 1) as u32;
    for (row_in, row_out) in source.chunks_exact(width).zip(out.chunks_exact_mut(width)) {
        let mut sum: u32 = 0;
        // The window starts centred on the first pixel, with the pixels
        // before the edge reading as the edge.
        for offset in 0..=radius {
            sum += u32::from(row_in[offset.min(width - 1)]);
        }
        sum += u32::from(row_in[0]) * radius as u32;
        for x in 0..width {
            row_out[x] = ((sum + window / 2) / window) as u8;
            let leaving = if x >= radius {
                row_in[x - radius]
            } else {
                row_in[0]
            };
            let arriving = row_in[(x + radius + 1).min(width - 1)];
            sum = sum + u32::from(arriving) - u32::from(leaving);
        }
    }
}

/// One vertical box blur, every column.
fn blur_columns(source: &[u8], out: &mut [u8], width: usize, height: usize, radius: usize) {
    let window = (2 * radius + 1) as u32;
    for x in 0..width {
        let at = |y: usize| source[y.min(height - 1) * width + x];
        let mut sum: u32 = 0;
        for offset in 0..=radius {
            sum += u32::from(at(offset));
        }
        sum += u32::from(at(0)) * radius as u32;
        for y in 0..height {
            out[y * width + x] = ((sum + window / 2) / window) as u8;
            let leaving = if y >= radius { at(y - radius) } else { at(0) };
            let arriving = at(y + radius + 1);
            sum = sum + u32::from(arriving) - u32::from(leaving);
        }
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

// Brush strokes over a mask.
//
// A custom cutout is the model's mask with corrections painted on it.
// Each stroke is a path of points in source fractions and a diameter as a
// fraction of the source's width; here it becomes discs along the path,
// and each disc changes the pixels beneath it the way its tool says:
//
// - **Brush** keeps everything under it, **Eraser** removes everything.
// - **Smart brush** keeps the thing under it, whole: the brush model
//   (see `brush`) reads what the stroke lies on and the region it answers
//   is kept. **Smart eraser** removes that thing. Until the region has
//   been read - or when it cannot be, with the model not to hand - the
//   smart tools fall back to the mask's own confidence: the brush keeps
//   only what the model gave some chance of being the subject, the
//   eraser removes only what it was not sure of.
//
// A mask may be any shape, so a round brush is an ellipse in mask
// pixels; `aspect` (width over height of the source) is what makes it
// round again on screen.

use std::sync::Arc;

use super::{Stroke, Tool as BrushTool};

/// Where a smart stroke's region is found, by the stroke: `None` when it
/// has not been read yet.
pub type Regions<'a> = &'a dyn Fn(&Stroke) -> Option<Arc<Mask>>;

/// A region pixel at least this likely is the thing.
const REGION_IS: f32 = 0.5;

/// Names a stroke for the file its region is kept in: everything about
/// it, hashed.
pub fn stroke_key(stroke: &Stroke) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(&[stroke.tool as u8]);
    feed(&stroke.size.to_bits().to_le_bytes());
    feed(&stroke.at.unwrap_or(-1.0).to_bits().to_le_bytes());
    for [x, y] in &stroke.points {
        feed(&x.to_bits().to_le_bytes());
        feed(&y.to_bits().to_le_bytes());
    }
    hash
}

/// Below this much confidence a smart brush leaves a pixel alone.
const SMART_KEEP: u8 = 38;
/// Above this much confidence a smart eraser leaves a pixel alone.
const SMART_DROP: u8 = 217;

/// The model's mask with `strokes` painted over it, in order. `aspect` is
/// the source's width over its height; `regions` finds what the brush
/// model read under each smart stroke.
pub fn paint(auto: &Mask, strokes: &[Stroke], aspect: f32, regions: Regions<'_>) -> Mask {
    let mut out = auto.clone();
    for stroke in strokes {
        match regions(stroke).filter(|_| stroke.is_smart()) {
            Some(region) => paint_region(&mut out, stroke.tool, &region),
            None => paint_one(&mut out, auto, stroke, aspect),
        }
    }
    out
}

/// The thing a smart stroke named, kept or removed whole.
fn paint_region(out: &mut Mask, tool: BrushTool, region: &Mask) {
    let width = out.width();
    let height = out.height();
    let keep = tool == BrushTool::SmartBrush;
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            if region.sample(u, v) >= REGION_IS {
                let index = (y * width + x) as usize;
                out.bytes_mut()[index] = if keep { 255 } else { 0 };
            }
        }
    }
}

fn paint_one(out: &mut Mask, auto: &Mask, stroke: &Stroke, aspect: f32) {
    let width = out.width() as f32;
    let height = out.height() as f32;
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    // The brush is `size` of the source's width across, and as tall as it
    // is wide on screen: in mask pixels that is an ellipse.
    let rx = (stroke.size as f32 * width / 2.0).max(0.5);
    let ry = (stroke.size as f32 * aspect * height / 2.0).max(0.5);
    let points: Vec<(f32, f32)> = stroke
        .points
        .iter()
        .map(|[x, y]| (*x as f32 * width, *y as f32 * height))
        .collect();
    let Some(&first) = points.first() else {
        return;
    };
    disc(out, auto, stroke.tool, first, rx, ry);
    for pair in points.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let distance = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        // A disc every quarter radius, so the path has no scallops.
        let steps = (distance / (rx.min(ry) * 0.25)).ceil().max(1.0) as usize;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let centre = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            disc(out, auto, stroke.tool, centre, rx, ry);
        }
    }
}

fn disc(out: &mut Mask, auto: &Mask, tool: BrushTool, centre: (f32, f32), rx: f32, ry: f32) {
    let width = out.width() as i64;
    let height = out.height() as i64;
    let x0 = ((centre.0 - rx).floor() as i64).max(0);
    let x1 = ((centre.0 + rx).ceil() as i64).min(width - 1);
    let y0 = ((centre.1 - ry).floor() as i64).max(0);
    let y1 = ((centre.1 + ry).ceil() as i64).min(height - 1);
    if x0 > x1 || y0 > y1 {
        return;
    }
    let stride = out.width() as usize;
    let data = out.bytes_mut();
    for y in y0..=y1 {
        let dy = (y as f32 + 0.5 - centre.1) / ry;
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5 - centre.0) / rx;
            if dx * dx + dy * dy > 1.0 {
                continue;
            }
            let index = y as usize * stride + x as usize;
            let was = auto.bytes()[index];
            match tool {
                BrushTool::Brush => data[index] = 255,
                BrushTool::Eraser => data[index] = 0,
                BrushTool::SmartBrush if was >= SMART_KEEP => data[index] = 255,
                BrushTool::SmartEraser if was <= SMART_DROP => data[index] = 0,
                BrushTool::SmartBrush | BrushTool::SmartEraser => {}
            }
        }
    }
}
