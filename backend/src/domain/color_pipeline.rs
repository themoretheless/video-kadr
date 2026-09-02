//! Explicit color/HDR contracts shared by probe, preview, and export adapters.

use serde::{Deserialize, Serialize};

use super::artifact_graph::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPrimaries {
    Unspecified,
    Bt709,
    Bt2020,
    P3D65,
    AcesAp0,
    AcesAp1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferFunction {
    Unspecified,
    Srgb,
    Bt1886,
    Pq,
    Hlg,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatrixCoefficients {
    Unspecified,
    Identity,
    Bt709,
    Bt2020Ncl,
    Bt2020Cl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorRange {
    Unspecified,
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromaLocation {
    Unspecified,
    Left,
    Center,
    TopLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorDescriptor {
    pub primaries: ColorPrimaries,
    pub transfer: TransferFunction,
    pub matrix: MatrixCoefficients,
    pub range: ColorRange,
    pub chroma_location: ChromaLocation,
}

impl ColorDescriptor {
    pub const SDR_BT709: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferFunction::Bt1886,
        matrix: MatrixCoefficients::Bt709,
        range: ColorRange::Limited,
        chroma_location: ChromaLocation::Left,
    };

    pub fn is_hdr(self) -> bool {
        matches!(self.transfer, TransferFunction::Pq | TransferFunction::Hlg)
    }

    pub fn is_explicit(self) -> bool {
        !matches!(self.primaries, ColorPrimaries::Unspecified)
            && !matches!(self.transfer, TransferFunction::Unspecified)
            && !matches!(self.matrix, MatrixCoefficients::Unspecified)
            && !matches!(self.range, ColorRange::Unspecified)
            && !matches!(self.chroma_location, ChromaLocation::Unspecified)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HdrMetadata {
    pub max_cll_nits: Option<f64>,
    pub max_fall_nits: Option<f64>,
    pub mastering_min_nits: Option<f64>,
    pub mastering_max_nits: Option<f64>,
}

impl HdrMetadata {
    pub fn validate(self) -> Result<Self, &'static str> {
        let values = [
            self.max_cll_nits,
            self.max_fall_nits,
            self.mastering_min_nits,
            self.mastering_max_nits,
        ];
        if values
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite() || !(0.0..=100_000.0).contains(&value))
        {
            return Err("HDR luminance is outside the supported range");
        }
        if self
            .max_fall_nits
            .zip(self.max_cll_nits)
            .is_some_and(|(fall, cll)| fall > cll)
        {
            return Err("MaxFALL exceeds MaxCLL");
        }
        if self
            .mastering_min_nits
            .zip(self.mastering_max_nits)
            .is_some_and(|(min, max)| min > max)
        {
            return Err("mastering minimum exceeds maximum");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorConfigIdentity {
    pub name: String,
    pub version: String,
    pub checksum: Fingerprint,
}

impl ColorConfigIdentity {
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::combine([
            self.name.as_bytes(),
            self.version.as_bytes(),
            self.checksum.as_str().as_bytes(),
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingSpace {
    SourceNative,
    AcesCg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorTransformNode {
    pub from: ColorDescriptor,
    pub to: ColorDescriptor,
    pub purpose: &'static str,
}

pub fn working_graph(
    source: ColorDescriptor,
    working: WorkingSpace,
    output: ColorDescriptor,
) -> Vec<ColorTransformNode> {
    let acescg = ColorDescriptor {
        primaries: ColorPrimaries::AcesAp1,
        transfer: TransferFunction::Linear,
        matrix: MatrixCoefficients::Identity,
        range: ColorRange::Full,
        chroma_location: ChromaLocation::Center,
    };
    match working {
        WorkingSpace::SourceNative if source == output => Vec::new(),
        WorkingSpace::SourceNative => vec![ColorTransformNode {
            from: source,
            to: output,
            purpose: "output",
        }],
        WorkingSpace::AcesCg => vec![
            ColorTransformNode {
                from: source,
                to: acescg,
                purpose: "working-space",
            },
            ColorTransformNode {
                from: acescg,
                to: output,
                purpose: "output",
            },
        ],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewDisplayProfile {
    pub descriptor: ColorDescriptor,
    pub peak_nits: f64,
    pub tone_map: ToneMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMap {
    None,
    Bt2390,
    Hable,
}

impl PreviewDisplayProfile {
    pub fn validate(self) -> Result<Self, &'static str> {
        if !self.peak_nits.is_finite() || self.peak_nits <= 0.0 || self.peak_nits > 10_000.0 {
            return Err("invalid display peak");
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HdrFrameArtifact {
    pub width: u32,
    pub height: u32,
    pub descriptor: ColorDescriptor,
    pub sample_format: HdrSampleFormat,
    pub checksum: Fingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdrSampleFormat {
    Float16,
    Float32,
}

pub trait HdrFramePort: Send + Sync {
    fn encode(&self, artifact: &HdrFrameArtifact, rgba: &[f32]) -> Result<Vec<u8>, String>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelTolerance {
    pub absolute: f32,
    pub delta_e: f32,
}

pub fn pixels_within_tolerance(
    reference: &[[f32; 4]],
    actual: &[[f32; 4]],
    tolerance: PixelTolerance,
) -> bool {
    reference.len() == actual.len()
        && reference.iter().zip(actual).all(|(expected, observed)| {
            expected
                .iter()
                .zip(observed)
                .all(|(left, right)| (*left - *right).abs() <= tolerance.absolute)
        })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneColorQa {
    pub start_ticks: i64,
    pub end_ticks: i64,
    pub clipped_ratio: f64,
    pub out_of_gamut_ratio: f64,
    pub delta_e_p95: f64,
    pub histogram: Vec<u64>,
}

impl SceneColorQa {
    pub fn validate(&self) -> bool {
        self.start_ticks >= 0
            && self.end_ticks > self.start_ticks
            && [self.clipped_ratio, self.out_of_gamut_ratio]
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            && self.delta_e_p95.is_finite()
            && self.delta_e_p95 >= 0.0
            && !self.histogram.is_empty()
    }
}

pub fn validate_color_round_trip(
    before: ColorDescriptor,
    after: ColorDescriptor,
    before_hdr: Option<HdrMetadata>,
    after_hdr: Option<HdrMetadata>,
) -> Result<(), &'static str> {
    if before != after {
        return Err("color descriptor changed");
    }
    if before_hdr != after_hdr {
        return Err("HDR mastering metadata changed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_descriptor_and_hdr_metadata_fail_closed() {
        assert!(ColorDescriptor::SDR_BT709.is_explicit());
        assert!(!ColorDescriptor::SDR_BT709.is_hdr());
        assert!(HdrMetadata {
            max_cll_nits: Some(1_000.0),
            max_fall_nits: Some(400.0),
            mastering_min_nits: Some(0.005),
            mastering_max_nits: Some(1_000.0)
        }
        .validate()
        .is_ok());
        assert!(HdrMetadata {
            max_cll_nits: Some(400.0),
            max_fall_nits: Some(500.0),
            mastering_min_nits: None,
            mastering_max_nits: None
        }
        .validate()
        .is_err());
    }

    #[test]
    fn working_and_preview_profiles_cannot_rewrite_export_descriptor() {
        let hdr = ColorDescriptor {
            primaries: ColorPrimaries::Bt2020,
            transfer: TransferFunction::Pq,
            matrix: MatrixCoefficients::Bt2020Ncl,
            range: ColorRange::Limited,
            chroma_location: ChromaLocation::Left,
        };
        let graph = working_graph(hdr, WorkingSpace::AcesCg, hdr);
        assert_eq!(graph.len(), 2);
        let preview = PreviewDisplayProfile {
            descriptor: ColorDescriptor::SDR_BT709,
            peak_nits: 200.0,
            tone_map: ToneMap::Bt2390,
        }
        .validate()
        .unwrap();
        assert_ne!(preview.descriptor, hdr);
        assert_eq!(graph.last().unwrap().to, hdr);
    }

    #[test]
    fn config_frame_pixel_and_qa_contracts_are_content_bound() {
        let config = ColorConfigIdentity {
            name: "studio".into(),
            version: "2".into(),
            checksum: Fingerprint::digest(b"ocio"),
        };
        assert_ne!(config.fingerprint(), Fingerprint::digest(b"path/to/config"));
        assert!(pixels_within_tolerance(
            &[[1.2, 0.1, 0.2, 1.0]],
            &[[1.19, 0.1, 0.2, 1.0]],
            PixelTolerance {
                absolute: 0.02,
                delta_e: 1.0
            }
        ));
        assert!(SceneColorQa {
            start_ticks: 0,
            end_ticks: 90_000,
            clipped_ratio: 0.01,
            out_of_gamut_ratio: 0.02,
            delta_e_p95: 0.7,
            histogram: vec![0, 4, 2]
        }
        .validate());
    }
}
