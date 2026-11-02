//! Source-UV cutout application; straight RGB and authored source alpha are retained.
use crate::{object_matting::Mask, Image};
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mapping {
    pub crop: [f32; 4],
    pub flip_h: bool,
    pub flip_v: bool,
}
impl Mapping {
    pub const IDENTITY: Self = Self {
        crop: [0.; 4],
        flip_h: false,
        flip_v: false,
    };
    fn validate(self) -> Result<()> {
        ensure!(
            self.crop
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid normalized mask mapping"
        );
        Ok(())
    }
    fn source(self, u: f32, v: f32) -> (f32, f32) {
        let x = if self.flip_h { 1. - u } else { u };
        let y = if self.flip_v { 1. - v } else { v };
        let [l, t, r, b] = self.crop;
        (l + x * (1. - l - r).max(0.), t + y * (1. - t - b).max(0.))
    }
}
impl Default for Mapping {
    fn default() -> Self {
        Self::IDENTITY
    }
}
pub(crate) fn sample(mask: &Mask, u: f32, v: f32) -> f32 {
    video_kadr_mask_core::sample(mask, u, v)
}
/// Returns a new frame or an error, never a partially edited input. Blank-mask
/// fallback belongs to the caller: a custom painted blank may intentionally cut all.
pub fn apply(image: &Image, mask: &Mask, mapping: Mapping, cancel: &AtomicBool) -> Result<Image> {
    ensure!(
        image.width > 0
            && image.height > 0
            && image
                .width
                .checked_mul(image.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024)
            && image.rgba.len() == image.width * image.height * 4,
        "invalid cutout image"
    );
    ensure!(
        mask.width > 0
            && mask.height > 0
            && mask
                .width
                .checked_mul(mask.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024)
            && mask.bytes.len() == mask.width * mask.height,
        "invalid cutout mask"
    );
    mapping.validate()?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "cutout application cancelled"
    );
    let mut output = image.clone();
    for y in 0..image.height {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "cutout application cancelled"
        );
        for x in 0..image.width {
            let (u, v) = mapping.source(
                (x as f32 + 0.5) / image.width as f32,
                (y as f32 + 0.5) / image.height as f32,
            );
            let index = (y * image.width + x) * 4 + 3;
            output.rgba[index] = (f32::from(image.rgba[index]) * sample(mask, u, v) + 0.5) as u8;
        }
    }
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "cutout application cancelled"
    );
    Ok(output)
}
#[cfg(test)]
#[allow(dead_code)]
#[path = "reference_apply.rs"]
pub(crate) mod reference;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cut_matches_crop_flip_alpha_and_parallel_row_boundaries() {
        let mask = Mask {
            width: 13,
            height: 7,
            bytes: (0..91).map(|n| ((n * 53) % 256) as u8).collect(),
        };
        let original_mask =
            crate::object_matting::reference::Mask::from_bytes(13, 7, mask.bytes.clone()).unwrap();
        for (w, h) in [(7, 5), (257, 9), (641, 625)] {
            let image = Image::new(
                w,
                h,
                (0..w * h)
                    .flat_map(|n| {
                        [
                            (n % 251) as u8,
                            ((n * 17) % 256) as u8,
                            ((n * 31) % 256) as u8,
                            ((n * 73) % 256) as u8,
                        ]
                    })
                    .collect(),
            )
            .unwrap();
            let before = image.clone();
            for crop in [[0.; 4], [0.13, 0.19, 0.23, 0.07], [0.8, 0.9, 0.8, 0.2]] {
                for flip_h in [false, true] {
                    for flip_v in [false, true] {
                        let mapping = Mapping {
                            crop,
                            flip_h,
                            flip_v,
                        };
                        let actual =
                            apply(&image, &mask, mapping, &AtomicBool::new(false)).unwrap();
                        let mut original = crate::Frame(image.clone());
                        reference::cut(
                            &mut original,
                            &original_mask,
                            &reference::Mapping {
                                crop,
                                flip_h,
                                flip_v,
                            },
                        );
                        assert_eq!(actual.rgba, original.pixels());
                        assert_eq!(image, before);
                    }
                }
            }
        }
    }
    #[test]
    fn malformed_mapping_mask_and_precancel_refuse_without_mutation() {
        let image = Image::new(2, 2, vec![150; 16]).unwrap();
        let mask = Mask {
            width: 1,
            height: 1,
            bytes: vec![128],
        };
        assert!(apply(&image, &mask, Mapping::IDENTITY, &AtomicBool::new(true)).is_err());
        for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            assert!(apply(
                &image,
                &mask,
                Mapping {
                    crop: [value, 0., 0., 0.],
                    ..Mapping::IDENTITY
                },
                &AtomicBool::new(false)
            )
            .is_err());
        }
        assert!(apply(
            &image,
            &Mask {
                width: 2,
                height: 2,
                bytes: vec![]
            },
            Mapping::IDENTITY,
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(image.rgba, vec![150; 16]);
    }
}
