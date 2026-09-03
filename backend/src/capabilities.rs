//! Runtime media capabilities derived from the tools installed on this host.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::state::ToolInfo;
use crate::tools::looks::look_preset_catalog;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub schema_version: u32,
    pub tool_fingerprint: String,
    pub formats: Vec<CapabilityOption>,
    pub codecs: Vec<CapabilityOption>,
    pub filters: Vec<CapabilityOption>,
    pub hardware: Vec<CapabilityOption>,
    /// End-to-end workflows whose availability depends on several codecs and
    /// filters rather than one selectable export option.
    pub features: Vec<CapabilityOption>,
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
            option(
                "wav",
                "Аудио WAV",
                has_muxer("wav") && has_encoder("pcm_s16le"),
                "нужны muxer wav и encoder pcm_s16le",
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
        let mut filters: Vec<CapabilityOption> = look_preset_catalog()
            .iter()
            .map(|definition| {
                let missing = definition
                    .required_filters
                    .iter()
                    .copied()
                    .filter(|required| !has_filter(required))
                    .collect::<Vec<_>>();
                let unavailable_reason = match missing.as_slice() {
                    [] => String::new(),
                    [required] => format!("нужен filter {required}"),
                    required => format!("нужны filters {}", required.join(", ")),
                };

                option(
                    definition.id(),
                    definition.label,
                    missing.is_empty(),
                    &unavailable_reason,
                )
            })
            .collect();
        filters.extend([
            option(
                "custom-curves",
                "Кривые",
                has_filter("curves"),
                "нужен filter curves",
            ),
            option("lut3d", "3D LUT", has_filter("lut3d"), "нужен filter lut3d"),
            option(
                "lut-intensity",
                "Частичная интенсивность 3D LUT",
                has_filter("lut3d") && has_filter("blend"),
                "нужны filters lut3d и blend",
            ),
            option(
                "chroma-key",
                "Chroma key",
                has_filter("chromakey"),
                "нужен filter chromakey",
            ),
            option(
                "chroma-spill",
                "Подавление chroma spill",
                has_filter("chromakey") && has_filter("despill"),
                "нужны filters chromakey и despill",
            ),
            option(
                "selective-hsl",
                "HSL по цветовым диапазонам",
                has_filter("huesaturation"),
                "нужен filter huesaturation",
            ),
            option(
                "color-wheels",
                "Цветовые колёса",
                has_filter("colorbalance"),
                "нужен filter colorbalance",
            ),
            option(
                "audio-eq",
                "Трёхполосный EQ",
                has_filter("equalizer"),
                "нужен filter equalizer",
            ),
            option(
                "audio-pan",
                "Стереопанорама",
                has_filter("aformat") && has_filter("stereotools"),
                "нужны filters aformat и stereotools",
            ),
            option(
                "audio-compressor",
                "Компрессор",
                has_filter("acompressor"),
                "нужен filter acompressor",
            ),
            option(
                "audio-limiter",
                "Лимитер",
                has_filter("alimiter"),
                "нужен filter alimiter",
            ),
            option(
                "audio-ducking",
                "Auto ducking",
                has_filter("sidechaincompress") && has_filter("asplit"),
                "нужны filters sidechaincompress и asplit",
            ),
            option(
                "audio-voice-echo",
                "Voice echo",
                has_filter("aecho") && has_filter("atrim") && has_filter("asetpts"),
                "нужны filters aecho, atrim и asetpts",
            ),
            option(
                "audio-voice-robot",
                "Robot voice",
                has_filter("tremolo") && has_filter("highpass") && has_filter("lowpass"),
                "нужны filters tremolo, highpass и lowpass",
            ),
            option(
                "audio-tone",
                "Bass/treble tone",
                has_filter("bass") && has_filter("treble"),
                "нужны filters bass и treble",
            ),
        ]);
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
        let composition_filters = [
            "trim",
            "setpts",
            "scale",
            "pad",
            "setsar",
            "fps",
            "concat",
            "split",
            "asplit",
            "format",
            "color",
            "overlay",
            "xfade",
            "drawtext",
            "rotate",
            "colorchannelmixer",
            "geq",
            "blend",
            "alphaextract",
            "maskedmerge",
            "chromakey",
            "despill",
            "atrim",
            "asetpts",
            "aresample",
            "aformat",
            "anullsrc",
            "atempo",
            "volume",
            "pan",
            "aeval",
            "afade",
            "adelay",
            "amix",
            "alimiter",
        ];
        let composition_missing = composition_filters
            .into_iter()
            .filter(|filter| !has_filter(filter))
            .collect::<Vec<_>>();
        let composition_available = has_muxer("mp4")
            && has_encoder("libx264")
            && has_encoder("aac")
            && composition_missing.is_empty();
        let composition_reason = if composition_available {
            String::new()
        } else if !composition_missing.is_empty() {
            format!("нужны FFmpeg filters {}", composition_missing.join(", "))
        } else {
            "нужны muxer mp4 и encoders libx264/aac".into()
        };
        let optical_flow_missing = ["minterpolate", "tpad"]
            .into_iter()
            .filter(|filter| !has_filter(filter))
            .collect::<Vec<_>>();
        let optical_flow_reason = if optical_flow_missing.is_empty() {
            String::new()
        } else {
            format!("нужны FFmpeg filters {}", optical_flow_missing.join(", "))
        };
        let reverse_missing = ["reverse", "areverse"]
            .into_iter()
            .filter(|filter| !has_filter(filter))
            .collect::<Vec<_>>();
        let reverse_reason = if reverse_missing.is_empty() {
            String::new()
        } else {
            format!("нужны FFmpeg filters {}", reverse_missing.join(", "))
        };
        let freeze_missing = ["tpad"]
            .into_iter()
            .filter(|filter| !has_filter(filter))
            .collect::<Vec<_>>();
        let freeze_reason = if freeze_missing.is_empty() {
            String::new()
        } else {
            format!("нужны FFmpeg filters {}", freeze_missing.join(", "))
        };
        let stabilization_available = has_filter("deshake");
        let stabilization_reason = if stabilization_available {
            String::new()
        } else {
            "нужен FFmpeg filter deshake".to_owned()
        };
        let speed_ramp_missing = [
            "trim", "setpts", "tpad", "fps", "atrim", "asetpts", "asplit", "atempo", "concat",
        ]
        .into_iter()
        .filter(|filter| !has_filter(filter))
        .collect::<Vec<_>>();
        let speed_ramp_reason = if speed_ramp_missing.is_empty() {
            String::new()
        } else {
            format!("нужны FFmpeg filters {}", speed_ramp_missing.join(", "))
        };
        let delivery_missing = |muxer: &str, encoders: &[&str]| {
            let mut missing = Vec::new();
            if !has_muxer(muxer) {
                missing.push(format!("muxer {muxer}"));
            }
            for encoder in encoders {
                if !has_encoder(encoder) {
                    missing.push(format!("encoder {encoder}"));
                }
            }
            missing
        };
        let delivery_reason = |missing: &[String]| {
            if missing.is_empty() {
                String::new()
            } else {
                format!("нужны {}", missing.join(", "))
            }
        };
        let composition_h265_missing = delivery_missing("mp4", &["libx265", "aac"]);
        let composition_vp9_missing = delivery_missing("webm", &["libvpx-vp9", "libopus"]);
        let mut composition_av1_missing = delivery_missing("webm", &["libopus"]);
        if !has_encoder("libsvtav1") && !has_encoder("libaom-av1") {
            composition_av1_missing.push("encoder libsvtav1 or libaom-av1".to_owned());
        }
        let composition_prores_missing = delivery_missing("mov", &["prores_ks", "pcm_s16le"]);
        let composition_mp3_missing = delivery_missing("mp3", &["libmp3lame"]);
        let composition_wav_missing = delivery_missing("wav", &["pcm_s16le"]);
        let composition_aac_missing = delivery_missing("adts", &["aac"]);
        let composition_flac_missing = delivery_missing("flac", &["flac"]);
        let composition_h265_reason = delivery_reason(&composition_h265_missing);
        let composition_vp9_reason = delivery_reason(&composition_vp9_missing);
        let composition_av1_reason = delivery_reason(&composition_av1_missing);
        let composition_prores_reason = delivery_reason(&composition_prores_missing);
        let composition_mp3_reason = delivery_reason(&composition_mp3_missing);
        let composition_wav_reason = delivery_reason(&composition_wav_missing);
        let composition_aac_reason = delivery_reason(&composition_aac_missing);
        let composition_flac_reason = delivery_reason(&composition_flac_missing);
        let features = vec![
            option(
                "composition-v1",
                "Многодорожечный монтаж",
                composition_available,
                &composition_reason,
            ),
            option(
                "optical-flow",
                "Оптический поток",
                optical_flow_missing.is_empty(),
                &optical_flow_reason,
            ),
            option(
                "reverse-playback",
                "Обратное воспроизведение",
                reverse_missing.is_empty(),
                &reverse_reason,
            ),
            option(
                "freeze-frame",
                "Стоп-кадр",
                freeze_missing.is_empty(),
                &freeze_reason,
            ),
            option(
                "stabilization",
                "Стабилизация",
                stabilization_available,
                &stabilization_reason,
            ),
            option(
                "speed-ramp",
                "Кривая скорости",
                speed_ramp_missing.is_empty(),
                &speed_ramp_reason,
            ),
            option(
                "composition-mp4-h265",
                "Composition MP4 H.265",
                composition_h265_missing.is_empty(),
                &composition_h265_reason,
            ),
            option(
                "composition-webm-vp9",
                "Composition WebM VP9",
                composition_vp9_missing.is_empty(),
                &composition_vp9_reason,
            ),
            option(
                "composition-webm-av1",
                "Composition WebM AV1",
                composition_av1_missing.is_empty(),
                &composition_av1_reason,
            ),
            option(
                "composition-mov-prores",
                "Composition MOV ProRes",
                composition_prores_missing.is_empty(),
                &composition_prores_reason,
            ),
            option(
                "composition-audio-mp3",
                "Composition audio-only MP3",
                composition_mp3_missing.is_empty(),
                &composition_mp3_reason,
            ),
            option(
                "composition-audio-wav",
                "Composition audio-only WAV",
                composition_wav_missing.is_empty(),
                &composition_wav_reason,
            ),
            option(
                "composition-audio-aac",
                "Composition audio-only AAC",
                composition_aac_missing.is_empty(),
                &composition_aac_reason,
            ),
            option(
                "composition-audio-flac",
                "Composition audio-only FLAC",
                composition_flac_missing.is_empty(),
                &composition_flac_reason,
            ),
        ];

        Self {
            schema_version: 1,
            tool_fingerprint: tool_fingerprint(tools),
            formats,
            codecs,
            filters,
            hardware,
            features,
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

    fn all_look_filters() -> Vec<&'static str> {
        let mut filters = look_preset_catalog()
            .iter()
            .flat_map(|definition| definition.required_filters.iter().copied())
            .collect::<Vec<_>>();
        filters.sort_unstable();
        filters.dedup();
        filters
    }

    fn tools_with_look_filters(filters: &[&str]) -> ToolInfo {
        ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: filters.iter().map(|filter| (*filter).to_owned()).collect(),
            ..ToolInfo::default()
        }
    }

    fn look_option<'a>(capabilities: &'a Capabilities, id: &str) -> &'a CapabilityOption {
        capabilities
            .filters
            .iter()
            .find(|option| option.id == id)
            .unwrap_or_else(|| panic!("missing look preset capability {id}"))
    }

    fn feature_option<'a>(capabilities: &'a Capabilities, id: &str) -> &'a CapabilityOption {
        capabilities
            .features
            .iter()
            .find(|option| option.id == id)
            .unwrap_or_else(|| panic!("missing feature capability {id}"))
    }

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
    fn manual_color_capabilities_match_exact_ffmpeg_filters() {
        let tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec!["huesaturation".into(), "colorbalance".into()],
            ..ToolInfo::default()
        };
        let capabilities = Capabilities::from_tools(&tools);
        assert!(
            capabilities
                .filters
                .iter()
                .find(|option| option.id == "selective-hsl")
                .unwrap()
                .available
        );
        assert!(
            capabilities
                .filters
                .iter()
                .find(|option| option.id == "color-wheels")
                .unwrap()
                .available
        );
    }

    #[test]
    fn deterministic_audio_dsp_capabilities_match_exact_filters() {
        let tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_filters: vec![
                "equalizer".into(),
                "aformat".into(),
                "stereotools".into(),
                "acompressor".into(),
                "alimiter".into(),
                "sidechaincompress".into(),
                "asplit".into(),
                "aecho".into(),
                "atrim".into(),
                "asetpts".into(),
                "tremolo".into(),
                "highpass".into(),
                "lowpass".into(),
                "bass".into(),
                "treble".into(),
            ],
            ..ToolInfo::default()
        };
        let capabilities = Capabilities::from_tools(&tools);
        for id in [
            "audio-eq",
            "audio-pan",
            "audio-compressor",
            "audio-limiter",
            "audio-ducking",
            "audio-voice-echo",
            "audio-voice-robot",
            "audio-tone",
        ] {
            assert!(
                capabilities
                    .filters
                    .iter()
                    .find(|option| option.id == id)
                    .unwrap()
                    .available
            );
        }
    }

    #[test]
    fn composition_feature_requires_the_complete_offline_pipeline() {
        let filters = [
            "trim",
            "setpts",
            "scale",
            "pad",
            "setsar",
            "fps",
            "concat",
            "split",
            "asplit",
            "format",
            "color",
            "overlay",
            "xfade",
            "drawtext",
            "rotate",
            "colorchannelmixer",
            "geq",
            "blend",
            "alphaextract",
            "maskedmerge",
            "chromakey",
            "despill",
            "atrim",
            "asetpts",
            "aresample",
            "aformat",
            "anullsrc",
            "atempo",
            "volume",
            "pan",
            "aeval",
            "afade",
            "adelay",
            "amix",
            "alimiter",
        ];
        let mut tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libx264".into(), "aac".into()],
            ffmpeg_muxers: vec!["mp4".into()],
            ffmpeg_filters: filters.iter().map(|filter| (*filter).into()).collect(),
            ..ToolInfo::default()
        };
        let available = Capabilities::from_tools(&tools);
        assert!(available.features[0].available);

        tools.ffmpeg_filters.retain(|filter| filter != "overlay");
        let unavailable = Capabilities::from_tools(&tools);
        assert!(!unavailable.features[0].available);
        assert!(unavailable.features[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("overlay"));
    }

    #[test]
    fn optional_composition_delivery_profiles_report_exact_components() {
        let mut tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libx264".into(), "aac".into()],
            ffmpeg_muxers: vec!["mp4".into()],
            ..ToolInfo::default()
        };
        let base_only = Capabilities::from_tools(&tools);
        assert_eq!(
            feature_option(&base_only, "composition-mp4-h265")
                .reason
                .as_deref(),
            Some("нужны encoder libx265")
        );
        assert_eq!(
            feature_option(&base_only, "composition-webm-vp9")
                .reason
                .as_deref(),
            Some("нужны muxer webm, encoder libvpx-vp9, encoder libopus")
        );
        assert_eq!(
            feature_option(&base_only, "composition-webm-av1")
                .reason
                .as_deref(),
            Some("нужны muxer webm, encoder libopus, encoder libsvtav1 or libaom-av1")
        );
        assert_eq!(
            feature_option(&base_only, "composition-mov-prores")
                .reason
                .as_deref(),
            Some("нужны muxer mov, encoder prores_ks, encoder pcm_s16le")
        );
        assert_eq!(
            feature_option(&base_only, "composition-audio-mp3")
                .reason
                .as_deref(),
            Some("нужны muxer mp3, encoder libmp3lame")
        );
        assert_eq!(
            feature_option(&base_only, "composition-audio-wav")
                .reason
                .as_deref(),
            Some("нужны muxer wav, encoder pcm_s16le")
        );
        assert_eq!(
            feature_option(&base_only, "composition-audio-aac")
                .reason
                .as_deref(),
            Some("нужны muxer adts")
        );
        assert_eq!(
            feature_option(&base_only, "composition-audio-flac")
                .reason
                .as_deref(),
            Some("нужны muxer flac, encoder flac")
        );

        tools.ffmpeg_muxers.extend([
            "webm".into(),
            "mov".into(),
            "mp3".into(),
            "wav".into(),
            "adts".into(),
            "flac".into(),
        ]);
        tools.ffmpeg_encoders.extend([
            "libx265".into(),
            "libvpx-vp9".into(),
            "libopus".into(),
            "libaom-av1".into(),
            "prores_ks".into(),
            "pcm_s16le".into(),
            "libmp3lame".into(),
            "flac".into(),
        ]);
        let complete = Capabilities::from_tools(&tools);
        for id in [
            "composition-mp4-h265",
            "composition-webm-vp9",
            "composition-webm-av1",
            "composition-mov-prores",
            "composition-audio-mp3",
            "composition-audio-wav",
            "composition-audio-aac",
            "composition-audio-flac",
        ] {
            assert!(feature_option(&complete, id).available, "{id}");
        }
    }

    #[test]
    fn optical_flow_is_optional_and_does_not_gate_base_composition() {
        let base_filters = [
            "trim",
            "setpts",
            "scale",
            "pad",
            "setsar",
            "fps",
            "concat",
            "split",
            "asplit",
            "format",
            "color",
            "overlay",
            "xfade",
            "drawtext",
            "rotate",
            "colorchannelmixer",
            "geq",
            "blend",
            "alphaextract",
            "maskedmerge",
            "chromakey",
            "despill",
            "atrim",
            "asetpts",
            "aresample",
            "aformat",
            "anullsrc",
            "atempo",
            "volume",
            "pan",
            "aeval",
            "afade",
            "adelay",
            "amix",
            "alimiter",
        ];
        let mut tools = ToolInfo {
            ffmpeg: true,
            ffmpeg_encoders: vec!["libx264".into(), "aac".into()],
            ffmpeg_muxers: vec!["mp4".into()],
            ffmpeg_filters: base_filters.iter().map(|filter| (*filter).into()).collect(),
            ..ToolInfo::default()
        };

        let without_optical_flow = Capabilities::from_tools(&tools);
        assert!(feature_option(&without_optical_flow, "composition-v1").available);
        assert!(!feature_option(&without_optical_flow, "freeze-frame").available);
        assert!(!feature_option(&without_optical_flow, "reverse-playback").available);
        let stabilization = feature_option(&without_optical_flow, "stabilization");
        assert!(!stabilization.available);
        assert!(stabilization.reason.as_deref().unwrap().contains("deshake"));
        let optical_flow = feature_option(&without_optical_flow, "optical-flow");
        assert!(!optical_flow.available);
        assert!(optical_flow
            .reason
            .as_deref()
            .unwrap()
            .contains("minterpolate"));
        let speed_ramp = feature_option(&without_optical_flow, "speed-ramp");
        assert!(!speed_ramp.available);
        assert_eq!(
            speed_ramp.reason.as_deref(),
            Some("нужны FFmpeg filters tpad")
        );

        tools.ffmpeg_filters.push("minterpolate".into());
        let without_tpad = Capabilities::from_tools(&tools);
        assert!(!feature_option(&without_tpad, "optical-flow").available);
        assert!(feature_option(&without_tpad, "optical-flow")
            .reason
            .as_deref()
            .unwrap()
            .contains("tpad"));

        tools.ffmpeg_filters.push("tpad".into());
        let with_tpad = Capabilities::from_tools(&tools);
        assert!(feature_option(&with_tpad, "optical-flow").available);
        assert!(feature_option(&with_tpad, "freeze-frame").available);
        assert!(feature_option(&with_tpad, "speed-ramp").available);
        assert!(!feature_option(&with_tpad, "reverse-playback").available);

        tools.ffmpeg_filters.push("reverse".into());
        let without_audio_reverse = Capabilities::from_tools(&tools);
        assert!(!feature_option(&without_audio_reverse, "reverse-playback").available);
        assert!(feature_option(&without_audio_reverse, "reverse-playback")
            .reason
            .as_deref()
            .unwrap()
            .contains("areverse"));
        tools.ffmpeg_filters.push("areverse".into());
        assert!(feature_option(&Capabilities::from_tools(&tools), "reverse-playback").available);
        tools.ffmpeg_filters.push("deshake".into());
        let complete_optional = Capabilities::from_tools(&tools);
        assert!(feature_option(&complete_optional, "stabilization").available);
        assert!(feature_option(&complete_optional, "composition-v1").available);
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

    #[test]
    fn noir_requires_eq() {
        let mut filters = all_look_filters();
        filters.retain(|filter| *filter != "eq");

        let capabilities = Capabilities::from_tools(&tools_with_look_filters(&filters));
        let noir = look_option(&capabilities, "noir");

        assert!(!noir.available);
        assert_eq!(noir.reason.as_deref(), Some("нужен filter eq"));
    }

    #[test]
    fn vintage_requires_colorbalance() {
        let mut filters = all_look_filters();
        filters.retain(|filter| *filter != "colorbalance");

        let capabilities = Capabilities::from_tools(&tools_with_look_filters(&filters));
        let vintage = look_option(&capabilities, "vintage");

        assert!(!vintage.available);
        assert_eq!(vintage.reason.as_deref(), Some("нужен filter colorbalance"));
    }

    #[test]
    fn every_look_preset_uses_catalog_metadata_and_requirements() {
        let all_filters = all_look_filters();
        let capabilities = Capabilities::from_tools(&tools_with_look_filters(&all_filters));

        assert_eq!(capabilities.filters.len(), look_preset_catalog().len() + 15);
        for (option, definition) in capabilities.filters.iter().zip(look_preset_catalog()) {
            assert_eq!(option.id, definition.id());
            assert_eq!(option.label, definition.label);
            assert!(option.available, "{} should be available", definition.id());
            assert_eq!(option.reason, None);

            for missing_filter in definition.required_filters {
                let available_filters = all_filters
                    .iter()
                    .copied()
                    .filter(|filter| filter != missing_filter)
                    .collect::<Vec<_>>();
                let without_required =
                    Capabilities::from_tools(&tools_with_look_filters(&available_filters));
                let unavailable = look_option(&without_required, definition.id());

                assert!(
                    !unavailable.available,
                    "{} should require {missing_filter}",
                    definition.id()
                );
                assert!(
                    unavailable
                        .reason
                        .as_deref()
                        .is_some_and(|reason| reason.contains(missing_filter)),
                    "{} should report missing {missing_filter}",
                    definition.id()
                );
            }
        }
    }

    #[test]
    fn chroma_key_and_spill_report_independent_filter_requirements() {
        let key_only = Capabilities::from_tools(&tools_with_look_filters(&["chromakey"]));
        assert!(look_option(&key_only, "chroma-key").available);
        assert!(!look_option(&key_only, "chroma-spill").available);

        let full = Capabilities::from_tools(&tools_with_look_filters(&["chromakey", "despill"]));
        assert!(look_option(&full, "chroma-key").available);
        assert!(look_option(&full, "chroma-spill").available);
    }
}
