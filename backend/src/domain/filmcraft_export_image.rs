//! Original Filmcraft export watermark (5231852443363f001c3f6b396dd9b1e6461ae2be).
//! MIT: docs/licenses/filmcraft-MIT.txt. Filesystem access belongs to the host.
use super::{
    filmcraft_export_text::Placement,
    group_surface::GroupSurface,
    transition_kernel_pack::{context::RenderContext, image::Image},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageOverlay {
    pub enabled: bool,
    pub source_id: String,
    pub placement: Placement,
    pub offset: (f32, f32),
    pub size_percent: f32,
    pub opacity: f32,
}
impl Default for ImageOverlay {
    fn default() -> Self {
        Self {
            enabled: false,
            source_id: String::new(),
            placement: Placement::TopRight,
            offset: (0., 0.),
            size_percent: 15.,
            opacity: 100.,
        }
    }
}
impl ImageOverlay {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.source_id.len() <= 256 && !self.source_id.contains('\0'),
            "invalid image overlay source"
        );
        ensure!(
            !self.enabled || !self.source_id.trim().is_empty(),
            "image overlay source required"
        );
        ensure!(
            self.offset.0.is_finite()
                && self.offset.1.is_finite()
                && self.offset.0.abs() <= 1_000_000.
                && self.offset.1.abs() <= 1_000_000.
                && self.size_percent.is_finite()
                && self.opacity.is_finite(),
            "invalid image overlay geometry or opacity"
        );
        Ok(())
    }
}
pub type ResolvedImage = std::sync::Arc<(ImageOverlay, GroupSurface)>;

/// Decode straight sRGB to linear premultiplied pixels, then original triangle resize.
pub fn prepare(
    context: &RenderContext,
    surface: &GroupSurface,
    settings: &ImageOverlay,
    width: usize,
) -> Result<Image> {
    settings.validate()?;
    surface.validate()?;
    let nw = (width as f32 * settings.size_percent.clamp(0.5, 100.) / 100.)
        .round()
        .max(1.) as usize;
    let nh = (surface.height as f32 * nw as f32 / surface.width.max(1) as f32)
        .round()
        .max(1.) as usize;
    ensure!(
        nw <= 3840 && nh <= 2160,
        "image overlay exceeds renderer budget"
    );
    let mut image = context
        .image_work(&[], || {
            let mut image = Image::new(surface.width, surface.height);
            for y in 0..surface.height {
                if context.is_cancelled() {
                    return Err("image overlay cancelled".into());
                }
                for x in 0..surface.width {
                    let i = (y * surface.width + x) * 4;
                    let a = surface.rgba[i + 3] as f32 / 255.;
                    for c in 0..3 {
                        image.px[i + c] =
                            filmcraft_color::srgb_to_linear(surface.rgba[i + c] as f32 / 255.) * a;
                    }
                    image.px[i + 3] = a;
                }
            }
            Ok(image)
        })
        .map_err(anyhow::Error::msg)?;
    if image.w != nw {
        image = super::filmcraft_export_geometry::resample_axis(context, &image, nw, true)?;
    }
    if image.h != nh {
        image = super::filmcraft_export_geometry::resample_axis(context, &image, nh, false)?;
    }
    Ok(image)
}
pub fn composite(
    context: &RenderContext,
    destination: &mut Image,
    source: &Image,
    settings: &ImageOverlay,
) -> Result<()> {
    settings.validate()?;
    let (ax, ay) = settings.placement.anchor();
    let m = 0.03 * destination.w.min(destination.h) as f32;
    let x = (m + ax * (destination.w as f32 - 2. * m - source.w as f32) + settings.offset.0).round()
        as i64;
    let y = (m + ay * (destination.h as f32 - 2. * m - source.h as f32) + settings.offset.1).round()
        as i64;
    let op = (settings.opacity / 100.).clamp(0., 1.);
    if op <= 0. {
        return Ok(());
    }
    let xs = x.max(0)..(x + source.w as i64).min(destination.w as i64);
    for dy in y.max(0)..(y + source.h as i64).min(destination.h as i64) {
        ensure!(!context.is_cancelled(), "image overlay cancelled");
        for dx in xs.clone() {
            let si = ((dy - y) as usize * source.w + (dx - x) as usize) * 4;
            let s = &source.px[si..si + 4];
            if s[3] <= 0. {
                continue;
            }
            let k = 1. - s[3] * op;
            let di = (dy as usize * destination.w + dx as usize) * 4;
            for c in 0..4 {
                destination.px[di + c] = s[c] * op + destination.px[di + c] * k;
            }
        }
    }
    Ok(())
}
