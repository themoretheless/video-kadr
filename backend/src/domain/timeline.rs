use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid timeline: {self:?}")
    }
}

impl std::error::Error for TimelineError {}

#[cfg(test)]
mod tests {
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
}
