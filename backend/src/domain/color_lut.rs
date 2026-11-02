//! Self-contained FilmCraft LUT effect with bounded content-addressed parse cache.
use super::{
    group_surface::GroupSurface,
    lut_tables::{Lut, LutFormat},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableFormat {
    Cube,
    ThreeDl,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorLut {
    pub format: TableFormat,
    pub text: String,
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}
fn default_intensity() -> f32 {
    100.0
}
type Cache = HashMap<[u8; 32], Arc<Lut>>;
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}
impl ColorLut {
    pub fn table(&self) -> Result<Arc<Lut>> {
        ensure!(self.text.len() <= 16 * 1024 * 1024, "LUT exceeds 16MiB");
        ensure!(
            self.intensity.is_finite() && (0.0..=200.0).contains(&self.intensity),
            "invalid LUT intensity"
        );
        let mut hash = Sha256::new();
        hash.update([self.format as u8]);
        hash.update(self.text.as_bytes());
        let key: [u8; 32] = hash.finalize().into();
        if let Some(table) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Ok(table.clone());
        }
        let format = match self.format {
            TableFormat::Cube => LutFormat::Cube,
            TableFormat::ThreeDl => LutFormat::ThreeDl,
        };
        let table = Arc::new(Lut::parse(&self.text, Some(format)).map_err(anyhow::Error::msg)?);
        let bytes = |l: &Lut| {
            l.shaper.as_ref().map_or(0, |s| s.data.len() * 12)
                + l.cube.as_ref().map_or(0, |c| c.data.len() * 12)
                + l.title.len()
        };
        let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
        // Another render may have parsed the same content while this thread
        // was outside the lock. Retain its shared allocation instead of replacing it.
        if let Some(existing) = cache.get(&key) {
            return Ok(existing.clone());
        }
        if cache.len() >= 32
            || cache.values().map(|v| bytes(v)).sum::<usize>() + bytes(&table) > 64 * 1024 * 1024
        {
            cache.clear();
        }
        cache.insert(key, table.clone());
        Ok(table)
    }
    pub fn validate(&self) -> Result<()> {
        self.table()?;
        Ok(())
    }
    pub fn apply(
        &self,
        surface: &mut GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        surface.validate()?;
        checkpoint()?;
        let mut frame =
            super::linear_color_frame::LinearColorFrame::from_surface(surface, checkpoint)?;
        self.apply_linear(&mut frame, checkpoint)?;
        *surface = frame.to_surface(checkpoint)?;
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let table = self.table()?;
        checkpoint()?;
        frame.map_signal(
            &mut |rgb, _, _| {
                let mapped = table.apply(rgb);
                Ok([0, 1, 2].map(|c| rgb[c] + (mapped[c] - rgb[c]) * self.intensity / 100.0))
            },
            checkpoint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::lut_tables::{Lut1d, Lut3d};
    #[test]
    fn shared_lut_chain_preserves_sub_byte_changes_and_hidden_rgb() {
        let lut = ColorLut {
            format: TableFormat::Cube,
            text: Lut3d::from_fn(2, |v| v.map(|q| q + 0.2 / 255.0)).to_cube(),
            intensity: 100.0,
        };
        let source = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 128, 12, 34, 56, 0],
        };
        let mut frame =
            crate::domain::linear_color_frame::LinearColorFrame::from_surface(&source, &mut || {
                Ok(())
            })
            .unwrap();
        for _ in 0..3 {
            lut.apply_linear(&mut frame, &mut || Ok(())).unwrap();
        }
        assert_eq!(
            frame.to_surface(&mut || Ok(())).unwrap().rgba,
            vec![1, 1, 1, 128, 12, 34, 56, 0]
        );
    }
    #[test]
    fn shaper_tetrahedral_intensity_alpha_cache_and_cancellation() {
        let table = Lut {
            title: "Swap".into(),
            shaper: Some(Lut1d {
                domain_min: [0.0; 3],
                domain_max: [1.0; 3],
                data: vec![[0.0; 3], [0.5; 3]],
            }),
            cube: Some(Lut3d::from_fn(2, |v| [v[2], v[1], v[0]])),
        };
        let lut = ColorLut {
            format: TableFormat::Cube,
            text: table.to_cube(),
            intensity: 100.0,
        };
        assert!(Arc::ptr_eq(&lut.table().unwrap(), &lut.table().unwrap()));
        let mut image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 73],
        };
        lut.apply(&mut image, &mut || Ok(())).unwrap();
        assert_eq!(image.rgba, [0, 0, 128, 73]);
        let mut half = lut.clone();
        half.intensity = 50.0;
        image.rgba = vec![255, 0, 0, 73];
        half.apply(&mut image, &mut || Ok(())).unwrap();
        assert_eq!(image.rgba, [128, 0, 64, 73]);
        half.intensity = 0.0;
        image.rgba = vec![255, 0, 0, 73];
        half.apply(&mut image, &mut || Ok(())).unwrap();
        assert_eq!(image.rgba, [255, 0, 0, 73]);
        assert!(lut
            .apply(&mut image, &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn hostile_domains_tables_size_intensity_and_wire() {
        for text in [
            "LUT_1D_SIZE 2\nDOMAIN_MAX NaN 1 1\n0 0 0\n1 1 1",
            "LUT_1D_SIZE 2\nLUT_1D_INPUT_RANGE 1 0\n0 0 0\n1 1 1",
            "LUT_3D_SIZE 66",
            "LUT_1D_SIZE 2\n0 0 0\n1e999 1 1",
        ] {
            assert!(
                ColorLut {
                    format: TableFormat::Cube,
                    text: text.into(),
                    intensity: 100.0
                }
                .validate()
                .is_err(),
                "{text}"
            );
        }
        assert!(ColorLut {
            format: TableFormat::ThreeDl,
            text: "0 1023\n0 0 0".into(),
            intensity: 100.0
        }
        .validate()
        .is_err());
        assert!(ColorLut {
            format: TableFormat::Cube,
            text: Lut3d::identity(2).to_cube(),
            intensity: f32::NAN
        }
        .validate()
        .is_err());
        assert!(
            serde_json::from_str::<ColorLut>(r#"{"format":"cube","text":"","unknown":1}"#).is_err()
        );
    }
}
