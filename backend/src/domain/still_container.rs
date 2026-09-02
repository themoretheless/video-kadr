//! Still-image container, brand, codec, item, and metadata semantics.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StillContainer {
    Avif,
    Heif,
    JpegXl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StillCodec {
    Av1,
    Hevc,
    JpegXl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StillBrand {
    Avif,
    Avis,
    Heic,
    Heix,
    Jxl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataKind {
    Exif,
    Icc,
    Xmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StillItemRole {
    Primary,
    AlphaAuxiliary,
    Thumbnail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StillImageItem {
    pub id: u32,
    pub role: StillItemRole,
    pub codec: StillCodec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPrimaries {
    Bt709,
    DisplayP3,
    Bt2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageOrientation {
    Identity,
    Rotate90,
    Rotate180,
    Rotate270,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StillImageMetadata {
    pub color_primaries: ColorPrimaries,
    pub has_icc_profile: bool,
    pub alpha: bool,
    pub orientation: ImageOrientation,
    pub blocks: BTreeSet<MetadataKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StillContainerSpec {
    pub container: StillContainer,
    pub codec: StillCodec,
    pub major_brand: StillBrand,
    pub items: Vec<StillImageItem>,
    pub metadata: StillImageMetadata,
}

impl StillContainerSpec {
    pub fn validate(&self) -> Result<(), StillContainerError> {
        let brand_codec_match = match self.container {
            StillContainer::Avif => {
                self.codec == StillCodec::Av1
                    && matches!(self.major_brand, StillBrand::Avif | StillBrand::Avis)
            }
            StillContainer::Heif => {
                self.codec == StillCodec::Hevc
                    && matches!(self.major_brand, StillBrand::Heic | StillBrand::Heix)
            }
            StillContainer::JpegXl => {
                self.codec == StillCodec::JpegXl && self.major_brand == StillBrand::Jxl
            }
        };
        if !brand_codec_match {
            return Err(StillContainerError::IncompatibleBrandOrCodec);
        }
        let mut ids = BTreeSet::new();
        if self.items.is_empty()
            || self
                .items
                .iter()
                .any(|item| item.codec != self.codec || !ids.insert(item.id))
        {
            return Err(StillContainerError::MissingPrimaryImage);
        }
        let primary_count = self
            .items
            .iter()
            .filter(|item| item.role == StillItemRole::Primary)
            .count();
        let has_alpha_item = self
            .items
            .iter()
            .any(|item| item.role == StillItemRole::AlphaAuxiliary);
        let sequence_brand = matches!(self.major_brand, StillBrand::Avis);
        if (primary_count > 1) != sequence_brand || primary_count == 0 {
            return Err(StillContainerError::InvalidSequenceBrand);
        }
        if has_alpha_item != self.metadata.alpha {
            return Err(StillContainerError::AlphaItemMismatch);
        }
        Ok(())
    }

    pub fn metadata_round_trip(&self, decoded: &StillImageMetadata) -> MetadataRoundTrip {
        let preserved = self
            .metadata
            .blocks
            .intersection(&decoded.blocks)
            .copied()
            .collect();
        let lost = self
            .metadata
            .blocks
            .difference(&decoded.blocks)
            .copied()
            .collect();
        let introduced = decoded
            .blocks
            .difference(&self.metadata.blocks)
            .copied()
            .collect();
        MetadataRoundTrip {
            preserved,
            lost,
            introduced,
            color_preserved: self.metadata.color_primaries == decoded.color_primaries,
            icc_preserved: self.metadata.has_icc_profile == decoded.has_icc_profile,
            alpha_preserved: self.metadata.alpha == decoded.alpha,
            orientation_preserved: self.metadata.orientation == decoded.orientation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataRoundTrip {
    pub preserved: BTreeSet<MetadataKind>,
    pub lost: BTreeSet<MetadataKind>,
    pub introduced: BTreeSet<MetadataKind>,
    pub color_preserved: bool,
    pub icc_preserved: bool,
    pub alpha_preserved: bool,
    pub orientation_preserved: bool,
}

impl MetadataRoundTrip {
    pub fn is_lossless(&self) -> bool {
        self.lost.is_empty()
            && self.introduced.is_empty()
            && self.color_preserved
            && self.icc_preserved
            && self.alpha_preserved
            && self.orientation_preserved
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StillContainerError {
    IncompatibleBrandOrCodec,
    MissingPrimaryImage,
    InvalidSequenceBrand,
    AlphaItemMismatch,
}

impl fmt::Display for StillContainerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid still-image container: {self:?}")
    }
}

impl std::error::Error for StillContainerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_single_image_and_sequence_brands() {
        let single = StillContainerSpec {
            container: StillContainer::Avif,
            codec: StillCodec::Av1,
            major_brand: StillBrand::Avif,
            items: vec![
                StillImageItem {
                    id: 1,
                    role: StillItemRole::Primary,
                    codec: StillCodec::Av1,
                },
                StillImageItem {
                    id: 2,
                    role: StillItemRole::AlphaAuxiliary,
                    codec: StillCodec::Av1,
                },
            ],
            metadata: StillImageMetadata {
                color_primaries: ColorPrimaries::Bt709,
                has_icc_profile: false,
                alpha: true,
                orientation: ImageOrientation::Identity,
                blocks: BTreeSet::new(),
            },
        };
        single.validate().unwrap();
        let mut sequence = single.clone();
        sequence.items.push(StillImageItem {
            id: 3,
            role: StillItemRole::Primary,
            codec: StillCodec::Av1,
        });
        assert_eq!(
            sequence.validate(),
            Err(StillContainerError::InvalidSequenceBrand)
        );
        sequence.major_brand = StillBrand::Avis;
        sequence.validate().unwrap();
    }
}
