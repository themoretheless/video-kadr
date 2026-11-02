//! IS-Net object mask with the original Concat nearest/normalization/byte-shrink contract.
use crate::Image;
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const MODEL_BYTES: u64 = 178648008;
pub const MODEL_SHA256: &str = "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a";
const EDGE: usize = 1024;
pub use video_kadr_mask_core::Mask;
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "object segmentation cancelled"
    );
    Ok(())
}
fn prepare(image: &Image, cancel: &AtomicBool) -> Result<Vec<f32>> {
    ensure!(
        image.width > 0
            && image.height > 0
            && image
                .width
                .checked_mul(image.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024)
            && image.rgba.len() == image.width * image.height * 4,
        "invalid object image"
    );
    check(cancel)?;
    let mut tensor = vec![0.; 3 * EDGE * EDGE];
    let mut brightest = 0f32;
    for channel in 0..3 {
        for y in 0..EDGE {
            check(cancel)?;
            let sy = y * image.height / EDGE;
            for x in 0..EDGE {
                let sx = x * image.width / EDGE;
                let value = f32::from(image.rgba[(sy * image.width + sx) * 4 + channel]) / 255.;
                tensor[(channel * EDGE + y) * EDGE + x] = value;
                brightest = brightest.max(value);
            }
        }
    }
    let scale = brightest.max(1e-6);
    for block in tensor.chunks_mut(65536) {
        check(cancel)?;
        for value in block {
            *value = *value / scale - 0.5;
        }
    }
    Ok(tensor)
}
fn quantize(values: &[f32], cancel: &AtomicBool) -> Result<Mask> {
    ensure!(
        values.len() == EDGE * EDGE && values.iter().all(|v| v.is_finite()),
        "invalid object model values"
    );
    check(cancel)?;
    let low = values.iter().copied().fold(f32::INFINITY, f32::min);
    let high = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (high - low).max(1e-6);
    ensure!(span.is_finite(), "invalid object model span");
    let mut bytes = Vec::with_capacity(EDGE * EDGE / 4);
    for y in 0..EDGE / 2 {
        check(cancel)?;
        for x in 0..EDGE / 2 {
            let mut sum = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let value = values[(2 * y + dy) * EDGE + 2 * x + dx];
                    sum += (((value - low) / span).clamp(0., 1.) * 255. + 0.5) as u32;
                }
            }
            bytes.push((sum / 4) as u8);
        }
    }
    check(cancel)?;
    Ok(Mask {
        width: EDGE / 2,
        height: EDGE / 2,
        bytes,
    })
}
pub struct ObjectSegmenter {
    session: ort::session::Session,
    #[cfg(test)]
    last_values: Vec<f32>,
}
impl ObjectSegmenter {
    pub fn load(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() == MODEL_BYTES,
            "invalid IS-Net model inventory"
        );
        let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
        std::fs::File::open(path)?
            .take(MODEL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == MODEL_BYTES
                && format!("{:x}", Sha256::digest(&bytes)) == MODEL_SHA256,
            "IS-Net model checksum mismatch"
        );
        let session = ort::session::Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("configure object session: {e}"))?
            .commit_from_memory(&bytes)?;
        Ok(Self {
            session,
            #[cfg(test)]
            last_values: vec![],
        })
    }
    pub fn mask(&mut self, image: &Image, cancel: &AtomicBool) -> Result<Mask> {
        let planes = prepare(image, cancel)?;
        check(cancel)?;
        let input =
            ort::value::Tensor::from_array(([1, 3, EDGE, EDGE], planes.into_boxed_slice()))?;
        let output = self.session.run(ort::inputs!["input_image"=>input])?;
        check(cancel)?;
        let (shape, values) = output["output_image"].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, 1, EDGE as i64, EDGE as i64],
            "unexpected IS-Net tensor shape"
        );
        #[cfg(test)]
        {
            self.last_values = values.to_vec();
        }
        quantize(values, cancel)
    }
}
#[cfg(test)]
#[allow(dead_code, unused_imports)]
#[path = "reference_object.rs"]
pub(crate) mod reference;
#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Image {
        Image::new(
            37,
            23,
            (0..37 * 23)
                .flat_map(|n| {
                    [
                        (n % 151) as u8,
                        ((n * 17) % 180) as u8,
                        ((n * 31) % 199) as u8,
                        (n % 256) as u8,
                    ]
                })
                .collect(),
        )
        .unwrap()
    }
    #[test]
    fn original_preprocess_postprocess_byte_oracle_and_refusal() {
        let cancel = AtomicBool::new(false);
        let image = image();
        let values: Vec<f32> = (0..EDGE * EDGE)
            .map(|n| ((n * 13) % 997) as f32 * 0.031 - 8.)
            .collect();
        let mask = quantize(&values, &cancel).unwrap();
        let oracle = reference::Oracle {
            values,
            expected: prepare(&image, &cancel).unwrap(),
        };
        let original = oracle
            .object_mask(
                reference::planes(&crate::Frame(image.clone()), 1024, 1024),
                1024,
                1024,
            )
            .unwrap();
        assert_eq!(mask.bytes, original.bytes());
        assert_eq!((mask.width, mask.height), (512, 512));
        assert!(quantize(&[f32::NAN], &cancel).is_err());
        let mut bad = vec![0.; EDGE * EDGE];
        bad[1] = f32::INFINITY;
        assert!(quantize(&bad, &cancel).is_err());
        assert!(quantize(&vec![3.; EDGE * EDGE], &cancel)
            .unwrap()
            .is_blank());
        cancel.store(true, Ordering::Relaxed);
        assert!(prepare(&image, &cancel).is_err());
        assert!(quantize(&vec![0.; EDGE * EDGE], &cancel).is_err());
        assert!(prepare(
            &Image {
                width: 0,
                height: 1,
                rgba: vec![]
            },
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    #[ignore = "requires pinned public IS-Net and ONNX Runtime"]
    fn actual_learned_object_mask_matches_original_postprocessing() {
        let path = std::env::var("VIDEO_KADR_TEST_OBJECT_MODEL").unwrap();
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut segmenter = ObjectSegmenter::load(Path::new(&path)).unwrap();
        let image = image();
        let cancel = AtomicBool::new(false);
        let mask = segmenter.mask(&image, &cancel).unwrap();
        let oracle = reference::Oracle {
            values: segmenter.last_values.clone(),
            expected: prepare(&image, &cancel).unwrap(),
        };
        let original = oracle
            .object_mask(
                reference::planes(&crate::Frame(image.clone()), 1024, 1024),
                1024,
                1024,
            )
            .unwrap();
        assert_eq!(mask.bytes, original.bytes());
        assert_eq!((mask.width, mask.height), (512, 512));
        assert!(mask.bytes.iter().min() < mask.bytes.iter().max());
        cancel.store(true, Ordering::Relaxed);
        assert!(segmenter.mask(&image, &cancel).is_err());
    }
}
