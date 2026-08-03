//! Strict, deterministic SDR colour metadata policy.
//!
//! This module only describes and validates colour semantics. Filter graph and
//! encoder adapters consume it in later layers; they must not reinterpret raw
//! ffprobe strings independently.

use serde::{Deserialize, Serialize};

pub const SDR_COLOR_MANAGEMENT_V1: &str = "sdr-color-management-v1";
pub const LINEAR_WORKING_SPACE_V1: &str = "linear-bt709-d65-full-f32";
pub const ENCODED_WORKING_SPACE_V1: &str = "encoded-srgb-bt709-d65-full-f32";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPrimariesV1 {
    Bt709,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorTransferV1 {
    Bt709,
    Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMatrixV1 {
    Bt709,
    Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorRangeV1 {
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelModelV1 {
    Yuv,
    Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChromaLocationV1 {
    Left,
    Center,
    TopLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdrColorDescriptorV1 {
    pub primaries: ColorPrimariesV1,
    pub transfer: ColorTransferV1,
    pub matrix: ColorMatrixV1,
    pub range: ColorRangeV1,
    pub pixel_model: PixelModelV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma_location: Option<ChromaLocationV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorProvenanceV1 {
    Signaled,
    LegacyAssumedBt709,
    BrowserDecoded,
}

impl SdrColorDescriptorV1 {
    pub fn validate(self) -> Result<Self, ColorPolicyErrorV1> {
        match (self.pixel_model, self.matrix, self.range) {
            (PixelModelV1::Yuv, ColorMatrixV1::Bt709, _) => Ok(self),
            (PixelModelV1::Rgb, ColorMatrixV1::Rgb, ColorRangeV1::Full)
                if self.chroma_location.is_none() =>
            {
                Ok(self)
            }
            _ => Err(ColorPolicyErrorV1::ContradictoryMetadata),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedColorReasonV1 {
    MissingRange,
    MissingMatrix,
    MissingTransfer,
    MissingPrimaries,
    MissingPixelFormat,
    UnknownRange,
    UnknownMatrix,
    UnknownTransfer,
    UnknownPrimaries,
    UnknownPixelFormat,
    UnknownChromaLocation,
    Bt601Unsupported,
    HdrUnsupported,
    WideGamutUnsupported,
    ContradictoryMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ColorManagementStatusV1 {
    Supported {
        descriptor: SdrColorDescriptorV1,
        provenance: ColorProvenanceV1,
    },
    Unsupported {
        reason: UnsupportedColorReasonV1,
    },
    #[default]
    NotApplicable,
}

impl ColorManagementStatusV1 {
    pub fn descriptor(&self) -> Option<SdrColorDescriptorV1> {
        match self {
            Self::Supported { descriptor, .. } => Some(*descriptor),
            Self::Unsupported { .. } | Self::NotApplicable => None,
        }
    }

    pub fn rec709_limited_output() -> Self {
        Self::Supported {
            descriptor: SdrColorDescriptorV1 {
                primaries: ColorPrimariesV1::Bt709,
                transfer: ColorTransferV1::Bt709,
                matrix: ColorMatrixV1::Bt709,
                range: ColorRangeV1::Limited,
                pixel_model: PixelModelV1::Yuv,
                chroma_location: Some(ChromaLocationV1::Left),
            },
            provenance: ColorProvenanceV1::Signaled,
        }
    }

    pub fn srgb_full_output() -> Self {
        Self::Supported {
            descriptor: SdrColorDescriptorV1 {
                primaries: ColorPrimariesV1::Bt709,
                transfer: ColorTransferV1::Srgb,
                matrix: ColorMatrixV1::Rgb,
                range: ColorRangeV1::Full,
                pixel_model: PixelModelV1::Rgb,
                chroma_location: None,
            },
            provenance: ColorProvenanceV1::Signaled,
        }
    }

    /// Chroma location is container/codec dependent and some valid encoders
    /// (notably ProRes) omit it. The mandatory output contract is otherwise
    /// exact; when present, the normalizer has already validated its token.
    pub fn is_rec709_limited_output(&self) -> bool {
        matches!(
            self,
            Self::Supported {
                descriptor: SdrColorDescriptorV1 {
                    primaries: ColorPrimariesV1::Bt709,
                    transfer: ColorTransferV1::Bt709,
                    matrix: ColorMatrixV1::Bt709,
                    range: ColorRangeV1::Limited,
                    pixel_model: PixelModelV1::Yuv,
                    ..
                },
                provenance: ColorProvenanceV1::Signaled,
            }
        )
    }

    pub fn is_prores_rec709_limited_output(&self) -> bool {
        self.is_rec709_limited_output()
            && matches!(
                self,
                Self::Supported {
                    descriptor: SdrColorDescriptorV1 {
                        chroma_location: None | Some(ChromaLocationV1::Left),
                        ..
                    },
                    ..
                }
            )
    }

    pub fn is_srgb_full_output(&self) -> bool {
        self == &Self::srgb_full_output()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdrColorPolicyV1 {
    pub source: SdrColorDescriptorV1,
    pub linear_working_space: LinearWorkingSpaceV1,
    pub encoded_working_space: EncodedWorkingSpaceV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinearWorkingSpaceV1 {
    LinearBt709D65FullF32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EncodedWorkingSpaceV1 {
    EncodedSrgbBt709D65FullF32,
}

impl SdrColorPolicyV1 {
    pub fn new(source: SdrColorDescriptorV1) -> Result<Self, ColorPolicyErrorV1> {
        Ok(Self {
            source: source.validate()?,
            linear_working_space: LinearWorkingSpaceV1::LinearBt709D65FullF32,
            encoded_working_space: EncodedWorkingSpaceV1::EncodedSrgbBt709D65FullF32,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPolicyErrorV1 {
    ContradictoryMetadata,
}

#[derive(Debug, Clone, Copy)]
pub struct FfprobeColorFields<'a> {
    pub range: Option<&'a str>,
    pub matrix: Option<&'a str>,
    pub transfer: Option<&'a str>,
    pub primaries: Option<&'a str>,
    pub chroma_location: Option<&'a str>,
    pub pixel_format: Option<&'a str>,
}

fn token(value: Option<&str>) -> Option<String> {
    value.map(|value| value.trim().to_ascii_lowercase())
}

fn unsupported(reason: UnsupportedColorReasonV1) -> ColorManagementStatusV1 {
    ColorManagementStatusV1::Unsupported { reason }
}

/// Normalize ffprobe aliases without guessing absent or contradictory fields.
pub fn normalize_ffprobe_sdr_v1(fields: FfprobeColorFields<'_>) -> ColorManagementStatusV1 {
    let pixel_format = match token(fields.pixel_format) {
        None => return unsupported(UnsupportedColorReasonV1::MissingPixelFormat),
        Some(value) => value,
    };
    let pixel_model = if pixel_format.starts_with("yuv") || pixel_format.starts_with("nv") {
        PixelModelV1::Yuv
    } else if pixel_format.starts_with("rgb")
        || pixel_format.starts_with("bgr")
        || pixel_format.starts_with("gbr")
    {
        PixelModelV1::Rgb
    } else {
        return unsupported(UnsupportedColorReasonV1::UnknownPixelFormat);
    };

    let primaries = match token(fields.primaries).as_deref() {
        None | Some("") | Some("unknown") | Some("unspecified") => {
            return unsupported(UnsupportedColorReasonV1::MissingPrimaries)
        }
        Some("bt709" | "709") => ColorPrimariesV1::Bt709,
        Some("bt2020" | "bt2020nc" | "bt2020c" | "smpte432" | "display-p3") => {
            return unsupported(UnsupportedColorReasonV1::WideGamutUnsupported)
        }
        Some(_) => return unsupported(UnsupportedColorReasonV1::UnknownPrimaries),
    };
    let transfer = match token(fields.transfer).as_deref() {
        None | Some("") | Some("unknown") | Some("unspecified") => {
            return unsupported(UnsupportedColorReasonV1::MissingTransfer)
        }
        Some("bt709" | "709" | "bt1886") => ColorTransferV1::Bt709,
        Some("iec61966-2-1" | "srgb") => ColorTransferV1::Srgb,
        Some("smpte2084" | "pq" | "arib-std-b67" | "hlg") => {
            return unsupported(UnsupportedColorReasonV1::HdrUnsupported)
        }
        Some(_) => return unsupported(UnsupportedColorReasonV1::UnknownTransfer),
    };
    let matrix = match token(fields.matrix).as_deref() {
        None | Some("") | Some("unknown") | Some("unspecified") => {
            return unsupported(UnsupportedColorReasonV1::MissingMatrix)
        }
        Some("bt709" | "709") => ColorMatrixV1::Bt709,
        Some("rgb" | "gbr") => ColorMatrixV1::Rgb,
        Some("smpte170m" | "bt470bg" | "fcc") => {
            return unsupported(UnsupportedColorReasonV1::Bt601Unsupported)
        }
        Some("bt2020nc" | "bt2020c") => {
            return unsupported(UnsupportedColorReasonV1::WideGamutUnsupported)
        }
        Some(_) => return unsupported(UnsupportedColorReasonV1::UnknownMatrix),
    };
    let range = match token(fields.range).as_deref() {
        None | Some("") | Some("unknown") | Some("unspecified") => {
            return unsupported(UnsupportedColorReasonV1::MissingRange)
        }
        Some("tv" | "mpeg" | "limited") => ColorRangeV1::Limited,
        Some("pc" | "jpeg" | "full") => ColorRangeV1::Full,
        Some(_) => return unsupported(UnsupportedColorReasonV1::UnknownRange),
    };
    let chroma_location = match token(fields.chroma_location).as_deref() {
        None | Some("") | Some("unknown") | Some("unspecified") => None,
        Some("left") => Some(ChromaLocationV1::Left),
        Some("center" | "centre") => Some(ChromaLocationV1::Center),
        Some("topleft" | "top_left" | "top-left") => Some(ChromaLocationV1::TopLeft),
        Some(_) => return unsupported(UnsupportedColorReasonV1::UnknownChromaLocation),
    };
    let descriptor = SdrColorDescriptorV1 {
        primaries,
        transfer,
        matrix,
        range,
        pixel_model,
        chroma_location,
    };
    match descriptor.validate() {
        Ok(descriptor) => ColorManagementStatusV1::Supported {
            descriptor,
            provenance: ColorProvenanceV1::Signaled,
        },
        Err(_) => unsupported(UnsupportedColorReasonV1::ContradictoryMetadata),
    }
}

/// Normalize an 8-bit legal-range luma code to full-range signal value.
pub fn limited_luma_8_to_full(code: u8) -> f64 {
    ((f64::from(code) - 16.0) / 219.0).clamp(0.0, 1.0)
}

pub fn srgb_eotf(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn bt709_eotf(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    if value < 0.081 {
        value / 4.5
    } else {
        ((value + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn fields<'a>(
        range: &'a str,
        matrix: &'a str,
        transfer: &'a str,
        primaries: &'a str,
        pixel_format: &'a str,
    ) -> FfprobeColorFields<'a> {
        FfprobeColorFields {
            range: Some(range),
            matrix: Some(matrix),
            transfer: Some(transfer),
            primaries: Some(primaries),
            chroma_location: Some("left"),
            pixel_format: Some(pixel_format),
        }
    }

    #[test]
    fn accepts_all_supported_ffprobe_aliases() {
        for range in ["tv", "mpeg", "limited", "pc", "jpeg", "full"] {
            for transfer in ["bt709", "709", "bt1886", "iec61966-2-1", "srgb"] {
                assert!(matches!(
                    normalize_ffprobe_sdr_v1(fields(range, "709", transfer, "709", "yuv420p")),
                    ColorManagementStatusV1::Supported { .. }
                ));
            }
        }
        let rgb = normalize_ffprobe_sdr_v1(FfprobeColorFields {
            range: Some("pc"),
            matrix: Some("gbr"),
            transfer: Some("srgb"),
            primaries: Some("bt709"),
            chroma_location: None,
            pixel_format: Some("gbrap16le"),
        });
        assert!(matches!(rgb, ColorManagementStatusV1::Supported { .. }));

        let prores_without_chroma = normalize_ffprobe_sdr_v1(FfprobeColorFields {
            range: Some("tv"),
            matrix: Some("bt709"),
            transfer: Some("bt709"),
            primaries: Some("bt709"),
            chroma_location: None,
            pixel_format: Some("yuv422p10le"),
        });
        assert!(prores_without_chroma.is_prores_rec709_limited_output());
        assert_ne!(
            prores_without_chroma,
            ColorManagementStatusV1::rec709_limited_output()
        );

        let centered = normalize_ffprobe_sdr_v1(FfprobeColorFields {
            chroma_location: Some("center"),
            ..fields("tv", "bt709", "bt709", "bt709", "yuv420p")
        });
        assert!(!centered.is_prores_rec709_limited_output());
        assert_ne!(centered, ColorManagementStatusV1::rec709_limited_output());
    }

    #[test]
    fn missing_contradictory_bt601_and_hdr_are_explicitly_unsupported() {
        let missing = normalize_ffprobe_sdr_v1(FfprobeColorFields {
            transfer: None,
            ..fields("tv", "bt709", "bt709", "bt709", "yuv420p")
        });
        assert_eq!(
            missing,
            unsupported(UnsupportedColorReasonV1::MissingTransfer)
        );
        assert_eq!(
            normalize_ffprobe_sdr_v1(fields("tv", "smpte170m", "bt709", "bt709", "yuv420p")),
            unsupported(UnsupportedColorReasonV1::Bt601Unsupported)
        );
        assert_eq!(
            normalize_ffprobe_sdr_v1(fields("tv", "bt709", "smpte2084", "bt2020", "yuv420p10le")),
            unsupported(UnsupportedColorReasonV1::WideGamutUnsupported)
        );
        assert_eq!(
            normalize_ffprobe_sdr_v1(fields("tv", "rgb", "srgb", "bt709", "rgb24")),
            unsupported(UnsupportedColorReasonV1::ContradictoryMetadata)
        );
    }

    #[test]
    fn endpoints_and_transfer_breakpoints_are_exact() {
        assert_eq!(limited_luma_8_to_full(16), 0.0);
        assert_eq!(limited_luma_8_to_full(235), 1.0);
        assert!((srgb_eotf(0.04045) - 0.04045 / 12.92).abs() < 1e-15);
        assert!((bt709_eotf(0.081) - ((0.081_f64 + 0.099) / 1.099).powf(1.0 / 0.45)).abs() < 1e-15);
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        schema_version: u32,
        policy_id: String,
        linear_working_space: String,
        encoded_working_space: String,
        limited_luma8: Vec<LumaSample>,
    }
    #[derive(Deserialize)]
    struct LumaSample {
        code: u8,
        expected: f64,
    }

    #[test]
    fn shared_sdr_v1_fixture_pins_policy_and_legal_range_math() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../fixtures/color-management/sdr-v1.json"
        ))
        .unwrap();
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.policy_id, SDR_COLOR_MANAGEMENT_V1);
        assert_eq!(fixture.linear_working_space, LINEAR_WORKING_SPACE_V1);
        assert_eq!(fixture.encoded_working_space, ENCODED_WORKING_SPACE_V1);
        for sample in fixture.limited_luma8 {
            assert!((limited_luma_8_to_full(sample.code) - sample.expected).abs() < 1e-12);
        }
    }
}
