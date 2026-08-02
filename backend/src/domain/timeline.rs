use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::project::{ProjectClip, ProjectDocument};

pub const TIMELINE_SCHEMA_VERSION: u32 = 1;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, TimelineError> {
                let value = value.into();
                if value.trim().is_empty() || value.len() > 128 {
                    return Err(TimelineError::InvalidId(value));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

stable_id!(ClipId);
stable_id!(OperationId);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clip {
    pub id: ClipId,
    pub source_id: String,
    pub label: String,
    pub duration_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Trim {
        id: OperationId,
        clip_id: ClipId,
        start_tick: u64,
        end_tick: u64,
    },
    Effect {
        id: OperationId,
        clip_id: ClipId,
        effect: String,
        #[serde(default)]
        parameters: BTreeMap<String, f64>,
    },
}

impl Operation {
    pub fn id(&self) -> &OperationId {
        match self {
            Self::Trim { id, .. } | Self::Effect { id, .. } => id,
        }
    }

    pub fn clip_id(&self) -> &ClipId {
        match self {
            Self::Trim { clip_id, .. } | Self::Effect { clip_id, .. } => clip_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Timeline {
    pub schema_version: u32,
    pub time_base: u32,
    pub clips: Vec<Clip>,
    pub operations: BTreeMap<OperationId, Operation>,
}

impl Timeline {
    pub fn new(time_base: u32) -> Result<Self, TimelineError> {
        if time_base == 0 {
            return Err(TimelineError::InvalidTimeBase);
        }
        Ok(Self {
            schema_version: TIMELINE_SCHEMA_VERSION,
            time_base,
            clips: Vec::new(),
            operations: BTreeMap::new(),
        })
    }

    pub fn insert_clip(&self, index: usize, clip: Clip) -> Result<Self, TimelineError> {
        if index > self.clips.len() {
            return Err(TimelineError::InvalidIndex(index));
        }
        if self.clips.iter().any(|existing| existing.id == clip.id) {
            return Err(TimelineError::DuplicateClip(clip.id));
        }
        let mut next = self.clone();
        next.clips.insert(index, clip);
        next.validate()?;
        Ok(next)
    }

    pub fn reorder_clip(&self, clip_id: &ClipId, to: usize) -> Result<Self, TimelineError> {
        if to >= self.clips.len() {
            return Err(TimelineError::InvalidIndex(to));
        }
        let from = self
            .clips
            .iter()
            .position(|clip| &clip.id == clip_id)
            .ok_or_else(|| TimelineError::MissingClip(clip_id.clone()))?;
        let mut next = self.clone();
        let clip = next.clips.remove(from);
        next.clips.insert(to, clip);
        next.validate()?;
        Ok(next)
    }

    pub fn add_operation(&self, operation: Operation) -> Result<Self, TimelineError> {
        if !self
            .clips
            .iter()
            .any(|clip| &clip.id == operation.clip_id())
        {
            return Err(TimelineError::MissingClip(operation.clip_id().clone()));
        }
        if self.operations.contains_key(operation.id()) {
            return Err(TimelineError::DuplicateOperation(operation.id().clone()));
        }
        validate_operation(&operation)?;
        let mut next = self.clone();
        next.operations.insert(operation.id().clone(), operation);
        Ok(next)
    }

    pub fn remove_operation(&self, id: &OperationId) -> Result<Self, TimelineError> {
        let mut next = self.clone();
        if next.operations.remove(id).is_none() {
            return Err(TimelineError::MissingOperation(id.clone()));
        }
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), TimelineError> {
        if self.schema_version != TIMELINE_SCHEMA_VERSION {
            return Err(TimelineError::UnsupportedSchema(self.schema_version));
        }
        if self.time_base == 0 {
            return Err(TimelineError::InvalidTimeBase);
        }
        let mut ids = BTreeSet::new();
        for clip in &self.clips {
            if !ids.insert(&clip.id) {
                return Err(TimelineError::DuplicateClip(clip.id.clone()));
            }
        }
        for (id, operation) in &self.operations {
            if id != operation.id() {
                return Err(TimelineError::OperationKeyMismatch(id.clone()));
            }
            if !ids.contains(operation.clip_id()) {
                return Err(TimelineError::MissingClip(operation.clip_id().clone()));
            }
            validate_operation(operation)?;
        }
        Ok(())
    }
}

fn validate_operation(operation: &Operation) -> Result<(), TimelineError> {
    if let Operation::Trim {
        start_tick,
        end_tick,
        ..
    } = operation
    {
        if end_tick <= start_tick {
            return Err(TimelineError::InvalidTrim);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveClipCommand {
    pub clip_id: ClipId,
    pub from: usize,
    pub to: usize,
}

impl MoveClipCommand {
    pub fn apply(&self, timeline: &Timeline) -> Result<Timeline, TimelineError> {
        if timeline.clips.get(self.from).map(|clip| &clip.id) != Some(&self.clip_id) {
            return Err(TimelineError::CommandPrecondition);
        }
        timeline.reorder_clip(&self.clip_id, self.to)
    }

    pub fn invert(&self) -> Self {
        Self {
            clip_id: self.clip_id.clone(),
            from: self.to,
            to: self.from,
        }
    }
}

/// Exact location of a canonical project clip. `index` is the index in the
/// resulting track for insert/move commands, and the current index for remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipPlacement {
    pub sequence_id: String,
    pub track_id: String,
    pub index: usize,
    pub timeline_start_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipTrim {
    pub timeline_start_tick: u64,
    pub source_in_tick: u64,
    pub source_out_tick: u64,
    pub duration_ticks: u64,
}

impl ClipTrim {
    fn from_clip(clip: &ProjectClip) -> Self {
        Self {
            timeline_start_tick: clip.timeline_start_tick,
            source_in_tick: clip.source_in_tick,
            source_out_tick: clip.source_out_tick,
            duration_ticks: clip.duration_ticks,
        }
    }
}

/// An invertible structural edit against the canonical project timeline.
/// Commands carry their pre-state so undo never depends on mutable UI state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StructuralCommand {
    Insert {
        placement: ClipPlacement,
        clip: ProjectClip,
    },
    Remove {
        placement: ClipPlacement,
        clip: ProjectClip,
    },
    Move {
        clip: ProjectClip,
        from: ClipPlacement,
        to: ClipPlacement,
    },
    Trim {
        clip_id: String,
        before: ClipTrim,
        after: ClipTrim,
    },
}

impl StructuralCommand {
    pub fn insert_clip(
        document: &ProjectDocument,
        sequence_id: impl Into<String>,
        track_id: impl Into<String>,
        index: usize,
        timeline_start_tick: u64,
        mut clip: ProjectClip,
    ) -> Result<Self, TimelineError> {
        clip.timeline_start_tick = timeline_start_tick;
        let command = Self::Insert {
            placement: ClipPlacement {
                sequence_id: sequence_id.into(),
                track_id: track_id.into(),
                index,
                timeline_start_tick,
            },
            clip,
        };
        command.apply(document)?;
        Ok(command)
    }

    pub fn remove_clip(document: &ProjectDocument, clip_id: &str) -> Result<Self, TimelineError> {
        validate_project(document)?;
        let (placement, clip) = locate_project_clip(document, clip_id)?;
        Ok(Self::Remove { placement, clip })
    }

    pub fn move_clip(
        document: &ProjectDocument,
        clip_id: &str,
        sequence_id: impl Into<String>,
        track_id: impl Into<String>,
        index: usize,
        timeline_start_tick: u64,
    ) -> Result<Self, TimelineError> {
        validate_project(document)?;
        let (from, clip) = locate_project_clip(document, clip_id)?;
        let command = Self::Move {
            clip,
            from,
            to: ClipPlacement {
                sequence_id: sequence_id.into(),
                track_id: track_id.into(),
                index,
                timeline_start_tick,
            },
        };
        command.apply(document)?;
        Ok(command)
    }

    pub fn trim_clip(
        document: &ProjectDocument,
        clip_id: &str,
        source_in_tick: u64,
        source_out_tick: u64,
    ) -> Result<Self, TimelineError> {
        let (_, clip) = locate_project_clip(document, clip_id)?;
        Self::trim_clip_at(
            document,
            clip_id,
            clip.timeline_start_tick,
            source_in_tick,
            source_out_tick,
        )
    }

    pub fn trim_clip_at(
        document: &ProjectDocument,
        clip_id: &str,
        timeline_start_tick: u64,
        source_in_tick: u64,
        source_out_tick: u64,
    ) -> Result<Self, TimelineError> {
        validate_project(document)?;
        let (_, clip) = locate_project_clip(document, clip_id)?;
        let duration_ticks = source_out_tick
            .checked_sub(source_in_tick)
            .filter(|duration| *duration > 0)
            .ok_or(TimelineError::InvalidTrim)?;
        let command = Self::Trim {
            clip_id: clip_id.to_owned(),
            before: ClipTrim::from_clip(&clip),
            after: ClipTrim {
                timeline_start_tick,
                source_in_tick,
                source_out_tick,
                duration_ticks,
            },
        };
        command.apply(document)?;
        Ok(command)
    }

    pub fn apply(&self, document: &ProjectDocument) -> Result<ProjectDocument, TimelineError> {
        match self {
            Self::Insert { placement, clip } => insert_project_clip(document, placement, clip),
            Self::Remove { placement, clip } => remove_project_clip(document, placement, clip),
            Self::Move { clip, from, to } => move_project_clip(document, from, to, clip),
            Self::Trim {
                clip_id,
                before,
                after,
            } => trim_project_clip(document, clip_id, *before, *after),
        }
    }

    pub fn undo(&self, document: &ProjectDocument) -> Result<ProjectDocument, TimelineError> {
        match self {
            Self::Insert { placement, clip } => remove_project_clip(document, placement, clip),
            Self::Remove { placement, clip } => insert_project_clip(document, placement, clip),
            Self::Move { clip, from, to } => move_project_clip(document, to, from, clip),
            Self::Trim {
                clip_id,
                before,
                after,
            } => trim_project_clip(document, clip_id, *after, *before),
        }
    }
}

/// Bounded structural command history. Failed commands do not mutate either
/// the document or the undo/redo stacks.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralHistory {
    maximum_bytes: usize,
    used_bytes: usize,
    past: Vec<(StructuralCommand, usize)>,
    future: Vec<(StructuralCommand, usize)>,
}

impl StructuralHistory {
    pub fn new(maximum_bytes: usize) -> Result<Self, TimelineError> {
        if maximum_bytes == 0 {
            return Err(TimelineError::InvalidHistoryCapacity);
        }
        Ok(Self {
            maximum_bytes,
            used_bytes: 0,
            past: Vec::new(),
            future: Vec::new(),
        })
    }

    pub fn execute(
        &mut self,
        document: &ProjectDocument,
        command: StructuralCommand,
    ) -> Result<ProjectDocument, TimelineError> {
        let next = command.apply(document)?;
        let bytes = serde_json::to_vec(&command)
            .map_err(|error| TimelineError::HistoryEncoding(error.to_string()))?
            .len();
        if bytes > self.maximum_bytes {
            return Err(TimelineError::HistoryBudgetExceeded {
                bytes,
                maximum: self.maximum_bytes,
            });
        }
        self.past.push((command, bytes));
        self.used_bytes = self.used_bytes.saturating_add(bytes);
        self.evict_to_budget();
        self.future.clear();
        Ok(next)
    }

    pub fn undo(
        &mut self,
        document: &ProjectDocument,
    ) -> Result<Option<ProjectDocument>, TimelineError> {
        let Some((command, _)) = self.past.last() else {
            return Ok(None);
        };
        let previous = command.undo(document)?;
        let stored = self.past.pop().expect("history was checked non-empty");
        self.used_bytes = self.used_bytes.saturating_sub(stored.1);
        self.future.push(stored);
        Ok(Some(previous))
    }

    pub fn redo(
        &mut self,
        document: &ProjectDocument,
    ) -> Result<Option<ProjectDocument>, TimelineError> {
        let Some((command, _)) = self.future.last() else {
            return Ok(None);
        };
        let next = command.apply(document)?;
        let stored = self.future.pop().expect("history was checked non-empty");
        self.used_bytes = self.used_bytes.saturating_add(stored.1);
        self.past.push(stored);
        self.evict_to_budget();
        Ok(Some(next))
    }

    pub fn undo_len(&self) -> usize {
        self.past.len()
    }

    pub fn redo_len(&self) -> usize {
        self.future.len()
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    fn evict_to_budget(&mut self) {
        while self.used_bytes > self.maximum_bytes && self.past.len() > 1 {
            let removed = self.past.remove(0);
            self.used_bytes = self.used_bytes.saturating_sub(removed.1);
        }
    }
}

fn validate_project(document: &ProjectDocument) -> Result<(), TimelineError> {
    document
        .validate()
        .map_err(|error| TimelineError::ProjectInvariant(error.to_string()))
}

fn locate_project_clip(
    document: &ProjectDocument,
    clip_id: &str,
) -> Result<(ClipPlacement, ProjectClip), TimelineError> {
    for sequence in &document.sequences {
        for track in &sequence.tracks {
            if let Some((index, clip)) = track
                .clips
                .iter()
                .enumerate()
                .find(|(_, clip)| clip.id == clip_id)
            {
                return Ok((
                    ClipPlacement {
                        sequence_id: sequence.id.clone(),
                        track_id: track.id.clone(),
                        index,
                        timeline_start_tick: clip.timeline_start_tick,
                    },
                    clip.clone(),
                ));
            }
        }
    }
    Err(TimelineError::MissingProjectClip(clip_id.to_owned()))
}

fn insert_project_clip(
    document: &ProjectDocument,
    placement: &ClipPlacement,
    clip: &ProjectClip,
) -> Result<ProjectDocument, TimelineError> {
    validate_project(document)?;
    if document
        .sequences
        .iter()
        .flat_map(|sequence| &sequence.tracks)
        .flat_map(|track| &track.clips)
        .any(|existing| existing.id == clip.id)
    {
        return Err(TimelineError::DuplicateProjectClip(clip.id.clone()));
    }
    if clip.timeline_start_tick != placement.timeline_start_tick {
        return Err(TimelineError::CommandPrecondition);
    }
    let current_sequence = document
        .sequences
        .iter()
        .find(|sequence| sequence.id == placement.sequence_id)
        .ok_or_else(|| TimelineError::MissingSequence(placement.sequence_id.clone()))?;
    let current_track = current_sequence
        .tracks
        .iter()
        .find(|track| track.id == placement.track_id)
        .ok_or_else(|| TimelineError::MissingTrack(placement.track_id.clone()))?;
    ensure_track_editable(current_track)?;
    ensure_track_accepts_clip(document, current_track, clip)?;
    let mut next = document.clone();
    let sequence = next
        .sequences
        .iter_mut()
        .find(|sequence| sequence.id == placement.sequence_id)
        .ok_or_else(|| TimelineError::MissingSequence(placement.sequence_id.clone()))?;
    let track = sequence
        .tracks
        .iter_mut()
        .find(|track| track.id == placement.track_id)
        .ok_or_else(|| TimelineError::MissingTrack(placement.track_id.clone()))?;
    if placement.index > track.clips.len() {
        return Err(TimelineError::InvalidIndex(placement.index));
    }
    track.clips.insert(placement.index, clip.clone());
    ensure_no_same_track_overlap(track)?;
    validate_project(&next)?;
    Ok(next)
}

fn remove_project_clip(
    document: &ProjectDocument,
    placement: &ClipPlacement,
    clip: &ProjectClip,
) -> Result<ProjectDocument, TimelineError> {
    validate_project(document)?;
    let mut next = document.clone();
    let sequence = next
        .sequences
        .iter_mut()
        .find(|sequence| sequence.id == placement.sequence_id)
        .ok_or_else(|| TimelineError::MissingSequence(placement.sequence_id.clone()))?;
    let track = sequence
        .tracks
        .iter_mut()
        .find(|track| track.id == placement.track_id)
        .ok_or_else(|| TimelineError::MissingTrack(placement.track_id.clone()))?;
    ensure_track_editable(track)?;
    if !track
        .clips
        .get(placement.index)
        .is_some_and(|actual| structurally_same_clip(actual, clip))
        || clip.timeline_start_tick != placement.timeline_start_tick
    {
        return Err(TimelineError::CommandPrecondition);
    }
    track.clips.remove(placement.index);
    validate_project(&next)?;
    Ok(next)
}

fn move_project_clip(
    document: &ProjectDocument,
    from: &ClipPlacement,
    to: &ClipPlacement,
    clip: &ProjectClip,
) -> Result<ProjectDocument, TimelineError> {
    let mut expected = clip.clone();
    expected.timeline_start_tick = from.timeline_start_tick;
    let (_, actual) = locate_project_clip(document, &clip.id)?;
    if !structurally_same_clip(&actual, &expected) {
        return Err(TimelineError::CommandPrecondition);
    }
    let without = remove_project_clip(document, from, &expected)?;
    let mut moved = actual;
    moved.timeline_start_tick = to.timeline_start_tick;
    insert_project_clip(&without, to, &moved)
}

fn structurally_same_clip(left: &ProjectClip, right: &ProjectClip) -> bool {
    left.id == right.id
        && left.media_id == right.media_id
        && left.timeline_start_tick == right.timeline_start_tick
        && left.duration_ticks == right.duration_ticks
        && left.source_in_tick == right.source_in_tick
        && left.source_out_tick == right.source_out_tick
}

fn trim_project_clip(
    document: &ProjectDocument,
    clip_id: &str,
    expected: ClipTrim,
    replacement: ClipTrim,
) -> Result<ProjectDocument, TimelineError> {
    validate_project(document)?;
    if replacement.duration_ticks == 0
        || replacement.source_out_tick <= replacement.source_in_tick
        || replacement.source_out_tick - replacement.source_in_tick != replacement.duration_ticks
    {
        return Err(TimelineError::InvalidTrim);
    }
    let (placement, _) = locate_project_clip(document, clip_id)?;
    let mut next = document.clone();
    let sequence = next
        .sequences
        .iter_mut()
        .find(|sequence| sequence.id == placement.sequence_id)
        .ok_or_else(|| TimelineError::MissingSequence(placement.sequence_id.clone()))?;
    let track = sequence
        .tracks
        .iter_mut()
        .find(|track| track.id == placement.track_id)
        .ok_or_else(|| TimelineError::MissingTrack(placement.track_id.clone()))?;
    ensure_track_editable(track)?;
    let clip = track
        .clips
        .iter_mut()
        .find(|clip| clip.id == clip_id)
        .ok_or_else(|| TimelineError::MissingProjectClip(clip_id.to_owned()))?;
    if ClipTrim::from_clip(clip) != expected {
        return Err(TimelineError::CommandPrecondition);
    }
    clip.source_in_tick = replacement.source_in_tick;
    clip.source_out_tick = replacement.source_out_tick;
    clip.duration_ticks = replacement.duration_ticks;
    clip.timeline_start_tick = replacement.timeline_start_tick;
    validate_project(&next)?;
    Ok(next)
}

fn ensure_track_editable(
    track: &crate::domain::project::ProjectTrack,
) -> Result<(), TimelineError> {
    if track
        .extra
        .get("locked")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Err(TimelineError::LockedTrack(track.id.clone()));
    }
    Ok(())
}

fn ensure_track_accepts_clip(
    document: &ProjectDocument,
    track: &crate::domain::project::ProjectTrack,
    clip: &ProjectClip,
) -> Result<(), TimelineError> {
    let media = document
        .media
        .iter()
        .find(|media| media.id == clip.media_id)
        .ok_or_else(|| {
            TimelineError::ProjectInvariant(format!("missing media {}", clip.media_id))
        })?;
    let compatible = match track.kind.as_str() {
        "video" => matches!(media.kind.as_str(), "video" | "image"),
        "audio" => media.kind == "audio",
        _ => false,
    };
    if !compatible {
        return Err(TimelineError::IncompatibleTrack {
            track_kind: track.kind.clone(),
            media_kind: media.kind.clone(),
        });
    }
    Ok(())
}

fn ensure_no_same_track_overlap(
    track: &crate::domain::project::ProjectTrack,
) -> Result<(), TimelineError> {
    let mut clips = track.clips.iter().collect::<Vec<_>>();
    clips.sort_by_key(|clip| (clip.timeline_start_tick, clip.id.as_str()));
    for pair in clips.windows(2) {
        let end = pair[0]
            .timeline_start_tick
            .checked_add(pair[0].duration_ticks)
            .ok_or(TimelineError::ProjectInvariant("timeline overflow".into()))?;
        if end > pair[1].timeline_start_tick {
            return Err(TimelineError::SameTrackOverlap(track.id.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineError {
    InvalidId(String),
    InvalidTimeBase,
    InvalidIndex(usize),
    InvalidTrim,
    DuplicateClip(ClipId),
    MissingClip(ClipId),
    DuplicateOperation(OperationId),
    MissingOperation(OperationId),
    OperationKeyMismatch(OperationId),
    UnsupportedSchema(u32),
    CommandPrecondition,
    InvalidHistoryCapacity,
    MissingSequence(String),
    MissingTrack(String),
    MissingProjectClip(String),
    DuplicateProjectClip(String),
    ProjectInvariant(String),
    HistoryEncoding(String),
    HistoryBudgetExceeded {
        bytes: usize,
        maximum: usize,
    },
    LockedTrack(String),
    IncompatibleTrack {
        track_kind: String,
        media_kind: String,
    },
    SameTrackOverlap(String),
}

impl fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid timeline: {self:?}")
    }
}

impl std::error::Error for TimelineError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::domain::project::{ProjectDocument, ProjectDocumentError};

    use super::*;

    fn clip(id: &str) -> Clip {
        Clip {
            id: ClipId::parse(id).unwrap(),
            source_id: format!("source-{id}"),
            label: id.to_owned(),
            duration_ticks: 1_000,
        }
    }

    #[test]
    fn stable_references_survive_reorder_undo_and_round_trip() {
        let first = clip("clip-a");
        let second = clip("clip-b");
        let timeline = Timeline::new(1_000)
            .unwrap()
            .insert_clip(0, first.clone())
            .unwrap()
            .insert_clip(1, second)
            .unwrap()
            .add_operation(Operation::Effect {
                id: OperationId::parse("operation-a").unwrap(),
                clip_id: first.id.clone(),
                effect: "opacity".to_owned(),
                parameters: BTreeMap::from([("value".to_owned(), 0.5)]),
            })
            .unwrap();
        let command = MoveClipCommand {
            clip_id: first.id.clone(),
            from: 0,
            to: 1,
        };
        let moved = command.apply(&timeline).unwrap();
        let restored = command.invert().apply(&moved).unwrap();
        let decoded: Timeline =
            serde_json::from_str(&serde_json::to_string(&moved).unwrap()).unwrap();

        assert_eq!(restored, timeline);
        assert_eq!(decoded, moved);
        assert_eq!(
            decoded.operations.values().next().unwrap().clip_id(),
            &first.id
        );
    }

    fn canonical_project() -> ProjectDocument {
        ProjectDocument::from_legacy(
            "Timeline",
            "media-main",
            json!({"id":"media-main", "duration":1}),
            json!({}),
        )
        .unwrap()
    }

    fn project_clip<'a>(document: &'a ProjectDocument, id: &str) -> &'a ProjectClip {
        document
            .sequences
            .iter()
            .flat_map(|sequence| &sequence.tracks)
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == id)
            .unwrap()
    }

    fn project_clip_mut<'a>(document: &'a mut ProjectDocument, id: &str) -> &'a mut ProjectClip {
        document
            .sequences
            .iter_mut()
            .flat_map(|sequence| &mut sequence.tracks)
            .flat_map(|track| &mut track.clips)
            .find(|clip| clip.id == id)
            .unwrap()
    }

    #[test]
    fn canonical_timeline_supports_four_av_tracks_and_twenty_four_positioned_clips() {
        let mut document = canonical_project()
            .ensure_creator_track_layout("sequence-main")
            .unwrap();
        let sequence = &document.sequences[0];
        assert_eq!(
            sequence
                .tracks
                .iter()
                .filter(|track| track.kind == "video")
                .count(),
            4
        );
        assert_eq!(
            sequence
                .tracks
                .iter()
                .filter(|track| track.kind == "audio")
                .count(),
            4
        );
        assert_eq!(
            document
                .ensure_creator_track_layout("sequence-main")
                .unwrap(),
            document,
            "ensuring capacity is idempotent"
        );

        let video_track_indices = document.sequences[0]
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.kind == "video")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let template = document.sequences[0].tracks[video_track_indices[0]].clips[0].clone();
        for index in 1..24 {
            let mut clip = template.clone();
            clip.id = format!("clip-{index}");
            // Starts repeat across parallel tracks while clips on each one
            // remain non-overlapping under the canonical invariant.
            clip.timeline_start_tick = (index as u64 / 4) * template.duration_ticks;
            clip.effects.clear();
            let track_index = video_track_indices[index % video_track_indices.len()];
            document.sequences[0].tracks[track_index].clips.push(clip);
        }

        document.validate().unwrap();
        let clip_count = document.sequences[0]
            .tracks
            .iter()
            .map(|track| track.clips.len())
            .sum::<usize>();
        assert_eq!(clip_count, 24);
        let starts = document.sequences[0]
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| clip.timeline_start_tick)
            .collect::<Vec<_>>();
        assert!(starts.iter().filter(|start| **start == 0).count() > 1);

        let round_trip =
            ProjectDocument::migrate(serde_json::to_value(&document).unwrap()).unwrap();
        assert_eq!(round_trip, document);
    }

    #[test]
    fn nondestructive_trim_round_trips_through_structural_undo_redo() {
        let document = canonical_project();
        let source_before = document.media[0].metadata.clone();
        assert!(matches!(
            StructuralCommand::trim_clip(&document, "clip-main", 0, 1_000_001),
            Err(TimelineError::ProjectInvariant(_))
        ));
        let command =
            StructuralCommand::trim_clip(&document, "clip-main", 100_000, 900_000).unwrap();
        let mut history = StructuralHistory::new(1_000_000).unwrap();
        let trimmed = history.execute(&document, command).unwrap();
        let clip = project_clip(&trimmed, "clip-main");

        assert_eq!(trimmed.media[0].metadata, source_before);
        assert_eq!(clip.media_id, "media-main");
        assert_eq!(clip.timeline_start_tick, 0);
        assert_eq!(
            (
                clip.source_in_tick,
                clip.source_out_tick,
                clip.duration_ticks
            ),
            (100_000, 900_000, 800_000)
        );

        let restored = history.undo(&trimmed).unwrap().unwrap();
        assert_eq!(restored, document);
        let redone = history.redo(&restored).unwrap().unwrap();
        assert_eq!(redone, trimmed);
    }

    #[test]
    fn insert_move_remove_commands_have_stable_history_and_clear_redo_branch() {
        let document = canonical_project()
            .ensure_creator_track_layout("sequence-main")
            .unwrap();
        let video_tracks = document.sequences[0]
            .tracks
            .iter()
            .filter(|track| track.kind == "video")
            .map(|track| track.id.clone())
            .collect::<Vec<_>>();
        let mut added = project_clip(&document, "clip-main").clone();
        added.id = "clip-added".into();
        added.effects.clear();
        added.source_out_tick = 500_000;
        added.duration_ticks = 500_000;

        let mut history = StructuralHistory::new(1_000_000).unwrap();
        let insert = StructuralCommand::insert_clip(
            &document,
            "sequence-main",
            &video_tracks[0],
            1,
            1_200_000,
            added,
        )
        .unwrap();
        let inserted = history.execute(&document, insert).unwrap();
        let move_clip = StructuralCommand::move_clip(
            &inserted,
            "clip-added",
            "sequence-main",
            &video_tracks[1],
            0,
            300_000,
        )
        .unwrap();
        let moved = history.execute(&inserted, move_clip).unwrap();
        assert_eq!(
            project_clip(&moved, "clip-added").timeline_start_tick,
            300_000
        );
        let remove = StructuralCommand::remove_clip(&moved, "clip-added").unwrap();
        let removed = history.execute(&moved, remove).unwrap();
        assert!(matches!(
            locate_project_clip(&removed, "clip-added"),
            Err(TimelineError::MissingProjectClip(_))
        ));

        let undo_remove = history.undo(&removed).unwrap().unwrap();
        assert_eq!(undo_remove, moved);
        let undo_move = history.undo(&undo_remove).unwrap().unwrap();
        assert_eq!(undo_move, inserted);
        assert_eq!(history.redo_len(), 2);

        let branch = StructuralCommand::trim_clip(&undo_move, "clip-added", 0, 400_000).unwrap();
        let branched = history.execute(&undo_move, branch).unwrap();
        assert_eq!(
            project_clip(&branched, "clip-added").duration_ticks,
            400_000
        );
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_len(), 2);

        assert_eq!(
            ProjectDocument::migrate(serde_json::to_value(&branched).unwrap())
                .map_err(|error: ProjectDocumentError| error.to_string())
                .unwrap(),
            branched
        );
    }

    #[test]
    fn canonical_commands_reject_locks_incompatible_tracks_overlap_and_budget_atomically() {
        let mut document = canonical_project()
            .ensure_creator_track_layout("sequence-main")
            .unwrap();
        let video_tracks = document.sequences[0]
            .tracks
            .iter()
            .filter(|track| track.kind == "video")
            .map(|track| track.id.clone())
            .collect::<Vec<_>>();
        let audio_track = document.sequences[0]
            .tracks
            .iter()
            .find(|track| track.kind == "audio")
            .unwrap()
            .id
            .clone();

        document.sequences[0]
            .tracks
            .iter_mut()
            .find(|track| track.id == video_tracks[0])
            .unwrap()
            .extra
            .insert("locked".into(), json!(true));
        let remove = StructuralCommand::remove_clip(&document, "clip-main").unwrap();
        let mut history = StructuralHistory::new(1_000_000).unwrap();
        assert!(matches!(
            history.execute(&document, remove),
            Err(TimelineError::LockedTrack(_))
        ));
        assert_eq!(history.undo_len(), 0);

        document.sequences[0]
            .tracks
            .iter_mut()
            .find(|track| track.id == video_tracks[0])
            .unwrap()
            .extra
            .insert("locked".into(), json!(false));
        assert!(matches!(
            StructuralCommand::move_clip(
                &document,
                "clip-main",
                "sequence-main",
                audio_track,
                0,
                2_000_000,
            ),
            Err(TimelineError::IncompatibleTrack { .. })
        ));

        let mut overlapping = project_clip(&document, "clip-main").clone();
        overlapping.id = "clip-overlap".into();
        overlapping.effects.clear();
        assert!(matches!(
            StructuralCommand::insert_clip(
                &document,
                "sequence-main",
                &video_tracks[0],
                1,
                500_000,
                overlapping,
            ),
            Err(TimelineError::SameTrackOverlap(_))
        ));

        let trim = StructuralCommand::trim_clip(&document, "clip-main", 0, 900_000).unwrap();
        let mut tiny_history = StructuralHistory::new(1).unwrap();
        assert!(matches!(
            tiny_history.execute(&document, trim),
            Err(TimelineError::HistoryBudgetExceeded { .. })
        ));
        assert_eq!(tiny_history.used_bytes(), 0);
    }

    #[test]
    fn structural_move_undo_preserves_newer_effect_parameters() {
        let document = canonical_project()
            .ensure_creator_track_layout("sequence-main")
            .unwrap();
        let target_track = document.sequences[0]
            .tracks
            .iter()
            .find(|track| track.kind == "video" && track.clips.is_empty())
            .unwrap()
            .id
            .clone();
        let command = StructuralCommand::move_clip(
            &document,
            "clip-main",
            "sequence-main",
            target_track,
            0,
            2_000_000,
        )
        .unwrap();
        let mut history = StructuralHistory::new(1_000_000).unwrap();
        let mut moved = history.execute(&document, command).unwrap();
        project_clip_mut(&mut moved, "clip-main").effects[0]
            .parameters = json!({"filter": "sepia"});

        let undone = history.undo(&moved).unwrap().unwrap();
        assert_eq!(
            project_clip(&undone, "clip-main").effects[0].parameters,
            json!({"filter": "sepia"})
        );
    }

    #[test]
    fn left_trim_moves_timeline_start_and_undo_restores_exact_document() {
        let document = canonical_project();
        let command =
            StructuralCommand::trim_clip_at(&document, "clip-main", 100_000, 100_000, 900_000)
                .unwrap();
        let trimmed = command.apply(&document).unwrap();
        let clip = project_clip(&trimmed, "clip-main");
        assert_eq!(clip.timeline_start_tick, 100_000);
        assert_eq!(clip.source_in_tick, 100_000);
        assert_eq!(clip.duration_ticks, 800_000);
        assert_eq!(command.undo(&trimmed).unwrap(), document);
    }
}
