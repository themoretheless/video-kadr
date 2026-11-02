//! Native Real-ESRGAN restoration with Concat's padded tile/byte averaging contract.
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[cfg(test)]
extern crate self as concat_core;
#[cfg(test)]
pub mod frame {
    pub use super::Frame;
    pub const BYTES_PER_PIXEL: usize = 4;
}
#[cfg(test)]
#[derive(Clone)]
pub struct Frame(Image);
#[cfg(test)]
impl Frame {
    pub fn from_rgba(w: u32, h: u32, p: Vec<u8>) -> std::result::Result<Self, String> {
        Image::new(w as usize, h as usize, p)
            .map(Self)
            .map_err(|e| e.to_string())
    }
    pub fn width(&self) -> u32 {
        self.0.width as u32
    }
    pub fn height(&self) -> u32 {
        self.0.height as u32
    }
    pub fn pixels(&self) -> &[u8] {
        &self.0.rgba
    }
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.0.rgba
    }
    pub fn black(w: u32, h: u32) -> Self {
        Self(Image::new(w as usize, h as usize, vec![0; w as usize * h as usize * 4]).unwrap())
    }
}
#[cfg(test)]
#[allow(dead_code)]
mod reference_enhance;
const CORE: usize = 256;
const PAD: usize = 16;
const WINDOW: usize = CORE + 2 * PAD;
const MODEL_SIDE: usize = WINDOW * 4;
pub const MODEL_SHA256: &str = "1940a93ee08283a0a7286183186357b1688fe9fa8ede74604b424586aaddf112";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
impl Image {
    pub fn new(width: usize, height: usize, rgba: Vec<u8>) -> Result<Self> {
        ensure!(
            width > 0
                && height > 0
                && width
                    .checked_mul(height)
                    .is_some_and(|n| n <= 64 * 1024 * 1024)
                && width * height * 4 == rgba.len(),
            "invalid bounded image"
        );
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}
pub fn factor_for(width: usize, height: usize) -> usize {
    if width.max(height) > 2048 {
        1
    } else {
        2
    }
}
#[derive(Clone, Copy)]
struct Tile {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}
fn window(image: &Image, tile: Tile) -> Vec<f32> {
    let mut result = vec![0.; 3 * WINDOW * WINDOW];
    for y in 0..WINDOW {
        for x in 0..WINDOW {
            let sx = (tile.x + x).saturating_sub(PAD).min(image.width - 1);
            let sy = (tile.y + y).saturating_sub(PAD).min(image.height - 1);
            for c in 0..3 {
                result[(c * WINDOW + y) * WINDOW + x] =
                    f32::from(image.rgba[(sy * image.width + sx) * 4 + c]) / 255.;
            }
        }
    }
    result
}
fn paste(output: &mut Image, tile: Tile, answer: &[f32], factor: usize) -> Result<()> {
    ensure!(
        answer.len() == 3 * MODEL_SIDE * MODEL_SIDE && answer.iter().all(|v| v.is_finite()),
        "invalid model output"
    );
    let by = 4 / factor;
    let count = (by * by) as u32;
    for y in 0..tile.height * factor {
        for x in 0..tile.width * factor {
            let at = ((tile.y * factor + y) * output.width + tile.x * factor + x) * 4;
            for c in 0..3 {
                let mut sum = 0u32;
                for dy in 0..by {
                    for dx in 0..by {
                        let sy = PAD * 4 + y * by + dy;
                        let sx = PAD * 4 + x * by + dx;
                        let byte = (answer[(c * MODEL_SIDE + sy) * MODEL_SIDE + sx].clamp(0., 1.)
                            * 255.
                            + 0.5) as u8;
                        sum += u32::from(byte);
                    }
                }
                output.rgba[at + c] = ((sum + count / 2) / count) as u8;
            }
            output.rgba[at + 3] = 255;
        }
    }
    Ok(())
}
pub struct Enhancer {
    session: ort::session::Session,
}
impl Enhancer {
    pub fn load(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() == 4866417,
            "invalid enhancement model file"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(4866418)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == 4866417 && format!("{:x}", Sha256::digest(&bytes)) == MODEL_SHA256,
            "enhancement model digest mismatch"
        );
        Ok(Self {
            session: ort::session::Session::builder()?
                .with_intra_threads(2)
                .map_err(|error| anyhow::anyhow!("configure enhancement session: {error}"))?
                .commit_from_file(path)?,
        })
    }
    pub fn enhance(
        &mut self,
        image: &Image,
        factor: usize,
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Image> {
        ensure!(
            [1, 2, 4].contains(&factor),
            "enhancement factor must be 1, 2 or 4"
        );
        // Image fields are public for transport, so revalidate at the boundary.
        ensure!(
            image.width > 0
                && image.height > 0
                && image
                    .width
                    .checked_mul(image.height)
                    .is_some_and(|n| n <= 64 * 1024 * 1024)
                && image.rgba.len() == image.width * image.height * 4,
            "invalid input image"
        );
        let width = image
            .width
            .checked_mul(factor)
            .ok_or_else(|| anyhow::anyhow!("output overflow"))?;
        let height = image
            .height
            .checked_mul(factor)
            .ok_or_else(|| anyhow::anyhow!("output overflow"))?;
        ensure!(
            width
                .checked_mul(height)
                .is_some_and(|n| n <= 64 * 1024 * 1024),
            "output exceeds pixel limit"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "enhancement cancelled");
        let mut output = Image::new(width, height, vec![0; width * height * 4])?;
        let total = image.width.div_ceil(CORE) * image.height.div_ceil(CORE);
        let mut completed = 0;
        for y in (0..image.height).step_by(CORE) {
            for x in (0..image.width).step_by(CORE) {
                ensure!(!cancel.load(Ordering::Relaxed), "enhancement cancelled");
                let tile = Tile {
                    x,
                    y,
                    width: CORE.min(image.width - x),
                    height: CORE.min(image.height - y),
                };
                let input = ort::value::Tensor::from_array((
                    [1, 3, WINDOW, WINDOW],
                    window(image, tile).into_boxed_slice(),
                ))?;
                let answer = self.session.run(ort::inputs!["input"=>input])?;
                ensure!(!cancel.load(Ordering::Relaxed), "enhancement cancelled");
                let (shape, values) = answer["output"].try_extract_tensor::<f32>()?;
                ensure!(
                    shape.as_ref() == [1, 3, MODEL_SIDE as i64, MODEL_SIDE as i64],
                    "unexpected enhancement tensor shape"
                );
                paste(&mut output, tile, values, factor)?;
                completed += 1;
                progress(completed, total);
                ensure!(!cancel.load(Ordering::Relaxed), "enhancement cancelled");
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_concat_tile_and_downscale_oracle() {
        let w = 257usize;
        let h = 259usize;
        let pixels = (0..w * h * 4)
            .map(|n| (n * 17 % 256) as u8)
            .collect::<Vec<_>>();
        let image = Image::new(w, h, pixels.clone()).unwrap();
        let original = Frame::from_rgba(w as u32, h as u32, pixels).unwrap();
        let tiles = reference_enhance::tiles(w as u32, h as u32);
        for factor in [1, 2, 4] {
            let mut actual =
                Image::new(w * factor, h * factor, vec![0; w * h * factor * factor * 4]).unwrap();
            let mut large = Frame::black((w * 4) as u32, (h * 4) as u32);
            for source in &tiles {
                let tile = Tile {
                    x: source.x as usize,
                    y: source.y as usize,
                    width: source.width as usize,
                    height: source.height as usize,
                };
                assert_eq!(
                    window(&image, tile),
                    reference_enhance::window_planes(&original, *source)
                );
                let answer = (0..3 * MODEL_SIDE * MODEL_SIDE)
                    .map(|n| ((n * 19 % 509) as f32 - 100.) / 300.)
                    .collect::<Vec<_>>();
                reference_enhance::paste(&mut large, *source, &answer).unwrap();
                paste(&mut actual, tile, &answer, factor).unwrap();
            }
            assert_eq!(
                actual.rgba,
                reference_enhance::downscale(&large, (4 / factor) as u32).pixels()
            );
        }
    }
    #[test]
    fn edge_windows_and_byte_averaging() {
        let image = Image::new(2, 1, vec![0, 64, 255, 0, 255, 128, 0, 100]).unwrap();
        let tile = Tile {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        let planes = window(&image, tile);
        assert_eq!(planes[0], 0.);
        assert_eq!(planes[WINDOW - 1], 1.);
        assert_eq!(planes[2 * WINDOW * WINDOW], 1.);
        let mut answer = vec![0.; 3 * MODEL_SIDE * MODEL_SIDE];
        for c in 0..3 {
            for y in 0..MODEL_SIDE {
                for x in 0..MODEL_SIDE {
                    answer[(c * MODEL_SIDE + y) * MODEL_SIDE + x] = ((x + y + c) % 7) as f32 / 6.;
                }
            }
        }
        for factor in [1, 2, 4] {
            let mut output =
                Image::new(2 * factor, factor, vec![0; 2 * factor * factor * 4]).unwrap();
            paste(&mut output, tile, &answer, factor).unwrap();
            let by = 4 / factor;
            for y in 0..factor {
                for x in 0..2 * factor {
                    for c in 0..3 {
                        let mut bytes = vec![];
                        for dy in 0..by {
                            for dx in 0..by {
                                let value = answer[(c * MODEL_SIDE + 64 + y * by + dy)
                                    * MODEL_SIDE
                                    + 64
                                    + x * by
                                    + dx];
                                bytes.push((value * 255. + 0.5) as u8);
                            }
                        }
                        let expected = (bytes.iter().map(|v| u32::from(*v)).sum::<u32>()
                            + (bytes.len() / 2) as u32)
                            / bytes.len() as u32;
                        assert_eq!(output.rgba[(y * output.width + x) * 4 + c], expected as u8);
                    }
                }
            }
            assert!(output.rgba.chunks_exact(4).all(|p| p[3] == 255));
        }
        assert_eq!(factor_for(2048, 100), 2);
        assert_eq!(factor_for(2049, 100), 1);
    }
    #[test]
    #[ignore = "requires verified Real-ESRGAN model and ONNX Runtime"]
    fn actual_learned_restoration_and_cancel() {
        let mut engine = Enhancer::load(Path::new(
            &std::env::var("VIDEO_KADR_TEST_ENHANCE_MODEL").unwrap(),
        ))
        .unwrap();
        let mut rgba = vec![];
        for y in 0..9 {
            for x in 0..257 {
                rgba.extend_from_slice(&[
                    ((x * 13 + y * 7) % 256) as u8,
                    ((x * 9 + y * 17) % 256) as u8,
                    80,
                    255,
                ]);
            }
        }
        let image = Image::new(257, 9, rgba).unwrap();
        let mut progress = vec![];
        let output = engine
            .enhance(&image, 2, &AtomicBool::new(false), |a, b| {
                progress.push((a, b))
            })
            .unwrap();
        assert_eq!((output.width, output.height), (514, 18));
        assert_eq!(progress, vec![(1, 2), (2, 2)]);
        let original =
            Frame::from_rgba(image.width as u32, image.height as u32, image.rgba.clone()).unwrap();
        let mut large = Frame::black(original.width() * 4, original.height() * 4);
        for tile in reference_enhance::tiles(original.width(), original.height()) {
            let input = ort::value::Tensor::from_array((
                [1, 3, WINDOW, WINDOW],
                reference_enhance::window_planes(&original, tile).into_boxed_slice(),
            ))
            .unwrap();
            let answer = engine.session.run(ort::inputs!["input"=>input]).unwrap();
            let (_, values) = answer["output"].try_extract_tensor::<f32>().unwrap();
            reference_enhance::paste(&mut large, tile, values).unwrap();
        }
        assert_eq!(
            output.rgba,
            reference_enhance::downscale(&large, 2).pixels()
        );
        assert!(output.rgba.chunks_exact(4).enumerate().any(|(i, p)| p[..3]
            != image.rgba[((i / output.width / 2) * image.width + i % output.width / 2) * 4..]
                [..3]));
        assert!(output.rgba.chunks_exact(4).all(|p| p[3] == 255));
        let cancelled = AtomicBool::new(false);
        assert!(engine
            .enhance(&image, 1, &cancelled, |_, _| cancelled
                .store(true, Ordering::Relaxed))
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert!(engine
            .enhance(&image, 1, &AtomicBool::new(false), |_, _| {})
            .is_ok());
    }
}

pub mod object_matting;

pub mod mask_apply;

pub mod mask_edit;

#[cfg(test)]
#[allow(dead_code)]
mod reference_brush;
#[cfg(test)]
mod sam_reference_runtime;
pub mod slim_sam;

pub mod selfie_matting;

pub mod person_matting;

pub mod sam_tracking;

pub mod sam_reidentification;

pub mod sam_continuity;

pub mod sam_instance;
