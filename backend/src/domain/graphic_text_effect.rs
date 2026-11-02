//! Atomic graphic text layer compositing into the native working frame.
use super::{
    graphic_text::GraphicTextParameters, graphic_text_render::raster_layer,
    linear_color_frame::LinearColorFrame,
};
use anyhow::Result;
impl GraphicTextParameters {
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let (layer, matrix) = self.evaluate(true, (w as u32, h as u32))?;
        checkpoint()?;
        let Some(raster) = raster_layer(&layer, &matrix, w, h) else {
            return Ok(());
        };
        checkpoint()?;
        let mut output = frame.clone();
        let mut index = 0;
        output.map_premultiplied(
            &mut |pixel| {
                let (x, y) = ((index % w) as i64, (index / w) as i64);
                index += 1;
                let (sx, sy) = (x - raster.x as i64, y - raster.y as i64);
                if sx < 0 || sy < 0 || sx >= raster.img.w as i64 || sy >= raster.img.h as i64 {
                    return pixel;
                }
                let offset = (sy as usize * raster.img.w + sx as usize) * 4;
                let s = &raster.img.px[offset..offset + 4];
                if s[3] <= 0. {
                    return pixel;
                }
                let k = 1. - s[3];
                std::array::from_fn(|i| s[i] + pixel[i] * k)
            },
            checkpoint,
        )?;
        checkpoint()?;
        *frame = output;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn every_checkpoint_preserves_input() {
        let source = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 12,
                height: 8,
                rgba: vec![128; 384],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let p = GraphicTextParameters {
            text: "AB".into(),
            size: 8.,
            shadow: true,
            ..Default::default()
        };
        let mut count = 0;
        p.apply_linear(&mut source.clone(), &mut || {
            count += 1;
            Ok(())
        })
        .unwrap();
        assert!(count > 8);
        for stop in 1..=count {
            let mut frame = source.clone();
            let mut seen = 0;
            assert!(p
                .apply_linear(&mut frame, &mut || {
                    seen += 1;
                    anyhow::ensure!(seen != stop, "cancelled");
                    Ok(())
                })
                .is_err());
            assert_eq!(frame.pixels(), source.pixels(), "checkpoint{stop}");
        }
    }
}
