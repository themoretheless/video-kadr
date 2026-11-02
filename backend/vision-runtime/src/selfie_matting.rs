//! MediaPipe Selfie mask with pinned model and Concat nearest/probability contract.
use crate::Image;
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const MODEL_BYTES: u64 = 462352;
pub const MODEL_SHA256: &str = "3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad";
const EDGE: usize = 256;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}
impl Mask {
    pub fn is_blank(&self) -> bool {
        self.bytes.iter().all(|value| *value < 10)
    }
}
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "selfie segmentation cancelled"
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
        "invalid selfie image"
    );
    check(cancel)?;
    let mut tensor = vec![0.; 3 * EDGE * EDGE];
    for channel in 0..3 {
        for y in 0..EDGE {
            check(cancel)?;
            let sy = y * image.height / EDGE;
            for x in 0..EDGE {
                let sx = x * image.width / EDGE;
                let value = f32::from(image.rgba[(sy * image.width + sx) * 4 + channel]) / 255.;
                tensor[(channel * EDGE + y) * EDGE + x] = value;
            }
        }
    }
    Ok(tensor)
}
fn quantize(values: &[f32], cancel: &AtomicBool) -> Result<Mask> {
    ensure!(
        values.len() == EDGE * EDGE && values.iter().all(|v| v.is_finite()),
        "invalid selfie model values"
    );
    check(cancel)?;
    let bytes = values
        .iter()
        .map(|p| (p.clamp(0., 1.) * 255. + 0.5) as u8)
        .collect();
    check(cancel)?;
    Ok(Mask {
        width: EDGE,
        height: EDGE,
        bytes,
    })
}

pub struct SelfieSegmenter {
    session: ort::session::Session,
    #[cfg(test)]
    last_values: Vec<f32>,
}
impl SelfieSegmenter {
    pub fn load(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() == MODEL_BYTES,
            "invalid Selfie model inventory"
        );
        let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
        std::fs::File::open(path)?
            .take(MODEL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == MODEL_BYTES
                && format!("{:x}", Sha256::digest(&bytes)) == MODEL_SHA256,
            "Selfie model checksum mismatch"
        );
        let session = ort::session::Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("configure selfie session: {e}"))?
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
        let output = self.session.run(ort::inputs!["pixel_values"=>input])?;
        check(cancel)?;
        let (shape, values) = output["alphas"].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, 1, EDGE as i64, EDGE as i64],
            "unexpected Selfie tensor shape"
        );
        #[cfg(test)]
        {
            self.last_values = values.to_vec();
        }
        quantize(values, cancel)
    }
}

#[cfg(test)]
#[path = "reference_selfie.rs"]
mod reference;
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
    fn source_preprocessing_and_probability_bytes_match_original() {
        let image = image();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            prepare(&image, &cancel).unwrap(),
            reference::planes(&crate::Frame(image), 256, 256)
        );
        let values: Vec<f32> = (0..256 * 256)
            .map(|n| (n % 300) as f32 / 255. - 0.05)
            .collect();
        assert_eq!(
            quantize(&values, &cancel).unwrap().bytes,
            reference::probabilities(&values, 256 * 256, 256, 256)
                .unwrap()
                .bytes()
        );
        let mut bad = values;
        bad[0] = f32::NAN;
        assert!(quantize(&bad, &cancel).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(prepare(&self::image(), &cancel).is_err());
        assert!(quantize(&vec![0.; 256 * 256], &cancel).is_err());
    }
    #[test]
    #[ignore = "requires pinned Selfie model and ORT"]
    fn actual_selfie_matches_original_pre_postprocessing() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let mut segmenter = SelfieSegmenter::load(Path::new(
            &std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL").unwrap(),
        ))
        .unwrap();
        let image = image();
        let cancel = AtomicBool::new(false);
        let mask = segmenter.mask(&image, &cancel).unwrap();
        assert_eq!((mask.width, mask.height), (256, 256));
        assert_eq!(
            mask.bytes,
            reference::probabilities(&segmenter.last_values, 256 * 256, 256, 256)
                .unwrap()
                .bytes()
        );
        assert!(mask.bytes.iter().min() < mask.bytes.iter().max());
        cancel.store(true, Ordering::Relaxed);
        assert!(segmenter.mask(&image, &cancel).is_err());
    }
}
