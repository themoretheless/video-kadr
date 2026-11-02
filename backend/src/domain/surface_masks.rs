//! Local-raster masks, evaluated before layer transform and opacity.
use super::{
    composition::{MaskShape, VideoEffect},
    group_surface::GroupSurface,
    nested_composition::sample_value,
};
use anyhow::{ensure, Result};

pub fn supported(effect: &VideoEffect) -> bool {
    matches!(
        effect,
        VideoEffect::Mask {
            shape: MaskShape::Rectangle | MaskShape::Ellipse,
            ..
        } | VideoEffect::LinearMask { .. }
    )
}

pub fn apply(
    surface: &mut GroupSurface,
    effects: &[VideoEffect],
    local_us: impl Into<super::animation_sample_time::AnimationSampleTime>,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    let local_us = local_us.into();
    ensure!(
        effects.iter().all(supported),
        "unsupported native surface effect"
    );
    surface.validate()?;
    for effect in effects {
        checkpoint()?;
        let (shape, x, y, width, height, rotation, feather, inverted) = match effect {
            VideoEffect::Mask {
                shape,
                x,
                y,
                width,
                height,
                rotation_degrees,
                feather,
                inverted,
            } => (
                *shape,
                sample_value(x, local_us)?,
                sample_value(y, local_us)?,
                sample_value(width, local_us)?,
                sample_value(height, local_us)?,
                sample_value(rotation_degrees, local_us)?,
                *feather,
                *inverted,
            ),
            VideoEffect::LinearMask {
                x,
                y,
                rotation_degrees,
                feather,
                inverted,
            } => (
                MaskShape::Linear,
                sample_value(x, local_us)?,
                sample_value(y, local_us)?,
                1.0,
                1.0,
                sample_value(rotation_degrees, local_us)?,
                *feather,
                *inverted,
            ),
            _ => unreachable!(),
        };
        ensure!(
            [x, y, width, height, rotation, feather]
                .iter()
                .all(|v| v.is_finite())
                && width > 0.0
                && height > 0.0
                && (0.0..=1.0).contains(&feather),
            "invalid sampled mask"
        );
        let (sin, cos) = rotation.to_radians().sin_cos();
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        for (row, pixels) in surface.rgba.chunks_mut(surface.width * 4).enumerate() {
            checkpoint()?;
            for (column, pixel) in pixels.chunks_exact_mut(4).enumerate() {
                let dx = (column as f64 + 0.5) / surface.width as f64 - x;
                let dy = (row as f64 + 0.5) / surface.height as f64 - y;
                let rx = dx * cos + dy * sin;
                let ry = -dx * sin + dy * cos;
                let coverage = match shape {
                    MaskShape::Rectangle if feather == 0.0 => {
                        f64::from(rx.abs() <= half_width && ry.abs() <= half_height)
                    }
                    MaskShape::Rectangle => ((half_width - rx.abs()) / (half_width * feather))
                        .min((half_height - ry.abs()) / (half_height * feather))
                        .clamp(0.0, 1.0),
                    MaskShape::Ellipse => {
                        let distance = (rx / half_width).hypot(ry / half_height);
                        if feather == 0.0 {
                            f64::from(distance <= 1.0)
                        } else {
                            ((1.0 - distance) / feather).clamp(0.0, 1.0)
                        }
                    }
                    MaskShape::Linear if feather == 0.0 => f64::from(rx <= 0.0),
                    MaskShape::Linear => (0.5 - rx / (half_width * feather)).clamp(0.0, 1.0),
                };
                let coverage = if inverted { 1.0 - coverage } else { coverage };
                pixel[3] = (f64::from(pixel[3]) * coverage).round() as u8;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mask(mut value: serde_json::Value) -> VideoEffect {
        for key in ["x", "y", "width", "height"] {
            if let Some(number) = value.get(key).cloned() {
                value[key] = serde_json::json!({"mode":"constant", "value":number});
            }
        }
        serde_json::from_value(value).unwrap()
    }
    fn pixels() -> GroupSurface {
        GroupSurface {
            width: 4,
            height: 1,
            rgba: [10, 20, 30, 128].repeat(4),
        }
    }
    #[test]
    fn shapes_feather_inversion_and_multiple_masks_preserve_rgb() {
        let rectangle = mask(
            serde_json::json!({"kind":"mask","shape":"rectangle","x":0.5,"y":0.5,"width":0.5,"height":1.0,"feather":0.0,"inverted":false}),
        );
        let mut surface = pixels();
        apply(&mut surface, &[rectangle], 0, &mut || Ok(())).unwrap();
        assert_eq!(
            surface
                .rgba
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [0, 128, 128, 0]
        );
        assert!(surface.rgba.chunks_exact(4).all(|p| p[..3] == [10, 20, 30]));
        let linear = mask(
            serde_json::json!({"kind":"linear_mask","x":0.5,"y":0.5,"rotationDegrees":0.0,"feather":1.0,"inverted":false}),
        );
        let mut surface = pixels();
        apply(&mut surface, &[linear], 0, &mut || Ok(())).unwrap();
        assert_eq!(
            surface
                .rgba
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [128, 96, 32, 0]
        );
        let ellipse = mask(
            serde_json::json!({"kind":"mask","shape":"ellipse","x":0.5,"y":0.5,"width":0.5,"height":1.0,"feather":0.5,"inverted":true}),
        );
        apply(&mut surface, &[ellipse], 0, &mut || Ok(())).unwrap();
        assert_eq!(
            surface
                .rgba
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [128, 0, 0, 0]
        );
    }
    #[test]
    fn rotation_and_cancellation() {
        let effect = mask(
            serde_json::json!({"kind":"linear_mask","x":0.5,"y":0.5,"rotationDegrees":90.0,"feather":0.0,"inverted":false}),
        );
        let mut surface = GroupSurface {
            width: 1,
            height: 2,
            rgba: [10, 20, 30, 128].repeat(2),
        };
        apply(&mut surface, &[effect.clone()], 0, &mut || Ok(())).unwrap();
        assert_eq!(surface.rgba[3], 128);
        assert_eq!(surface.rgba[7], 0);
        assert!(apply(&mut surface, &[effect], 0, &mut || anyhow::bail!(
            "cancelled"
        ))
        .is_err());
    }
}
