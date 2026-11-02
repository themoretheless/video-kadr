// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// Original pinned segment.rs helpers; only imports/visibility adapted.
use crate::object_matting::reference::Mask;
use crate::Frame;
const BYTES_PER_PIXEL: usize = 4;
pub fn planes(frame: &Frame, width: u32, height: u32) -> Vec<f32> {
    let (fw, fh) = (frame.width() as usize, frame.height() as usize);
    let (width, height) = (width as usize, height as usize);
    let pixels = frame.pixels();
    let mut data = vec![0f32; 3 * width * height];
    for c in 0..3 {
        for y in 0..height {
            let sy = (y * fh / height).min(fh - 1);
            for x in 0..width {
                let sx = (x * fw / width).min(fw - 1);
                data[(c * height + y) * width + x] =
                    f32::from(pixels[(sy * fw + sx) * BYTES_PER_PIXEL + c]) / 255.0;
            }
        }
    }
    data
}

/// A model's `0..=1` answer as a mask.
pub fn probabilities(data: &[f32], size: usize, width: u32, height: u32) -> Result<Mask, String> {
    let bytes: Vec<u8> = data
        .iter()
        .take(size)
        .map(|&p| (p.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
        .collect();
    Mask::from_bytes(width, height, bytes)
        .ok_or_else(|| "cutout: the model's answer is the wrong size".to_owned())
}
