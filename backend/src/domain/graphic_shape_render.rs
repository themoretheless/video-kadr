//! Filmcraft vector shape appearance rasterizer, ported under MIT.
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::graphic_shape_path::GraphicShapePath;
use filmcraft_text::{
    Mask, StrokeKind, mask,
    raster::{Xform, apply, fill_into, xform_scale},
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeAppearance {
    pub fill: Option<[f32; 4]>,
    pub strokes: Vec<([f32; 4], f32, u32)>,
    pub background: Option<([f32; 4], f32, f32)>,
    pub shadow: Option<ShapeShadow>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeShadow {
    pub color: [f32; 4],
    pub offset: (f32, f32),
    pub size: f32,
    pub blur: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShapeLayer {
    pub enabled: bool,
    pub shape: GraphicShapePath,
    pub appearance: ShapeAppearance,
    pub opacity: f32,
}
pub struct RasterImage {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}
pub struct LayerRaster {
    pub x: i32,
    pub y: i32,
    pub img: RasterImage,
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

pub fn raster_layer(spec: &ShapeLayer, m: &Xform, cw: usize, ch: usize) -> Option<LayerRaster> {
    raster_impl(spec, m, cw, ch, false)
}
pub(crate) fn raster_layer_signed(
    spec: &ShapeLayer,
    m: &Xform,
    cw: usize,
    ch: usize,
) -> Option<LayerRaster> {
    raster_impl(spec, m, cw, ch, true)
}
fn raster_impl(
    spec: &ShapeLayer,
    m: &Xform,
    cw: usize,
    ch: usize,
    signed: bool,
) -> Option<LayerRaster> {
    if !spec.enabled || spec.opacity <= 0.0 {
        return None;
    }
    let s = xform_scale(m).max(1e-6);
    let ap = &spec.appearance;
    let path = if signed {
        spec.shape.path_with_signed_size()
    } else {
        spec.shape.path()
    }
    .ok()?;
    let lb = path
        .bounds()
        .map(|b| [b.0, b.1, b.2, b.3])
        .unwrap_or([0.; 4]);
    let shape = path.transformed(m);
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
    let mut bb = bbox_of(m, lb, stroke_out + 2.0 / s);
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
    fill_into(&shape, &mut cover, -x0 as f32, -y0 as f32);
    let strokes: Vec<(Mask, [f32; 4])> = ap
        .strokes
        .iter()
        .map(|(c, sw, k)| (mask::stroke(&cover, sw * s, stroke_kind(*k)), *c))
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
        for (sm, _) in &strokes {
            u.max_with(sm);
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
        over_mask(&mut px, &cover, premul_linear(c));
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
    #[test]
    fn original_shape_appearance_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-shape.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 72);
        for (index, c) in cases.iter().enumerate() {
            let mut out = vec![0f32; 64 * 48 * 4];
            if !c["evaluated"].is_null() {
                let layer: ShapeLayer =
                    serde_json::from_value(c["evaluated"]["layer"].clone()).unwrap();
                let matrix: Xform =
                    serde_json::from_value(c["evaluated"]["matrix"].clone()).unwrap();
                if let Some(r) = raster_layer(&layer, &matrix, 64, 48) {
                    for y in r.y.max(0)..(r.y + r.img.h as i32).min(48) {
                        for x in r.x.max(0)..(r.x + r.img.w as i32).min(64) {
                            let si = ((y - r.y) as usize * r.img.w + (x - r.x) as usize) * 4;
                            let di = (y as usize * 64 + x as usize) * 4;
                            let s = &r.img.px[si..si + 4];
                            if s[3] <= 0. {
                                continue;
                            }
                            let k = 1. - s[3];
                            for channel in 0..4 {
                                out[di + channel] = s[channel] + out[di + channel] * k;
                            }
                        }
                    }
                }
            }
            let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected,
                "appearance case{index}"
            );
        }
    }
}
