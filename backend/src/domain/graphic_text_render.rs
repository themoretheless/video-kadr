//! Filmcraft rich graphic text appearance rasterizer (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::graphic_shape_render::{LayerRaster, RasterImage, ShapeAppearance};
use super::graphic_text_layout::{text_layout, GraphicText};
use filmcraft_text::{
    mask,
    raster::{apply, fill_into, xform_scale, Xform},
    render, Mask, StrokeKind,
};
use rayon::prelude::*;
pub struct TextLayer {
    pub enabled: bool,
    pub text: GraphicText,
    pub appearance: ShapeAppearance,
    pub opacity: f32,
}
fn premul_linear(c: [f32; 4]) -> [f32; 4] {
    let a = c[3].clamp(0., 1.);
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [linear(c[0]) * a, linear(c[1]) * a, linear(c[2]) * a, a]
}
fn stroke_kind(k: u32) -> StrokeKind {
    match k {
        1 => StrokeKind::Center,
        2 => StrokeKind::Inner,
        _ => StrokeKind::Outer,
    }
}

fn bbox_of(m: &Xform, b: [f32; 4], pad: f32) -> [f32; 4] {
    let pts = [
        (b[0] - pad, b[1] - pad),
        (b[2] + pad, b[1] - pad),
        (b[0] - pad, b[3] + pad),
        (b[2] + pad, b[3] + pad),
    ];
    let mut o = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, y) in pts {
        let (a, c) = apply(m, x, y);
        o = [o[0].min(a), o[1].min(c), o[2].max(a), o[3].max(c)];
    }
    o
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Over-composite `c` (premultiplied) through `m` into `px` (premultiplied RGBA, same size).
fn over_mask(px: &mut [f32], m: &Mask, c: [f32; 4]) {
    if c[3] <= 0.0 || m.w == 0 {
        return;
    }
    px.par_chunks_mut(4 * m.w)
        .zip(m.a.par_chunks(m.w))
        .for_each(|(row, mrow)| {
            for (p, &a) in row.as_chunks_mut::<4>().0.iter_mut().zip(mrow) {
                if a > 0.0 {
                    let k = 1.0 - c[3] * a;
                    for i in 0..4 {
                        p[i] = c[i] * a + p[i] * k;
                    }
                }
            }
        });
}

fn over_sweep(px: &mut [f32], mask: &Mask, primary: [f32; 4], secondary: [f32; 4], edge: f32, m: &Xform, origin: (i32, i32)) {
    let determinant = m[0] * m[3] - m[1] * m[2];
    if determinant.abs() <= f32::EPSILON {
        over_mask(px, mask, secondary);
        return;
    }
    let (dx, dy) = (m[3] / determinant, -m[2] / determinant);
    let normal_length = dx.hypot(dy);
    px.par_chunks_mut(4 * mask.w).zip(mask.a.par_chunks(mask.w)).enumerate().for_each(|(y, (row, alphas))| {
        for (x, (pixel, alpha)) in row.chunks_exact_mut(4).zip(alphas).enumerate() {
            if *alpha <= 0. {
                continue;
            }
            let local_x = dx * (x as f32 + origin.0 as f32 + 0.5 - m[4])
                + dy * (y as f32 + origin.1 as f32 + 0.5 - m[5]);
            let weight = ((edge - local_x) / normal_length + 0.5).clamp(0., 1.);
            let color = std::array::from_fn::<_, 4, _>(|i| secondary[i] + (primary[i] - secondary[i]) * weight);
            let remaining = 1. - color[3] * alpha;
            for i in 0..4 {
                pixel[i] = color[i] * alpha + pixel[i] * remaining;
            }
        }
    });
}

pub fn raster_layer(spec: &TextLayer, m: &Xform, cw: usize, ch: usize) -> Option<LayerRaster> {
    if !spec.enabled || spec.opacity <= 0.0 {
        return None;
    }
    let s = xform_scale(m).max(1e-6);
    let ap = &spec.appearance;
    let layout = text_layout(&spec.text);
    if layout.glyphs.is_empty() && layout.underlines.is_empty() && ap.background.is_none() {
        return None;
    }
    let lb = spec.text.block_bounds(layout.bounds);
    let text = Some(layout);
    let stroke_out = ap
        .strokes
        .iter()
        .map(|(_, w, k)| {
            if *k == 2 {
                0.0
            } else if *k == 1 {
                w / 2.0
            } else {
                *w
            }
        })
        .fold(0.0f32, f32::max);
    let overhang = text.as_ref().map_or(0.0, |l| {
        l.glyphs.iter().map(|g| g.size).fold(0.0f32, f32::max) * 0.35
    });
    let mut bb = bbox_of(m, lb, overhang + stroke_out + 2.0 / s);
    if let Some((_, pad, _)) = ap.background {
        bb = union(bb, bbox_of(m, lb, pad + 1.0 / s));
    }
    let mut shadow_dev = (0.0f32, 0.0f32);
    if let Some(sh) = &ap.shadow {
        // the offset is in layer space, so it turns and scales with the layer
        shadow_dev = (
            m[0] * sh.offset.0 + m[2] * sh.offset.1,
            m[1] * sh.offset.0 + m[3] * sh.offset.1,
        );
        let grow = (sh.size + sh.blur) * s + 2.0;
        bb = union(
            bb,
            [
                bb[0] + shadow_dev.0 - grow,
                bb[1] + shadow_dev.1 - grow,
                bb[2] + shadow_dev.0 + grow,
                bb[3] + shadow_dev.1 + grow,
            ],
        );
    }
    // clip to the canvas, keeping enough margin for blur to read content just outside it
    let margin = ap.shadow.as_ref().map_or(2.0, |sh| {
        (sh.blur + sh.size) * s + shadow_dev.0.abs().max(shadow_dev.1.abs()) + 2.0
    });
    let x0 = bb[0].floor().max(-margin) as i32;
    let y0 = bb[1].floor().max(-margin) as i32;
    let x1 = bb[2].ceil().min(cw as f32 + margin) as i32;
    let y1 = bb[3].ceil().min(ch as f32 + margin) as i32;
    if x1 <= x0 || y1 <= y0 || x1 <= 0 || y1 <= 0 || x0 >= cw as i32 || y0 >= ch as i32 {
        return None;
    }
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if w * h > 64 << 20 {
        return None;
    }
    let mut cover = Mask::new(w, h);
    if let Some(l) = &text {
        render::draw(l, m, &mut cover, (x0, y0));
    }
    let mut outline_cover = None;
    if !ap.strokes.is_empty() && spec.text.runs.iter().any(|(_, style)| style.suppress_outline == Some(true)) {
        if let Some(layout) = &text {
            let mut eligible = Mask::new(w, h);
            render::draw_run(layout, m, &mut eligible, (x0, y0), Some(0));
            for (index, (_, style)) in spec.text.runs.iter().enumerate() {
                if style.suppress_outline != Some(true) {
                    render::draw_run(layout, m, &mut eligible, (x0, y0), Some((index + 1) as u16));
                }
            }
            outline_cover = Some(eligible);
        }
    }
    let strokes: Vec<(Mask, [f32; 4])> = ap
        .strokes
        .iter()
        .map(|(c, sw, k)| (mask::stroke(outline_cover.as_ref().unwrap_or(&cover), sw * s, stroke_kind(*k)), *c))
        .collect();
    let bg = ap.background.map(|(c, pad, r)| {
        let p =
            filmcraft_text::Path::round_rect(lb[0] - pad, lb[1] - pad, lb[2] + pad, lb[3] + pad, r)
                .transformed(m);
        let mut bm = Mask::new(w, h);
        fill_into(&p, &mut bm, -x0 as f32, -y0 as f32);
        (bm, c)
    });
    let mut px = vec![0.0f32; w * h * 4];
    if let Some(sh) = &ap.shadow {
        let mut u = if ap.fill.is_some() {
            cover.clone()
        } else {
            Mask::new(w, h)
        };
        if outline_cover.is_some() {
            // A suppressed karaoke outline still contributes to the shadow.
            for (_, width, kind) in &ap.strokes {
                u.max_with(&mask::stroke(&cover, width * s, stroke_kind(*kind)));
            }
        } else {
            for (sm, _) in &strokes {
                u.max_with(sm);
            }
        }
        if let Some((bm, _)) = &bg {
            u.max_with(bm);
        }
        if sh.size > 0.0 {
            u = mask::stroke(&u, sh.size * s, StrokeKind::Outer);
        }
        let u = mask::offset(&u, shadow_dev.0.round() as i32, shadow_dev.1.round() as i32);
        let u = mask::blur(&u, sh.blur * s / 3.0);
        over_mask(&mut px, &u, premul_linear(sh.color));
    }
    if let Some((bm, c)) = &bg {
        over_mask(&mut px, bm, premul_linear(*c));
    }
    // outer strokes sit under the fill; centre and inner strokes are drawn over it
    let outer = |k: u32| k != 1 && k != 2;
    for ((sm, c), (_, _, k)) in strokes.iter().zip(&ap.strokes).rev() {
        if outer(*k) {
            over_mask(&mut px, sm, premul_linear(*c));
        }
    }
    if let Some(c) = ap.fill {
        // per-character fills: each style run with its own colour is drawn on its own
        let run_fills: Vec<Option<[f32; 4]>> = spec.text.runs.iter().map(|(_, s)| s.fill).collect();
        match &text {
            Some(l) if run_fills.iter().any(Option::is_some) => {
                let mut groups: Vec<([f32; 4], Vec<u16>)> = Vec::new();
                for r in 0..=run_fills.len() {
                    let col = if r == 0 {
                        c
                    } else {
                        run_fills[r - 1].unwrap_or(c)
                    };
                    if let Some(progress) = r.checked_sub(1).and_then(|i| spec.text.runs[i].1.fill_sweep) {
                        let mut run_mask = Mask::new(w, h);
                        render::draw_run(l, m, &mut run_mask, (x0, y0), Some(r as u16));
                        let mut glyphs = l.glyphs.iter().filter(|glyph| {
                            glyph.run == r as u16 && spec.text.text.get(glyph.cluster..)
                                .and_then(|text| text.chars().next()).is_some_and(|character| !character.is_whitespace())
                        });
                        if let Some(first) = glyphs.next() {
                            let last = glyphs.last().unwrap_or(first);
                            let edge = first.x + (last.x + last.advance_x - first.x) * progress.clamp(0., 1.);
                            if progress > 0. {
                                over_sweep(&mut px, &run_mask, premul_linear(c), premul_linear(col), edge, m, (x0, y0));
                            } else {
                                over_mask(&mut px, &run_mask, premul_linear(col));
                            }
                        } else {
                            over_mask(&mut px, &run_mask, premul_linear(col));
                        }
                        continue;
                    }
                    match groups.iter_mut().find(|g| g.0 == col) {
                        Some(g) => g.1.push(r as u16),
                        None => groups.push((col, vec![r as u16])),
                    }
                }
                for (col, runs) in groups {
                    let mut m_run = Mask::new(w, h);
                    for r in runs {
                        render::draw_run(l, m, &mut m_run, (x0, y0), Some(r));
                    }
                    over_mask(&mut px, &m_run, premul_linear(col));
                }
            }
            _ => over_mask(&mut px, &cover, premul_linear(c)),
        }
    }
    for ((sm, c), (_, _, k)) in strokes.iter().zip(&ap.strokes).rev() {
        if !outer(*k) {
            over_mask(&mut px, sm, premul_linear(*c));
        }
    }
    let op = spec.opacity;
    if op < 1.0 {
        px.iter_mut().for_each(|v| *v *= op);
    }
    Some(LayerRaster {
        x: x0,
        y: y0,
        img: RasterImage { w, h, px },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::graphic_text_layout::GraphicCharStyle;
    #[test]
    fn per_character_fill_and_layer_opacity_reach_raster() {
        let mut layer = TextLayer {
            enabled: true,
            text: GraphicText {
                text: "AB".into(),
                size: 30.,
                runs: vec![(
                    0..1,
                    GraphicCharStyle {
                        fill: Some([1., 0., 0., 1.]),
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
            appearance: ShapeAppearance {
                fill: Some([0., 0., 1., 1.]),
                strokes: vec![],
                background: None,
                shadow: None,
            },
            opacity: 1.,
        };
        let matrix = [1., 0., 0., 1., 10., 40.];
        let full = raster_layer(&layer, &matrix, 100, 80).unwrap();
        assert!(full
            .img
            .px
            .chunks_exact(4)
            .any(|p| p[0] > 0.5 && p[2] == 0.));
        assert!(full
            .img
            .px
            .chunks_exact(4)
            .any(|p| p[2] > 0.5 && p[0] == 0.));
        layer.opacity = 0.5;
        let half = raster_layer(&layer, &matrix, 100, 80).unwrap();
        assert_eq!(
            (full.x, full.y, full.img.w, full.img.h),
            (half.x, half.y, half.img.w, half.img.h)
        );
        for (a, b) in full.img.px.iter().zip(&half.img.px) {
            assert_eq!(*a * 0.5, *b);
        }
        layer.enabled = false;
        assert!(raster_layer(&layer, &matrix, 100, 80).is_none());
    }
}
