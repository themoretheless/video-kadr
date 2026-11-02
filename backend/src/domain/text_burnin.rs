//! Filmcraft text burn-in drawing primitives. MIT: docs/licenses/filmcraft-MIT.txt.
//! Source: crates/render/src/vfx/text.rs and graphics.rs at pinned Filmcraft revision.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use filmcraft_text::{layout, render, Align, Mask, ParagraphStyle, Path, TextStyle};

fn paint_mask(
    pixels: &mut [[f32; 4]],
    width: usize,
    height: usize,
    mask: &Mask,
    x0: i32,
    y0: i32,
    color: [f32; 4],
    opacity: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    if mask.w == 0 || mask.h == 0 || color[3] <= 0.0 || opacity <= 0.0 {
        return Ok(());
    }
    let ys = y0.max(0) as usize..((y0 + mask.h as i32).min(height as i32)).max(0) as usize;
    let xs = x0.max(0)..(x0 + mask.w as i32).min(width as i32);
    for y in ys {
        checkpoint()?;
        for x in xs.clone() {
            let coverage = mask.a[(y as i32 - y0) as usize * mask.w + (x - x0) as usize] * opacity;
            if coverage <= 0.0 {
                continue;
            }
            let p = &mut pixels[y * width + x as usize];
            let k = 1.0 - color[3] * coverage;
            for c in 0..4 {
                p[c] = color[c] * coverage + p[c] * k;
            }
        }
    }
    Ok(())
}

/// First baseline cap height centred on position, using original layout and rasterisation.
/// The caller resolves automatic position and effect-specific size before drawing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(
    frame: &mut LinearColorFrame,
    text: &str,
    family: &str,
    position: [f64; 2],
    size: f32,
    alignment: u32,
    color: [f32; 4],
    box_alpha: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    draw_text_impl(
        frame, text, family, position, size, alignment, color, box_alpha, false, checkpoint,
    )
}

/// Original obsolete Timecode/Clip Name box uses cap height and descent, even for multiline text.
pub(crate) fn draw_clock_label(
    frame: &mut LinearColorFrame,
    text: &str,
    family: &str,
    position: [f64; 2],
    size: f32,
    box_alpha: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    draw_text_impl(
        frame, text, family, position, size, 1, [1.; 4], box_alpha, true, checkpoint,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_text_impl(
    frame: &mut LinearColorFrame,
    text: &str,
    family: &str,
    position: [f64; 2],
    size: f32,
    alignment: u32,
    color: [f32; 4],
    box_alpha: f32,
    clock_box: bool,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    ensure!(
        size.is_finite() && size > 0.0 && position.iter().all(|v| v.is_finite()),
        "invalid text geometry"
    );
    ensure!(
        color.iter().all(|v| v.is_finite()) && box_alpha.is_finite(),
        "invalid text color"
    );
    if text.is_empty() {
        return Ok(());
    }
    let align = match alignment {
        0 => Align::Left,
        2 => Align::Right,
        _ => Align::Center,
    };
    let style = TextStyle {
        family: family.into(),
        style: "Regular".into(),
        size,
        ..Default::default()
    };
    let paragraph = layout(
        text,
        &style,
        &ParagraphStyle {
            align,
            ..Default::default()
        },
    );
    checkpoint()?;
    let metrics =
        filmcraft_text::fonts::face(filmcraft_text::resolve(family, "Regular").face).metrics(size);
    let (bx, by) = (
        position[0] as f32,
        position[1] as f32 + metrics.cap_height / 2.0,
    );
    let (width, height) = frame.dimensions();
    let mut pixels = frame.pixels().to_vec();
    if box_alpha > 0.0 {
        let b = paragraph.bounds;
        let pad = size * 0.25;
        let path = Path::rect(
            bx + b[0] - pad,
            if clock_box {
                by - metrics.cap_height - pad
            } else {
                by + b[1] - pad
            },
            bx + b[2] + pad,
            if clock_box {
                by + metrics.descent * 0.6 + pad
            } else {
                by + b[3] + pad
            },
        );
        if let Some(bounds) = path.bounds() {
            let (x, y) = (bounds.0.floor() as i32, bounds.1.floor() as i32);
            let mask = filmcraft_text::raster::fill(
                &path,
                (bounds.2.ceil() as i32 - x).max(1) as usize,
                (bounds.3.ceil() as i32 - y).max(1) as usize,
                -x as f32,
                -y as f32,
            );
            paint_mask(
                &mut pixels,
                width,
                height,
                &mask,
                x,
                y,
                [0.0, 0.0, 0.0, 1.0],
                box_alpha.clamp(0.0, 1.0),
                checkpoint,
            )?;
        }
    }
    let (mask, x, y) = render::rasterize(&paragraph, &render::at(bx, by.round()));
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let alpha = color[3].clamp(0.0, 1.0);
    let color = [
        linear(color[0]) * alpha,
        linear(color[1]) * alpha,
        linear(color[2]) * alpha,
        alpha,
    ];
    paint_mask(
        &mut pixels,
        width,
        height,
        &mask,
        x,
        y,
        color,
        1.0,
        checkpoint,
    )?;
    let mut output = frame.clone();
    let mut index = 0;
    output.map_premultiplied(
        &mut |_| {
            let p = pixels[index];
            index += 1;
            p
        },
        checkpoint,
    )?;
    *frame = output;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        text: String,
        size: f32,
        position: [f64; 2],
        color: [f32; 4],
        background: f32,
        font: usize,
        alignment: u32,
        bits: Vec<u32>,
    }
    #[test]
    fn original_simple_text_working_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-simple-text.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 144);
        for (index, case) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: case.width,
                    height: case.height,
                    rgba: case.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            crate::domain::simple_text::SimpleText {
                text: case.text,
                position: case.position.map(Some),
                size: case.size,
                font: case.font as u32,
                alignment: case.alignment,
                color: case.color,
                bg_opacity: case.background * 100.0,
            }
            .apply_linear(&mut frame, 1.0, &mut || Ok(()))
            .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                case.bits,
                "Simple Text case {index}"
            );
        }
    }
    #[test]
    fn cancellation_at_every_checkpoint_preserves_input() {
        let source = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 96,
                height: 48,
                rgba: vec![128; 96 * 48 * 4],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut count = 0;
        draw_text(
            &mut source.clone(),
            "Cancel",
            "Inter",
            [48., 24.],
            14.,
            1,
            [1.; 4],
            0.6,
            &mut || {
                count += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(count > 48);
        for stop in 1..=count {
            let mut frame = source.clone();
            let mut seen = 0;
            let result = draw_text(
                &mut frame,
                "Cancel",
                "Inter",
                [48., 24.],
                14.,
                1,
                [1.; 4],
                0.6,
                &mut || {
                    seen += 1;
                    anyhow::ensure!(seen != stop, "cancelled");
                    Ok(())
                },
            );
            assert!(result.is_err(), "checkpoint {stop}");
            assert_eq!(frame.pixels(), source.pixels(), "checkpoint {stop}");
        }
    }
}
