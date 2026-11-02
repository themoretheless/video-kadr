//! Native group surface composition. Inputs and outputs are straight RGBA8;
//! filtering/blending uses premultiplied color to preserve translucent groups.
use super::{composition::BlendMode, group_transform::GroupAffine};
use anyhow::{ensure, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct GroupSurface {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}
impl GroupSurface {
    pub fn transparent(width: usize, height: usize) -> Result<Self> {
        ensure!(
            width > 0 && width <= 3840 && height > 0 && height <= 2160,
            "invalid group surface dimensions"
        );
        Ok(Self {
            width,
            height,
            rgba: vec![0; width * height * 4],
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.width > 0
                && self.width <= 3840
                && self.height > 0
                && self.height <= 2160
                && self.rgba.len() == self.width * self.height * 4,
            "invalid RGBA group surface"
        );
        Ok(())
    }
    pub fn composite_normal(
        &mut self,
        child: &Self,
        matrix: GroupAffine,
        opacity: f64,
    ) -> Result<()> {
        self.composite(child, matrix, opacity, BlendMode::Normal)
    }
    pub fn composite(
        &mut self,
        child: &Self,
        matrix: GroupAffine,
        opacity: f64,
        blend: BlendMode,
    ) -> Result<()> {
        self.composite_checked(child, matrix, opacity, blend, &mut || Ok(()))
    }
    /// Cancellation may leave this working surface partially composed; callers
    /// must discard it on error rather than publish an incomplete frame.
    pub fn composite_checked(
        &mut self,
        child: &Self,
        matrix: GroupAffine,
        opacity: f64,
        blend: BlendMode,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        checkpoint()?;
        self.validate()?;
        child.validate()?;
        ensure!(
            opacity.is_finite() && (0.0..=1.0).contains(&opacity),
            "invalid group opacity"
        );
        let [a, b, c, d, x, y] = matrix.0;
        ensure!(
            matrix.0.into_iter().all(f64::is_finite),
            "invalid group matrix"
        );
        let determinant = a * d - b * c;
        ensure!(
            determinant.is_finite() && determinant != 0.0,
            "singular group matrix"
        );
        let inverse = [
            d / determinant,
            -c / determinant,
            -b / determinant,
            a / determinant,
        ];
        ensure!(
            inverse.into_iter().all(f64::is_finite),
            "invalid inverse group matrix"
        );
        for row in 0..self.height {
            checkpoint()?;
            for column in 0..self.width {
                let px = column as f64 + 0.5 - x;
                let py = row as f64 + 0.5 - y;
                let u = inverse[0] * px + inverse[1] * py;
                let v = inverse[2] * px + inverse[3] * py;
                if !u.is_finite()
                    || !v.is_finite()
                    || u < 0.0
                    || v < 0.0
                    || u >= child.width as f64
                    || v >= child.height as f64
                {
                    continue;
                }
                let source = (v.floor() as usize * child.width + u.floor() as usize) * 4;
                let destination = (row * self.width + column) * 4;
                let sa = child.rgba[source + 3] as f64 / 255.0 * opacity;
                let da = self.rgba[destination + 3] as f64 / 255.0;
                let alpha = sa + da * (1.0 - sa);
                for channel in 0..3 {
                    self.rgba[destination + channel] = if alpha > 0.0 {
                        let cs = child.rgba[source + channel] as f64 / 255.0;
                        let cb = self.rgba[destination + channel] as f64 / 255.0;
                        let mixed = match blend {
                            BlendMode::Normal => cs,
                            BlendMode::Multiply => cb * cs,
                            BlendMode::Screen => cb + cs - cb * cs,
                            BlendMode::Overlay => {
                                if cb <= 0.5 {
                                    2.0 * cb * cs
                                } else {
                                    1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
                                }
                            }
                            BlendMode::Darken => cb.min(cs),
                            BlendMode::Lighten => cb.max(cs),
                            BlendMode::Difference => (cb - cs).abs(),
                            BlendMode::Addition => (cb + cs).min(1.0),
                        };
                        ((cs * sa * (1.0 - da) + mixed * sa * da + cb * da * (1.0 - sa)) * 255.0
                            / alpha)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    } else {
                        0
                    };
                }
                self.rgba[destination + 3] = (alpha * 255.0).round() as u8;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raster_checkpoint_stops_before_next_row() {
        let child = GroupSurface {
            width: 2,
            height: 2,
            rgba: [255, 0, 0, 255].repeat(4),
        };
        let mut parent = GroupSurface::transparent(2, 2).unwrap();
        let mut checks = 0;
        let result = parent.composite_checked(
            &child,
            GroupAffine::IDENTITY,
            1.0,
            BlendMode::Normal,
            &mut || {
                checks += 1;
                ensure!(checks < 3, "cancelled");
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(&parent.rgba[..8], &[255, 0, 0, 255].repeat(2));
        assert_eq!(&parent.rgba[8..], &[0; 8]);
    }
    #[test]
    fn blend_modes_preserve_transparent_backdrops_and_mix_opaque_pixels() {
        let child = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![192, 192, 192, 255],
        };
        for (mode, expected) in [
            (BlendMode::Normal, 192),
            (BlendMode::Multiply, 48),
            (BlendMode::Screen, 208),
            (BlendMode::Overlay, 96),
            (BlendMode::Darken, 64),
            (BlendMode::Lighten, 192),
            (BlendMode::Difference, 128),
            (BlendMode::Addition, 255),
        ] {
            let mut parent = GroupSurface {
                width: 1,
                height: 1,
                rgba: vec![64, 64, 64, 255],
            };
            parent
                .composite(&child, GroupAffine::IDENTITY, 1.0, mode)
                .unwrap();
            assert_eq!(parent.rgba, [expected, expected, expected, 255], "{mode:?}");
            let mut clear = GroupSurface::transparent(1, 1).unwrap();
            clear
                .composite(&child, GroupAffine::IDENTITY, 0.5, mode)
                .unwrap();
            assert_eq!(clear.rgba, [192, 192, 192, 128], "{mode:?}");
        }
    }
    #[test]
    fn opacity_applies_once_to_finished_group_and_preserves_straight_color() {
        let red = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        };
        let blue = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 255, 255],
        };
        let mut group = GroupSurface::transparent(1, 1).unwrap();
        group
            .composite_normal(&red, GroupAffine::IDENTITY, 1.0)
            .unwrap();
        group
            .composite_normal(&blue, GroupAffine::IDENTITY, 1.0)
            .unwrap();
        let mut root = GroupSurface::transparent(1, 1).unwrap();
        root.composite_normal(&group, GroupAffine::IDENTITY, 0.5)
            .unwrap();
        assert_eq!(root.rgba, [0, 0, 255, 128]);
        let mut independently_faded = GroupSurface::transparent(1, 1).unwrap();
        independently_faded
            .composite_normal(&red, GroupAffine::IDENTITY, 0.5)
            .unwrap();
        independently_faded
            .composite_normal(&blue, GroupAffine::IDENTITY, 0.5)
            .unwrap();
        assert_ne!(independently_faded.rgba, root.rgba);
    }
    #[test]
    fn translated_surface_clips_to_parent_and_invalid_input_is_unchanged() {
        let child = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![10, 20, 30, 128],
        };
        let mut parent = GroupSurface::transparent(2, 1).unwrap();
        parent
            .composite_normal(&child, GroupAffine([1.0, 0.0, 0.0, 1.0, 1.0, 0.0]), 1.0)
            .unwrap();
        assert_eq!(parent.rgba, [0, 0, 0, 0, 10, 20, 30, 128]);
        let before = parent.clone();
        assert!(parent
            .composite_normal(&child, GroupAffine([0.0; 6]), 1.0)
            .is_err());
        assert_eq!(parent, before);
        assert!(parent
            .composite_normal(&child, GroupAffine([1e-320, 0.0, 0.0, 1.0, 0.0, 0.0]), 1.0)
            .is_err());
        assert_eq!(parent, before);
    }
}
