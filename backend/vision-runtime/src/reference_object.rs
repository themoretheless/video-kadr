// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The probability picture: how much of each pixel is the subject.

use std::io::Cursor;

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
}

use crate::{frame::BYTES_PER_PIXEL, Frame};
const OBJECT_SHRINK: u32 = 2;
pub(super) fn planes(frame: &Frame, width: u32, height: u32) -> Vec<f32> {
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

struct Data(Vec<f32>);
impl From<Vec<f32>> for Data {
    fn from(value: Vec<f32>) -> Self {
        Self(value)
    }
}
struct Input<'a> {
    name: &'a str,
    dims: Vec<usize>,
    data: Data,
}
struct Output {
    data: Vec<f32>,
}
pub(super) struct Oracle {
    pub(super) values: Vec<f32>,
    pub(super) expected: Vec<f32>,
}
impl Oracle {
    fn run(&self, inputs: Vec<Input<'_>>, outputs: &[&str]) -> Result<Vec<Output>, String> {
        assert_eq!(outputs, ["output_image"]);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "input_image");
        assert_eq!(inputs[0].dims, [1, 3, 1024, 1024]);
        assert_eq!(inputs[0].data.0, self.expected);
        Ok(vec![Output {
            data: self.values.clone(),
        }])
    }
    pub(super) fn object_mask(
        &self,
        mut planes: Vec<f32>,
        width: u32,
        height: u32,
    ) -> Result<Mask, String> {
        // As the model was trained: the picture scaled by its brightest
        // value, then centred on a half.
        let brightest = planes.iter().copied().fold(0.0f32, f32::max).max(1e-6);
        for value in &mut planes {
            *value = *value / brightest - 0.5;
        }
        let size = (width * height) as usize;
        let input = Input {
            name: "input_image",
            dims: vec![1, 3, height as usize, width as usize],
            data: planes.into(),
        };
        let outputs = self.run(vec![input], &["output_image"])?;
        // The model answers in its own range; the span of the answer is
        // the mask, as the reference implementation reads it.
        let data = &outputs[0].data[..size.min(outputs[0].data.len())];
        let low = data.iter().copied().fold(f32::INFINITY, f32::min);
        let high = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (high - low).max(1e-6);
        let bytes: Vec<u8> = data
            .iter()
            .map(|&p| (((p - low) / span).clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
            .collect();
        let mask = Mask::from_bytes(width, height, bytes)
            .ok_or_else(|| "cutout: the model's answer is the wrong size".to_owned())?;
        Ok(mask.shrunk(OBJECT_SHRINK))
    }
}
