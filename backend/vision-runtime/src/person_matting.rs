//! Ordered recurrent RVM person matting. One instance belongs to one source run.
use crate::{object_matting::Mask, Image};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const MODEL_BYTES: u64 = 14_975_696;
pub const MODEL_SHA256: &str = "88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828";
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "person matting cancelled");
    Ok(())
}
pub fn working_size(width: usize, height: usize) -> Result<(usize, usize)> {
    ensure!(
        width > 0
            && height > 0
            && width
                .checked_mul(height)
                .is_some_and(|n| n <= 64 * 1024 * 1024),
        "invalid person source dimensions"
    );
    let scale = 512. / width.max(height) as f64;
    let round = |edge: usize| (((edge as f64 * scale / 16.).round() as usize) * 16).max(64);
    Ok((round(width), round(height)))
}
fn planes(image: &Image, width: usize, height: usize, cancel: &AtomicBool) -> Result<Vec<f32>> {
    ensure!(
        image
            .width
            .checked_mul(image.height)
            .and_then(|n| n.checked_mul(4))
            == Some(image.rgba.len()),
        "invalid person source pixels"
    );
    check(cancel)?;
    let mut values = vec![0.; 3 * width * height];
    for channel in 0..3 {
        for y in 0..height {
            check(cancel)?;
            for x in 0..width {
                let source = (y * image.height / height * image.width + x * image.width / width)
                    * 4
                    + channel;
                values[(channel * height + y) * width + x] = f32::from(image.rgba[source]) / 255.;
            }
        }
    }
    Ok(values)
}
#[derive(Clone)]
struct State {
    dims: Vec<usize>,
    values: Vec<f32>,
}
pub struct PersonSegmenter {
    #[cfg(test)]
    last_alpha: Vec<f32>,
    #[cfg(test)]
    entered: Option<std::sync::Arc<AtomicBool>>,
    session: ort::session::Session,
    state: Option<[State; 4]>,
    source: Option<(usize, usize)>,
    next: u64,
}
impl PersonSegmenter {
    pub fn load(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.file_type().is_file() && meta.len() == MODEL_BYTES,
            "invalid RVM model inventory"
        );
        let mut bytes = Vec::with_capacity(MODEL_BYTES as usize);
        std::fs::File::open(path)?
            .take(MODEL_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == MODEL_BYTES
                && format!("{:x}", Sha256::digest(&bytes)) == MODEL_SHA256,
            "RVM model checksum mismatch"
        );
        let session = ort::session::Session::builder()?
            .with_intra_threads(2)
            .map_err(|e| anyhow::anyhow!("configure person session: {e}"))?
            .commit_from_memory(&bytes)?;
        Ok(Self {
            session,
            #[cfg(test)]
            last_alpha: Vec::new(),
            #[cfg(test)]
            entered: None,
            state: None,
            source: None,
            next: 0,
        })
    }
    pub fn begin(&mut self) {
        self.state = None;
        self.source = None;
        self.next = 0;
    }
    pub fn next_ordinal(&self) -> u64 {
        self.next
    }
    /// Refused/cancelled frames do not advance the source clock or recurrent state.
    pub fn mask(&mut self, image: &Image, ordinal: u64, cancel: &AtomicBool) -> Result<Mask> {
        check(cancel)?;
        ensure!(ordinal == self.next, "out-of-order person frame");
        ensure!(
            self.source.is_none_or(|s| s == (image.width, image.height)),
            "person source dimensions changed; begin a new run"
        );
        let next = self
            .next
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("person ordinal overflow"))?;
        let (w, h) = working_size(image.width, image.height)?;
        let values = planes(image, w, h, cancel)?;
        let src = ort::value::Tensor::from_array(([1, 3, h, w], values.into_boxed_slice()))?;
        let mut input = Vec::with_capacity(4);
        for index in 0..4 {
            let state = self
                .state
                .as_ref()
                .map(|s| s[index].clone())
                .unwrap_or_else(|| State {
                    dims: vec![1, 1, 1, 1],
                    values: vec![0.],
                });
            input.push(ort::value::Tensor::from_array((
                state.dims,
                state.values.into_boxed_slice(),
            ))?);
        }
        let ratio = ort::value::Tensor::from_array(([1], vec![1f32].into_boxed_slice()))?;
        check(cancel)?;
        #[cfg(test)]
        if let Some(signal) = &self.entered {
            signal.store(true, Ordering::SeqCst);
        }
        let outputs=self.session.run(ort::inputs!["src"=>src,"r1i"=>input.remove(0),"r2i"=>input.remove(0),"r3i"=>input.remove(0),"r4i"=>input.remove(0),"downsample_ratio"=>ratio])?;
        check(cancel)?;
        let (shape, alpha) = outputs["pha"].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, 1, h as i64, w as i64]
                && alpha.len() == w * h
                && alpha.iter().all(|v| v.is_finite()),
            "invalid RVM alpha tensor"
        );
        let mut states = Vec::with_capacity(4);
        for (index, name) in ["r1o", "r2o", "r3o", "r4o"].iter().enumerate() {
            let (shape, data) = outputs[*name].try_extract_tensor::<f32>()?;
            let divisor = 1usize << (index + 1);
            ensure!(
                shape.as_ref()
                    == [
                        1,
                        [16, 20, 40, 64][index],
                        (h / divisor) as i64,
                        (w / divisor) as i64
                    ]
                    && data.len() <= 2 * 1024 * 1024
                    && data.iter().all(|v| v.is_finite()),
                "invalid RVM recurrent tensor"
            );
            states.push(State {
                dims: shape.iter().map(|v| *v as usize).collect(),
                values: data.to_vec(),
            });
        }
        let mask = Mask {
            width: w,
            height: h,
            bytes: alpha
                .iter()
                .map(|v| (v.clamp(0., 1.) * 255. + 0.5) as u8)
                .collect(),
        };
        check(cancel)?;
        #[cfg(test)]
        {
            self.last_alpha = alpha.to_vec();
        }
        self.state = Some(
            states
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid RVM state count"))?,
        );
        self.source = Some((image.width, image.height));
        self.next = next;
        Ok(mask)
    }
}
#[cfg(test)]
#[path = "reference_person.rs"]
mod reference;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes_and_planes_match_source_contract() {
        assert_eq!(working_size(37, 23).unwrap(), (512, 320));
        assert_eq!(working_size(23, 37).unwrap(), (320, 512));
        assert_eq!(working_size(1, 10000).unwrap(), (64, 512));
        assert!(working_size(0, 1).is_err());
        let image = Image::new(3, 2, vec![127; 24]).unwrap();
        assert_eq!(
            planes(&image, 512, 336, &AtomicBool::new(false)).unwrap(),
            reference::planes(&crate::Frame(image), 512, 336)
        );
    }
    #[test]
    #[ignore = "requires pinned RVM model and ORT"]
    fn actual_ordered_recurrence_matches_original() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let path = std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap();
        let mut engine = PersonSegmenter::load(Path::new(&path)).unwrap();
        let original = reference::Person::load(Path::new(&path)).unwrap();
        let cancel = AtomicBool::new(false);
        let images: Vec<_> = (0..4)
            .map(|t| {
                Image::new(
                    37,
                    23,
                    (0..37 * 23)
                        .flat_map(|n| {
                            [
                                ((n + t * 13) % 151) as u8,
                                ((n * 17 + t * 7) % 180) as u8,
                                ((n * 31 + t * 19) % 199) as u8,
                                (n % 256) as u8,
                            ]
                        })
                        .collect(),
                )
                .unwrap()
            })
            .collect();
        let mut original_masks = Vec::new();
        let mut original_rgba = Vec::new();
        let mut first = None;
        let mut previous_state: Option<Vec<u32>> = None;
        for (index, image) in images.iter().enumerate() {
            assert!(engine.mask(image, index as u64 + 1, &cancel).is_err());
            assert_eq!(engine.next_ordinal(), index as u64);
            let mask = engine.mask(image, index as u64, &cancel).unwrap();
            let expected = original
                .mask(&crate::Frame(image.clone()), 512, 320)
                .unwrap();
            assert_eq!(mask.bytes, expected.bytes());
            original_masks.extend_from_slice(expected.bytes());
            let mut frame = crate::Frame(image.clone());
            if !expected.is_blank() {
                crate::mask_apply::reference::cut(
                    &mut frame,
                    &expected,
                    &crate::mask_apply::reference::Mapping::IDENTITY,
                );
            }
            original_rgba.extend_from_slice(frame.pixels());
            assert_eq!(
                engine
                    .last_alpha
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                original
                    .alpha()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
            let actual: Vec<_> = engine
                .state
                .as_ref()
                .unwrap()
                .iter()
                .map(|s| {
                    (
                        s.dims.clone(),
                        s.values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    )
                })
                .collect();
            assert_eq!(actual, original.state_bits());
            let bits: Vec<_> = actual.iter().flat_map(|(_, v)| v.iter().copied()).collect();
            assert!(bits.iter().any(|v| *v != 0));
            if let Some(previous) = &previous_state {
                assert_ne!(previous, &bits);
            }
            previous_state = Some(bits);

            assert_eq!((mask.width, mask.height), (512, 320));
            if index == 0 {
                first = Some(mask.bytes.clone());
            }
            assert!(engine.mask(image, index as u64, &cancel).is_err());
        }
        if let Some(root) = std::env::var_os("VIDEO_KADR_TEST_PERSON_ORACLE_DIR") {
            let root = std::path::PathBuf::from(root);
            assert!(root.is_absolute() && root.is_dir());
            std::fs::write(
                root.join("input.rgba"),
                images
                    .iter()
                    .flat_map(|image| image.rgba.iter().copied())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            std::fs::write(root.join("reference.masks"), &original_masks).unwrap();
            std::fs::write(root.join("reference.rgba"), &original_rgba).unwrap();
            std::fs::write(root.join("reference.json"),serde_json::to_vec(&serde_json::json!({"width":37,"height":23,"frames":4,"maskWidth":512,"maskHeight":320,"maskHash":format!("{:x}",Sha256::digest(&original_masks)),"outputHash":format!("{:x}",Sha256::digest(&original_rgba))})).unwrap()).unwrap();
        }
        let prior = engine.next_ordinal();
        cancel.store(true, Ordering::Relaxed);
        assert!(engine.mask(&images[0], prior, &cancel).is_err());
        assert_eq!(engine.next_ordinal(), prior);
        assert_eq!(
            &engine
                .state
                .as_ref()
                .unwrap()
                .iter()
                .flat_map(|s| s.values.iter().map(|v| v.to_bits()))
                .collect::<Vec<_>>(),
            previous_state.as_ref().unwrap()
        );
        cancel.store(false, Ordering::Relaxed);
        assert!(engine
            .mask(
                &Image::new(23, 37, vec![0; 23 * 37 * 4]).unwrap(),
                prior,
                &cancel
            )
            .is_err());
        assert_eq!(engine.next_ordinal(), prior);
        assert_eq!(
            &engine
                .state
                .as_ref()
                .unwrap()
                .iter()
                .flat_map(|s| s.values.iter().map(|v| v.to_bits()))
                .collect::<Vec<_>>(),
            previous_state.as_ref().unwrap()
        );
        let entered = std::sync::Arc::new(AtomicBool::new(false));
        let signal = std::sync::Arc::new(AtomicBool::new(false));
        engine.entered = Some(entered.clone());
        let trigger = signal.clone();
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !entered.load(Ordering::SeqCst) {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            trigger.store(true, Ordering::SeqCst);
        });
        assert!(engine.mask(&images[0], prior, &signal).is_err());
        worker.join().unwrap();
        engine.entered = None;
        assert_eq!(engine.next_ordinal(), prior);
        assert_eq!(
            &engine
                .state
                .as_ref()
                .unwrap()
                .iter()
                .flat_map(|s| s.values.iter().map(|v| v.to_bits()))
                .collect::<Vec<_>>(),
            previous_state.as_ref().unwrap()
        );
        signal.store(false, Ordering::SeqCst);
        assert_eq!(
            engine.mask(&images[0], prior, &signal).unwrap().bytes,
            original
                .mask(&crate::Frame(images[0].clone()), 512, 320)
                .unwrap()
                .bytes()
        );
        engine.begin();
        original.begin();
        assert_eq!(engine.next_ordinal(), 0);
        assert_eq!(
            engine.mask(&images[0], 0, &cancel).unwrap().bytes,
            first.unwrap()
        );
        assert_eq!(engine.mask(&images[1], 1, &cancel).unwrap().bytes, {
            original
                .mask(&crate::Frame(images[0].clone()), 512, 320)
                .unwrap();
            original
                .mask(&crate::Frame(images[1].clone()), 512, 320)
                .unwrap()
                .bytes()
                .to_vec()
        });
    }
}
