//! Runtime media capabilities derived from the tools installed on this host.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::state::ToolInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub schema_version: u32,
    pub tool_fingerprint: String,
    pub formats: Vec<CapabilityOption>,
    pub codecs: Vec<CapabilityOption>,
    pub filters: Vec<CapabilityOption>,
    pub hardware: Vec<CapabilityOption>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityOption {
    pub id: &'static str,
    pub label: &'static str,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Capabilities {
    pub fn from_tools(tools: &ToolInfo) -> Self {
        let has_encoder =
            |name: &str| tools.ffmpeg && tools.ffmpeg_encoders.iter().any(|v| v == name);
        let has_muxer = |name: &str| tools.ffmpeg && tools.ffmpeg_muxers.iter().any(|v| v == name);
        let has_filter =
            |name: &str| tools.ffmpeg && tools.ffmpeg_filters.iter().any(|v| v == name);

        let formats = vec![
            option(
                "mp4",
                "MP4",
                has_muxer("mp4") && (has_encoder("libx264") || has_encoder("libx265")),
                "нужны muxer mp4 и encoder libx264 или libx265",
            ),
            option(
                "webm",
                "WebM",
                has_muxer("webm") && has_encoder("libvpx-vp9") && has_encoder("libopus"),
                "нужны muxer webm и encoders libvpx-vp9/libopus",
            ),
            option(
                "av1",
                "AV1",
                has_muxer("mp4") && has_encoder("libsvtav1"),
                "нужны muxer mp4 и encoder libsvtav1",
            ),
            option(
                "prores",
                "ProRes",
                has_muxer("mov") && has_encoder("prores_ks"),
                "нужны muxer mov и encoder prores_ks",
            ),
            option(
                "gif",
                "GIF",
                has_muxer("gif")
                    && has_encoder("gif")
                    && has_filter("palettegen")
                    && has_filter("paletteuse"),
                "нужны muxer/encoder gif и palette filters",
            ),
            option(
                "png",
                "Кадр PNG",
                has_muxer("image2") && has_encoder("png"),
                "нужны muxer image2 и encoder png",
            ),
            option(
                "jpg",
                "Кадр JPG",
                has_muxer("image2") && has_encoder("mjpeg"),
                "нужны muxer image2 и encoder mjpeg",
            ),
            option(
                "mp3",
                "Аудио MP3",
                has_muxer("mp3") && has_encoder("libmp3lame"),
                "нужны muxer mp3 и encoder libmp3lame",
            ),
        ];
        let codecs = vec![
            option(
                "h264",
                "H.264",
                has_encoder("libx264"),
                "нужен encoder libx264",
            ),
            option(
                "h265",
                "H.265",
                has_encoder("libx265"),
                "нужен encoder libx265",
            ),
        ];
        let mut filters: Vec<CapabilityOption> = [
            ("grayscale", "Ч/Б", "hue"),
            ("sepia", "Сепия", "colorchannelmixer"),
            ("warm", "Тёплый", "colorbalance"),
            ("cold", "Холодный", "colorbalance"),
            ("teal-orange", "Teal-Orange", "colorbalance"),
            ("faded", "Выцветший", "curves"),
            ("noir", "Нуар", "hue"),
            ("vintage", "Винтаж", "curves"),
        ]
        .into_iter()
        .map(|(id, label, required)| {
            option(
                id,
                label,
                has_filter(required),
                &format!("нужен filter {required}"),
            )
        })
        .collect();
        filters.push(option(
            "custom-curves",
            "Кривые",
            has_filter("curves"),
            "нужен filter curves",
        ));
        filters.push(option(
            "lut3d",
            "3D LUT",
            has_filter("lut3d"),
            "нужен filter lut3d",
        ));
        filters.push(option(
            "lut-intensity",
            "Частичная интенсивность 3D LUT",
            has_filter("lut3d") && has_filter("blend"),
            "нужны filters lut3d и blend",
        ));
        let hardware = [
            (
                "videotoolbox-h264",
                "VideoToolbox H.264",
                "h264_videotoolbox",
            ),
            ("nvenc-h264", "NVIDIA NVENC H.264", "h264_nvenc"),
            ("qsv-h264", "Intel Quick Sync H.264", "h264_qsv"),
        ]
        .into_iter()
        .map(|(id, label, required)| {
            option(
                id,
                label,
                has_encoder(required),
                &format!("нужен hardware encoder {required}"),
            )
        })
        .collect();

        Self {
            schema_version: 1,
            tool_fingerprint: tool_fingerprint(tools),
            formats,
            codecs,
            filters,
            hardware,
        }
    }
}

fn option(
    id: &'static str,
    label: &'static str,
    available: bool,
    unavailable_reason: &str,
) -> CapabilityOption {
    CapabilityOption {
        id,
        label,
        available,
        reason: (!available).then(|| unavailable_reason.to_owned()),
    }
}

fn tool_fingerprint(tools: &ToolInfo) -> String {
    let mut encoders = tools.ffmpeg_encoders.clone();
    let mut muxers = tools.ffmpeg_muxers.clone();
    let mut filters = tools.ffmpeg_filters.clone();
    encoders.sort();
    muxers.sort();
    filters.sort();

    let mut hash = Sha256::new();
    hash.update(tools.ffmpeg_version.as_deref().unwrap_or("missing"));
    hash.update([0]);
    hash.update(encoders.join(","));
    hash.update([0]);
    hash.update(muxers.join(","));
    hash.update([0]);
    hash.update(filters.join(","));
    format!("{:x}", hash.finalize())[..16].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_reports_runtime_requirements_and_stable_fingerprint() {
        let tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_version: Some("ffmpeg 7".into()),
            ffmpeg_encoders: vec!["libx264".into(), "aac".into(), "png".into(), "mjpeg".into()],
            ffmpeg_muxers: vec!["mp4".into(), "image2".into()],
            ffmpeg_filters: vec!["hue".into(), "colorbalance".into()],
            ..ToolInfo::default()
        };
        let first = Capabilities::from_tools(&tools);
        let second = Capabilities::from_tools(&tools);

        assert_eq!(first.schema_version, 1);
        assert_eq!(first.tool_fingerprint, second.tool_fingerprint);
        assert!(
            first
                .formats
                .iter()
                .find(|v| v.id == "mp4")
                .unwrap()
                .available
        );
        let av1 = first.formats.iter().find(|v| v.id == "av1").unwrap();
        assert!(!av1.available);
        assert!(av1.reason.as_deref().unwrap().contains("libsvtav1"));
    }

    #[test]
    fn mp4_can_use_h265_when_h264_is_not_compiled_in() {
        let tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libx265".into()],
            ffmpeg_muxers: vec!["mp4".into()],
            ..ToolInfo::default()
        };

        let capabilities = Capabilities::from_tools(&tools);
        assert!(
            capabilities
                .formats
                .iter()
                .find(|option| option.id == "mp4")
                .unwrap()
                .available
        );
        assert!(
            !capabilities
                .codecs
                .iter()
                .find(|option| option.id == "h264")
                .unwrap()
                .available
        );
    }

    #[test]
    fn full_lut_does_not_require_the_partial_intensity_blend_filter() {
        let tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["lut3d".into()],
            ..ToolInfo::default()
        };

        let capabilities = Capabilities::from_tools(&tools);
        assert!(
            capabilities
                .filters
                .iter()
                .find(|option| option.id == "lut3d")
                .unwrap()
                .available
        );
        assert!(
            !capabilities
                .filters
                .iter()
                .find(|option| option.id == "lut-intensity")
                .unwrap()
                .available
        );
    }
}
