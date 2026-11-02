//! Source-UV region propagation: confident interior prompts and overlap selection.
use crate::{mask_apply, object_matting::Mask};
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "SAM tracking cancelled");
    Ok(())
}
fn validate(mask: &Mask) -> Result<()> {
    ensure!(
        mask.width > 0
            && mask.height > 0
            && mask
                .width
                .checked_mul(mask.height)
                .is_some_and(|n| n <= 64 * 1024 * 1024 && mask.bytes.len() == n),
        "invalid tracked region"
    );
    Ok(())
}
/// At most nine distinct confident source points, with stable row-major tie breaks.
pub fn interior_points(previous: &Mask, cancel: &AtomicBool) -> Result<Vec<[f64; 2]>> {
    check(cancel)?;
    validate(previous)?;
    let mut bounds: Option<[usize; 4]> = None;
    for (index, &value) in previous.bytes.iter().enumerate() {
        if index % 1024 == 0 {
            check(cancel)?;
        }
        if value >= 230 {
            let x = index % previous.width;
            let y = index / previous.width;
            match &mut bounds {
                Some(b) => {
                    b[0] = b[0].min(x);
                    b[1] = b[1].max(x);
                    b[2] = b[2].min(y);
                    b[3] = b[3].max(y)
                }
                None => bounds = Some([x, x, y, y]),
            }
        }
    }
    let Some([left, right, top, bottom]) = bounds else {
        return Ok(Vec::new());
    };
    let mut cells = Vec::with_capacity(9);
    for row in 0..3 {
        for column in 0..3 {
            cells.push((
                [
                    left as f64 + (right - left) as f64 * (column as f64 + 0.5) / 3.,
                    top as f64 + (bottom - top) as f64 * (row as f64 + 0.5) / 3.,
                ],
                f64::INFINITY,
                [0usize; 2],
            ));
        }
    }
    // Scan without collecting every confident pixel into a second image-sized buffer.
    for (index, &value) in previous.bytes.iter().enumerate() {
        if index % 1024 == 0 {
            check(cancel)?;
        }
        if value < 230 {
            continue;
        }
        let pixel = [index % previous.width, index / previous.width];
        for (target, best, position) in &mut cells {
            let distance =
                (pixel[0] as f64 - target[0]).powi(2) + (pixel[1] as f64 - target[1]).powi(2);
            if distance < *best {
                *best = distance;
                *position = pixel;
            }
        }
    }
    let reach = ((right - left).max(bottom - top) as f64 / 3.).max(2.);
    let mut result: Vec<_> = cells
        .into_iter()
        .filter(|(_, distance, _)| distance.sqrt() <= reach)
        .map(|(_, _, [x, y])| {
            [
                (x as f64 + 0.5) / previous.width as f64,
                (y as f64 + 0.5) / previous.height as f64,
            ]
        })
        .collect();
    result.sort_by(|a, b| a.partial_cmp(b).unwrap());
    result.dedup();
    check(cancel)?;
    Ok(result)
}
/// Binary IoU on the candidate grid; previous confidence is sampled bilinearly.
pub fn overlap(candidate: &Mask, previous: &Mask, cancel: &AtomicBool) -> Result<f32> {
    check(cancel)?;
    validate(candidate)?;
    validate(previous)?;
    let (mut intersection, mut union) = (0u32, 0u32);
    for y in 0..candidate.height {
        check(cancel)?;
        let v = (y as f32 + 0.5) / candidate.height as f32;
        for x in 0..candidate.width {
            if x % 1024 == 0 {
                check(cancel)?;
            }
            let a = candidate.bytes[y * candidate.width + x] >= 128;
            let b =
                mask_apply::sample(previous, (x as f32 + 0.5) / candidate.width as f32, v) >= 0.5;
            intersection += u32::from(a && b);
            union += u32::from(a || b);
        }
    }
    Ok(if union == 0 {
        0.
    } else {
        intersection as f32 / union as f32
    })
}
/// Temporal overlap wins over model confidence; the last tied candidate wins.
pub(crate) fn select(
    previous: &Mask,
    candidates: Vec<(f32, Mask)>,
    cancel: &AtomicBool,
) -> Result<Option<Mask>> {
    check(cancel)?;
    validate(previous)?;
    let mut winner: Option<(f32, Mask)> = None;
    for (score, mask) in candidates {
        ensure!(score.is_finite(), "invalid tracked candidate score");
        let iou = overlap(&mask, previous, cancel)?;
        if winner
            .as_ref()
            .is_none_or(|(best, _)| iou.total_cmp(best).is_ge())
        {
            winner = Some((iou, mask));
        }
    }
    check(cancel)?;
    Ok(winner.filter(|(iou, _)| *iou >= 0.25).map(|(_, mask)| mask))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mask(w: usize, h: usize, bytes: Vec<u8>) -> Mask {
        Mask {
            width: w,
            height: h,
            bytes,
        }
    }
    #[test]
    fn prompts_have_exact_grid_ties_and_confidence_threshold() {
        let cancel = AtomicBool::new(false);
        let mut previous = mask(32, 32, vec![0; 1024]);
        for y in 8..24 {
            for x in 8..24 {
                previous.bytes[y * 32 + x] = 255;
            }
        }
        let values = [10.5 / 32., 15.5 / 32., 20.5 / 32.];
        let expected: Vec<_> = values
            .into_iter()
            .flat_map(|x| values.into_iter().map(move |y| [x, y]))
            .collect();
        assert_eq!(interior_points(&previous, &cancel).unwrap(), expected);
        let mut single = mask(12, 10, vec![229; 120]);
        assert!(interior_points(&single, &cancel).unwrap().is_empty());
        single.bytes[7 * 12 + 4] = 230;
        assert_eq!(
            interior_points(&single, &cancel).unwrap(),
            vec![[4.5 / 12., 7.5 / 10.]]
        );
    }
    #[test]
    fn overlap_threshold_ties_and_candidate_scores() {
        let cancel = AtomicBool::new(false);
        let previous = mask(4, 4, vec![255; 16]);
        let quarter = mask(4, 4, (0..16).map(|i| if i < 4 { 128 } else { 0 }).collect());
        assert_eq!(overlap(&quarter, &previous, &cancel).unwrap(), 0.25);
        let selected = select(
            &previous,
            vec![
                (100., quarter.clone()),
                (
                    -1.,
                    Mask {
                        bytes: quarter
                            .bytes
                            .iter()
                            .map(|v| if *v > 0 { 255 } else { 0 })
                            .collect(),
                        ..quarter.clone()
                    },
                ),
            ],
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(selected.bytes[0], 255);
        assert!(select(
            &previous,
            vec![(
                1.,
                mask(4, 4, (0..16).map(|i| if i < 3 { 255 } else { 0 }).collect())
            )],
            &cancel
        )
        .unwrap()
        .is_none());
        assert_eq!(
            overlap(
                &mask(4, 1, vec![0, 255, 255, 0]),
                &mask(2, 1, vec![0, 255]),
                &cancel
            )
            .unwrap(),
            1. / 3.
        );
        assert_eq!(
            overlap(&mask(1, 1, vec![0]), &mask(2, 2, vec![0; 4]), &cancel).unwrap(),
            0.
        );
    }
    #[test]
    fn malformed_and_cancelled_regions_refuse_without_mutation() {
        let cancel = AtomicBool::new(false);
        let bad = mask(2, 2, vec![0]);
        assert!(interior_points(&bad, &cancel).is_err());
        assert!(overlap(&bad, &mask(1, 1, vec![0]), &cancel).is_err());
        let good = mask(2, 2, vec![255; 4]);
        let before = good.clone();
        cancel.store(true, Ordering::Relaxed);
        assert!(interior_points(&good, &cancel).is_err());
        assert!(select(&good, vec![], &cancel).is_err());
        assert_eq!(good, before);
    }
}
