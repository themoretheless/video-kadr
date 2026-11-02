use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Edit,
    Composition,
    NestedExport,
    FilmcraftVideo,
    Enhance,
    Cutout,
    Selfie,
    CutoutEdit,
    SamRegion,
    EnhanceVideo,
    PersonVideo,
    Speech,
    SpeechModelDownload,
    Tts,
    TtsModelDownload,
    EnhanceModelDownload,
    CutoutModelDownload,
    SamModelDownload,
    Stabilization,
    Proxy,
    Publish,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Edit => "edit",
            Self::Composition => "composition",
            Self::NestedExport => "nested_export",
            Self::FilmcraftVideo => "filmcraft_video",
            Self::SamRegion => "sam_region",
            Self::CutoutEdit => "cutout_edit",
            Self::Cutout => "cutout",
            Self::Selfie => "selfie",
            Self::Enhance => "enhance",
            Self::PersonVideo => "person_video",
            Self::EnhanceVideo => "enhance_video",
            Self::Speech => "speech",
            Self::TtsModelDownload => "tts_model_download",
            Self::CutoutModelDownload => "cutout_model_download",
            Self::SamModelDownload => "sam_model_download",
            Self::EnhanceModelDownload => "enhance_model_download",
            Self::Tts => "tts",
            Self::SpeechModelDownload => "speech_model_download",
            Self::Stabilization => "stabilization",
            Self::Proxy => "proxy",
            Self::Publish => "publish",
        }
    }

    pub fn from_token(token: &str) -> anyhow::Result<Self> {
        match token {
            "import" => Ok(Self::Import),
            "edit" => Ok(Self::Edit),
            "composition" => Ok(Self::Composition),
            "nested_export" => Ok(Self::NestedExport),
            "filmcraft_video" => Ok(Self::FilmcraftVideo),
            "sam_region" => Ok(Self::SamRegion),
            "cutout_edit" => Ok(Self::CutoutEdit),
            "cutout" => Ok(Self::Cutout),
            "selfie" => Ok(Self::Selfie),
            "enhance" => Ok(Self::Enhance),
            "person_video" => Ok(Self::PersonVideo),
            "enhance_video" => Ok(Self::EnhanceVideo),
            "speech" => Ok(Self::Speech),
            "tts_model_download" => Ok(Self::TtsModelDownload),
            "cutout_model_download" => Ok(Self::CutoutModelDownload),
            "sam_model_download" => Ok(Self::SamModelDownload),
            "enhance_model_download" => Ok(Self::EnhanceModelDownload),
            "tts" => Ok(Self::Tts),
            "speech_model_download" => Ok(Self::SpeechModelDownload),
            "stabilization" => Ok(Self::Stabilization),
            "proxy" => Ok(Self::Proxy),
            "publish" => Ok(Self::Publish),
            _ => anyhow::bail!("unknown job kind {token}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobEnvelope {
    pub job_id: String,
    pub kind: JobKind,
    pub payload: Value,
    pub attempt: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_job_kind_has_a_stable_durable_token() {
        assert_eq!(JobKind::Proxy.as_str(), "proxy");
        assert_eq!(JobKind::from_token("proxy").unwrap(), JobKind::Proxy);
    }
}
