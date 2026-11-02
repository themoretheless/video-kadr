// Adapted from Filmcraft 5231852443363f001c3f6b396dd9b1e6461ae2be. MIT; see LICENSE-MIT.
use super::image::Image;
use rayon::prelude::*;
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let wideal = ((12.0 * sigma * sigma / n as f32) + 1.0).sqrt();
    let mut wl = wideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let mideal = (12.0 * sigma * sigma - (n as i32 * wl * wl) as f32 - 4.0 * n as f32 * wl as f32 - 3.0 * n as f32) / (-4.0 * wl as f32 - 4.0);
    let m = mideal.round() as i32;
    (0..n as i32).map(|i| (((if i < m { wl } else { wu }) - 1) / 2).max(0) as usize).collect()
}

fn box_rows(px: &mut [f32], w: usize, r: usize, repeat: bool) {
    if r == 0 {
        return;
    }
    px.par_chunks_mut(w * 4).for_each_init(
        || super::budget::PixelBuffer::zeroed(w * 4),
        |tmp, row| {
            if super::context::cancelled(){return;}
            tmp.copy_from_slice(row);
            let inv = 1.0 / (2 * r + 1) as f32;
            let fetch = |i: isize| -> usize { if repeat { i.clamp(0, w as isize - 1) as usize } else { i as usize } };
            let mut acc = [0f32; 4];
            for i in -(r as isize)..=(r as isize) {
                if repeat || (i >= 0 && (i as usize) < w) {
                    let j = fetch(i);
                    for k in 0..4 {
                        acc[k] += tmp[j * 4 + k];
                    }
                }
            }
            for x in 0..w {
                for k in 0..4 {
                    row[x * 4 + k] = acc[k] * inv;
                }
                let out_i = x as isize - r as isize;
                let in_i = x as isize + r as isize + 1;
                if repeat || out_i >= 0 {
                    let j = fetch(out_i);
                    for k in 0..4 {
                        acc[k] -= tmp[j * 4 + k];
                    }
                }
                if repeat || (in_i as usize) < w {
                    let j = fetch(in_i);
                    for k in 0..4 {
                        acc[k] += tmp[j * 4 + k];
                    }
                }
            }
        },
    );
}

fn transpose(img: &Image) -> Image {
    let (w, h) = (img.w, img.h);
    let mut out = Image::new(h, w);
    const B: usize = 32;
    out.px.par_chunks_mut(h * 4 * B).enumerate().for_each(|(bi, chunk)| {
            if super::context::cancelled(){return;}
        let x0 = bi * B;
        let rows = chunk.len() / (h * 4);
        for y in 0..h {
            if super::context::cancelled(){return;}
            for dx in 0..rows {
                let x = x0 + dx;
                let s = (y * w + x) * 4;
                let d = (dx * h + y) * 4;
                chunk[d..d + 4].copy_from_slice(&img.px[s..s + 4]);
            }
        }
    });
    out
}

/// Gaussian blur via 3 box passes per axis (O(1) per pixel for any radius).
pub fn gaussian(img: &mut Image, sigma_x: f32, sigma_y: f32, repeat_edge: bool) {
    let (rx, ry) = gaussian_boxes(img.w, img.h, sigma_x, sigma_y);
    box_blur(img, &rx, &ry, repeat_edge);
}

/// The box radii [`gaussian`] runs on a `w`×`h` image, per axis (empty: that axis is untouched).
pub(crate) fn gaussian_boxes(w: usize, h: usize, sigma_x: f32, sigma_y: f32) -> (Vec<u32>, Vec<u32>) {
    // beyond a few image sizes every radius gives the same (flat) result; cap to keep box sizes sane
    let sigma_x = if sigma_x.is_finite() { sigma_x.min(w.max(8) as f32 * 2.0) } else { 0.0 };
    let sigma_y = if sigma_y.is_finite() { sigma_y.min(h.max(8) as f32 * 2.0) } else { 0.0 };
    let radii = |s: f32| if s > 0.3 { boxes_for_gauss(s, 3).into_iter().map(|r| r.min(u32::MAX as usize) as u32).collect() } else { Vec::new() };
    (radii(sigma_x), radii(sigma_y))
}

/// Box passes of radii `rx` along rows, then `ry` along columns.
pub(crate) fn box_blur(img: &mut Image, rx: &[u32], ry: &[u32], repeat_edge: bool) {
    for r in rx {
        box_rows(&mut img.px, img.w, *r as usize, repeat_edge);
    }
    if !ry.is_empty() {
        let mut t = transpose(img);
        for r in ry {
            box_rows(&mut t.px, t.w, *r as usize, repeat_edge);
        }
        *img = transpose(&t);
    }
}

