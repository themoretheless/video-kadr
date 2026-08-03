use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Version 1 was the legacy `{ videoId, video, edit }` autosave payload.
/// Version 2 introduced canonical multitrack documents; version 3 added durable
/// asset identity; version 4 adds explicit, stable multicam edit decisions.
pub const PROJECT_DOCUMENT_SCHEMA_VERSION: u32 = 4;
pub const PROJECT_ENVELOPE_SCHEMA_VERSION: u32 = 1;
pub const PROJECT_TIME_BASE: u32 = 1_000_000;
pub const PROJECT_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const CREATOR_VIDEO_TRACK_COUNT: usize = 4;
pub const CREATOR_AUDIO_TRACK_COUNT: usize = 4;
pub const MAX_MULTICAM_GROUPS: usize = 32;
pub const MAX_MULTICAM_ANGLES: usize = 9;
pub const MAX_MULTICAM_DECISIONS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProxyPolicy {
    #[default]
    Auto,
    Original,
    Proxy,
}

/// Persistence metadata is kept outside the editable document so autosave can
/// advance a revision without mutating timeline content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnvelope {
    pub schema_version: u32,
    pub project_id: String,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub document: ProjectDocument,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDocument {
    pub schema_version: u32,
    pub name: String,
    pub primary_media_id: String,
    pub active_sequence_id: String,
    #[serde(default)]
    pub proxy_policy: ProxyPolicy,
    pub media: Vec<ProjectMedia>,
    pub sequences: Vec<ProjectSequence>,
    #[serde(default)]
    pub multicam_groups: Vec<MulticamGroup>,
    /// Fields unknown to the v1 reader are retained when that payload is
    /// migrated. They cannot always remain at the top level because a future
    /// v1 extension may now have the same name as a canonical v2 field.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legacy_fields: BTreeMap<String, Value>,
    /// Unknown v2 fields round-trip unchanged so a newer producer does not
    /// silently lose data when the document is opened by this version.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMedia {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_fingerprint: Option<String>,
    pub metadata: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSequence {
    pub id: String,
    pub name: String,
    pub settings: SequenceSettings,
    pub tracks: Vec<ProjectTrack>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceSettings {
    pub time_base: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTrack {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub clips: Vec<ProjectClip>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectClip {
    pub id: String,
    pub media_id: String,
    pub timeline_start_tick: u64,
    pub duration_ticks: u64,
    pub source_in_tick: u64,
    pub source_out_tick: u64,
    #[serde(default)]
    pub effects: Vec<ProjectEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multicam_group_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticamGroup {
    pub contract: String,
    pub id: String,
    pub name: String,
    pub time_base: u64,
    pub duration_ticks: u64,
    pub reference_angle_id: String,
    pub audio_angle_id: String,
    pub sync: MulticamSync,
    pub angles: Vec<MulticamAngle>,
    pub decisions: Vec<MulticamDecision>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticamSync {
    pub method: MulticamSyncMethod,
    pub algorithm_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MulticamSyncMethod {
    Audio,
    Timecode,
    Marker,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticamRate {
    pub numerator: u64,
    pub denominator: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticamAngle {
    pub id: String,
    pub media_id: String,
    pub label: String,
    pub source_origin_tick: u64,
    pub rate: MulticamRate,
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MulticamDecision {
    pub id: String,
    pub offset_tick: u64,
    pub angle_id: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEffect {
    pub id: String,
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "empty_object")]
    pub parameters: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_true() -> bool {
    true
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

impl ProjectEnvelope {
    pub fn new(
        project_id: impl Into<String>,
        document: ProjectDocument,
        timestamp: i64,
    ) -> Result<Self, ProjectDocumentError> {
        let envelope = Self {
            schema_version: PROJECT_ENVELOPE_SCHEMA_VERSION,
            project_id: project_id.into(),
            revision: 1,
            created_at: timestamp,
            updated_at: timestamp,
            document,
            extra: BTreeMap::new(),
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn decode(value: Value) -> Result<Self, ProjectDocumentError> {
        let object = value
            .as_object()
            .ok_or(ProjectDocumentError::ExpectedObject)?;
        let version = object
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(ProjectDocumentError::InvalidSchemaVersion)?;
        if version != PROJECT_ENVELOPE_SCHEMA_VERSION {
            return Err(ProjectDocumentError::UnsupportedEnvelopeSchema(version));
        }
        let envelope: Self = serde_json::from_value(value)
            .map_err(|error| ProjectDocumentError::Malformed(error.to_string()))?;
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn next_revision(
        &self,
        document: ProjectDocument,
        updated_at: i64,
    ) -> Result<Self, ProjectDocumentError> {
        let next = Self {
            schema_version: PROJECT_ENVELOPE_SCHEMA_VERSION,
            project_id: self.project_id.clone(),
            revision: self
                .revision
                .checked_add(1)
                .ok_or(ProjectDocumentError::RevisionOverflow)?,
            created_at: self.created_at,
            updated_at,
            document,
            extra: self.extra.clone(),
        };
        next.validate()?;
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), ProjectDocumentError> {
        if self.schema_version != PROJECT_ENVELOPE_SCHEMA_VERSION {
            return Err(ProjectDocumentError::UnsupportedEnvelopeSchema(
                self.schema_version,
            ));
        }
        validate_id("projectId", &self.project_id)?;
        if self.revision == 0 || self.revision > PROJECT_MAX_SAFE_INTEGER {
            return Err(ProjectDocumentError::InvalidField("revision"));
        }
        if self.created_at < 0
            || self.updated_at < self.created_at
            || self.created_at as u64 > PROJECT_MAX_SAFE_INTEGER
            || self.updated_at as u64 > PROJECT_MAX_SAFE_INTEGER
        {
            return Err(ProjectDocumentError::InvalidField("timestamps"));
        }
        self.document.validate()
    }
}

impl ProjectDocument {
    /// Decode either the current document or the legacy v1 autosave payload.
    /// Newer schema versions fail explicitly instead of being partially read.
    pub fn migrate(value: Value) -> Result<Self, ProjectDocumentError> {
        let object = value
            .as_object()
            .ok_or(ProjectDocumentError::ExpectedObject)?;
        let version = match object.get("schemaVersion") {
            Some(value) => value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or(ProjectDocumentError::InvalidSchemaVersion)?,
            None => 1,
        };

        match version {
            1 => Self::migrate_v1(value),
            2 => Self::migrate_v2(value),
            3 => Self::migrate_v3(value),
            PROJECT_DOCUMENT_SCHEMA_VERSION => {
                let mut document: Self = serde_json::from_value(value)
                    .map_err(|error| ProjectDocumentError::Malformed(error.to_string()))?;
                for media in &mut document.media {
                    strip_runtime_locators(&mut media.metadata);
                }
                document.validate()?;
                Ok(document)
            }
            version => Err(ProjectDocumentError::UnsupportedSchema(version)),
        }
    }

    fn migrate_v2(value: Value) -> Result<Self, ProjectDocumentError> {
        let mut document: Self = serde_json::from_value(value)
            .map_err(|error| ProjectDocumentError::Malformed(error.to_string()))?;
        document.schema_version = PROJECT_DOCUMENT_SCHEMA_VERSION;
        for media in &mut document.media {
            let metadata = media
                .metadata
                .as_object_mut()
                .ok_or(ProjectDocumentError::InvalidField("media.metadata"))?;
            media.asset_ref = media
                .asset_ref
                .take()
                .or_else(|| {
                    metadata
                        .remove("assetId")
                        .and_then(|value| value.as_str().map(str::to_owned))
                })
                .or_else(|| Some(media.id.clone()));
            media.content_fingerprint = media
                .content_fingerprint
                .take()
                .or_else(|| {
                    metadata
                        .remove("fingerprint")
                        .and_then(|value| value.as_str().map(str::to_owned))
                })
                .filter(|value| valid_fingerprint(value));
            strip_runtime_locators(&mut media.metadata);
        }
        document.validate()?;
        Ok(document)
    }

    fn migrate_v3(value: Value) -> Result<Self, ProjectDocumentError> {
        let mut document: Self = serde_json::from_value(value)
            .map_err(|error| ProjectDocumentError::Malformed(error.to_string()))?;
        document.schema_version = PROJECT_DOCUMENT_SCHEMA_VERSION;
        for media in &mut document.media {
            strip_runtime_locators(&mut media.metadata);
        }
        document.validate()?;
        Ok(document)
    }

    pub fn from_legacy(
        name: impl Into<String>,
        video_id: impl Into<String>,
        video: Value,
        edit: Value,
    ) -> Result<Self, ProjectDocumentError> {
        Self::from_legacy_parts(name.into(), video_id.into(), video, edit, BTreeMap::new())
    }

    fn migrate_v1(value: Value) -> Result<Self, ProjectDocumentError> {
        let mut object = value
            .as_object()
            .cloned()
            .ok_or(ProjectDocumentError::ExpectedObject)?;
        object.remove("schemaVersion");
        let video = object
            .remove("video")
            .ok_or(ProjectDocumentError::MissingField("video"))?;
        let edit = object
            .remove("edit")
            .ok_or(ProjectDocumentError::MissingField("edit"))?;
        let video_id = object
            .remove("videoId")
            .and_then(|value| value.as_str().map(str::to_owned))
            .or_else(|| video.get("id").and_then(Value::as_str).map(str::to_owned))
            .ok_or(ProjectDocumentError::MissingField("videoId"))?;
        let name = object
            .remove("name")
            .and_then(|value| value.as_str().map(str::to_owned))
            .or_else(|| {
                video
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| {
                video
                    .get("filename")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Без названия".to_owned());
        Self::from_legacy_parts(name, video_id, video, edit, object.into_iter().collect())
    }

    fn from_legacy_parts(
        name: String,
        video_id: String,
        mut video: Value,
        edit: Value,
        legacy_fields: BTreeMap<String, Value>,
    ) -> Result<Self, ProjectDocumentError> {
        if !video.is_object() {
            return Err(ProjectDocumentError::InvalidField("video"));
        }
        if !edit.is_object() {
            return Err(ProjectDocumentError::InvalidField("edit"));
        }
        strip_runtime_locators(&mut video);
        let duration_ticks = legacy_duration_ticks(&video);
        let primary_kind = match video.get("mediaKind").and_then(Value::as_str) {
            Some("audio") => "audio",
            Some("video") => "video",
            _ if video.get("acodec").and_then(Value::as_str).is_some()
                && video.get("vcodec").and_then(Value::as_str).is_none()
                && positive_u32(video.get("width")).is_none() =>
            {
                "audio"
            }
            _ => "video",
        };
        let primary_clip = ProjectClip {
            id: "clip-main".to_owned(),
            media_id: video_id.clone(),
            timeline_start_tick: 0,
            duration_ticks,
            source_in_tick: 0,
            source_out_tick: duration_ticks,
            multicam_group_id: None,
            effects: vec![ProjectEffect {
                id: "effect-legacy-edit".to_owned(),
                kind: "legacy_edit".to_owned(),
                enabled: true,
                parameters: edit,
                extra: BTreeMap::new(),
            }],
            extra: BTreeMap::new(),
        };
        let settings = SequenceSettings {
            time_base: PROJECT_TIME_BASE,
            frame_rate: finite_positive_f64(video.get("fps")),
            width: positive_u32(video.get("width")),
            height: positive_u32(video.get("height")),
            extra: BTreeMap::new(),
        };
        let asset_ref = video
            .get("assetId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some(video_id.clone()));
        let content_fingerprint = video
            .get("fingerprint")
            .and_then(Value::as_str)
            .filter(|value| valid_fingerprint(value))
            .map(str::to_owned);
        if let Some(metadata) = video.as_object_mut() {
            metadata.remove("assetId");
            metadata.remove("fingerprint");
        }
        let document = Self {
            schema_version: PROJECT_DOCUMENT_SCHEMA_VERSION,
            name,
            primary_media_id: video_id.clone(),
            active_sequence_id: "sequence-main".to_owned(),
            proxy_policy: ProxyPolicy::default(),
            media: vec![ProjectMedia {
                id: video_id.clone(),
                kind: primary_kind.to_owned(),
                asset_ref,
                content_fingerprint,
                metadata: video,
                extra: BTreeMap::new(),
            }],
            sequences: vec![ProjectSequence {
                id: "sequence-main".to_owned(),
                name: "Основная".to_owned(),
                settings,
                tracks: vec![
                    ProjectTrack {
                        id: "track-video-main".to_owned(),
                        kind: "video".to_owned(),
                        name: "Видео 1".to_owned(),
                        clips: if primary_kind == "video" {
                            vec![primary_clip.clone()]
                        } else {
                            Vec::new()
                        },
                        extra: BTreeMap::new(),
                    },
                    ProjectTrack {
                        id: "track-audio-main".to_owned(),
                        kind: "audio".to_owned(),
                        name: "Аудио 1".to_owned(),
                        clips: if primary_kind == "audio" {
                            vec![primary_clip]
                        } else {
                            Vec::new()
                        },
                        extra: BTreeMap::new(),
                    },
                ],
                extra: BTreeMap::new(),
            }],
            multicam_groups: Vec::new(),
            legacy_fields,
            extra: BTreeMap::new(),
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), ProjectDocumentError> {
        if self.schema_version != PROJECT_DOCUMENT_SCHEMA_VERSION {
            return Err(ProjectDocumentError::UnsupportedSchema(self.schema_version));
        }
        validate_id("primaryMediaId", &self.primary_media_id)?;
        validate_id("activeSequenceId", &self.active_sequence_id)?;
        if self.name.trim().is_empty() {
            return Err(ProjectDocumentError::InvalidField("name"));
        }

        let mut media_ids = BTreeSet::new();
        for media in &self.media {
            validate_id("media.id", &media.id)?;
            validate_token("media.kind", &media.kind)?;
            if let Some(asset_ref) = &media.asset_ref {
                validate_id("media.assetRef", asset_ref)?;
            }
            if media
                .content_fingerprint
                .as_deref()
                .is_some_and(|value| !valid_fingerprint(value))
            {
                return Err(ProjectDocumentError::InvalidField(
                    "media.contentFingerprint",
                ));
            }
            if !media.metadata.is_object() {
                return Err(ProjectDocumentError::InvalidField("media.metadata"));
            }
            if !media_ids.insert(media.id.as_str()) {
                return Err(ProjectDocumentError::DuplicateId(media.id.clone()));
            }
        }
        if !media_ids.contains(self.primary_media_id.as_str()) {
            return Err(ProjectDocumentError::MissingReference(
                self.primary_media_id.clone(),
            ));
        }

        if self.multicam_groups.len() > MAX_MULTICAM_GROUPS {
            return Err(ProjectDocumentError::InvalidField("multicamGroups.count"));
        }
        let mut multicam_group_ids = BTreeSet::new();
        for group in &self.multicam_groups {
            validate_multicam_group(group, &self.media)?;
            if !multicam_group_ids.insert(group.id.as_str()) {
                return Err(ProjectDocumentError::DuplicateId(group.id.clone()));
            }
        }

        let mut sequence_ids = BTreeSet::new();
        let mut track_ids = BTreeSet::new();
        let mut clip_ids = BTreeSet::new();
        let mut effect_ids = BTreeSet::new();
        for sequence in &self.sequences {
            validate_id("sequence.id", &sequence.id)?;
            if !sequence_ids.insert(sequence.id.as_str()) {
                return Err(ProjectDocumentError::DuplicateId(sequence.id.clone()));
            }
            if sequence.name.trim().is_empty() || sequence.settings.time_base == 0 {
                return Err(ProjectDocumentError::InvalidField("sequence"));
            }
            if sequence
                .settings
                .frame_rate
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            {
                return Err(ProjectDocumentError::InvalidField(
                    "sequence.settings.frameRate",
                ));
            }
            if sequence.settings.width == Some(0) || sequence.settings.height == Some(0) {
                return Err(ProjectDocumentError::InvalidField(
                    "sequence.settings.dimensions",
                ));
            }
            for track in &sequence.tracks {
                validate_id("track.id", &track.id)?;
                validate_token("track.kind", &track.kind)?;
                for key in ["muted", "solo", "locked", "hidden"] {
                    if track
                        .extra
                        .get(key)
                        .is_some_and(|value| !value.is_boolean())
                    {
                        return Err(ProjectDocumentError::InvalidField("track.state"));
                    }
                }
                if !track_ids.insert(track.id.as_str()) {
                    return Err(ProjectDocumentError::DuplicateId(track.id.clone()));
                }
                for clip in &track.clips {
                    validate_id("clip.id", &clip.id)?;
                    if !clip_ids.insert(clip.id.as_str()) {
                        return Err(ProjectDocumentError::DuplicateId(clip.id.clone()));
                    }
                    if let Some(group_id) = &clip.multicam_group_id {
                        validate_id("clip.multicamGroupId", group_id)?;
                        if !multicam_group_ids.contains(group_id.as_str()) {
                            return Err(ProjectDocumentError::MissingReference(group_id.clone()));
                        }
                    }
                    if !media_ids.contains(clip.media_id.as_str()) {
                        return Err(ProjectDocumentError::MissingReference(
                            clip.media_id.clone(),
                        ));
                    }
                    let media = self
                        .media
                        .iter()
                        .find(|media| media.id == clip.media_id)
                        .expect("media reference was checked");
                    let compatible = match track.kind.as_str() {
                        "video" => matches!(media.kind.as_str(), "video" | "image"),
                        "audio" => media.kind == "audio",
                        _ => false,
                    };
                    if !compatible {
                        return Err(ProjectDocumentError::InvalidField("clip.trackKind"));
                    }
                    if let Some(source_duration) = self
                        .media
                        .iter()
                        .find(|media| media.id == clip.media_id)
                        .and_then(|media| {
                            source_duration_ticks(&media.metadata, sequence.settings.time_base)
                        })
                    {
                        if clip.source_out_tick > source_duration {
                            return Err(ProjectDocumentError::InvalidField("clip.sourceRange"));
                        }
                    }
                    if clip.duration_ticks == 0
                        || clip.timeline_start_tick > PROJECT_MAX_SAFE_INTEGER
                        || clip.duration_ticks > PROJECT_MAX_SAFE_INTEGER
                        || clip.source_in_tick > PROJECT_MAX_SAFE_INTEGER
                        || clip.source_out_tick > PROJECT_MAX_SAFE_INTEGER
                        || clip.source_out_tick <= clip.source_in_tick
                        || clip.source_out_tick - clip.source_in_tick != clip.duration_ticks
                    {
                        return Err(ProjectDocumentError::InvalidField("clip.duration"));
                    }
                    let timeline_end = clip
                        .timeline_start_tick
                        .checked_add(clip.duration_ticks)
                        .ok_or(ProjectDocumentError::InvalidField("clip.timelineRange"))?;
                    if timeline_end > PROJECT_MAX_SAFE_INTEGER {
                        return Err(ProjectDocumentError::InvalidField("clip.timelineRange"));
                    }
                    for effect in &clip.effects {
                        validate_id("effect.id", &effect.id)?;
                        validate_token("effect.kind", &effect.kind)?;
                        if !effect.parameters.is_object() {
                            return Err(ProjectDocumentError::InvalidField("effect.parameters"));
                        }
                        if !effect_ids.insert(effect.id.as_str()) {
                            return Err(ProjectDocumentError::DuplicateId(effect.id.clone()));
                        }
                    }
                }
                let mut ordered_clips = track.clips.iter().collect::<Vec<_>>();
                ordered_clips.sort_by_key(|clip| clip.timeline_start_tick);
                for clips in ordered_clips.windows(2) {
                    let previous = clips[0];
                    let current = clips[1];
                    let previous_end = previous
                        .timeline_start_tick
                        .checked_add(previous.duration_ticks)
                        .ok_or(ProjectDocumentError::InvalidField("clip.timelineRange"))?;
                    if previous_end > current.timeline_start_tick {
                        return Err(ProjectDocumentError::InvalidField("clip.overlap"));
                    }
                }
            }
            if sequence
                .tracks
                .iter()
                .flat_map(|track| &track.clips)
                .filter(|clip| clip.multicam_group_id.is_some())
                .count()
                > 1
            {
                return Err(ProjectDocumentError::InvalidField(
                    "sequence.multicamAttachment",
                ));
            }
        }
        if !sequence_ids.contains(self.active_sequence_id.as_str()) {
            return Err(ProjectDocumentError::MissingReference(
                self.active_sequence_id.clone(),
            ));
        }
        Ok(())
    }

    /// Add missing video/audio tracks without replacing legacy tracks, clips,
    /// ordering, or extension data. Track IDs remain globally unique across
    /// every sequence in the document.
    pub fn ensure_track_capacity(
        &self,
        sequence_id: &str,
        video_tracks: usize,
        audio_tracks: usize,
    ) -> Result<Self, ProjectDocumentError> {
        let mut next = self.clone();
        let mut used_ids = next
            .sequences
            .iter()
            .flat_map(|sequence| &sequence.tracks)
            .map(|track| track.id.clone())
            .collect::<BTreeSet<_>>();
        let sequence = next
            .sequences
            .iter_mut()
            .find(|sequence| sequence.id == sequence_id)
            .ok_or_else(|| ProjectDocumentError::MissingReference(sequence_id.to_owned()))?;

        for (kind, minimum, label) in [
            ("video", video_tracks, "Видео"),
            ("audio", audio_tracks, "Аудио"),
        ] {
            let mut count = sequence
                .tracks
                .iter()
                .filter(|track| track.kind == kind)
                .count();
            while count < minimum {
                let ordinal = count + 1;
                let mut suffix = ordinal;
                let mut id = format!("track-{kind}-{suffix}");
                while used_ids.contains(&id) {
                    suffix += 1;
                    id = format!("track-{kind}-{suffix}");
                }
                used_ids.insert(id.clone());
                sequence.tracks.push(ProjectTrack {
                    id,
                    kind: kind.to_owned(),
                    name: format!("{label} {ordinal}"),
                    clips: Vec::new(),
                    extra: BTreeMap::new(),
                });
                count += 1;
            }
        }
        next.validate()?;
        Ok(next)
    }

    pub fn ensure_creator_track_layout(
        &self,
        sequence_id: &str,
    ) -> Result<Self, ProjectDocumentError> {
        self.ensure_track_capacity(
            sequence_id,
            CREATOR_VIDEO_TRACK_COUNT,
            CREATOR_AUDIO_TRACK_COUNT,
        )
    }

    /// Compatibility projection used by the current single-clip editor while
    /// the rest of the UI moves to sequences and tracks.
    pub fn legacy_video_and_edit(&self) -> (Value, Value) {
        let video = self
            .media
            .iter()
            .find(|media| media.id == self.primary_media_id)
            .map(|media| media.metadata.clone())
            .unwrap_or_else(empty_object);
        let edit = self
            .sequences
            .iter()
            .find(|sequence| sequence.id == self.active_sequence_id)
            .into_iter()
            .flat_map(|sequence| &sequence.tracks)
            .flat_map(|track| &track.clips)
            .filter(|clip| clip.media_id == self.primary_media_id)
            .flat_map(|clip| &clip.effects)
            .find(|effect| effect.kind == "legacy_edit")
            .map(|effect| effect.parameters.clone())
            .unwrap_or_else(empty_object);
        (video, edit)
    }

    /// Patch compatibility values while preserving native tracks and opaque extensions.
    pub fn update_legacy_values(
        &mut self,
        name: impl Into<String>,
        mut video: Value,
        edit: Value,
    ) -> Result<(), ProjectDocumentError> {
        if !video.is_object() || !edit.is_object() {
            return Err(ProjectDocumentError::InvalidField("legacy values"));
        }
        self.name = name.into();
        let asset_ref = video
            .get("assetId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let content_fingerprint = video
            .get("fingerprint")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(object) = video.as_object_mut() {
            object.remove("assetId");
            object.remove("fingerprint");
        }
        strip_runtime_locators(&mut video);
        let media = self
            .media
            .iter_mut()
            .find(|media| media.id == self.primary_media_id)
            .ok_or_else(|| ProjectDocumentError::MissingReference(self.primary_media_id.clone()))?;
        if media.asset_ref.is_none() {
            if let Some(asset_ref) = asset_ref {
                media.asset_ref = Some(asset_ref);
            }
        }
        if media.content_fingerprint.is_none() {
            if let Some(content_fingerprint) =
                content_fingerprint.filter(|value| valid_fingerprint(value))
            {
                media.content_fingerprint = Some(content_fingerprint);
            }
        }
        merge_objects(&mut media.metadata, video);

        let sequence = self
            .sequences
            .iter_mut()
            .find(|sequence| sequence.id == self.active_sequence_id)
            .ok_or_else(|| {
                ProjectDocumentError::MissingReference(self.active_sequence_id.clone())
            })?;
        if let Some(effect) = sequence
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.clips)
            .filter(|clip| clip.media_id == self.primary_media_id)
            .flat_map(|clip| &mut clip.effects)
            .find(|effect| effect.kind == "legacy_edit")
        {
            merge_objects(&mut effect.parameters, edit);
        } else {
            let used_ids = sequence
                .tracks
                .iter()
                .flat_map(|track| &track.clips)
                .flat_map(|clip| &clip.effects)
                .map(|effect| effect.id.as_str())
                .collect::<BTreeSet<_>>();
            let mut effect_id = "effect-legacy-edit".to_owned();
            let mut suffix = 0_u32;
            while used_ids.contains(effect_id.as_str()) {
                suffix += 1;
                effect_id = format!("effect-legacy-edit-{suffix}");
            }
            if let Some(clip) = sequence
                .tracks
                .iter_mut()
                .flat_map(|track| &mut track.clips)
                .find(|clip| clip.media_id == self.primary_media_id)
            {
                clip.effects.push(ProjectEffect {
                    id: effect_id,
                    kind: "legacy_edit".into(),
                    enabled: true,
                    parameters: edit,
                    extra: BTreeMap::new(),
                });
            }
        }
        self.validate()
    }
}

fn merge_objects(target: &mut Value, update: Value) {
    if let (Some(target), Some(update)) = (target.as_object_mut(), update.as_object()) {
        target.extend(update.clone());
    }
}

fn strip_runtime_locators(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        for key in ["url", "path", "file", "availability"] {
            object.remove(key);
        }
    }
}

fn legacy_duration_ticks(video: &Value) -> u64 {
    finite_positive_f64(video.get("duration"))
        .map(|seconds| (seconds * f64::from(PROJECT_TIME_BASE)).round())
        .filter(|ticks| ticks.is_finite() && *ticks >= 1.0 && *ticks <= u64::MAX as f64)
        .map(|ticks| ticks as u64)
        .unwrap_or(1)
}

fn validate_multicam_group(
    group: &MulticamGroup,
    media: &[ProjectMedia],
) -> Result<(), ProjectDocumentError> {
    validate_exact_id("multicamGroup.id", &group.id)?;
    validate_label("multicamGroup.name", &group.name)?;
    if group.contract != "multicam-v1"
        || group.time_base == 0
        || group.time_base > PROJECT_MAX_SAFE_INTEGER
        || group.duration_ticks == 0
        || group.duration_ticks > PROJECT_MAX_SAFE_INTEGER
        || !(2..=MAX_MULTICAM_ANGLES).contains(&group.angles.len())
        || group.decisions.is_empty()
        || group.decisions.len() > MAX_MULTICAM_DECISIONS
    {
        return Err(ProjectDocumentError::InvalidField("multicamGroup.range"));
    }
    validate_exact_id(
        "multicamGroup.sync.algorithmVersion",
        &group.sync.algorithm_version,
    )?;
    if group
        .sync
        .confidence
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(ProjectDocumentError::InvalidField(
            "multicamGroup.sync.confidence",
        ));
    }

    let mut angle_ids = BTreeSet::new();
    let mut angle_media_ids = BTreeSet::new();
    for angle in &group.angles {
        validate_exact_id("multicamAngle.id", &angle.id)?;
        validate_label("multicamAngle.label", &angle.label)?;
        if !angle_ids.insert(angle.id.as_str()) {
            return Err(ProjectDocumentError::DuplicateId(angle.id.clone()));
        }
        if !angle_media_ids.insert(angle.media_id.as_str()) {
            return Err(ProjectDocumentError::InvalidField("multicamAngle.mediaId"));
        }
        let source = media
            .iter()
            .find(|candidate| candidate.id == angle.media_id)
            .ok_or_else(|| ProjectDocumentError::MissingReference(angle.media_id.clone()))?;
        if source.kind != "video"
            || angle.source_origin_tick > PROJECT_MAX_SAFE_INTEGER
            || angle.rate.numerator == 0
            || angle.rate.denominator == 0
            || angle.rate.numerator > PROJECT_MAX_SAFE_INTEGER
            || angle.rate.denominator > PROJECT_MAX_SAFE_INTEGER
            || gcd(angle.rate.numerator, angle.rate.denominator) != 1
            || u128::from(angle.rate.numerator) > u128::from(angle.rate.denominator) * 16
            || u128::from(angle.rate.denominator) > u128::from(angle.rate.numerator) * 16
        {
            return Err(ProjectDocumentError::InvalidField("multicamAngle.source"));
        }
        let duration = source_duration_ticks_u64(&source.metadata, group.time_base).ok_or(
            ProjectDocumentError::InvalidField("multicamAngle.mediaDuration"),
        )?;
        let denominator = u128::from(angle.rate.denominator);
        let end_numerator = u128::from(angle.source_origin_tick) * denominator
            + u128::from(group.duration_ticks) * u128::from(angle.rate.numerator);
        if end_numerator > u128::from(duration) * denominator {
            return Err(ProjectDocumentError::InvalidField(
                "multicamAngle.sourceRange",
            ));
        }
    }
    if !angle_ids.contains(group.reference_angle_id.as_str())
        || !angle_ids.contains(group.audio_angle_id.as_str())
    {
        return Err(ProjectDocumentError::InvalidField(
            "multicamGroup.angleReference",
        ));
    }

    let enabled = group
        .angles
        .iter()
        .filter(|angle| angle.enabled)
        .map(|angle| angle.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut decision_ids = BTreeSet::new();
    let mut previous_tick = None;
    let mut previous_angle = None;
    for decision in &group.decisions {
        validate_exact_id("multicamDecision.id", &decision.id)?;
        if !decision_ids.insert(decision.id.as_str()) {
            return Err(ProjectDocumentError::DuplicateId(decision.id.clone()));
        }
        if !enabled.contains(decision.angle_id.as_str())
            || decision.offset_tick >= group.duration_ticks
            || previous_tick.is_some_and(|tick| decision.offset_tick <= tick)
            || previous_angle == Some(decision.angle_id.as_str())
        {
            return Err(ProjectDocumentError::InvalidField(
                "multicamGroup.decisions",
            ));
        }
        previous_tick = Some(decision.offset_tick);
        previous_angle = Some(decision.angle_id.as_str());
    }
    if group.decisions[0].offset_tick != 0 {
        return Err(ProjectDocumentError::InvalidField(
            "multicamGroup.decisionCoverage",
        ));
    }
    Ok(())
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn source_duration_ticks_u64(metadata: &Value, time_base: u64) -> Option<u64> {
    finite_positive_f64(metadata.get("duration"))
        .map(|seconds| (seconds * time_base as f64).round())
        .filter(|ticks| {
            ticks.is_finite() && *ticks >= 1.0 && *ticks <= PROJECT_MAX_SAFE_INTEGER as f64
        })
        .map(|ticks| ticks as u64)
}

fn validate_exact_id(field: &'static str, value: &str) -> Result<(), ProjectDocumentError> {
    validate_id(field, value)?;
    if value.trim() != value {
        return Err(ProjectDocumentError::InvalidField(field));
    }
    Ok(())
}

fn validate_label(field: &'static str, value: &str) -> Result<(), ProjectDocumentError> {
    if value.trim().is_empty() || value.chars().count() > 256 || value.chars().any(char::is_control)
    {
        return Err(ProjectDocumentError::InvalidField(field));
    }
    Ok(())
}

fn source_duration_ticks(metadata: &Value, time_base: u32) -> Option<u64> {
    finite_positive_f64(metadata.get("duration"))
        .map(|seconds| (seconds * f64::from(time_base)).round())
        .filter(|ticks| {
            ticks.is_finite() && *ticks >= 1.0 && *ticks <= PROJECT_MAX_SAFE_INTEGER as f64
        })
        .map(|ticks| ticks as u64)
}

fn finite_positive_f64(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn positive_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn validate_id(field: &'static str, value: &str) -> Result<(), ProjectDocumentError> {
    if value.trim().is_empty() || value.len() > 128 {
        return Err(ProjectDocumentError::InvalidField(field));
    }
    Ok(())
}

fn validate_token(field: &'static str, value: &str) -> Result<(), ProjectDocumentError> {
    if value.trim().is_empty() || value.len() > 64 {
        return Err(ProjectDocumentError::InvalidField(field));
    }
    Ok(())
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectDocumentError {
    ExpectedObject,
    InvalidSchemaVersion,
    UnsupportedSchema(u32),
    UnsupportedEnvelopeSchema(u32),
    MissingField(&'static str),
    InvalidField(&'static str),
    DuplicateId(String),
    MissingReference(String),
    RevisionOverflow,
    Malformed(String),
}

impl fmt::Display for ProjectDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => write!(
                formatter,
                "unsupported project schemaVersion {version}; latest supported is {PROJECT_DOCUMENT_SCHEMA_VERSION}"
            ),
            Self::UnsupportedEnvelopeSchema(version) => write!(
                formatter,
                "unsupported project envelope schemaVersion {version}; expected {PROJECT_ENVELOPE_SCHEMA_VERSION}"
            ),
            other => write!(formatter, "invalid project document: {other:?}"),
        }
    }
}

impl std::error::Error for ProjectDocumentError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn migrates_v1_to_v4_and_preserves_unknown_fields() {
        let document = ProjectDocument::migrate(json!({
            "schemaVersion": 1,
            "videoId": "video-1",
            "name": "Demo",
            "video": {"id": "video-1", "duration": 12.5, "width": 1920, "height": 1080},
            "edit": {"filter": "sepia"},
            "pluginState": {"vendor": "fixture", "revision": 7}
        }))
        .unwrap();

        assert_eq!(document.schema_version, PROJECT_DOCUMENT_SCHEMA_VERSION);
        assert_eq!(document.sequences[0].tracks.len(), 2);
        assert_eq!(
            document.sequences[0].tracks[0].clips[0].duration_ticks,
            12_500_000
        );
        assert_eq!(document.legacy_fields["pluginState"]["revision"], 7);
        let (video, edit) = document.legacy_video_and_edit();
        assert_eq!(video["id"], "video-1");
        assert_eq!(edit["filter"], "sepia");
    }

    #[test]
    fn migrates_v2_asset_identity_to_v4_preserving_order_and_extensions() {
        let fingerprint = "ab".repeat(32);
        let document = ProjectDocument::migrate(json!({
            "schemaVersion": 2,
            "name": "v2", "primaryMediaId": "a", "activeSequenceId": "main",
            "pluginTop": {"keep": true},
            "media": [
                {"id":"a", "kind":"video", "pluginMedia":1,
                 "metadata":{"duration":1, "assetId":"asset-a", "fingerprint":fingerprint, "url":"blob:x"}},
                {"id":"b", "kind":"video", "pluginMedia":2, "metadata":{"duration":1}}
            ],
            "sequences":[{"id":"main", "name":"Main", "settings":{"timeBase":1000000}, "tracks":[]}]
        })).unwrap();
        assert_eq!(document.schema_version, 4);
        assert_eq!(
            document
                .media
                .iter()
                .map(|media| media.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(document.media[0].asset_ref.as_deref(), Some("asset-a"));
        assert_eq!(
            document.media[0].content_fingerprint.as_deref(),
            Some(fingerprint.as_str())
        );
        assert_eq!(document.media[0].extra["pluginMedia"], 1);
        assert!(document.media[0].metadata.get("url").is_none());
        assert!(document.media[0].metadata.get("assetId").is_none());
        assert_eq!(document.extra["pluginTop"]["keep"], true);
    }

    #[test]
    fn migrates_audio_primary_to_the_audio_track() {
        let document = ProjectDocument::migrate(json!({
            "videoId": "audio-1",
            "video": {
                "id": "audio-1",
                "filename": "voice.wav",
                "duration": 3.0,
                "width": 0,
                "height": 0,
                "mediaKind": "audio",
                "acodec": "pcm_s16le",
                "url": "blob:old-tab",
                "path": "/private/voice.wav"
            },
            "edit": {}
        }))
        .unwrap();
        assert_eq!(document.media[0].kind, "audio");
        assert!(document.media[0].metadata.get("url").is_none());
        assert!(document.media[0].metadata.get("path").is_none());
        let sequence = &document.sequences[0];
        assert!(sequence
            .tracks
            .iter()
            .find(|track| track.kind == "video")
            .unwrap()
            .clips
            .is_empty());
        assert_eq!(
            sequence
                .tracks
                .iter()
                .find(|track| track.kind == "audio")
                .unwrap()
                .clips[0]
                .media_id,
            "audio-1"
        );
    }

    #[test]
    fn current_document_round_trip_preserves_unknown_fields_at_every_level() {
        let mut value = serde_json::to_value(
            ProjectDocument::from_legacy(
                "Demo",
                "video-1",
                json!({"id": "video-1", "duration": 1}),
                json!({}),
            )
            .unwrap(),
        )
        .unwrap();
        value["pluginTop"] = json!({"enabled": true});
        value["media"][0]["pluginMedia"] = json!("keep");
        value["sequences"][0]["tracks"][0]["clips"][0]["pluginClip"] = json!(9);
        value["sequences"][0]["tracks"][0]["clips"][0]["effects"][0]["pluginEffect"] =
            json!([1, 2, 3]);

        let document = ProjectDocument::migrate(value.clone()).unwrap();
        let encoded = serde_json::to_value(document).unwrap();
        assert_eq!(encoded["pluginTop"], value["pluginTop"]);
        assert_eq!(encoded["media"][0]["pluginMedia"], "keep");
        assert_eq!(
            encoded["sequences"][0]["tracks"][0]["clips"][0]["pluginClip"],
            9
        );
        assert_eq!(
            encoded["sequences"][0]["tracks"][0]["clips"][0]["effects"][0]["pluginEffect"],
            json!([1, 2, 3])
        );
    }

    #[test]
    fn rejects_forward_versions_explicitly() {
        let error = ProjectDocument::migrate(json!({"schemaVersion": 5})).unwrap_err();
        assert_eq!(error, ProjectDocumentError::UnsupportedSchema(5));
        assert!(error.to_string().contains("latest supported is 4"));
    }

    #[test]
    fn persistence_envelope_advances_revision_and_preserves_unknown_fields() {
        let document = ProjectDocument::from_legacy(
            "Demo",
            "video-1",
            json!({"id": "video-1", "duration": 1}),
            json!({}),
        )
        .unwrap();
        let first = ProjectEnvelope::new("project-1", document.clone(), 10).unwrap();
        let mut value = serde_json::to_value(&first).unwrap();
        value["storageLease"] = json!({"owner": "fixture"});
        let decoded = ProjectEnvelope::decode(value).unwrap();
        let second = decoded.next_revision(document, 20).unwrap();

        assert_eq!(second.project_id, "project-1");
        assert_eq!(second.revision, 2);
        assert_eq!(second.created_at, 10);
        assert_eq!(second.updated_at, 20);
        assert_eq!(second.extra["storageLease"]["owner"], "fixture");
    }

    #[test]
    fn validator_rejects_ambiguous_ids_invalid_dimensions_and_ranges() {
        let base = ProjectDocument::from_legacy(
            "Demo",
            "video-1",
            json!({"id": "video-1", "duration": 1}),
            json!({}),
        )
        .unwrap();

        let mut zero_width = base.clone();
        zero_width.sequences[0].settings.width = Some(0);
        assert_eq!(
            zero_width.validate(),
            Err(ProjectDocumentError::InvalidField(
                "sequence.settings.dimensions"
            ))
        );

        let mut bad_range = base.clone();
        bad_range.sequences[0].tracks[0].clips[0].source_out_tick = 2;
        assert_eq!(
            bad_range.validate(),
            Err(ProjectDocumentError::InvalidField("clip.duration"))
        );

        let mut duplicate_track = base;
        duplicate_track
            .sequences
            .push(duplicate_track.sequences[0].clone());
        duplicate_track.sequences[1].id = "sequence-second".to_owned();
        assert_eq!(
            duplicate_track.validate(),
            Err(ProjectDocumentError::DuplicateId(
                "track-video-main".to_owned()
            ))
        );
    }

    #[test]
    fn migration_is_idempotent_after_serialization() {
        let migrated = ProjectDocument::migrate(json!({
            "videoId": "video-1",
            "video": {"duration": 2},
            "edit": {"filter": "sepia"}
        }))
        .unwrap();
        let mut encoded = serde_json::to_value(&migrated).unwrap();
        encoded["media"][0]["metadata"]["url"] = json!("blob:v2");
        encoded["media"][0]["metadata"]["availability"] = json!("ready");
        let reopened = ProjectDocument::migrate(encoded).unwrap();
        assert_eq!(reopened, migrated);
    }

    #[test]
    fn canonical_multitrack_golden_round_trip_preserves_order_and_extensions() {
        let mut document = ProjectDocument::from_legacy(
            "Монтаж 🎬",
            "media-1",
            json!({"id":"media-1", "duration":8}),
            json!({"filter":"sepia"}),
        )
        .unwrap();
        document
            .extra
            .insert("vendor.top".into(), json!({"keep":true}));
        let base_clip = document.sequences[0].tracks[0].clips[0].clone();
        let mut tracks = Vec::new();
        for index in 0..8 {
            let is_video = index < 4;
            let mut track = ProjectTrack {
                id: format!("track-{index}"),
                kind: if is_video { "video" } else { "audio" }.into(),
                name: format!("Дорожка {index}"),
                clips: Vec::new(),
                extra: BTreeMap::from([("vendor.track".into(), json!(index))]),
            };
            if is_video {
                let mut clip = base_clip.clone();
                clip.id = format!("clip-{index}");
                clip.timeline_start_tick = index * 100_000;
                clip.effects[0].id = format!("effect-{index}");
                clip.extra.insert("vendor.clip".into(), json!([index]));
                track.clips.push(clip);
            }
            tracks.push(track);
        }
        document.sequences[0].tracks = tracks;
        document.sequences[0]
            .settings
            .extra
            .insert("vendor.settings".into(), json!(null));
        document.sequences[0]
            .extra
            .insert("vendor.sequence".into(), json!({"x":1}));
        document.validate().unwrap();

        let encoded = serde_json::to_value(&document).unwrap();
        let reopened = ProjectDocument::migrate(encoded).unwrap();
        assert_eq!(reopened, document);
        assert_eq!(
            reopened.sequences[0]
                .tracks
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            (0..8)
                .map(|index| format!("track-{index}"))
                .collect::<Vec<_>>()
        );
    }

    fn valid_multicam_document() -> ProjectDocument {
        let mut document = ProjectDocument::from_legacy(
            "Multicam",
            "camera-a",
            json!({"id":"camera-a", "duration":20, "mediaKind":"video"}),
            json!({}),
        )
        .unwrap();
        let mut second = document.media[0].clone();
        second.id = "camera-b".into();
        second.asset_ref = Some("camera-b".into());
        second.metadata["id"] = json!("camera-b");
        document.media.push(second);
        document.multicam_groups.push(MulticamGroup {
            contract: "multicam-v1".into(),
            id: "multicam-main".into(),
            name: "Interview".into(),
            time_base: u64::from(PROJECT_TIME_BASE),
            duration_ticks: 10_000_000,
            reference_angle_id: "angle-a".into(),
            audio_angle_id: "angle-a".into(),
            sync: MulticamSync {
                method: MulticamSyncMethod::Marker,
                algorithm_version: "marker-v1".into(),
                confidence: Some(1.0),
                extra: BTreeMap::new(),
            },
            angles: vec![
                MulticamAngle {
                    id: "angle-a".into(),
                    media_id: "camera-a".into(),
                    label: "Camera A".into(),
                    source_origin_tick: 1_000_000,
                    rate: MulticamRate {
                        numerator: 1,
                        denominator: 1,
                    },
                    enabled: true,
                    extra: BTreeMap::new(),
                },
                MulticamAngle {
                    id: "angle-b".into(),
                    media_id: "camera-b".into(),
                    label: "Camera B".into(),
                    source_origin_tick: 2_000_000,
                    rate: MulticamRate {
                        numerator: 1,
                        denominator: 1,
                    },
                    enabled: true,
                    extra: BTreeMap::new(),
                },
            ],
            decisions: vec![
                MulticamDecision {
                    id: "decision-start".into(),
                    offset_tick: 0,
                    angle_id: "angle-a".into(),
                    extra: BTreeMap::new(),
                },
                MulticamDecision {
                    id: "decision-cut".into(),
                    offset_tick: 5_000_000,
                    angle_id: "angle-b".into(),
                    extra: BTreeMap::new(),
                },
            ],
            extra: BTreeMap::from([("vendor.group".into(), json!({"keep":true}))]),
        });
        document
    }

    #[test]
    fn v3_migrates_to_v4_without_inventing_multicam_state() {
        let document =
            ProjectDocument::from_legacy("Legacy v3", "video-1", json!({"duration":1}), json!({}))
                .unwrap();
        let mut value = serde_json::to_value(document).unwrap();
        value["schemaVersion"] = json!(3);
        value.as_object_mut().unwrap().remove("multicamGroups");
        let migrated = ProjectDocument::migrate(value).unwrap();
        assert_eq!(migrated.schema_version, 4);
        assert!(migrated.multicam_groups.is_empty());
    }

    #[test]
    fn multicam_v4_round_trips_stable_ids_mappings_and_decisions() {
        let document = valid_multicam_document();
        document.validate().unwrap();
        let encoded = serde_json::to_value(&document).unwrap();
        let reopened = ProjectDocument::migrate(encoded).unwrap();
        assert_eq!(reopened, document);
        let group = &reopened.multicam_groups[0];
        assert_eq!(
            group
                .angles
                .iter()
                .map(|angle| angle.id.as_str())
                .collect::<Vec<_>>(),
            vec!["angle-a", "angle-b"]
        );
        assert_eq!(group.decisions[1].id, "decision-cut");
        assert_eq!(group.extra["vendor.group"]["keep"], true);
    }

    #[test]
    fn multicam_rejects_ambiguous_or_noncanonical_decisions() {
        let mut zero_cut = valid_multicam_document();
        zero_cut.multicam_groups[0].decisions[1].offset_tick = 0;
        assert_eq!(
            zero_cut.validate(),
            Err(ProjectDocumentError::InvalidField(
                "multicamGroup.decisions"
            ))
        );

        let mut no_op = valid_multicam_document();
        no_op.multicam_groups[0].decisions[1].angle_id = "angle-a".into();
        assert_eq!(
            no_op.validate(),
            Err(ProjectDocumentError::InvalidField(
                "multicamGroup.decisions"
            ))
        );

        let mut missing_reference = valid_multicam_document();
        missing_reference.multicam_groups[0].reference_angle_id = "gone".into();
        assert_eq!(
            missing_reference.validate(),
            Err(ProjectDocumentError::InvalidField(
                "multicamGroup.angleReference"
            ))
        );
    }

    #[test]
    fn multicam_rejects_nonreduced_rate_and_source_overrun() {
        let mut rate = valid_multicam_document();
        rate.multicam_groups[0].angles[0].rate = MulticamRate {
            numerator: 2,
            denominator: 2,
        };
        assert_eq!(
            rate.validate(),
            Err(ProjectDocumentError::InvalidField("multicamAngle.source"))
        );

        let mut overrun = valid_multicam_document();
        overrun.multicam_groups[0].duration_ticks = 20_000_000;
        assert_eq!(
            overrun.validate(),
            Err(ProjectDocumentError::InvalidField(
                "multicamAngle.sourceRange"
            ))
        );
    }
}
