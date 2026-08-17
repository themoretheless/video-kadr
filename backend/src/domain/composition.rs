//! Multi-source timeline: clips, their in/out points, and the transitions
//! between them. The FFmpeg adapter turns this into an `xfade`/`acrossfade`
//! concat; the domain only guarantees bounded, ordered, playable clips.

use serde::{Deserialize, Serialize};

use super::edit::{asset_reference, clamped, EditSpecError, MAX_CLIPS};
use crate::model;

/// Minimum and maximum clip playback rate.
const MIN_CLIP_SPEED: f64 = 0.25;
const MAX_CLIP_SPEED: f64 = 4.0;
const MAX_CLIP_VOLUME: f64 = 4.0;
/// A transition consumes this many seconds of overlap from both neighbours.
pub const MIN_TRANSITION_SECONDS: f64 = 0.05;
pub const MAX_TRANSITION_SECONDS: f64 = 3.0;

/// The transition catalogue. Every entry maps 1:1 onto an FFmpeg `xfade`
/// transition name, so the emitted filter never contains a client string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Fade,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    CircleOpen,
    CircleClose,
    Dissolve,
    Pixelize,
    Radial,
    SmoothLeft,
    SmoothRight,
    ZoomIn,
}

impl TransitionKind {
    pub const ALL: [Self; 17] = [
        Self::Fade,
        Self::WipeLeft,
        Self::WipeRight,
        Self::WipeUp,
        Self::WipeDown,
        Self::SlideLeft,
        Self::SlideRight,
        Self::SlideUp,
        Self::SlideDown,
        Self::CircleOpen,
        Self::CircleClose,
        Self::Dissolve,
        Self::Pixelize,
        Self::Radial,
        Self::SmoothLeft,
        Self::SmoothRight,
        Self::ZoomIn,
    ];

    /// Wire id and FFmpeg `xfade=transition=` value are intentionally equal.
    pub const fn wire_id(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::WipeLeft => "wipeleft",
            Self::WipeRight => "wiperight",
            Self::WipeUp => "wipeup",
            Self::WipeDown => "wipedown",
            Self::SlideLeft => "slideleft",
            Self::SlideRight => "slideright",
            Self::SlideUp => "slideup",
            Self::SlideDown => "slidedown",
            Self::CircleOpen => "circleopen",
            Self::CircleClose => "circleclose",
            Self::Dissolve => "dissolve",
            Self::Pixelize => "pixelize",
            Self::Radial => "radial",
            Self::SmoothLeft => "smoothleft",
            Self::SmoothRight => "smoothright",
            Self::ZoomIn => "zoomin",
        }
    }

    pub fn ffmpeg_name(self) -> &'static str {
        self.wire_id()
    }

    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.wire_id() == value)
            .ok_or(EditSpecError::InvalidTransition)
    }
}

impl Serialize for TransitionKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.wire_id())
    }
}

impl<'de> Deserialize<'de> for TransitionKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = String::deserialize(deserializer)?;
        Self::parse(&id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransitionSpec {
    pub(crate) kind: TransitionKind,
    pub(crate) duration_seconds: f64,
}

impl TransitionSpec {
    pub fn from_wire(value: &model::Transition) -> Result<Self, EditSpecError> {
        let spec = Self {
            kind: TransitionKind::parse(&value.kind)?,
            duration_seconds: clamped(
                value.duration,
                MIN_TRANSITION_SECONDS,
                MAX_TRANSITION_SECONDS,
                EditSpecError::InvalidTransition,
            )?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn kind(self) -> TransitionKind {
        self.kind
    }

    pub fn duration_seconds(self) -> f64 {
        self.duration_seconds
    }

    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if !self.duration_seconds.is_finite()
            || !(MIN_TRANSITION_SECONDS..=MAX_TRANSITION_SECONDS).contains(&self.duration_seconds)
        {
            return Err(EditSpecError::InvalidTransition);
        }
        Ok(())
    }
}

/// One clip on the timeline. `start`/`end` are in-points inside the source, so
/// they are independent of where the clip lands on the output timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipSpec {
    pub(crate) source_id: String,
    pub(crate) start_seconds: f64,
    pub(crate) end_seconds: f64,
    pub(crate) speed: f64,
    pub(crate) volume: f64,
    pub(crate) muted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) transition_in: Option<TransitionSpec>,
}

impl ClipSpec {
    pub fn from_wire(value: &model::Clip) -> Result<Self, EditSpecError> {
        let spec = Self {
            source_id: asset_reference(&value.source_id)?,
            start_seconds: clamped(value.start, 0.0, f64::MAX, EditSpecError::InvalidClip)?,
            end_seconds: clamped(value.end, 0.0, f64::MAX, EditSpecError::InvalidClip)?,
            speed: clamped(
                value.speed,
                MIN_CLIP_SPEED,
                MAX_CLIP_SPEED,
                EditSpecError::InvalidClip,
            )?,
            volume: clamped(
                value.volume,
                0.0,
                MAX_CLIP_VOLUME,
                EditSpecError::InvalidClip,
            )?,
            muted: value.muted,
            transition_in: value
                .transition_in
                .as_ref()
                .map(TransitionSpec::from_wire)
                .transpose()?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    pub fn end_seconds(&self) -> f64 {
        self.end_seconds
    }

    /// Length inside the source, before `speed` is applied.
    pub fn source_duration_seconds(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    /// Length this clip occupies on the output timeline.
    pub fn output_duration_seconds(&self) -> f64 {
        self.source_duration_seconds() / self.speed
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn volume(&self) -> f64 {
        self.volume
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn transition_in(&self) -> Option<TransitionSpec> {
        self.transition_in
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.source_id.is_empty()
            || !self.start_seconds.is_finite()
            || !self.end_seconds.is_finite()
            || self.start_seconds < 0.0
            || self.source_duration_seconds() <= 0.01
            || !(MIN_CLIP_SPEED..=MAX_CLIP_SPEED).contains(&self.speed)
            || !(0.0..=MAX_CLIP_VOLUME).contains(&self.volume)
        {
            return Err(EditSpecError::InvalidClip);
        }
        if let Some(transition) = self.transition_in {
            transition.validate()?;
            // A transition cannot consume more than the clip it opens.
            if transition.duration_seconds() >= self.output_duration_seconds() {
                return Err(EditSpecError::InvalidTransition);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionSpec {
    pub(crate) clips: Vec<ClipSpec>,
}

impl CompositionSpec {
    /// An empty clip list means the legacy `trim`/`segments` timeline is still
    /// in charge, so it maps to `None` rather than an empty composition.
    pub fn from_wire(clips: &[model::Clip]) -> Result<Option<Self>, EditSpecError> {
        if clips.is_empty() {
            return Ok(None);
        }
        if clips.len() > MAX_CLIPS {
            return Err(EditSpecError::InvalidClip);
        }
        let spec = Self {
            clips: clips
                .iter()
                .map(ClipSpec::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
        };
        spec.validate()?;
        Ok(Some(spec))
    }

    pub fn clips(&self) -> &[ClipSpec] {
        &self.clips
    }

    /// Expected output length, accounting for the overlap each transition eats.
    pub fn output_duration_seconds(&self) -> f64 {
        let played: f64 = self
            .clips
            .iter()
            .map(ClipSpec::output_duration_seconds)
            .sum();
        let overlap: f64 = self
            .clips
            .iter()
            .filter_map(|clip| clip.transition_in.map(TransitionSpec::duration_seconds))
            .sum();
        (played - overlap).max(0.0)
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.clips.is_empty() || self.clips.len() > MAX_CLIPS {
            return Err(EditSpecError::InvalidClip);
        }
        for clip in &self.clips {
            clip.validate()?;
        }
        // The first clip has no predecessor to cross-fade with.
        if self.clips[0].transition_in.is_some() {
            return Err(EditSpecError::InvalidTransition);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(start: f64, end: f64) -> model::Clip {
        model::Clip {
            source_id: "vid_1".into(),
            start,
            end,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            transition_in: None,
        }
    }

    #[test]
    fn transition_ids_map_one_to_one_onto_xfade_names() {
        assert_eq!(TransitionKind::ALL.len(), 17);
        for kind in TransitionKind::ALL {
            assert_eq!(TransitionKind::parse(kind.wire_id()), Ok(kind));
            assert_eq!(kind.ffmpeg_name(), kind.wire_id());
        }
        assert_eq!(
            TransitionKind::parse("fade,drawtext=text=x"),
            Err(EditSpecError::InvalidTransition)
        );
    }

    #[test]
    fn transition_duration_is_clamped_into_the_documented_range() {
        let long = TransitionSpec::from_wire(&model::Transition {
            kind: "fade".into(),
            duration: 99.0,
        })
        .unwrap();
        assert_eq!(long.duration_seconds(), MAX_TRANSITION_SECONDS);
        assert!(TransitionSpec::from_wire(&model::Transition {
            kind: "fade".into(),
            duration: f64::NAN,
        })
        .is_err());
    }

    #[test]
    fn composition_rejects_a_transition_on_the_first_clip() {
        let mut first = clip(0.0, 5.0);
        first.transition_in = Some(model::Transition {
            kind: "fade".into(),
            duration: 0.5,
        });
        assert_eq!(
            CompositionSpec::from_wire(&[first, clip(0.0, 5.0)]),
            Err(EditSpecError::InvalidTransition)
        );
    }

    #[test]
    fn output_duration_subtracts_the_transition_overlap() {
        let mut second = clip(0.0, 4.0);
        second.transition_in = Some(model::Transition {
            kind: "fade".into(),
            duration: 0.5,
        });
        let composition = CompositionSpec::from_wire(&[clip(0.0, 6.0), second])
            .unwrap()
            .unwrap();
        assert_eq!(composition.output_duration_seconds(), 9.5);
    }

    #[test]
    fn clips_are_bounded_and_source_ids_cannot_be_paths() {
        assert!(CompositionSpec::from_wire(&[]).unwrap().is_none());
        assert_eq!(
            CompositionSpec::from_wire(&[clip(5.0, 5.0)]),
            Err(EditSpecError::InvalidClip)
        );
        let mut traversal = clip(0.0, 1.0);
        traversal.source_id = "../../etc/passwd".into();
        assert_eq!(
            CompositionSpec::from_wire(&[traversal]),
            Err(EditSpecError::InvalidAssetReference)
        );
        let too_many = vec![clip(0.0, 1.0); MAX_CLIPS + 1];
        assert_eq!(
            CompositionSpec::from_wire(&too_many),
            Err(EditSpecError::InvalidClip)
        );
        let mut fast = clip(0.0, 1.0);
        fast.speed = 99.0;
        assert_eq!(
            CompositionSpec::from_wire(&[fast]).unwrap().unwrap().clips[0].speed(),
            MAX_CLIP_SPEED
        );
    }
}
