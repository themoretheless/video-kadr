//! Container metadata, provenance, and post-mux policy contracts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::arithmetic::rescale_ticks_i64;
use super::media_probe::{ProbeError, ProbeResult, Rational, StreamKind, StreamMetadata};

pub const MAX_RAW_DIAGNOSTIC_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeEnvelope {
    pub normalized: ProbeResult,
    pub raw_diagnostics: Option<Value>,
    pub warnings: Vec<String>,
}

impl ProbeEnvelope {
    pub fn from_ffprobe(raw: &Value) -> Result<Self, ProbeError> {
        let normalized = ProbeResult::from_ffprobe_json(raw)?;
        let raw_diagnostics = serde_json::to_vec(raw)
            .ok()
            .filter(|bytes| bytes.len() <= MAX_RAW_DIAGNOSTIC_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let warnings = raw_diagnostics
            .is_none()
            .then(|| "raw probe diagnostics omitted because they exceed the limit".into())
            .into_iter()
            .collect();
        Ok(Self {
            normalized,
            raw_diagnostics,
            warnings,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StreamIdentity {
    pub track_id: String,
    pub kind: StreamKind,
    pub language: Option<String>,
    pub dispositions: BTreeSet<String>,
}

impl StreamIdentity {
    pub fn from_stream(stream: &StreamMetadata) -> Self {
        let language = stream
            .tags
            .get("language")
            .map(|value| value.to_lowercase());
        let track_id = stream
            .track_id
            .clone()
            .unwrap_or_else(|| stable_track_token(stream));
        Self {
            track_id,
            kind: stream.kind,
            language,
            dispositions: stream.dispositions.clone(),
        }
    }
}

fn stable_track_token(stream: &StreamMetadata) -> String {
    format!(
        "{:?}:{}:{}:{}",
        stream.kind,
        stream.codec_name.as_deref().unwrap_or("unknown"),
        stream
            .tags
            .get("language")
            .map(String::as_str)
            .unwrap_or("und"),
        stream
            .dispositions
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("+")
    )
    .to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaTime {
    pub ticks: i64,
    pub time_base: Rational,
}

impl MediaTime {
    pub fn new(ticks: i64, time_base: Rational) -> Option<Self> {
        (time_base.numerator > 0 && time_base.denominator > 0).then_some(Self { ticks, time_base })
    }

    pub fn rescale(self, target: Rational) -> Option<Self> {
        if target.numerator <= 0 || target.denominator <= 0 {
            return None;
        }
        Some(Self {
            ticks: rescale_ticks_i64(
                self.ticks,
                self.time_base.numerator,
                self.time_base.denominator,
                target.numerator,
                target.denominator,
            )?,
            time_base: target,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayTransform {
    pub rotation_degrees: i32,
    pub sample_aspect_ratio: Rational,
    pub matrix: [i64; 9],
}

impl DisplayTransform {
    pub fn normalized(rotation_degrees: i32, sample_aspect_ratio: Rational) -> Option<Self> {
        if sample_aspect_ratio.numerator <= 0 || sample_aspect_ratio.denominator <= 0 {
            return None;
        }
        let rotation_degrees = rotation_degrees.rem_euclid(360);
        let matrix = match rotation_degrees {
            0 => [1, 0, 0, 0, 1, 0, 0, 0, 1],
            90 => [0, -1, 0, 1, 0, 0, 0, 0, 1],
            180 => [-1, 0, 0, 0, -1, 0, 0, 0, 1],
            270 => [0, 1, 0, -1, 0, 0, 0, 0, 1],
            _ => return None,
        };
        Some(Self {
            rotation_degrees,
            sample_aspect_ratio,
            matrix,
        })
    }
}

pub fn private_metadata(tags: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    const ALLOW: &[&str] = &["title", "language", "chapter", "timecode"];
    tags.iter()
        .filter(|(key, _)| ALLOW.contains(&key.to_ascii_lowercase().as_str()))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.chars().take(512).collect()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub mime: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentPolicy {
    pub max_count: usize,
    pub max_each_bytes: u64,
    pub max_total_bytes: u64,
}

impl AttachmentPolicy {
    pub fn validate(self, attachments: &[Attachment]) -> bool {
        const ALLOWED: &[&str] = &["font/ttf", "font/otf", "image/jpeg", "image/png"];
        attachments.len() <= self.max_count
            && attachments.iter().all(|item| {
                ALLOWED.contains(&item.mime.as_str()) && item.bytes <= self.max_each_bytes
            })
            && attachments
                .iter()
                .try_fold(0_u64, |total, item| total.checked_add(item.bytes))
                .is_some_and(|total| total <= self.max_total_bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Chapter,
    SourceTimecode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMarker {
    pub id: String,
    pub kind: MarkerKind,
    pub source_time: MediaTime,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceProvenance {
    pub sha256: String,
    pub adapter: String,
    pub adapter_version: String,
    pub probe_version: String,
    pub sanitized_origin: Option<String>,
}

impl SourceProvenance {
    pub fn validate(&self) -> bool {
        self.sha256.len() == 64
            && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            && self.adapter.len() <= 64
            && self.adapter_version.len() <= 64
            && self.probe_version.len() <= 64
            && self.sanitized_origin.as_ref().is_none_or(|origin| {
                !origin.contains('?') && !origin.contains('#') && !origin.contains('@')
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceSpec {
    pub formats: BTreeSet<String>,
    pub required_streams: BTreeSet<StreamKind>,
    pub expected_duration: MediaTime,
    pub duration_tolerance: MediaTime,
}

pub fn validate_conformance(
    probe: &ProbeResult,
    spec: &ConformanceSpec,
) -> Result<(), &'static str> {
    if probe
        .container
        .format_names
        .iter()
        .all(|format| !spec.formats.contains(format))
    {
        return Err("unexpected container");
    }
    let kinds: BTreeSet<_> = probe.streams.iter().map(|stream| stream.kind).collect();
    if !spec.required_streams.is_subset(&kinds) {
        return Err("required stream is missing");
    }
    let actual_ticks = (probe.duration / spec.expected_duration.time_base.as_f64()).round();
    if !actual_ticks.is_finite() || actual_ticks < i64::MIN as f64 || actual_ticks > i64::MAX as f64
    {
        return Err("duration is invalid");
    }
    if (actual_ticks as i64 - spec.expected_duration.ticks).unsigned_abs()
        > spec.duration_tolerance.ticks.unsigned_abs()
    {
        return Err("duration is outside tolerance");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterCapability {
    pub id: String,
    pub formats: BTreeSet<String>,
    pub codecs: BTreeSet<String>,
    pub metadata: bool,
}

pub fn select_adapter<'a>(
    capabilities: &'a [AdapterCapability],
    format: &str,
    codecs: &BTreeSet<String>,
    metadata: bool,
) -> Result<&'a AdapterCapability, String> {
    capabilities
        .iter()
        .find(|capability| {
            capability.formats.contains(format)
                && codecs.is_subset(&capability.codecs)
                && (!metadata || capability.metadata)
        })
        .ok_or_else(|| {
            format!("no adapter supports {format} with requested codecs and metadata policy")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_probe() -> Value {
        serde_json::json!({"format":{"format_name":"mov,mp4","duration":"10"},"streams":[{"index":9,"id":"0x101","codec_type":"video","codec_name":"h264","time_base":"1/90000","avg_frame_rate":"30000/1001","tags":{"language":"ENG","location":"secret"},"disposition":{"default":1},"width":1920,"height":1080},{"index":2,"id":"0x102","codec_type":"audio","codec_name":"aac","time_base":"1/48000","tags":{"language":"eng"}}]})
    }

    #[test]
    fn envelope_identity_and_privacy_are_stable() {
        let envelope = ProbeEnvelope::from_ffprobe(&raw_probe()).unwrap();
        assert!(envelope.raw_diagnostics.is_some());
        let identities: BTreeSet<_> = envelope
            .normalized
            .streams
            .iter()
            .map(StreamIdentity::from_stream)
            .collect();
        assert!(identities
            .iter()
            .any(|identity| identity.track_id == "0x101"
                && identity.language.as_deref() == Some("eng")));
        let tags = BTreeMap::from([
            ("title".into(), "Clip".into()),
            ("location".into(), "GPS".into()),
            ("author".into(), "PII".into()),
        ]);
        assert_eq!(
            private_metadata(&tags),
            BTreeMap::from([("title".into(), "Clip".into())])
        );
    }

    #[test]
    fn rational_time_has_no_long_frame_drift() {
        let frame = Rational {
            numerator: 1001,
            denominator: 30_000,
        };
        let time = MediaTime::new(1_000_000, frame).unwrap();
        let round_trip = time
            .rescale(Rational {
                numerator: 1,
                denominator: 90_000,
            })
            .unwrap()
            .rescale(frame)
            .unwrap();
        assert_eq!(round_trip.ticks, time.ticks);
    }

    #[test]
    fn transform_attachments_conformance_and_capabilities_fail_closed() {
        assert_eq!(
            DisplayTransform::normalized(
                450,
                Rational {
                    numerator: 4,
                    denominator: 3
                }
            )
            .unwrap()
            .rotation_degrees,
            90
        );
        let policy = AttachmentPolicy {
            max_count: 2,
            max_each_bytes: 1_000,
            max_total_bytes: 1_500,
        };
        assert!(policy.validate(&[Attachment {
            mime: "font/ttf".into(),
            bytes: 900
        }]));
        assert!(!policy.validate(&[Attachment {
            mime: "application/x-executable".into(),
            bytes: 1
        }]));
        let probe = ProbeEnvelope::from_ffprobe(&raw_probe())
            .unwrap()
            .normalized;
        let base = Rational {
            numerator: 1,
            denominator: 1_000,
        };
        validate_conformance(
            &probe,
            &ConformanceSpec {
                formats: BTreeSet::from(["mp4".into()]),
                required_streams: BTreeSet::from([StreamKind::Video, StreamKind::Audio]),
                expected_duration: MediaTime::new(10_000, base).unwrap(),
                duration_tolerance: MediaTime::new(1, base).unwrap(),
            },
        )
        .unwrap();
        let adapter = AdapterCapability {
            id: "ffmpeg".into(),
            formats: BTreeSet::from(["mp4".into()]),
            codecs: BTreeSet::from(["h264".into(), "aac".into()]),
            metadata: true,
        };
        assert_eq!(
            select_adapter(&[adapter], "mp4", &BTreeSet::from(["h264".into()]), true)
                .unwrap()
                .id,
            "ffmpeg"
        );
    }

    #[test]
    fn provenance_and_marker_contracts_are_bounded() {
        let provenance = SourceProvenance {
            sha256: "a".repeat(64),
            adapter: "srt".into(),
            adapter_version: "1".into(),
            probe_version: "ffprobe-8".into(),
            sanitized_origin: Some("https://example.test/live".into()),
        };
        assert!(provenance.validate());
        let marker = SourceMarker {
            id: "chapter-intro".into(),
            kind: MarkerKind::Chapter,
            source_time: MediaTime::new(
                90_000,
                Rational {
                    numerator: 1,
                    denominator: 90_000,
                },
            )
            .unwrap(),
            label: "Intro".into(),
        };
        assert_eq!(marker.source_time.ticks, 90_000);
    }
}
