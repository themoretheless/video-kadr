//! Export label geometry and linear-premultiplied painting from Filmcraft
//! 5231852443363f001c3f6b396dd9b1e6461ae2be, pipeline.rs / graphics.rs.
//! MIT: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use filmcraft_text::{layout, render, Align, Mask, ParagraphStyle, Path, TextStyle};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}
impl Placement {
    pub fn anchor(self) -> (f32, f32) {
        let i = self as usize;
        ((i % 3) as f32 * 0.5, (i / 3) as f32 * 0.5)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TextOverlay {
    pub enabled: bool,
    pub text: String,
    pub placement: Placement,
    pub offset: (f32, f32),
    pub size_percent: f32,
    pub opacity: f32,
}
impl Default for TextOverlay {
    fn default() -> Self {
        Self {
            enabled: false,
            text: String::new(),
            placement: Placement::BottomCenter,
            offset: (0., 0.),
            size_percent: 5.,
            opacity: 100.,
        }
    }
}
impl TextOverlay {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.offset.0.abs() <= 1_000_000. && self.offset.1.abs() <= 1_000_000.,
            "export text offset exceeds host geometry budget"
        );
        ensure!(
            self.offset.0.is_finite()
                && self.offset.1.is_finite()
                && self.size_percent.is_finite()
                && self.opacity.is_finite(),
            "invalid export text geometry or opacity"
        );
        ensure!(
            self.text.len() <= 16384 && !self.text.contains('\0'),
            "invalid export text"
        );
        Ok(())
    }
}

/// The caller reserves two canvas-sized f32 masks before entering this operation.
/// Only visible mask pixels are allocated; glyph shaping and coverage remain original.
pub fn paint(
    pixels: &mut [f32],
    width: usize,
    height: usize,
    settings: &TextOverlay,
    text: &str,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    settings.validate()?;
    ensure!(
        width > 0
            && width <= 3840
            && height > 0
            && height <= 2160
            && pixels.len() == width * height * 4,
        "invalid export text canvas"
    );
    ensure!(
        text.len() <= 16384 && !text.contains('\0'),
        "invalid export label"
    );
    checkpoint()?;
    if !settings.enabled || text.is_empty() {
        return Ok(());
    }
    let px = (height as f32 * settings.size_percent.clamp(1., 50.) / 100.).max(6.);
    let family = filmcraft_text::fonts::DEFAULT_FAMILY;
    let style = TextStyle {
        family: family.into(),
        style: "Regular".into(),
        size: px,
        ..Default::default()
    };
    let left = layout(text, &style, &ParagraphStyle::default());
    let pad = px * 0.25;
    let (bw, bh) = (
        left.bounds[2] - left.bounds[0] + 2. * pad,
        px * 1.1 + 2. * pad,
    );
    let (ax, ay) = settings.placement.anchor();
    let margin = 0.03 * width.min(height) as f32;
    let x = margin + ax * (width as f32 - 2. * margin - bw) + settings.offset.0;
    let y = margin + ay * (height as f32 - 2. * margin - bh) + settings.offset.1;
    let centered = layout(
        text,
        &style,
        &ParagraphStyle {
            align: Align::Center,
            ..Default::default()
        },
    );
    let metrics =
        filmcraft_text::fonts::face(filmcraft_text::resolve(family, "Regular").face).metrics(px);
    let (bx, by) = (x + bw / 2., y + bh / 2. + metrics.cap_height / 2.);
    let b = centered.bounds;
    let path = Path::rect(
        bx + b[0] - pad,
        by - metrics.cap_height - pad,
        bx + b[2] + pad,
        by + metrics.descent * 0.6 + pad,
    );
    let mut backing = Mask::new(width, height);
    filmcraft_text::raster::fill_into(&path, &mut backing, 0., 0.);
    checkpoint()?;
    let mut ink = Mask::new(width, height);
    render::draw(&centered, &render::at(bx, by.round()), &mut ink, (0, 0));
    checkpoint()?;
    let opacity = (settings.opacity / 100.).clamp(0., 1.);
    for row in 0..height {
        checkpoint()?;
        for col in 0..width {
            let i = row * width + col;
            let box_cov = backing.a[i] * 0.6;
            let text_cov = ink.a[i];
            if box_cov <= 0. && text_cov <= 0. {
                continue;
            }
            let p = &mut pixels[i * 4..i * 4 + 4];
            if opacity >= 0.999 {
                for (c, v) in p.iter_mut().enumerate() {
                    if box_cov > 0. {
                        *v = if c == 3 { box_cov } else { 0. } + *v * (1. - box_cov);
                    }
                    if text_cov > 0. {
                        *v = text_cov + *v * (1. - text_cov);
                    }
                }
            } else {
                // Source paints an opaque white label over a black backing on a clear
                // layer, then applies opacity to the entire group in one composite.
                let alpha = text_cov + box_cov * (1. - text_cov);
                let k = 1. - alpha * opacity;
                for (c, v) in p.iter_mut().enumerate() {
                    let source = if c == 3 { alpha } else { text_cov };
                    *v = source * opacity + *v * k;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source_mask(
        p: &mut [f32],
        w: usize,
        h: usize,
        m: &Mask,
        x: i32,
        y: i32,
        color: [f32; 4],
        op: f32,
    ) {
        for yy in 0..h {
            for xx in 0..w {
                let (mx, my) = (xx as i32 - x, yy as i32 - y);
                if mx < 0 || my < 0 || mx >= m.w as i32 || my >= m.h as i32 {
                    continue;
                }
                let cov = m.a[my as usize * m.w + mx as usize] * op;
                if cov <= 0. {
                    continue;
                }
                let k = 1. - color[3] * cov;
                for c in 0..4 {
                    let i = (yy * w + xx) * 4 + c;
                    p[i] = color[c] * cov + p[i] * k;
                }
            }
        }
    }
    // Independent original tight-mask / full-RGBA-layer path, rather than the
    // clipped-mask / scalar group path used above.
    fn reference(p: &mut [f32], w: usize, h: usize, o: &TextOverlay, text: &str) {
        let px = (h as f32 * o.size_percent.clamp(1., 50.) / 100.).max(6.);
        let family = filmcraft_text::fonts::DEFAULT_FAMILY;
        let st = TextStyle {
            family: family.into(),
            style: "Regular".into(),
            size: px,
            ..Default::default()
        };
        let l = layout(text, &st, &ParagraphStyle::default());
        let pad = px * 0.25;
        let bw = l.bounds[2] - l.bounds[0] + 2. * pad;
        let bh = px * 1.1 + 2. * pad;
        let (ax, ay) = o.placement.anchor();
        let m = 0.03 * w.min(h) as f32;
        let x = m + ax * (w as f32 - 2. * m - bw) + o.offset.0;
        let y = m + ay * (h as f32 - 2. * m - bh) + o.offset.1;
        let l = layout(
            text,
            &st,
            &ParagraphStyle {
                align: Align::Center,
                ..Default::default()
            },
        );
        let vm = filmcraft_text::fonts::face(filmcraft_text::resolve(family, "Regular").face)
            .metrics(px);
        let (bx, by) = (x + bw / 2., y + bh / 2. + vm.cap_height / 2.);
        let b = l.bounds;
        let path = Path::rect(
            bx + b[0] - pad,
            by - vm.cap_height - pad,
            bx + b[2] + pad,
            by + vm.descent * 0.6 + pad,
        );
        let opacity = (o.opacity / 100.).clamp(0., 1.);
        let mut layer = if opacity >= 0.999 {
            p.to_vec()
        } else {
            vec![0.; p.len()]
        };
        if let Some(b) = path.bounds() {
            let (x, y) = (b.0.floor() as i32, b.1.floor() as i32);
            let mask = filmcraft_text::raster::fill(
                &path,
                (b.2.ceil() as i32 - x).max(1) as usize,
                (b.3.ceil() as i32 - y).max(1) as usize,
                -x as f32,
                -y as f32,
            );
            source_mask(&mut layer, w, h, &mask, x, y, [0., 0., 0., 1.], 0.6);
        }
        let (mask, x, y) = render::rasterize(&l, &render::at(bx, by.round()));
        source_mask(&mut layer, w, h, &mask, x, y, [1.; 4], 1.);
        if opacity >= 0.999 {
            p.copy_from_slice(&layer)
        } else if opacity > 0. {
            for (d, s) in p.chunks_exact_mut(4).zip(layer.chunks_exact(4)) {
                if s[3] <= 0. {
                    continue;
                }
                let k = 1. - s[3] * opacity;
                for c in 0..4 {
                    d[c] = s[c] * opacity + d[c] * k;
                }
            }
        }
    }
    #[test]
    fn clipped_masks_match_original_export_label() {
        for placement in [
            Placement::TopLeft,
            Placement::TopCenter,
            Placement::TopRight,
            Placement::CenterLeft,
            Placement::Center,
            Placement::CenterRight,
            Placement::BottomLeft,
            Placement::BottomCenter,
            Placement::BottomRight,
        ] {
            for opacity in [0., 50., 99.9, 100.] {
                for text in ["Filmcraft", "Таймкод 00:01:02;03", "First\nSecond"] {
                    let o = TextOverlay {
                        enabled: true,
                        placement,
                        opacity,
                        offset: (-7.25, 3.125),
                        size_percent: 17.,
                        ..Default::default()
                    };
                    let mut actual = [0.12, 0.08, 0.04, 0.4].repeat(192 * 108);
                    let mut expected = actual.clone();
                    paint(&mut actual, 192, 108, &o, text, &mut || Ok(())).unwrap();
                    reference(&mut expected, 192, 108, &o, text);
                    assert_eq!(actual, expected, "{placement:?} {opacity} {text}");
                }
            }
        }
    }
    #[test]
    fn cancellation_and_invalid_values_are_reported() {
        let mut p = vec![0.; 32 * 32 * 4];
        let mut o = TextOverlay {
            enabled: true,
            ..Default::default()
        };
        assert!(paint(&mut p, 32, 32, &o, "A", &mut || anyhow::bail!("cancelled")).is_err());
        o.offset.0 = f32::NAN;
        assert!(o.validate().is_err());
        assert!(p.iter().all(|v| *v == 0.));
    }
}
