use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::keyframes::KeyframeTrack;

pub const COMPOSITION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TIME_BASE: u32 = 1_000_000;
pub const MAX_TRACKS: usize = 16;
pub const MAX_CLIPS: usize = 512;
pub const MAX_SOURCES: usize = 32;
pub const MAX_VISIBLE_LAYERS: usize = 8;
pub const MAX_OUTPUT_TICKS: u64 = 24 * 60 * 60 * DEFAULT_TIME_BASE as u64;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, CompositionError> {
                let value = value.into();
                let valid = !value.trim().is_empty()
                    && value.len() <= 128
                    && value.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    });
                if !valid {
                    return Err(CompositionError::InvalidId(value));
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

stable_id!(SourceId);
stable_id!(TrackId);
stable_id!(CompositionClipId);
stable_id!(TransitionId);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasSpec {
    pub width: u32,
    pub height: u32,
    pub fps_milli: u32,
    pub background: Rgba,
}

impl Default for CanvasSpec {
    fn default() -> Self {
        Self {
            width: 1_920,
            height: 1_080,
            fps_milli: 30_000,
            background: Rgba::BLACK,
        }
    }
}

impl CanvasSpec {
    fn validate(self) -> Result<(), CompositionError> {
        if !(2..=3_840).contains(&self.width)
            || !(2..=2_160).contains(&self.height)
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            return Err(CompositionError::InvalidCanvas);
        }
        if !(1_000..=60_000).contains(&self.fps_milli) {
            return Err(CompositionError::InvalidFrameRate);
        }
        self.background.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rgba {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Rgba {
    pub const BLACK: Self = Self {
        red: 0.0,
        green: 0.0,
        blue: 0.0,
        alpha: 1.0,
    };

    fn validate(self) -> Result<(), CompositionError> {
        if [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            Ok(())
        } else {
            Err(CompositionError::InvalidColor)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionSource {
    pub id: SourceId,
    pub kind: SourceKind,
    pub duration_ticks: u64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

impl CompositionSource {
    fn validate(&self) -> Result<(), CompositionError> {
        let valid = match self.kind {
            SourceKind::Video => self.duration_ticks > 0 && self.width > 0 && self.height > 0,
            SourceKind::Audio => self.duration_ticks > 0 && self.has_audio,
            SourceKind::Image => self.width > 0 && self.height > 0 && !self.has_audio,
        };
        if !valid {
            return Err(CompositionError::InvalidSource(self.id.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipPlacement {
    pub timeline_start_tick: u64,
    pub source_in_tick: u64,
    pub source_out_tick: u64,
    pub speed: f64,
}

impl ClipPlacement {
    pub fn timeline_duration_ticks(self) -> Result<u64, CompositionError> {
        if self.source_out_tick <= self.source_in_tick
            || !self.speed.is_finite()
            || !(0.05..=16.0).contains(&self.speed)
        {
            return Err(CompositionError::InvalidPlacement);
        }
        let duration = (self.source_out_tick - self.source_in_tick) as f64 / self.speed;
        if !duration.is_finite() || duration < 1.0 || duration > u64::MAX as f64 {
            return Err(CompositionError::InvalidPlacement);
        }
        Ok(duration.round() as u64)
    }

    pub fn timeline_end_tick(self) -> Result<u64, CompositionError> {
        self.timeline_start_tick
            .checked_add(self.timeline_duration_ticks()?)
            .ok_or(CompositionError::InvalidPlacement)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimatableValue {
    Constant { value: f64 },
    Keyframes { track: KeyframeTrack<f64> },
}

impl AnimatableValue {
    pub fn constant(value: f64) -> Self {
        Self::Constant { value }
    }

    fn validate(&self) -> Result<(), CompositionError> {
        match self {
            Self::Constant { value } if value.is_finite() => Ok(()),
            Self::Constant { .. } => Err(CompositionError::InvalidAnimation),
            Self::Keyframes { track } => KeyframeTrack::new(
                track.time_base,
                track.interpolation,
                track.keyframes.clone(),
            )
            .map(|_| ())
            .map_err(|_| CompositionError::InvalidAnimation),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransformSpec {
    pub x: AnimatableValue,
    pub y: AnimatableValue,
    pub scale_x: AnimatableValue,
    pub scale_y: AnimatableValue,
    pub rotation_degrees: AnimatableValue,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

impl Default for TransformSpec {
    fn default() -> Self {
        Self {
            x: AnimatableValue::constant(0.0),
            y: AnimatableValue::constant(0.0),
            scale_x: AnimatableValue::constant(1.0),
            scale_y: AnimatableValue::constant(1.0),
            rotation_degrees: AnimatableValue::constant(0.0),
            anchor_x: 0.5,
            anchor_y: 0.5,
        }
    }
}

impl TransformSpec {
    fn validate(&self) -> Result<(), CompositionError> {
        self.x.validate()?;
        self.y.validate()?;
        self.scale_x.validate()?;
        self.scale_y.validate()?;
        self.rotation_degrees.validate()?;
        if self.anchor_x.is_finite()
            && self.anchor_y.is_finite()
            && (0.0..=1.0).contains(&self.anchor_x)
            && (0.0..=1.0).contains(&self.anchor_y)
        {
            Ok(())
        } else {
            Err(CompositionError::InvalidTransform)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Addition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskShape {
    Rectangle,
    Ellipse,
    Linear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoEffect {
    ChromaKey {
        color: Rgba,
        similarity: f64,
        softness: f64,
        spill: f64,
    },
    Mask {
        shape: MaskShape,
        x: AnimatableValue,
        y: AnimatableValue,
        width: AnimatableValue,
        height: AnimatableValue,
        feather: f64,
        inverted: bool,
    },
}

impl VideoEffect {
    fn validate(&self) -> Result<(), CompositionError> {
        match self {
            Self::ChromaKey {
                color,
                similarity,
                softness,
                spill,
            } => {
                color.validate()?;
                if [similarity, softness, spill]
                    .into_iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                {
                    Ok(())
                } else {
                    Err(CompositionError::InvalidEffect)
                }
            }
            Self::Mask {
                x,
                y,
                width,
                height,
                feather,
                ..
            } => {
                x.validate()?;
                y.validate()?;
                width.validate()?;
                height.validate()?;
                if feather.is_finite() && (0.0..=1.0).contains(feather) {
                    Ok(())
                } else {
                    Err(CompositionError::InvalidEffect)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoClip {
    pub id: CompositionClipId,
    pub source_id: SourceId,
    pub placement: ClipPlacement,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    pub blend_mode: BlendMode,
    #[serde(default)]
    pub effects: Vec<VideoEffect>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioClip {
    pub id: CompositionClipId,
    pub source_id: SourceId,
    pub placement: ClipPlacement,
    pub gain: AnimatableValue,
    pub pan: AnimatableValue,
    pub fade_in_ticks: u64,
    pub fade_out_ticks: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageClip {
    pub id: CompositionClipId,
    pub source_id: SourceId,
    pub timeline_start_tick: u64,
    pub duration_ticks: u64,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    pub blend_mode: BlendMode,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    pub font_family: String,
    pub font_size: f64,
    pub color: Rgba,
    pub background: Rgba,
    pub stroke: Rgba,
    pub stroke_width: f64,
    pub shadow: Rgba,
    pub shadow_x: f64,
    pub shadow_y: f64,
}

impl TextStyle {
    fn validate(&self) -> Result<(), CompositionError> {
        if self.font_family.trim().is_empty()
            || self.font_family.len() > 256
            || !self.font_size.is_finite()
            || !(1.0..=1_000.0).contains(&self.font_size)
            || !self.stroke_width.is_finite()
            || !(0.0..=100.0).contains(&self.stroke_width)
            || !self.shadow_x.is_finite()
            || !self.shadow_y.is_finite()
        {
            return Err(CompositionError::InvalidText);
        }
        self.color.validate()?;
        self.background.validate()?;
        self.stroke.validate()?;
        self.shadow.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextClip {
    pub id: CompositionClipId,
    pub timeline_start_tick: u64,
    pub timeline_end_tick: u64,
    pub text: String,
    pub style: TextStyle,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    Dissolve,
    FadeBlack,
    WipeLeft,
    WipeRight,
    SlideLeft,
    SlideRight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipTransition {
    pub id: TransitionId,
    pub from_clip_id: CompositionClipId,
    pub to_clip_id: CompositionClipId,
    pub duration_ticks: u64,
    pub kind: TransitionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompositionTrack {
    Video {
        id: TrackId,
        name: String,
        #[serde(default)]
        hidden: bool,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        clips: Vec<VideoClip>,
        #[serde(default)]
        transitions: Vec<ClipTransition>,
    },
    Audio {
        id: TrackId,
        name: String,
        #[serde(default)]
        muted: bool,
        #[serde(default)]
        solo: bool,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        clips: Vec<AudioClip>,
    },
    Image {
        id: TrackId,
        name: String,
        #[serde(default)]
        hidden: bool,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        clips: Vec<ImageClip>,
    },
    Text {
        id: TrackId,
        name: String,
        #[serde(default)]
        hidden: bool,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        clips: Vec<TextClip>,
    },
}

impl CompositionTrack {
    pub fn id(&self) -> &TrackId {
        match self {
            Self::Video { id, .. }
            | Self::Audio { id, .. }
            | Self::Image { id, .. }
            | Self::Text { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Composition {
    pub schema_version: u32,
    pub time_base: u32,
    pub canvas: CanvasSpec,
    pub sources: BTreeMap<SourceId, CompositionSource>,
    pub tracks: Vec<CompositionTrack>,
}

impl Composition {
    pub fn new(canvas: CanvasSpec) -> Self {
        Self {
            schema_version: COMPOSITION_SCHEMA_VERSION,
            time_base: DEFAULT_TIME_BASE,
            canvas,
            sources: BTreeMap::new(),
            tracks: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.schema_version != COMPOSITION_SCHEMA_VERSION {
            return Err(CompositionError::UnsupportedSchema(self.schema_version));
        }
        // Composition v1 uses microsecond ticks across browser preview,
        // persistence, cache identity, and FFmpeg compilation. Accepting an
        // arbitrary client time base here would make those layers disagree.
        if self.time_base != DEFAULT_TIME_BASE {
            return Err(CompositionError::InvalidTimeBase);
        }
        self.canvas.validate()?;
        if self.tracks.len() > MAX_TRACKS {
            return Err(CompositionError::TooManyTracks);
        }
        if self.sources.len() > MAX_SOURCES {
            return Err(CompositionError::TooManySources);
        }

        for (id, source) in &self.sources {
            if id != &source.id {
                return Err(CompositionError::SourceKeyMismatch(id.clone()));
            }
            source.validate()?;
        }

        let mut track_ids = BTreeSet::new();
        let mut clip_ids = BTreeSet::new();
        let mut clip_count = 0_usize;
        let mut output_end = 0_u64;
        let mut visible_layers = 0_usize;
        for track in &self.tracks {
            if !track_ids.insert(track.id()) {
                return Err(CompositionError::DuplicateTrack(track.id().clone()));
            }
            match track {
                CompositionTrack::Video {
                    clips,
                    transitions,
                    hidden,
                    ..
                } => {
                    if !hidden {
                        visible_layers = visible_layers.saturating_add(1);
                    }
                    clip_count = clip_count.saturating_add(clips.len());
                    let mut local_ids = BTreeSet::new();
                    for clip in clips {
                        validate_media_clip(
                            &self.sources,
                            &clip.id,
                            &clip.source_id,
                            clip.placement,
                            true,
                        )?;
                        clip.transform.validate()?;
                        clip.opacity.validate()?;
                        for effect in &clip.effects {
                            effect.validate()?;
                        }
                        if !local_ids.insert(&clip.id) || !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.placement.timeline_end_tick()?);
                    }
                    validate_transitions(transitions, &local_ids)?;
                }
                CompositionTrack::Audio { clips, .. } => {
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        validate_media_clip(
                            &self.sources,
                            &clip.id,
                            &clip.source_id,
                            clip.placement,
                            false,
                        )?;
                        clip.gain.validate()?;
                        clip.pan.validate()?;
                        let duration = clip.placement.timeline_duration_ticks()?;
                        if clip.fade_in_ticks > duration || clip.fade_out_ticks > duration {
                            return Err(CompositionError::InvalidAudioFade);
                        }
                        if !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.placement.timeline_end_tick()?);
                    }
                }
                CompositionTrack::Image { clips, hidden, .. } => {
                    if !hidden {
                        visible_layers = visible_layers.saturating_add(1);
                    }
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        let source = self.sources.get(&clip.source_id).ok_or_else(|| {
                            CompositionError::MissingSource(clip.source_id.clone())
                        })?;
                        if source.kind != SourceKind::Image || clip.duration_ticks == 0 {
                            return Err(CompositionError::InvalidClipSource(clip.id.clone()));
                        }
                        clip.transform.validate()?;
                        clip.opacity.validate()?;
                        if !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        let end = clip
                            .timeline_start_tick
                            .checked_add(clip.duration_ticks)
                            .ok_or(CompositionError::InvalidPlacement)?;
                        output_end = output_end.max(end);
                    }
                }
                CompositionTrack::Text { clips, hidden, .. } => {
                    if !hidden {
                        visible_layers = visible_layers.saturating_add(1);
                    }
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        if clip.timeline_end_tick <= clip.timeline_start_tick
                            || clip.text.trim().is_empty()
                            || clip.text.chars().count() > 512
                        {
                            return Err(CompositionError::InvalidText);
                        }
                        clip.style.validate()?;
                        clip.transform.validate()?;
                        clip.opacity.validate()?;
                        if !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.timeline_end_tick);
                    }
                }
            }
        }
        if clip_count > MAX_CLIPS {
            return Err(CompositionError::TooManyClips);
        }
        if visible_layers > MAX_VISIBLE_LAYERS {
            return Err(CompositionError::TooManyVisibleLayers);
        }
        let max_ticks = 24_u64
            .checked_mul(60)
            .and_then(|value| value.checked_mul(60))
            .and_then(|value| value.checked_mul(self.time_base as u64))
            .ok_or(CompositionError::OutputTooLong)?;
        if output_end > max_ticks {
            return Err(CompositionError::OutputTooLong);
        }
        Ok(())
    }

    pub fn duration_ticks(&self) -> Result<u64, CompositionError> {
        self.validate()?;
        let mut end = 0;
        for track in &self.tracks {
            match track {
                CompositionTrack::Video { clips, .. } => {
                    for clip in clips {
                        end = end.max(clip.placement.timeline_end_tick()?);
                    }
                }
                CompositionTrack::Audio { clips, .. } => {
                    for clip in clips {
                        end = end.max(clip.placement.timeline_end_tick()?);
                    }
                }
                CompositionTrack::Image { clips, .. } => {
                    for clip in clips {
                        end = end.max(
                            clip.timeline_start_tick
                                .checked_add(clip.duration_ticks)
                                .ok_or(CompositionError::InvalidPlacement)?,
                        );
                    }
                }
                CompositionTrack::Text { clips, .. } => {
                    for clip in clips {
                        end = end.max(clip.timeline_end_tick);
                    }
                }
            }
        }
        Ok(end)
    }
}

fn validate_media_clip(
    sources: &BTreeMap<SourceId, CompositionSource>,
    clip_id: &CompositionClipId,
    source_id: &SourceId,
    placement: ClipPlacement,
    needs_video: bool,
) -> Result<(), CompositionError> {
    let source = sources
        .get(source_id)
        .ok_or_else(|| CompositionError::MissingSource(source_id.clone()))?;
    if placement.source_out_tick > source.duration_ticks
        || (needs_video && source.kind != SourceKind::Video)
        || (!needs_video && !source.has_audio)
    {
        return Err(CompositionError::InvalidClipSource(clip_id.clone()));
    }
    placement.timeline_end_tick()?;
    Ok(())
}

fn validate_transitions(
    transitions: &[ClipTransition],
    clip_ids: &BTreeSet<&CompositionClipId>,
) -> Result<(), CompositionError> {
    let mut ids = BTreeSet::new();
    for transition in transitions {
        if transition.duration_ticks == 0
            || transition.from_clip_id == transition.to_clip_id
            || !clip_ids.contains(&transition.from_clip_id)
            || !clip_ids.contains(&transition.to_clip_id)
        {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
        if !ids.insert(&transition.id) {
            return Err(CompositionError::DuplicateTransition(transition.id.clone()));
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    UnsupportedSchema(u32),
    InvalidId(String),
    InvalidTimeBase,
    InvalidCanvas,
    InvalidFrameRate,
    InvalidColor,
    InvalidSource(SourceId),
    SourceKeyMismatch(SourceId),
    MissingSource(SourceId),
    InvalidPlacement,
    InvalidAnimation,
    InvalidTransform,
    InvalidEffect,
    InvalidText,
    InvalidAudioFade,
    InvalidClipSource(CompositionClipId),
    InvalidTransition(TransitionId),
    DuplicateTransition(TransitionId),
    DuplicateTrack(TrackId),
    DuplicateClip(CompositionClipId),
    TooManyTracks,
    TooManyClips,
    TooManySources,
    TooManyVisibleLayers,
    OutputTooLong,
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid composition: {self:?}")
    }
}

impl std::error::Error for CompositionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str) -> CompositionSource {
        CompositionSource {
            id: SourceId::parse(id).unwrap(),
            kind: SourceKind::Video,
            duration_ticks: 10_000_000,
            width: 1_920,
            height: 1_080,
            has_audio: true,
        }
    }

    fn placement(start: u64) -> ClipPlacement {
        ClipPlacement {
            timeline_start_tick: start,
            source_in_tick: 0,
            source_out_tick: 2_000_000,
            speed: 1.0,
        }
    }

    fn video_clip(id: &str, source_id: &str, start: u64) -> VideoClip {
        VideoClip {
            id: CompositionClipId::parse(id).unwrap(),
            source_id: SourceId::parse(source_id).unwrap(),
            placement: placement(start),
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
            effects: Vec::new(),
            enabled: true,
        }
    }

    #[test]
    fn multi_source_tracks_round_trip_with_stable_ids() {
        let first = source("source-a");
        let second = source("source-b");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(first.id.clone(), first);
        composition.sources.insert(second.id.clone(), second);
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            locked: false,
            clips: vec![
                video_clip("clip-a", "source-a", 0),
                video_clip("clip-b", "source-b", 2_000_000),
            ],
            transitions: vec![ClipTransition {
                id: TransitionId::parse("transition-a-b").unwrap(),
                from_clip_id: CompositionClipId::parse("clip-a").unwrap(),
                to_clip_id: CompositionClipId::parse("clip-b").unwrap(),
                duration_ticks: 250_000,
                kind: TransitionKind::Dissolve,
            }],
        }];

        composition.validate().unwrap();
        assert_eq!(composition.duration_ticks().unwrap(), 4_000_000);
        let encoded = serde_json::to_string(&composition).unwrap();
        let decoded: Composition = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, composition);
    }

    #[test]
    fn rejects_missing_or_wrong_media_sources() {
        let mut first = source("source-a");
        first.has_audio = false;
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(first.id.clone(), first);
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            locked: false,
            clips: vec![video_clip("clip-a", "missing", 0)],
            transitions: Vec::new(),
        }];
        assert_eq!(
            composition.validate(),
            Err(CompositionError::MissingSource(
                SourceId::parse("missing").unwrap()
            ))
        );

        composition.tracks = vec![CompositionTrack::Audio {
            id: TrackId::parse("audio-main").unwrap(),
            name: "Audio".into(),
            muted: false,
            solo: false,
            locked: false,
            clips: vec![AudioClip {
                id: CompositionClipId::parse("clip-audio").unwrap(),
                source_id: SourceId::parse("source-a").unwrap(),
                placement: ClipPlacement {
                    source_out_tick: 99_000,
                    ..placement(0)
                },
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                enabled: true,
            }],
        }];
        assert_eq!(
            composition.validate(),
            Err(CompositionError::InvalidClipSource(
                CompositionClipId::parse("clip-audio").unwrap()
            ))
        );
    }

    #[test]
    fn validates_keyframes_effects_and_global_clip_identity() {
        let first = source("source-a");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(first.id.clone(), first);
        let duplicate = video_clip("same-clip", "source-a", 0);
        composition.tracks = vec![
            CompositionTrack::Video {
                id: TrackId::parse("video-a").unwrap(),
                name: "A".into(),
                hidden: false,
                locked: false,
                clips: vec![duplicate.clone()],
                transitions: Vec::new(),
            },
            CompositionTrack::Video {
                id: TrackId::parse("video-b").unwrap(),
                name: "B".into(),
                hidden: false,
                locked: false,
                clips: vec![duplicate],
                transitions: Vec::new(),
            },
        ];
        assert_eq!(
            composition.validate(),
            Err(CompositionError::DuplicateClip(
                CompositionClipId::parse("same-clip").unwrap()
            ))
        );
    }

    #[test]
    fn rejects_more_than_twenty_four_hours_after_speed() {
        let mut long = source("source-long");
        long.duration_ticks = 100_000_000_000;
        let mut clip = video_clip("clip-long", "source-long", 0);
        clip.placement.source_out_tick = 90_000_000_000;
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(long.id.clone(), long);
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];

        assert_eq!(composition.validate(), Err(CompositionError::OutputTooLong));
    }
}
