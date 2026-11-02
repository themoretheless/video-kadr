//! Warp Stabilizer working-linear pixel delivery. Filmcraft MIT attribution:
//! docs/licenses/filmcraft-MIT.txt.
use super::{
    group_transform::GroupAffine,
    linear_color_frame::LinearColorFrame,
    warp_stabilizer::{StabilizerPath, WarpStabilizer},
};
use anyhow::{ensure, Result};

pub fn apply_linear(
    effect: &WarpStabilizer,
    path: &StabilizerPath,
    seconds: f64,
    image: &mut LinearColorFrame,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    let (w, h) = image.dimensions();
    let plan = effect
        .frame_plan(path, seconds, w, h)
        .ok_or_else(|| anyhow::anyhow!("invalid Warp Stabilizer frame plan"))?;
    let m = plan.transform;
    ensure!(
        [m.a, m.b, m.c, m.d, m.e, m.f]
            .into_iter()
            .all(f64::is_finite),
        "nonfinite stabilization transform"
    );
    let source = image.clone();
    *image = source.transformed(
        w,
        h,
        GroupAffine([m.a, m.b, m.c, m.d, m.e, m.f]),
        checkpoint,
    )?;
    if let Some([x, y]) = plan.crop {
        let (wf, hf) = (w as f32, h as f32);
        let (x0, x1, y0, y1) = (x * wf, wf * (1. - x), y * hf, hf * (1. - y));
        let mut i = 0;
        image.map_premultiplied(
            &mut |mut p| {
                let (px, py) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                i += 1;
                let a = ((px - x0).min(x1 - px).min(py - y0).min(y1 - py) + 0.5).clamp(0., 1.);
                if a < 1. {
                    for v in &mut p {
                        *v *= a;
                    }
                }
                p
            },
            checkpoint,
        )?;
    }
    if plan.synthesize_edges {
        if let Some(inv) = m.inverse() {
            let pixels = source.pixels();
            let mut i = 0;
            image.map_premultiplied(
                &mut |p| {
                    let (x, y) = (i % w, i / w);
                    i += 1;
                    if p[3] >= 0.999 {
                        return p;
                    }
                    let q = inv.apply(super::warp_stabilizer::Vec2::new(
                        x as f64 + 0.5,
                        y as f64 + 0.5,
                    ));
                    let fx = (q.x as f32 - 0.5).clamp(0., w as f32 - 1.);
                    let fy = (q.y as f32 - 0.5).clamp(0., h as f32 - 1.);
                    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
                    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                    let (a, b, c, d) = (
                        pixels[y0 * w + x0],
                        pixels[y0 * w + x1],
                        pixels[y1 * w + x0],
                        pixels[y1 * w + x1],
                    );
                    std::array::from_fn(|k| {
                        let top = a[k] + (b[k] - a[k]) * tx;
                        let bot = c[k] + (d[k] - c[k]) * tx;
                        let fill = top + (bot - top) * ty;
                        p[k] + fill * (1. - p[3])
                    })
                },
                checkpoint,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface, warp_stabilizer::Affine};
    #[test]
    fn stationary_modes_and_translucent_synthesis_follow_source_over() {
        let mut checkpoint = || Ok(());
        let source = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 6,
                height: 4,
                rgba: [90, 120, 180, 128].repeat(24),
            },
            &mut checkpoint,
        )
        .unwrap();
        let path = StabilizerPath {
            w: 6,
            h: 4,
            steps: vec![Affine::IDENTITY],
            fps: 24.,
        };
        for framing in 0..4 {
            let mut frame = source.clone();
            apply_linear(
                &WarpStabilizer {
                    framing,
                    ..Default::default()
                },
                &path,
                0.,
                &mut frame,
                &mut checkpoint,
            )
            .unwrap();
            for (actual, p) in frame.pixels().iter().zip(source.pixels()) {
                let expected = if framing == 3 {
                    p.map(|v| v + v * (1. - p[3]))
                } else {
                    *p
                };
                assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
            }
        }
    }
    #[test]
    fn cancelled_delivery_does_not_mutate_frame() {
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 2,
                height: 2,
                rgba: [255, 0, 0, 255].repeat(4),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let before = frame.pixels().to_vec();
        let path = StabilizerPath {
            w: 2,
            h: 2,
            steps: vec![],
            fps: 24.,
        };
        assert!(apply_linear(
            &Default::default(),
            &path,
            0.,
            &mut frame,
            &mut || anyhow::bail!("cancelled")
        )
        .is_err());
        assert_eq!(frame.pixels(), before);
    }
}

#[cfg(test)]
mod public_effect_oracle_tests {
    use super::*;
    use crate::domain::{
        group_surface::GroupSurface, motion_tracker::Gray, warp_stabilizer::analyse_clip,
    };
    fn picture(k: i64, alpha: u8) -> LinearColorFrame {
        let rgba = (0..96 * 64)
            .flat_map(|i| {
                let x = (i % 96) as i64 - 2 * k;
                let y = (i / 96) as i64 - k;
                let u = x.rem_euclid(96) as usize;
                let v = y.rem_euclid(64) as usize;
                let a = ((u * 17 + v * 31 + (u / 7) * (v / 5) * 53) % 256) as u8;
                [
                    a,
                    ((u * 3 + v * 11) % 256) as u8,
                    ((u * 29 + v * 7) % 256) as u8,
                    alpha,
                ]
            })
            .collect();
        LinearColorFrame::from_surface(
            &GroupSurface {
                width: 96,
                height: 64,
                rgba,
            },
            &mut || Ok(()),
        )
        .unwrap()
    }
    #[test]
    fn original_public_effect_moving_working_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-warp-stabilizer.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 32);
        for (index, case) in cases.iter().enumerate() {
            let p: WarpStabilizer = serde_json::from_value(case["parameters"].clone()).unwrap();
            let alpha = case["alpha"].as_u64().unwrap() as u8;
            let path = analyse_clip(
                0.5,
                1.,
                6.,
                96,
                p.tracking_method(),
                p.detailed,
                &mut |dt, _scale| -> Result<Option<Gray>> {
                    let t = 0.5 + dt;
                    if !(0. ..1.).contains(&t) {
                        return Ok(None);
                    }
                    let image = picture((t * 6.).round() as i64, alpha);
                    let gray = image
                        .pixels()
                        .iter()
                        .map(|v| {
                            super::super::basic_grade::display(
                                (0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]).clamp(0., 1.),
                            )
                        })
                        .collect();
                    Ok(Some(Gray::new(96, 64, gray)))
                },
                &mut || Ok(()),
            )
            .unwrap()
            .unwrap();
            let mut image = picture(3, alpha);
            apply_linear(&p, &path, 0.5, &mut image, &mut || Ok(())).unwrap();
            let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(
                image
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "original Warp Stabilizer public effect case {index}"
            );
        }
    }
}
