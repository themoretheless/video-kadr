//! Filmcraft SDR RGBA8 -> limited-range BT.709 planar 4:2:0.
//! Formula reference (MIT): Filmcraft export/lib.rs at 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
pub fn yuv420(surface: &GroupSurface) -> Result<Vec<u8>> {
    surface.validate()?;
    let (w, h) = (surface.width, surface.height);
    ensure!(w % 2 == 0 && h % 2 == 0, "H.264 requires even dimensions");
    let pixels = w * h;
    let mut output = vec![0; pixels + pixels / 2];
    for cy in 0..h / 2 {
        for cx in 0..w / 2 {
            let (mut u, mut v) = (0f32, 0f32);
            for dy in 0..2 {
                for dx in 0..2 {
                    let p = (cy * 2 + dy) * w + cx * 2 + dx;
                    let at = p * 4;
                    let (r, g, b) = (
                        surface.rgba[at] as f32 / 255.,
                        surface.rgba[at + 1] as f32 / 255.,
                        surface.rgba[at + 2] as f32 / 255.,
                    );
                    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    output[p] = (16. + 219. * y).round().clamp(1., 254.) as u8;
                    u += (b - y) / 1.8556;
                    v += (r - y) / 1.5748;
                }
            }
            let c = cy * (w / 2) + cx;
            output[pixels + c] = (128. + 224. * (u / 4.)).round().clamp(1., 254.) as u8;
            output[pixels + pixels / 4 + c] =
                (128. + 224. * (v / 4.)).round().clamp(1., 254.) as u8;
        }
    }
    Ok(output)
}
