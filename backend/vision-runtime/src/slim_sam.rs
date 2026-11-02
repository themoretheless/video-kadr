//! Pinned SlimSAM source-coordinate regions; reusable embeddings per image.
use crate::{object_matting::Mask, Image};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const ENCODER_BYTES: u64 = 23_276_014;
pub const DECODER_BYTES: u64 = 16_557_892;
pub const ENCODER_SHA256: &str = "9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0";
pub const DECODER_SHA256: &str = "f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb";
const EDGE: usize = 1024;
const MASK_EDGE: usize = 256;
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "SAM cancelled");
    Ok(())
}
pub fn input_size(width: usize, height: usize) -> Result<[usize; 2]> {
    ensure!(width > 0 && height > 0, "empty SAM image");
    let scale = EDGE as f64 / width.max(height) as f64;
    Ok([
        ((width as f64 * scale).round() as usize).clamp(1, EDGE),
        ((height as f64 * scale).round() as usize).clamp(1, EDGE),
    ])
}
fn prepare(image: &Image, cancel: &AtomicBool) -> Result<([usize; 2], Vec<f32>)> {
    ensure!(
        image
            .width
            .checked_mul(image.height)
            .is_some_and(|n| n <= 64 * 1024 * 1024 && image.rgba.len() == n * 4),
        "invalid SAM image"
    );
    let size = input_size(image.width, image.height)?;
    check(cancel)?;
    let mut data = vec![0.; 3 * EDGE * EDGE];
    let mean = [0.485, 0.456, 0.406];
    let deviation = [0.229, 0.224, 0.225];
    for c in 0..3 {
        for y in 0..size[1] {
            check(cancel)?;
            for x in 0..size[0] {
                let i = ((y * image.height / size[1]) * image.width + x * image.width / size[0])
                    * 4
                    + c;
                data[(c * EDGE + y) * EDGE + x] =
                    (f32::from(image.rgba[i]) / 255. - mean[c]) / deviation[c];
            }
        }
    }
    Ok((size, data))
}
fn session(path: &Path, bytes: u64, hash: &str) -> Result<ort::session::Session> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() == bytes,
        "invalid SAM model inventory"
    );
    let mut data = Vec::with_capacity(bytes as usize);
    std::fs::File::open(path)?
        .take(bytes + 1)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 == bytes && format!("{:x}", Sha256::digest(&data)) == hash,
        "SAM model checksum mismatch"
    );
    Ok(ort::session::Session::builder()?
        .with_intra_threads(2)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .commit_from_memory(&data)?)
}
/// Embedding bytes belong to this exact decoded image and may be reused for strokes.
pub struct Embedding {
    pub(crate) image: Vec<f32>,
    pub(crate) position: Vec<f32>,
    pub(crate) scaled: [usize; 2],
}
pub struct Brush {
    encoder: ort::session::Session,
    decoder: ort::session::Session,
}
impl Brush {
    pub fn load(encoder: &Path, decoder: &Path) -> Result<Self> {
        Ok(Self {
            encoder: session(encoder, ENCODER_BYTES, ENCODER_SHA256)?,
            decoder: session(decoder, DECODER_BYTES, DECODER_SHA256)?,
        })
    }
    pub fn embed(&mut self, image: &Image, cancel: &AtomicBool) -> Result<Embedding> {
        let (scaled, data) = prepare(image, cancel)?;
        check(cancel)?;
        let tensor = ort::value::Tensor::from_array(([1, 3, EDGE, EDGE], data.into_boxed_slice()))?;
        let outputs = self.encoder.run(ort::inputs!["pixel_values"=>tensor])?;
        check(cancel)?;
        let extract = |name: &str| -> Result<Vec<f32>> {
            let (shape, values) = outputs[name].try_extract_tensor::<f32>()?;
            ensure!(
                shape.as_ref() == [1, 256, 64, 64] && values.iter().all(|v| v.is_finite()),
                "invalid SAM embeddings"
            );
            Ok(values.to_vec())
        };
        Ok(Embedding {
            image: extract("image_embeddings")?,
            position: extract("image_positional_embeddings")?,
            scaled,
        })
    }
    pub fn region(
        &mut self,
        embedding: &Embedding,
        points: &[[f64; 2]],
        cancel: &AtomicBool,
    ) -> Result<Mask> {
        self.candidates(embedding, points, cancel)?
            .into_iter()
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, mask)| mask)
            .ok_or_else(|| anyhow::anyhow!("SAM answered no candidates"))
    }
    /// Global learned-feature identity matching, including after an arbitrary
    /// occlusion gap. The source-owned identity is never adapted to a new target.
    pub fn reidentify(
        &mut self,
        embedding: &Embedding,
        image: &Image,
        identity: &crate::sam_reidentification::Identity,
        previous: Option<&Mask>,
        cancel: &AtomicBool,
    ) -> Result<(crate::sam_reidentification::State, Option<Mask>)> {
        self.reidentify_inner(embedding, image, identity, previous, None, cancel)
    }
    /// Visibility continuity is measured from actual adjacent source RGB frames.
    /// No prior frame means global recovery, with no continuity exception.
    pub fn reidentify_visible(
        &mut self,
        embedding: &Embedding,
        image: &Image,
        identity: &crate::sam_reidentification::Identity,
        previous: Option<(&Image, &Mask)>,
        cancel: &AtomicBool,
    ) -> Result<(crate::sam_reidentification::State, Option<Mask>)> {
        let flow = if let Some((old, mask)) = previous {
            crate::sam_continuity::Continuity::measure(old, image, mask, cancel)?
        } else {
            None
        };
        #[cfg(test)]
        eprintln!(
            "visible continuity measured matches={}",
            flow.as_ref().map_or(0, |f| f.matches.len())
        );
        self.reidentify_inner(
            embedding,
            image,
            identity,
            previous.map(|(_, mask)| mask),
            flow.as_ref(),
            cancel,
        )
    }
    fn reidentify_inner(
        &mut self,
        embedding: &Embedding,
        image: &Image,
        identity: &crate::sam_reidentification::Identity,
        previous: Option<&Mask>,
        continuity: Option<&crate::sam_continuity::Continuity>,
        cancel: &AtomicBool,
    ) -> Result<(crate::sam_reidentification::State, Option<Mask>)> {
        let mut scored = vec![];
        for mask in self.identity_candidates(embedding, image, identity, previous, cancel)? {
            if let Some(score) = identity.similarity(embedding, image, &mask, cancel)? {
                if score >= 0.72
                    && (identity.accepts_instance(image, &mask, cancel)?
                        || continuity.is_some_and(|flow| {
                            flow.support(&mask) * flow.matches.len() as f32 >= 1.99
                        }) && continuity.unwrap().overlap(&mask, cancel)? >= 0.2)
                {
                    scored.push((score, mask));
                }
            }
        }
        // Two longitude cuts are evaluated together, rather than resolving an
        // ambiguous identity by selecting whichever cut happens to prefer it.
        if image.width >= 2 {
            let shift = image.width / 2;
            let mut rgba = vec![0; image.rgba.len()];
            for y in 0..image.height {
                check(cancel)?;
                for x in 0..image.width {
                    let src = (y * image.width + (x + shift) % image.width) * 4;
                    let out = (y * image.width + x) * 4;
                    rgba[out..out + 4].copy_from_slice(&image.rgba[src..src + 4]);
                }
            }
            let rolled = Image::new(image.width, image.height, rgba)?;
            let embedding = self.embed(&rolled, cancel)?;
            for mask in self.identity_candidates(&embedding, &rolled, identity, None, cancel)? {
                if let Some(score) = identity.similarity(&embedding, &rolled, &mask, cancel)? {
                    if score < 0.72 {
                        continue;
                    }
                    let witnessed = identity.accepts_instance(&rolled, &mask, cancel)?;
                    let mut bytes = vec![0; mask.bytes.len()];
                    for y in 0..mask.height {
                        for x in 0..mask.width {
                            let u = ((x as f32 + 0.5) / mask.width as f32
                                - shift as f32 / image.width as f32)
                                .rem_euclid(1.);
                            let v = (y as f32 + 0.5) / mask.height as f32;
                            bytes[y * mask.width + x] =
                                (crate::mask_apply::sample(&mask, u, v) * 255. + 0.5)
                                    .clamp(0., 255.) as u8;
                        }
                    }
                    let source_mask = Mask { bytes, ..mask };
                    if witnessed
                        || continuity.is_some_and(|flow| {
                            flow.support(&source_mask) * flow.matches.len() as f32 >= 1.99
                        }) && continuity.unwrap().overlap(&source_mask, cancel)? >= 0.2
                    {
                        scored.push((score, source_mask));
                    }
                }
            }
        }
        let (mut state, mask) = crate::sam_reidentification::select_scored_visible(
            identity, previous, continuity, scored, cancel,
        )?;
        if let Some(current) = &mask {
            let source_owned = if let Some(flow) = continuity {
                flow.support(current) >= 0.75 && flow.overlap(current, cancel)? >= 0.45
            } else {
                false
            };
            if !source_owned && !identity.accepts_instance(image, current, cancel)? {
                return Ok((crate::sam_reidentification::State::Ambiguous, None));
            }
        }
        if let (Some(flow), Some(current)) = (continuity, mask.as_ref()) {
            if flow.support(current) >= 0.75 && flow.overlap(current, cancel)? >= 0.45 {
                state = crate::sam_reidentification::State::Tracked;
            }
        }
        Ok((state, mask))
    }
    fn identity_candidates(
        &mut self,
        embedding: &Embedding,
        image: &Image,
        identity: &crate::sam_reidentification::Identity,
        previous: Option<&Mask>,
        cancel: &AtomicBool,
    ) -> Result<Vec<Mask>> {
        let mut masks = vec![];
        if let Some(previous) = previous {
            let points = crate::sam_tracking::interior_points(previous, cancel)?;
            if !points.is_empty() {
                masks.extend(
                    self.candidates(embedding, &points, cancel)?
                        .into_iter()
                        .map(|(_, m)| m),
                );
            }
        }
        for point in identity.proposals(embedding, image, cancel)? {
            masks.extend(
                self.candidates(embedding, &[point], cancel)?
                    .into_iter()
                    .map(|(_, m)| m),
            );
        }
        Ok(masks)
    }
    /// Follow a prior source-coordinate region; loss is explicit, never a guessed mask.
    pub fn track(
        &mut self,
        embedding: &Embedding,
        previous: &Mask,
        cancel: &AtomicBool,
    ) -> Result<Option<Mask>> {
        let points = crate::sam_tracking::interior_points(previous, cancel)?;
        if points.is_empty() {
            return Ok(None);
        }
        let candidates = self.candidates(embedding, &points, cancel)?;
        crate::sam_tracking::select(previous, candidates, cancel)
    }
    pub(crate) fn candidates(
        &mut self,
        embedding: &Embedding,
        points: &[[f64; 2]],
        cancel: &AtomicBool,
    ) -> Result<Vec<(f32, Mask)>> {
        ensure!(
            !points.is_empty()
                && points.len() <= 65536
                && points.iter().flatten().all(|v| v.is_finite()),
            "invalid SAM prompts"
        );
        check(cancel)?;
        let count = points.len().min(16);
        let coords: Vec<f32> = (0..count)
            .flat_map(|i| {
                let p = points[if points.len() <= 16 {
                    i
                } else {
                    i * (points.len() - 1) / 15
                }];
                [
                    (p[0] as f32).clamp(0., 1.) * embedding.scaled[0] as f32,
                    (p[1] as f32).clamp(0., 1.) * embedding.scaled[1] as f32,
                ]
            })
            .collect();
        let coords = ort::value::Tensor::from_array(([1, 1, count, 2], coords.into_boxed_slice()))?;
        let labels =
            ort::value::Tensor::from_array(([1, 1, count], vec![1i64; count].into_boxed_slice()))?;
        let image = ort::value::Tensor::from_array((
            [1, 256, 64, 64],
            embedding.image.clone().into_boxed_slice(),
        ))?;
        let position = ort::value::Tensor::from_array((
            [1, 256, 64, 64],
            embedding.position.clone().into_boxed_slice(),
        ))?;
        let outputs=self.decoder.run(ort::inputs!["input_points"=>coords,"input_labels"=>labels,"image_embeddings"=>image,"image_positional_embeddings"=>position])?;
        check(cancel)?;
        let (score_shape, scores) = outputs["iou_scores"].try_extract_tensor::<f32>()?;
        let (mask_shape, masks) = outputs["pred_masks"].try_extract_tensor::<f32>()?;
        ensure!(
            score_shape.as_ref() == [1, 1, 3]
                && mask_shape.as_ref() == [1, 1, 3, 256, 256]
                && scores.iter().chain(masks.iter()).all(|v| v.is_finite()),
            "invalid SAM decoder output"
        );
        (0..3)
            .map(|index| {
                Ok((
                    scores[index],
                    restore(
                        &masks[index * MASK_EDGE * MASK_EDGE..(index + 1) * MASK_EDGE * MASK_EDGE],
                        embedding.scaled,
                        cancel,
                    )?,
                ))
            })
            .collect()
    }
}
fn restore(logits: &[f32], scaled: [usize; 2], cancel: &AtomicBool) -> Result<Mask> {
    ensure!(
        logits.len() == MASK_EDGE * MASK_EDGE
            && logits.iter().all(|v| v.is_finite())
            && scaled.iter().all(|v| *v > 0 && *v <= EDGE),
        "invalid SAM logits"
    );
    let mut bytes = Vec::with_capacity(MASK_EDGE * MASK_EDGE);
    for y in 0..MASK_EDGE {
        check(cancel)?;
        for x in 0..MASK_EDGE {
            let px = ((x as f32 + 0.5) / MASK_EDGE as f32) * scaled[0] as f32 / EDGE as f32
                * MASK_EDGE as f32
                - 0.5;
            let py = ((y as f32 + 0.5) / MASK_EDGE as f32) * scaled[1] as f32 / EDGE as f32
                * MASK_EDGE as f32
                - 0.5;
            let x0 = px.floor();
            let y0 = py.floor();
            let fx = px - x0;
            let fy = py - y0;
            let at = |x: i64, y: i64| {
                logits[y.clamp(0, 255) as usize * MASK_EDGE + x.clamp(0, 255) as usize]
            };
            let top = at(x0 as i64, y0 as i64) * (1. - fx) + at(x0 as i64 + 1, y0 as i64) * fx;
            let bottom =
                at(x0 as i64, y0 as i64 + 1) * (1. - fx) + at(x0 as i64 + 1, y0 as i64 + 1) * fx;
            let logit = top * (1. - fy) + bottom * fy;
            let probability = 1. / (1. + (-logit).exp());
            bytes.push((probability.clamp(0., 1.) * 255. + 0.5) as u8);
        }
    }
    check(cancel)?;
    Ok(Mask {
        width: MASK_EDGE,
        height: MASK_EDGE,
        bytes,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_padding_and_refusals() {
        let cancel = AtomicBool::new(false);
        assert_eq!(input_size(1920, 1080).unwrap(), [1024, 576]);
        assert_eq!(input_size(1080, 1920).unwrap(), [576, 1024]);
        assert!(input_size(0, 1).is_err());
        let image = Image::new(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 0]).unwrap();
        let (size, data) = prepare(&image, &cancel).unwrap();
        assert_eq!(size, [1024, 512]);
        assert_eq!(data[0], -0.485 / 0.229);
        assert_eq!(data[511 * 1024], -0.485 / 0.229);
        assert_eq!(data[512 * 1024], 0.);
        assert!(restore(&[0.], size, &cancel).is_err());
        assert_eq!(
            restore(&vec![0.; 65536], size, &cancel).unwrap().bytes,
            vec![128; 65536]
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(prepare(&image, &cancel).is_err());
        assert!(restore(&vec![0.; 65536], size, &cancel).is_err());
    }
    #[test]
    #[ignore = "requires pinned SlimSAM models and host ORT"]
    fn learned_regions_match_original_and_reuse_embedding() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let encoder = std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap();
        let decoder = std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap();
        let image = Image::new(
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
        .unwrap();
        let mut brush = Brush::load(Path::new(&encoder), Path::new(&decoder)).unwrap();
        let cancel = AtomicBool::new(false);
        let embedding = brush.embed(&image, &cancel).unwrap();
        let reference =
            crate::reference_brush::Brush::load(Path::new(&encoder), Path::new(&decoder)).unwrap();
        let original = crate::Frame::from_rgba(37, 23, image.rgba.clone()).unwrap();
        let ref_embedding = reference.embed(&original).unwrap();
        for points in [
            vec![[0.5, 0.5]],
            vec![[-0.5, 0.2], [1.5, 0.9]],
            (0..40).map(|i| [i as f64 / 39., 0.3]).collect(),
        ] {
            let actual = brush.region(&embedding, &points, &cancel).unwrap();
            let expected = reference.region(&ref_embedding, &points).unwrap();
            assert_eq!(actual.bytes, expected.bytes());
            assert_eq!((actual.width, actual.height), (256, 256));
        }
        assert!(brush.region(&embedding, &[], &cancel).is_err());
        assert!(brush
            .region(&embedding, &[[f64::NAN, 0.]], &cancel)
            .is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(brush.region(&embedding, &[[0.5, 0.5]], &cancel).is_err());
    }

    #[test]
    #[ignore = "requires pinned SlimSAM models and host ORT"]
    fn tracked_regions_match_original_on_same_and_moving_frames() {
        ort::init_from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap())
            .unwrap()
            .commit();
        let encoder = std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap();
        let decoder = std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap();
        let mut engine = Brush::load(Path::new(&encoder), Path::new(&decoder)).unwrap();
        let reference =
            crate::reference_brush::Brush::load(Path::new(&encoder), Path::new(&decoder)).unwrap();
        let cancel = AtomicBool::new(false);
        let (mut found, mut lost) = (0usize, 0usize);
        for (width, height) in [(37, 23), (23, 37), (80, 80)] {
            let make = |shift: i64| {
                Image::new(
                    width,
                    height,
                    (0..width * height)
                        .flat_map(|i| {
                            let x = (i % width) as i64 - width as i64 / 2 - shift;
                            let y = (i / width) as i64 - height as i64 / 2;
                            let radius = width.min(height) as i64 / 3;
                            let rgba = if x * x + y * y < radius * radius {
                                [220, 25, 45, 255]
                            } else {
                                [22, 130, 180, (170 + i % 86) as u8]
                            };
                            rgba
                        })
                        .collect(),
                )
                .unwrap()
            };
            if let Ok(root) = std::env::var("VIDEO_KADR_TEST_SAM_TRACK_ORACLE_DIR") {
                let directory = Path::new(&root).join(format!("{width}x{height}"));
                assert!(directory.is_absolute());
                std::fs::create_dir_all(&directory).unwrap();
                let images: Vec<_> = [0, 2, 4, 6].into_iter().map(make).collect();
                let raw: Vec<u8> = images
                    .iter()
                    .flat_map(|image| image.rgba.iter().copied())
                    .collect();
                let anchor =
                    crate::Frame::from_rgba(width as u32, height as u32, images[1].rgba.clone())
                        .unwrap();
                let embedding = reference.embed(&anchor).unwrap();
                let seed = reference.region(&embedding, &[[0.5, 0.5]]).unwrap();
                let mut masks = vec![0u8; 4 * 65536];
                masks[65536..2 * 65536].copy_from_slice(seed.bytes());
                let mut tracked = 1u64;
                let (mut forward_lost, mut backward_lost) = (None, None);
                for forward in [true, false] {
                    let mut previous = Some(seed.clone());
                    let ordinals: Box<dyn Iterator<Item = usize>> = if forward {
                        Box::new(2..4)
                    } else {
                        Box::new((0..1).rev())
                    };
                    for ordinal in ordinals {
                        if let Some(prior) = previous.take() {
                            let frame = crate::Frame::from_rgba(
                                width as u32,
                                height as u32,
                                images[ordinal].rgba.clone(),
                            )
                            .unwrap();
                            let embedding = reference.embed(&frame).unwrap();
                            previous = reference.track(&embedding, &prior).unwrap();
                            if let Some(mask) = &previous {
                                masks[ordinal * 65536..(ordinal + 1) * 65536]
                                    .copy_from_slice(mask.bytes());
                                tracked += 1;
                            } else if forward {
                                forward_lost = Some(ordinal);
                            } else {
                                backward_lost = Some(ordinal);
                            }
                        }
                    }
                }
                std::fs::write(directory.join("source.rgba"), &raw).unwrap();
                std::fs::write(directory.join("seed.gray"), seed.bytes()).unwrap();
                std::fs::write(directory.join("reference.masks"), &masks).unwrap();
                std::fs::write(directory.join("reference.json"),serde_json::to_vec(&serde_json::json!({"width":width,"height":height,"frames":4,"seedFrame":1,"trackedFrames":tracked,"forwardLostAt":forward_lost,"backwardLostAt":backward_lost,"sourceHash":format!("{:x}",Sha256::digest(&raw)),"seedHash":format!("{:x}",Sha256::digest(seed.bytes())),"outputHash":format!("{:x}",Sha256::digest(&masks))})).unwrap()).unwrap();
            }
            let first = make(0);
            let first_ref =
                crate::Frame::from_rgba(width as u32, height as u32, first.rgba.clone()).unwrap();
            let first_embedding = reference.embed(&first_ref).unwrap();
            let learned = reference.region(&first_embedding, &[[0.5, 0.5]]).unwrap();
            let seeds = vec![
                Mask {
                    width: 256,
                    height: 256,
                    bytes: learned.bytes().to_vec(),
                },
                Mask {
                    width: 32,
                    height: 16,
                    bytes: vec![255; 512],
                },
                Mask {
                    width: 32,
                    height: 16,
                    bytes: vec![0; 512],
                },
                Mask {
                    width: 32,
                    height: 16,
                    bytes: vec![229; 512],
                },
            ];
            for shift in [0, 2] {
                let next = make(shift);
                let embedding = engine.embed(&next, &cancel).unwrap();
                let original =
                    crate::Frame::from_rgba(width as u32, height as u32, next.rgba.clone())
                        .unwrap();
                let ref_embedding = reference.embed(&original).unwrap();
                for previous in &seeds {
                    let before = previous.clone();
                    let ref_previous = crate::object_matting::reference::Mask::from_bytes(
                        previous.width as u32,
                        previous.height as u32,
                        previous.bytes.clone(),
                    )
                    .unwrap();
                    let expected = reference.track(&ref_embedding, &ref_previous).unwrap();
                    let actual = engine.track(&embedding, previous, &cancel).unwrap();
                    assert_eq!(
                        actual.as_ref().map(|m| m.bytes.as_slice()),
                        expected.as_ref().map(|m| m.bytes()),
                        "{width}x{height}/shift{shift}"
                    );
                    if actual.is_some() {
                        found += 1;
                    } else {
                        lost += 1;
                    }
                    assert_eq!(*previous, before);
                }
                cancel.store(true, Ordering::Relaxed);
                assert!(engine.track(&embedding, &seeds[0], &cancel).is_err());
                cancel.store(false, Ordering::Relaxed);
            }
        }
        assert!(
            found > 0,
            "must qualify actual retained regions, not just loss"
        );
        assert!(lost > 0);
        eprintln!("SAM original tracking parity: {found} retained, {lost} lost");
    }
}
