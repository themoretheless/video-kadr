// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Enhance: a picture restored and enlarged by a model.
//!
//! The model is Real-ESRGAN's compact general network: a frame in, the
//! same frame four times the size out, with its noise, blocking and
//! softness gone. It reads any size in principle, but a whole frame at
//! once is more memory than a phone has and a shape the accelerators
//! compile for badly, so the frame is cut into tiles of one fixed size,
//! each run with a margin of its neighbours around it so the seams do
//! not show, and the answers are pasted back. Everything about the tiling
//! is a pure function here, tested without a model; only [`Enhancer`]
//! needs one, and the `infer` feature that brings the runtime.
//!
//! Four times is more than a clip usually wants - a phone's 1080p is
//! plenty at 4K, and 8K is a file nobody can play - so [`factor_for`]
//! picks two, or one when the picture is already large, and the four-times
//! answer is averaged down to it. Averaging down from the restored picture
//! keeps the restoration and drops only the size.

use concat_core::frame::BYTES_PER_PIXEL;
use concat_core::Frame;

/// How much larger the model's answer is than what it read.
pub const SCALE: u32 = 4;
/// The square of source pixels one run restores.
pub const CORE: u32 = 256;
/// The margin of neighbouring pixels read around a tile and thrown away
/// after, so the tile's edge is restored knowing what is past it.
pub const PAD: u32 = 16;
/// The square the model actually reads: the tile and its margin.
pub const WINDOW: u32 = CORE + 2 * PAD;
/// The longest side Enhance will produce: past this the picture is
/// restored at its own size rather than enlarged.
pub const LONGEST_SIDE: u32 = 4096;

/// One tile: a rectangle of source pixels restored in one run. Every tile
/// but the last in a row or column is [`CORE`] square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// Left edge, in source pixels.
    pub x: u32,
    /// Top edge, in source pixels.
    pub y: u32,
    /// Width, in source pixels: `CORE`, or what is left of the row.
    pub width: u32,
    /// Height, in source pixels: `CORE`, or what is left of the column.
    pub height: u32,
}

/// The tiles of a `width` by `height` picture, row by row: they cover it
/// exactly, and no pixel is in two. Empty for an empty picture.
pub fn tiles(width: u32, height: u32) -> Vec<Tile> {
    let mut tiles = Vec::new();
    let mut y = 0;
    while y < height {
        let mut x = 0;
        while x < width {
            tiles.push(Tile {
                x,
                y,
                width: CORE.min(width - x),
                height: CORE.min(height - y),
            });
            x += CORE;
        }
        y += CORE;
    }
    tiles
}

/// How many times larger a `width` by `height` picture comes out: twice,
/// unless that would pass [`LONGEST_SIDE`], and then once, restored at
/// its own size.
pub fn factor_for(width: u32, height: u32) -> u32 {
    if width.max(height) * 2 > LONGEST_SIDE {
        1
    } else {
        2
    }
}

/// The model's input for one tile: the [`WINDOW`] square around it as
/// three planes of `0..=1`, red then green then blue, row-major. Past the
/// picture's edge the edge pixel repeats, which is what a restoration
/// network expects to see there.
pub fn window_planes(frame: &Frame, tile: Tile) -> Vec<f32> {
    let (fw, fh) = (frame.width() as i64, frame.height() as i64);
    let pixels = frame.pixels();
    let side = WINDOW as usize;
    let mut planes = vec![0f32; 3 * side * side];
    for wy in 0..side {
        let sy = (tile.y as i64 + wy as i64 - PAD as i64).clamp(0, fh - 1) as usize;
        for wx in 0..side {
            let sx = (tile.x as i64 + wx as i64 - PAD as i64).clamp(0, fw - 1) as usize;
            let at = (sy * fw as usize + sx) * BYTES_PER_PIXEL;
            for c in 0..3 {
                planes[(c * side + wy) * side + wx] = f32::from(pixels[at + c]) / 255.0;
            }
        }
    }
    planes
}

/// The model's answer for one tile - three planes of `WINDOW * SCALE`
/// square, red then green then blue - pasted into `out`, which is the
/// source picture's size times [`SCALE`]. The margin is dropped: only the
/// tile's own pixels, enlarged, land. Values past `0..=1` are clamped.
pub fn paste(out: &mut Frame, tile: Tile, answer: &[f32]) -> Result<(), String> {
    let side = (WINDOW * SCALE) as usize;
    if answer.len() != 3 * side * side {
        return Err(format!(
            "enhance: the model answered {} values for a tile, not {}",
            answer.len(),
            3 * side * side
        ));
    }
    let ow = out.width() as usize;
    let pixels = out.pixels_mut();
    let (tw, th) = (
        (tile.width * SCALE) as usize,
        (tile.height * SCALE) as usize,
    );
    let (ox, oy) = ((tile.x * SCALE) as usize, (tile.y * SCALE) as usize);
    let margin = (PAD * SCALE) as usize;
    for y in 0..th {
        let ay = y + margin;
        for x in 0..tw {
            let ax = x + margin;
            let at = ((oy + y) * ow + ox + x) * BYTES_PER_PIXEL;
            for c in 0..3 {
                let value = answer[(c * side + ay) * side + ax];
                pixels[at + c] = (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            pixels[at + 3] = 255;
        }
    }
    Ok(())
}

/// `frame` averaged down by `by` in each direction: every `by` by `by`
/// block becomes one pixel. `by` of 1 is a copy. A picture whose sides
/// are not multiples of `by` loses the remainder at the right and bottom.
pub fn downscale(frame: &Frame, by: u32) -> Frame {
    if by <= 1 {
        return frame.clone();
    }
    let (fw, fh) = (frame.width() as usize, frame.height() as usize);
    let by = by as usize;
    let (ow, oh) = (fw / by, fh / by);
    let source = frame.pixels();
    let mut pixels = vec![0u8; ow * oh * BYTES_PER_PIXEL];
    let count = (by * by) as u32;
    for y in 0..oh {
        for x in 0..ow {
            let mut sum = [0u32; 4];
            for dy in 0..by {
                for dx in 0..by {
                    let at = ((y * by + dy) * fw + x * by + dx) * BYTES_PER_PIXEL;
                    for c in 0..4 {
                        sum[c] += u32::from(source[at + c]);
                    }
                }
            }
            let at = (y * ow + x) * BYTES_PER_PIXEL;
            for c in 0..4 {
                pixels[at + c] = ((sum[c] + count / 2) / count) as u8;
            }
        }
    }
    Frame::from_rgba(ow as u32, oh as u32, pixels).expect("sides and bytes agree")
}
