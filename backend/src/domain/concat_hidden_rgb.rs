//! Compact scene-linear RGB for zero-alpha Concat pixels; ordinary pixels remain premultiplied.
use anyhow::{ensure, Result};
#[derive(Debug, Clone)]
pub(crate) struct HiddenRgb {
    mask: Vec<u64>,
    offsets: Vec<u32>,
    colors: Vec<[half::f16; 3]>,
}
impl HiddenRgb {
    /// Sorted explicit straight colors for an alpha-changing Concat pass.
    pub(crate) fn from_entries(
        count: usize,
        entries: &[(usize, [f32; 3])],
    ) -> Result<Option<Self>> {
        if entries.is_empty() {
            return Ok(None);
        }
        let mut mask = vec![0u64; count.div_ceil(64)];
        let mut colors = Vec::with_capacity(entries.len());
        let mut previous = None;
        for &(index, color) in entries {
            ensure!(
                index < count && previous.is_none_or(|p| p < index),
                "invalid transparent color indices"
            );
            ensure!(
                color.iter().all(|v| v.is_finite() && v.abs() <= 65504.),
                "invalid transparent color"
            );
            mask[index / 64] |= 1u64 << (index % 64);
            colors.push(color.map(half::f16::from_f32));
            previous = Some(index);
        }
        let mut offsets = Vec::with_capacity(mask.len());
        let mut total = 0u32;
        for word in &mask {
            offsets.push(total);
            total = total
                .checked_add(word.count_ones())
                .ok_or_else(|| anyhow::anyhow!("transparent count overflow"))?;
        }
        Ok(Some(Self {
            mask,
            offsets,
            colors,
        }))
    }
    pub(crate) fn target_zero(p: [f32; 4]) -> bool {
        p[3] <= 0. || half::f16::from_f32(p[3]).to_f32() == 0.
    }
    pub(crate) fn mask(
        pixels: &[[f32; 4]],
        width: usize,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Option<Vec<u64>>> {
        let mut result: Option<Vec<u64>> = None;
        for (y, row) in pixels.chunks(width).enumerate() {
            checkpoint()?;
            for (x, p) in row.iter().enumerate() {
                if Self::target_zero(*p) {
                    let mask = result.get_or_insert_with(|| vec![0; pixels.len().div_ceil(64)]);
                    let index = y * width + x;
                    mask[index / 64] |= 1u64 << (index % 64);
                }
            }
        }
        Ok(result)
    }
    pub(crate) fn matches(&self, mask: &[u64]) -> bool {
        self.mask == mask
    }
    fn rank(&self, index: usize) -> Option<usize> {
        let block = index / 64;
        let bit = index % 64;
        let word = *self.mask.get(block)?;
        if word & (1u64 << bit) == 0 {
            return None;
        }
        Some(self.offsets[block] as usize + (word & ((1u64 << bit) - 1)).count_ones() as usize)
    }
    pub(crate) fn get(&self, index: usize) -> Option<[f32; 3]> {
        self.rank(index)
            .map(|rank| self.colors[rank].map(half::f16::to_f32))
    }
    pub(crate) fn set(&mut self, index: usize, value: [f32; 3]) -> Result<()> {
        let rank = self
            .rank(index)
            .ok_or_else(|| anyhow::anyhow!("missing transparent Concat color"))?;
        self.colors[rank] = value.map(half::f16::from_f32);
        Ok(())
    }
    pub(crate) fn new(
        mask: Vec<u64>,
        pixels: &[[f32; 4]],
        template: &[u8],
        width: usize,
        old: Option<&Self>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        let mut offsets = Vec::with_capacity(mask.len());
        let mut count = 0u32;
        for word in &mask {
            offsets.push(count);
            count = count
                .checked_add(word.count_ones())
                .ok_or_else(|| anyhow::anyhow!("transparent color count overflow"))?;
        }
        let mut colors = Vec::with_capacity(count as usize);
        for (y, row) in pixels.chunks(width).enumerate() {
            checkpoint()?;
            for (x, p) in row.iter().enumerate() {
                let index = y * width + x;
                if mask[index / 64] & (1u64 << (index % 64)) == 0 {
                    continue;
                }
                let color = if p[3] > 0. {
                    std::array::from_fn(|i| p[i] / p[3])
                } else if let Some(color) = old.and_then(|old| old.get(index)) {
                    color
                } else {
                    std::array::from_fn(|i| {
                        super::basic_grade::linear(template[index * 4 + i] as f32 / 255.)
                    })
                };
                ensure!(
                    color.iter().all(|v| v.is_finite()),
                    "nonfinite transparent Concat color"
                );
                colors.push(color.map(half::f16::from_f32));
            }
        }
        Ok(Self {
            mask,
            offsets,
            colors,
        })
    }
}
