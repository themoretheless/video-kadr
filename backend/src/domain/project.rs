use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Version 1 was the legacy `{ videoId, video, edit }` autosave payload.
/// Version 2 is the first canonical, multitrack project document.
pub const PROJECT_DOCUMENT_SCHEMA_VERSION: u32 = 2;
pub const PROJECT_ENVELOPE_SCHEMA_VERSION: u32 = 1;
pub const PROJECT_TIME_BASE: u32 = 1_000_000;
pub const PROJECT_MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const CREATOR_VIDEO_TRACK_COUNT: usize = 4;
pub const CREATOR_AUDIO_TRACK_COUNT: usize = 4;

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
    pub media: Vec<ProjectMedia>,
    pub sequences: Vec<ProjectSequence>,
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
            PROJECT_DOCUMENT_SCHEMA_VERSION => {
                let document: Self = serde_json::from_value(value)
                    .map_err(|error| ProjectDocumentError::Malformed(error.to_string()))?;
                document.validate()?;
                Ok(document)
            }
            version => Err(ProjectDocumentError::UnsupportedSchema(version)),
        }
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
        video: Value,
        edit: Value,
        legacy_fields: BTreeMap<String, Value>,
    ) -> Result<Self, ProjectDocumentError> {
        if !video.is_object() {
            return Err(ProjectDocumentError::InvalidField("video"));
        }
        if !edit.is_object() {
            return Err(ProjectDocumentError::InvalidField("edit"));
        }
        let duration_ticks = legacy_duration_ticks(&video);
        let settings = SequenceSettings {
            time_base: PROJECT_TIME_BASE,
            frame_rate: finite_positive_f64(video.get("fps")),
            width: positive_u32(video.get("width")),
            height: positive_u32(video.get("height")),
            extra: BTreeMap::new(),
        };
        let document = Self {
            schema_version: PROJECT_DOCUMENT_SCHEMA_VERSION,
            name,
            primary_media_id: video_id.clone(),
            active_sequence_id: "sequence-main".to_owned(),
            media: vec![ProjectMedia {
                id: video_id.clone(),
                kind: "video".to_owned(),
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
                        clips: vec![ProjectClip {
                            id: "clip-main".to_owned(),
                            media_id: video_id,
                            timeline_start_tick: 0,
                            duration_ticks,
                            source_in_tick: 0,
                            source_out_tick: duration_ticks,
                            effects: vec![ProjectEffect {
                                id: "effect-legacy-edit".to_owned(),
                                kind: "legacy_edit".to_owned(),
                                enabled: true,
                                parameters: edit,
                                extra: BTreeMap::new(),
                            }],
                            extra: BTreeMap::new(),
                        }],
                        extra: BTreeMap::new(),
                    },
                    ProjectTrack {
                        id: "track-audio-main".to_owned(),
                        kind: "audio".to_owned(),
                        name: "Аудио 1".to_owned(),
                        clips: Vec::new(),
                        extra: BTreeMap::new(),
                    },
                ],
                extra: BTreeMap::new(),
            }],
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
        video: Value,
        edit: Value,
    ) -> Result<(), ProjectDocumentError> {
        if !video.is_object() || !edit.is_object() {
            return Err(ProjectDocumentError::InvalidField("legacy values"));
        }
        self.name = name.into();
        let media = self
            .media
            .iter_mut()
            .find(|media| media.id == self.primary_media_id)
            .ok_or_else(|| ProjectDocumentError::MissingReference(self.primary_media_id.clone()))?;
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
            let clip = sequence
                .tracks
                .iter_mut()
                .flat_map(|track| &mut track.clips)
                .find(|clip| clip.media_id == self.primary_media_id)
                .ok_or(ProjectDocumentError::InvalidField("primary media clip"))?;
            clip.effects.push(ProjectEffect {
                id: effect_id,
                kind: "legacy_edit".into(),
                enabled: true,
                parameters: edit,
                extra: BTreeMap::new(),
            });
        }
        self.validate()
    }
}

fn merge_objects(target: &mut Value, update: Value) {
    if let (Some(target), Some(update)) = (target.as_object_mut(), update.as_object()) {
        target.extend(update.clone());
    }
}

fn legacy_duration_ticks(video: &Value) -> u64 {
    finite_positive_f64(video.get("duration"))
        .map(|seconds| (seconds * f64::from(PROJECT_TIME_BASE)).round())
        .filter(|ticks| ticks.is_finite() && *ticks >= 1.0 && *ticks <= u64::MAX as f64)
        .map(|ticks| ticks as u64)
        .unwrap_or(1)
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
    fn migrates_v1_to_v2_and_preserves_unknown_fields() {
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
        let error = ProjectDocument::migrate(json!({"schemaVersion": 3})).unwrap_err();
        assert_eq!(error, ProjectDocumentError::UnsupportedSchema(3));
        assert!(error.to_string().contains("latest supported is 2"));
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
        let reopened = ProjectDocument::migrate(serde_json::to_value(&migrated).unwrap()).unwrap();
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
}
