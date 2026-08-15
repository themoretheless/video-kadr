use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::Error as _;
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
pub const MAX_KEYFRAMES_PER_VALUE: usize = 32;
pub const MAX_SPEED_RAMP_POINTS: usize = 32;
pub(crate) const MAX_ACTIVE_SPEED_RAMP_SEGMENTS: usize = 256;
pub(crate) const MAX_SPEED_RAMP_CLIP_TICKS: u64 = 2 * 60 * 60 * DEFAULT_TIME_BASE as u64;
pub(crate) const MIN_SPEED_RAMP_SEGMENT_TICKS: u64 = DEFAULT_TIME_BASE as u64 / 1_000;
// One active composition request may spend at most one minute of 1080p60
// equivalent work on motion interpolation. Portrait Full HD is accepted too.
pub(crate) const MAX_OPTICAL_FLOW_EDGE: u32 = 2_160;
pub(crate) const MAX_OPTICAL_FLOW_PIXELS: u128 = 1_920 * 1_080;
pub(crate) const MAX_OPTICAL_FLOW_CLIP_TICKS: u64 = 120 * DEFAULT_TIME_BASE as u64;
pub(crate) const MAX_OPTICAL_FLOW_PIXEL_FRAMES: u128 = 1_920 * 1_080 * 60 * 60;
// `reverse` buffers its complete input segment. Reverse video is normalized to
// the canvas frame rate before buffering, making this an enforceable upper
// bound instead of relying on an untrusted source frame rate.
pub(crate) const MAX_REVERSE_EDGE: u32 = 2_160;
pub(crate) const MAX_REVERSE_PIXELS: u128 = 1_920 * 1_080;
pub(crate) const MAX_REVERSE_CLIP_TICKS: u64 = 30 * DEFAULT_TIME_BASE as u64;
pub(crate) const MAX_REVERSE_PIXEL_FRAMES: u128 = 1_920 * 1_080 * 30 * 30;
// Stabilization is normalized to the canvas fps before `deshake`, so this
// streaming work budget is deterministic even for untrusted source frame rates.
pub(crate) const MAX_STABILIZATION_EDGE: u32 = 2_160;
pub(crate) const MAX_STABILIZATION_PIXELS: u128 = 1_920 * 1_080;
pub(crate) const MAX_STABILIZATION_CLIP_TICKS: u64 = 15 * DEFAULT_TIME_BASE as u64;
pub(crate) const MAX_STABILIZATION_PIXEL_FRAMES: u128 = 1_920 * 1_080 * 30 * 10;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Self::parse(String::deserialize(deserializer)?).map_err(D::Error::custom)
            }
        }

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipPlacement {
    pub timeline_start_tick: u64,
    pub source_in_tick: u64,
    pub source_out_tick: u64,
    pub speed: f64,
    /// Optional source-progress speed curve. The first point is anchored to
    /// `speed`, so old constant-speed documents and authoring controls retain
    /// one unambiguous baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_ramp: Option<SpeedRampSpec>,
}

impl ClipPlacement {
    pub fn timeline_duration_ticks(&self) -> Result<u64, CompositionError> {
        if self.source_out_tick <= self.source_in_tick
            || !self.speed.is_finite()
            || !(0.05..=16.0).contains(&self.speed)
        {
            return Err(CompositionError::InvalidPlacement);
        }
        if let Some(segments) = self.speed_ramp_segments()? {
            return segments
                .last()
                .map(|segment| segment.timeline_end_tick)
                .ok_or(CompositionError::InvalidSpeedRamp);
        }
        let duration = self.source_span_ticks() as f64 / self.speed;
        if !duration.is_finite() || duration < 1.0 || duration > u64::MAX as f64 {
            return Err(CompositionError::InvalidPlacement);
        }
        Ok(duration.round() as u64)
    }

    pub fn timeline_end_tick(&self) -> Result<u64, CompositionError> {
        self.timeline_start_tick
            .checked_add(self.timeline_duration_ticks()?)
            .ok_or(CompositionError::InvalidPlacement)
    }

    pub fn source_span_ticks(&self) -> u64 {
        self.source_out_tick.saturating_sub(self.source_in_tick)
    }

    pub fn minimum_speed(&self) -> Result<f64, CompositionError> {
        self.speed_ramp
            .as_ref()
            .map(|ramp| {
                ramp.points
                    .iter()
                    .map(|point| point.speed)
                    .reduce(f64::min)
                    .ok_or(CompositionError::InvalidSpeedRamp)
            })
            .transpose()
            .map(|minimum| minimum.unwrap_or(self.speed))
    }

    pub fn speed_ramp_segments(&self) -> Result<Option<Vec<SpeedRampSegment>>, CompositionError> {
        let Some(ramp) = &self.speed_ramp else {
            return Ok(None);
        };
        ramp.segments(self.source_span_ticks(), self.speed)
            .map(Some)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedRampInterpolation {
    /// A point's speed is constant until the next point. The final point
    /// closes the source interval and does not create a zero-length segment.
    Hold,
    /// Speed changes linearly over source progress between adjacent points;
    /// output duration is the exact integral of reciprocal speed.
    Linear,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedRampAudioPolicy {
    /// Segment the selected source audio and use bounded `atempo` filters so
    /// pitch stays stable and every ramp-point boundary remains deterministic.
    #[default]
    PreservePitch,
    /// Emit timeline silence for the ramped clip. An otherwise-unused audio
    /// source does not need to be resolved for export.
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeedRampPoint {
    /// Progress from the first presented source frame. On reverse clips this
    /// advances from `sourceOutTick` towards `sourceInTick`.
    pub source_progress_tick: u64,
    pub speed: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeedRampSpec {
    pub interpolation: SpeedRampInterpolation,
    /// Source-progress knots. The first must be `(0, placement.speed)` and the
    /// final tick must equal the selected source span.
    pub points: Vec<SpeedRampPoint>,
    #[serde(default)]
    pub audio_policy: SpeedRampAudioPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedRampSegment {
    pub source_start_tick: u64,
    pub source_end_tick: u64,
    pub timeline_start_tick: u64,
    pub timeline_end_tick: u64,
    pub start_speed: f64,
    pub end_speed: f64,
    pub interpolation: SpeedRampInterpolation,
}

impl SpeedRampSpec {
    fn segments(
        &self,
        source_span_ticks: u64,
        baseline_speed: f64,
    ) -> Result<Vec<SpeedRampSegment>, CompositionError> {
        if !(2..=MAX_SPEED_RAMP_POINTS).contains(&self.points.len())
            || source_span_ticks > MAX_SPEED_RAMP_CLIP_TICKS
            || self.points.first().is_none_or(|point| {
                point.source_progress_tick != 0 || point.speed != baseline_speed
            })
            || self
                .points
                .last()
                .is_none_or(|point| point.source_progress_tick != source_span_ticks)
            || self
                .points
                .iter()
                .any(|point| !point.speed.is_finite() || !(0.05..=16.0).contains(&point.speed))
        {
            return Err(CompositionError::InvalidSpeedRamp);
        }

        let mut cumulative_output = 0.0_f64;
        let mut previous_output_tick = 0_u64;
        let mut segments = Vec::with_capacity(self.points.len() - 1);
        for pair in self.points.windows(2) {
            let start = pair[0];
            let end = pair[1];
            let source_ticks = end
                .source_progress_tick
                .checked_sub(start.source_progress_tick)
                .ok_or(CompositionError::InvalidSpeedRamp)?;
            if source_ticks < MIN_SPEED_RAMP_SEGMENT_TICKS {
                return Err(CompositionError::InvalidSpeedRamp);
            }
            let output_ticks = match self.interpolation {
                SpeedRampInterpolation::Hold => source_ticks as f64 / start.speed,
                SpeedRampInterpolation::Linear => {
                    let speed_delta = end.speed - start.speed;
                    if speed_delta.abs() <= f64::EPSILON * start.speed.max(end.speed) {
                        source_ticks as f64 / start.speed
                    } else {
                        source_ticks as f64 * (end.speed / start.speed).ln() / speed_delta
                    }
                }
            };
            cumulative_output += output_ticks;
            if !cumulative_output.is_finite()
                || cumulative_output > MAX_SPEED_RAMP_CLIP_TICKS as f64
            {
                return Err(CompositionError::InvalidSpeedRamp);
            }
            let timeline_end_tick = cumulative_output.round() as u64;
            if timeline_end_tick.saturating_sub(previous_output_tick) < MIN_SPEED_RAMP_SEGMENT_TICKS
            {
                return Err(CompositionError::InvalidSpeedRamp);
            }
            segments.push(SpeedRampSegment {
                source_start_tick: start.source_progress_tick,
                source_end_tick: end.source_progress_tick,
                timeline_start_tick: previous_output_tick,
                timeline_end_tick,
                start_speed: start.speed,
                end_speed: end.speed,
                interpolation: self.interpolation,
            });
            previous_output_tick = timeline_end_tick;
        }
        Ok(segments)
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
            .and_then(|validated| {
                if validated.keyframes.len() <= MAX_KEYFRAMES_PER_VALUE {
                    Ok(())
                } else {
                    Err(super::keyframes::KeyframeError::TooManyKeyframes)
                }
            })
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameInterpolation {
    /// Preserve the established composition path: the canvas `fps` filter
    /// duplicates or drops decoded source frames as needed.
    #[default]
    Duplicate,
    /// Deterministic FFmpeg motion-compensated interpolation for slow motion.
    /// It is valid only on enabled clips in visible video tracks with speed
    /// below one; inactive/normal-speed uses fail closed during validation.
    OpticalFlow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaybackMode {
    /// Decode the selected source interval in its natural order.
    #[default]
    Forward,
    /// Decode the selected source interval from its end to its beginning.
    /// Video and embedded audio are reversed before speed and automation.
    Reverse,
    /// Hold the first decoded frame whose presentation timestamp is at or
    /// after `sourceTick`. The tick is in the composition time base and must
    /// be within `[sourceInTick, sourceOutTick)`. Placement still defines the
    /// exact timeline duration and embedded source audio is always silent.
    Freeze {
        #[serde(rename = "sourceTick")]
        source_tick: u64,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum StabilizationSpec {
    /// Preserve the decoded source frames without temporal stabilization.
    #[default]
    Disabled,
    /// Classical block-matching stabilization. `radiusX` and `radiusY` are
    /// maximum per-frame translation search radii in pixels of the decoded,
    /// pre-transform source raster. FFmpeg deshake requires multiples of 16;
    /// the bounded contract accepts 16, 32, 48, or 64.
    Deshake {
        #[serde(rename = "radiusX")]
        radius_x: u32,
        #[serde(rename = "radiusY")]
        radius_y: u32,
    },
}

impl StabilizationSpec {
    fn validate(self) -> Result<(), CompositionError> {
        match self {
            Self::Disabled => Ok(()),
            Self::Deshake { radius_x, radius_y }
                if (16..=64).contains(&radius_x)
                    && (16..=64).contains(&radius_y)
                    && radius_x.is_multiple_of(16)
                    && radius_y.is_multiple_of(16) =>
            {
                Ok(())
            }
            Self::Deshake { .. } => Err(CompositionError::InvalidStabilization),
        }
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Deshake { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskShape {
    /// Axis-aligned rectangle in the clip's pre-transform local raster.
    Rectangle,
    /// Axis-aligned ellipse in the clip's pre-transform local raster.
    Ellipse,
    /// Reserved wire value; render remains fail-closed until direction and
    /// falloff semantics are added without changing the saved schema.
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
        /// Clip-local normalized mask centre. `(0, 0)` is the top-left and
        /// `(1, 1)` the bottom-right of the pre-transform clip raster. A
        /// keyframe tick zero is the clip's first rendered frame.
        x: AnimatableValue,
        y: AnimatableValue,
        /// Normalized full width/height relative to the pre-transform clip.
        /// Values may extend to `2` so a centred mask can cover beyond an edge.
        width: AnimatableValue,
        height: AnimatableValue,
        /// Inward soft edge as a fraction of the mask radius/half-extent. Zero
        /// is a hard inclusive edge. For Rectangle, each axis ramps linearly
        /// from zero at its edge and the lower ramp wins; for Ellipse, radial
        /// distance ramps linearly. One feathers from the edge to the centre.
        feather: f64,
        /// Swap selected/rejected alpha, then multiply the clip's incoming
        /// alpha. Multiple masks multiply in declaration order before opacity.
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
                if animatable_values_in(x, 0.0, 1.0, true)
                    && animatable_values_in(y, 0.0, 1.0, true)
                    && animatable_values_in(width, 0.0, 2.0, false)
                    && animatable_values_in(height, 0.0, 2.0, false)
                    && feather.is_finite()
                    && (0.0..=1.0).contains(feather)
                {
                    Ok(())
                } else {
                    Err(CompositionError::InvalidEffect)
                }
            }
        }
    }
}

fn animatable_values_in(
    value: &AnimatableValue,
    minimum: f64,
    maximum: f64,
    inclusive_minimum: bool,
) -> bool {
    let valid = |candidate: f64| {
        candidate.is_finite()
            && if inclusive_minimum {
                (minimum..=maximum).contains(&candidate)
            } else {
                candidate > minimum && candidate <= maximum
            }
    };
    match value {
        AnimatableValue::Constant { value } => valid(*value),
        AnimatableValue::Keyframes { track } => {
            track.keyframes.iter().all(|keyframe| valid(keyframe.value))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoClip {
    pub id: CompositionClipId,
    pub source_id: SourceId,
    pub placement: ClipPlacement,
    /// Missing in older v1 documents preserves natural forward playback.
    #[serde(default)]
    pub playback_mode: PlaybackMode,
    /// Missing in older v1 documents preserves the unstabilized render path.
    #[serde(default)]
    pub stabilization: StabilizationSpec,
    /// Missing in older v1 documents keeps the historical duplicate/drop
    /// frame-rate conversion path.
    #[serde(default)]
    pub frame_interpolation: FrameInterpolation,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    pub blend_mode: BlendMode,
    /// Ordered visual effects. The composition exporter currently supports
    /// ChromaKey effects followed by Rectangle/Ellipse masks; unsupported
    /// orderings remain saved but fail closed at render planning.
    #[serde(default)]
    pub effects: Vec<VideoEffect>,
    /// Whether the embedded audio stream is audible when this clip belongs to
    /// the primary video track. Missing in older v1 documents means enabled.
    #[serde(default = "default_true")]
    pub source_audio_enabled: bool,
    /// Clip-local gain for the embedded source audio. Missing in older v1
    /// documents means unity gain.
    #[serde(default = "default_audio_gain")]
    pub audio_gain: AnimatableValue,
    /// Clip-local stereo pan for the embedded source audio. Missing in older
    /// v1 documents means centre.
    #[serde(default = "default_audio_pan")]
    pub audio_pan: AnimatableValue,
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
        if !is_supported_composition_font_family(&self.font_family)
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

pub fn is_supported_composition_font_family(value: &str) -> bool {
    matches!(
        value,
        "Noto Sans" | "Arial Unicode MS" | "DejaVu Sans" | "Arial"
    )
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
        /// Mutes only embedded source audio; video frames remain visible.
        #[serde(default)]
        muted: bool,
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
                        clip.stabilization.validate()?;
                        if clip.placement.speed_ramp.is_some()
                            && matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                        {
                            return Err(CompositionError::InvalidSpeedRamp);
                        }
                        if clip.stabilization.is_enabled()
                            && matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                        {
                            return Err(CompositionError::InvalidStabilization);
                        }
                        if let PlaybackMode::Freeze { source_tick } = clip.playback_mode {
                            if source_tick < clip.placement.source_in_tick
                                || source_tick >= clip.placement.source_out_tick
                            {
                                return Err(CompositionError::InvalidPlaybackMode(clip.id.clone()));
                            }
                            if clip.frame_interpolation == FrameInterpolation::OpticalFlow {
                                return Err(CompositionError::InvalidPlaybackMode(clip.id.clone()));
                            }
                        }
                        if clip.frame_interpolation == FrameInterpolation::OpticalFlow
                            && (*hidden || !clip.enabled || clip.placement.minimum_speed()? >= 1.0)
                        {
                            return Err(CompositionError::InvalidFrameInterpolation(
                                clip.id.clone(),
                            ));
                        }
                        validate_media_clip(
                            &self.sources,
                            &clip.id,
                            &clip.source_id,
                            &clip.placement,
                            true,
                        )?;
                        clip.transform.validate()?;
                        clip.opacity.validate()?;
                        clip.audio_gain.validate()?;
                        clip.audio_pan.validate()?;
                        for effect in &clip.effects {
                            effect.validate()?;
                        }
                        if !local_ids.insert(&clip.id) || !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.placement.timeline_end_tick()?);
                    }
                    validate_transitions(transitions, clips, &self.sources)?;
                }
                CompositionTrack::Audio { clips, .. } => {
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        validate_media_clip(
                            &self.sources,
                            &clip.id,
                            &clip.source_id,
                            &clip.placement,
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
                            || clip.text.contains('\0')
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

    /// Whether this already-validated document needs the optional FFmpeg
    /// optical-flow pipeline for its active render graph.
    pub fn requires_optical_flow(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| {
                    clip.enabled
                        && clip.frame_interpolation == FrameInterpolation::OpticalFlow
                })
            )
        })
    }

    /// Whether the active render graph needs FFmpeg's buffered video reverse.
    pub fn requires_reverse_video(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| {
                    clip.enabled && clip.playback_mode == PlaybackMode::Reverse
                })
            )
        })
    }

    /// Whether the primary embedded source-audio graph needs `areverse`.
    /// Overlay video never contributes embedded audio.
    pub fn requires_reverse_audio(&self) -> bool {
        let primary = self.tracks.iter().rev().find(|track| match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            _ => false,
        });
        matches!(
            primary,
            Some(CompositionTrack::Video {
                muted: false,
                clips,
                ..
            }) if clips.iter().any(|clip| {
                clip.enabled
                    && clip.playback_mode == PlaybackMode::Reverse
                    && clip.source_audio_enabled
                    && self.sources[&clip.source_id].has_audio
            })
        )
    }

    /// Whether the active render graph needs a cloned freeze frame.
    pub fn requires_freeze_frame(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| {
                    clip.enabled && matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                })
            )
        })
    }

    /// Whether the active render graph needs FFmpeg's classical `deshake`.
    pub fn requires_stabilization(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| clip.enabled && clip.stabilization.is_enabled())
            )
        })
    }

    /// Whether the active render graph needs variable video timing or
    /// pitch-preserving segmented audio timing.
    pub fn requires_speed_ramp(&self) -> bool {
        let visual = self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| clip.enabled && clip.placement.speed_ramp.is_some())
            )
        });
        visual || self.active_audio_speed_ramp()
    }

    /// Whether an active ramp asks FFmpeg to preserve pitch through segmented
    /// `atempo`. Muted ramp audio is deliberately excluded.
    pub fn requires_speed_ramp_pitch_audio(&self) -> bool {
        let primary = self.tracks.iter().rev().find(|track| match track {
            CompositionTrack::Video {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Image {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            CompositionTrack::Text {
                hidden: false,
                clips,
                ..
            } => clips.iter().any(|clip| clip.enabled),
            _ => false,
        });
        let primary_audio = matches!(
            primary,
            Some(CompositionTrack::Video {
                muted: false,
                clips,
                ..
            }) if clips.iter().any(|clip| {
                clip.enabled
                    && clip.source_audio_enabled
                    && !matches!(clip.playback_mode, PlaybackMode::Freeze { .. })
                    && self.sources[&clip.source_id].has_audio
                    && clip.placement.speed_ramp.as_ref().is_some_and(|ramp| {
                        ramp.audio_policy == SpeedRampAudioPolicy::PreservePitch
                    })
            })
        );
        primary_audio || self.active_audio_speed_ramp()
    }

    fn active_audio_speed_ramp(&self) -> bool {
        let has_solo = self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Audio {
                    muted: false,
                    solo: true,
                    ..
                }
            )
        });
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Audio {
                    muted: false,
                    solo,
                    clips,
                    ..
                } if (!has_solo || *solo) && clips.iter().any(|clip| {
                    clip.enabled && clip.placement.speed_ramp.as_ref().is_some_and(|ramp| {
                        ramp.audio_policy == SpeedRampAudioPolicy::PreservePitch
                    })
                })
            )
        })
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
    placement: &ClipPlacement,
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
    clips: &[VideoClip],
    sources: &BTreeMap<SourceId, CompositionSource>,
) -> Result<(), CompositionError> {
    let mut ids = BTreeSet::new();
    let mut from_ids = BTreeSet::new();
    let mut to_ids = BTreeSet::new();
    let mut ordered: Vec<_> = clips.iter().collect();
    ordered.sort_by_key(|clip| (clip.placement.timeline_start_tick, clip.id.as_str()));
    let indexes: BTreeMap<_, _> = ordered
        .iter()
        .enumerate()
        .map(|(index, clip)| (&clip.id, index))
        .collect();
    let mut head_occupancy = BTreeMap::<&CompositionClipId, u64>::new();
    let mut tail_occupancy = BTreeMap::<&CompositionClipId, u64>::new();
    for transition in transitions {
        if !ids.insert(&transition.id) {
            return Err(CompositionError::DuplicateTransition(transition.id.clone()));
        }
        let Some(&from_index) = indexes.get(&transition.from_clip_id) else {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        };
        let Some(&to_index) = indexes.get(&transition.to_clip_id) else {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        };
        if transition.duration_ticks == 0
            || from_index.checked_add(1) != Some(to_index)
            || ordered[from_index].placement.timeline_end_tick().ok()
                != Some(ordered[to_index].placement.timeline_start_tick)
            || !from_ids.insert(&transition.from_clip_id)
            || !to_ids.insert(&transition.to_clip_id)
        {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
        let before_edit = transition.duration_ticks / 2;
        let after_edit = transition.duration_ticks - before_edit;
        let from = ordered[from_index];
        let to = ordered[to_index];
        if from.placement.speed_ramp.is_some() || to.placement.speed_ramp.is_some() {
            return Err(CompositionError::InvalidSpeedRampTransition(
                transition.id.clone(),
            ));
        }
        if from.playback_mode != PlaybackMode::Forward || to.playback_mode != PlaybackMode::Forward
        {
            return Err(CompositionError::InvalidPlaybackMode(
                if from.playback_mode != PlaybackMode::Forward {
                    from.id.clone()
                } else {
                    to.id.clone()
                },
            ));
        }
        let from_source = &sources[&from.source_id];
        let from_handle = after_edit as f64 * from.placement.speed;
        let to_handle = before_edit as f64 * to.placement.speed;
        if from_source
            .duration_ticks
            .saturating_sub(from.placement.source_out_tick) as f64
            + f64::EPSILON
            < from_handle
            || to.placement.source_in_tick as f64 + f64::EPSILON < to_handle
        {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
        tail_occupancy.insert(&from.id, before_edit);
        head_occupancy.insert(&to.id, after_edit);
    }
    for clip in ordered {
        let occupied = head_occupancy.get(&clip.id).copied().unwrap_or(0)
            + tail_occupancy.get(&clip.id).copied().unwrap_or(0);
        if clip.placement.timeline_duration_ticks()? < occupied {
            let transition = transitions
                .iter()
                .find(|transition| {
                    transition.from_clip_id == clip.id || transition.to_clip_id == clip.id
                })
                .expect("occupied duration comes from a transition");
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_audio_gain() -> AnimatableValue {
    AnimatableValue::constant(1.0)
}

fn default_audio_pan() -> AnimatableValue {
    AnimatableValue::constant(0.0)
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
    InvalidSpeedRamp,
    InvalidAnimation,
    InvalidTransform,
    InvalidEffect,
    InvalidText,
    InvalidAudioFade,
    InvalidFrameInterpolation(CompositionClipId),
    InvalidPlaybackMode(CompositionClipId),
    InvalidStabilization,
    InvalidClipSource(CompositionClipId),
    InvalidTransition(TransitionId),
    InvalidSpeedRampTransition(TransitionId),
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
    use crate::domain::keyframes::{Interpolation, Keyframe};

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

    #[test]
    fn wire_ids_reject_paths_and_control_characters() {
        for invalid in ["../source", "/tmp/source", "source id", "source\nnext"] {
            let encoded = serde_json::to_string(invalid).unwrap();
            assert!(serde_json::from_str::<SourceId>(&encoded).is_err());
        }
        assert_eq!(
            serde_json::from_str::<SourceId>(r#""safe-source_1""#).unwrap(),
            SourceId::parse("safe-source_1").unwrap()
        );
    }

    fn placement(start: u64) -> ClipPlacement {
        ClipPlacement {
            timeline_start_tick: start,
            source_in_tick: 0,
            source_out_tick: 2_000_000,
            speed: 1.0,
            speed_ramp: None,
        }
    }

    fn video_clip(id: &str, source_id: &str, start: u64) -> VideoClip {
        VideoClip {
            id: CompositionClipId::parse(id).unwrap(),
            source_id: SourceId::parse(source_id).unwrap(),
            placement: placement(start),
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.0),
            blend_mode: BlendMode::Normal,
            effects: Vec::new(),
            source_audio_enabled: true,
            audio_gain: AnimatableValue::constant(1.0),
            audio_pan: AnimatableValue::constant(0.0),
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
        let mut clip_a = video_clip("clip-a", "source-a", 0);
        clip_a.placement.source_in_tick = 1_000_000;
        clip_a.placement.source_out_tick = 3_000_000;
        let mut clip_b = video_clip("clip-b", "source-b", 2_000_000);
        clip_b.placement.source_in_tick = 1_000_000;
        clip_b.placement.source_out_tick = 3_000_000;
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip_a, clip_b],
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
    fn older_v1_video_audio_payloads_receive_compatible_defaults() {
        let source = source("legacy-source");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source.id.clone(), source);
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("legacy-video").unwrap(),
            name: "Legacy".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![video_clip("legacy-clip", "legacy-source", 0)],
            transitions: Vec::new(),
        }];
        let mut wire = serde_json::to_value(&composition).unwrap();
        let track = wire["tracks"][0].as_object_mut().unwrap();
        track.remove("muted");
        let clip = track["clips"][0].as_object_mut().unwrap();
        clip.remove("sourceAudioEnabled");
        clip.remove("audioGain");
        clip.remove("audioPan");
        clip.remove("frameInterpolation");
        clip.remove("playbackMode");
        clip.remove("stabilization");

        let decoded: Composition = serde_json::from_value(wire).unwrap();
        let CompositionTrack::Video { muted, clips, .. } = &decoded.tracks[0] else {
            unreachable!();
        };
        assert!(!muted);
        assert!(clips[0].source_audio_enabled);
        assert_eq!(clips[0].audio_gain, AnimatableValue::constant(1.0));
        assert_eq!(clips[0].audio_pan, AnimatableValue::constant(0.0));
        assert_eq!(clips[0].frame_interpolation, FrameInterpolation::Duplicate);
        assert_eq!(clips[0].playback_mode, PlaybackMode::Forward);
        assert_eq!(clips[0].stabilization, StabilizationSpec::Disabled);
        decoded.validate().unwrap();
    }

    #[test]
    fn optical_flow_is_only_valid_for_active_slow_motion_video_clips() {
        let source = source("slow-source");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source.id.clone(), source);
        let mut clip = video_clip("slow-clip", "slow-source", 0);
        clip.placement.speed = 0.5;
        clip.frame_interpolation = FrameInterpolation::OpticalFlow;
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("slow-track").unwrap(),
            name: "Slow motion".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];
        composition.validate().unwrap();
        assert!(composition.requires_optical_flow());
        let wire = serde_json::to_value(&composition).unwrap();
        assert_eq!(
            wire["tracks"][0]["clips"][0]["frameInterpolation"],
            "optical_flow"
        );
        assert_eq!(
            serde_json::from_value::<Composition>(wire).unwrap(),
            composition
        );

        for invalid in ["normal-speed", "disabled", "hidden"] {
            let mut invalid_composition = composition.clone();
            let CompositionTrack::Video { hidden, clips, .. } = &mut invalid_composition.tracks[0]
            else {
                unreachable!();
            };
            match invalid {
                "normal-speed" => clips[0].placement.speed = 1.0,
                "disabled" => clips[0].enabled = false,
                "hidden" => *hidden = true,
                _ => unreachable!(),
            }
            assert!(matches!(
                invalid_composition.validate(),
                Err(CompositionError::InvalidFrameInterpolation(_))
            ));
        }
    }

    #[test]
    fn playback_modes_round_trip_and_fail_closed_on_ambiguous_combinations() {
        let source = source("playback-source");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source.id.clone(), source);
        let mut clip = video_clip("playback-clip", "playback-source", 0);
        clip.playback_mode = PlaybackMode::Freeze {
            source_tick: clip.placement.source_in_tick,
        };
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("playback-track").unwrap(),
            name: "Playback".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];
        composition.validate().unwrap();
        assert!(composition.requires_freeze_frame());
        assert!(!composition.requires_reverse_video());
        let wire = serde_json::to_value(&composition).unwrap();
        assert_eq!(
            wire["tracks"][0]["clips"][0]["playbackMode"]["mode"],
            "freeze"
        );
        assert_eq!(
            wire["tracks"][0]["clips"][0]["playbackMode"]["sourceTick"],
            0
        );
        assert_eq!(
            serde_json::from_value::<Composition>(wire).unwrap(),
            composition
        );

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze {
            source_tick: clips[0].placement.source_out_tick,
        };
        assert!(matches!(
            composition.validate(),
            Err(CompositionError::InvalidPlaybackMode(_))
        ));

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        clips[0].placement.speed = 0.5;
        clips[0].frame_interpolation = FrameInterpolation::OpticalFlow;
        assert!(matches!(
            composition.validate(),
            Err(CompositionError::InvalidPlaybackMode(_))
        ));

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].frame_interpolation = FrameInterpolation::Duplicate;
        clips[0].placement.speed = 1.0;
        clips[0].playback_mode = PlaybackMode::Reverse;
        composition.validate().unwrap();
        assert!(composition.requires_reverse_video());
        assert!(composition.requires_reverse_audio());
        assert!(!composition.requires_freeze_frame());
    }

    #[test]
    fn transitions_reject_non_forward_endpoints() {
        let first = source("reverse-a");
        let second = source("reverse-b");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(first.id.clone(), first);
        composition.sources.insert(second.id.clone(), second);
        let mut from = video_clip("reverse-clip-a", "reverse-a", 0);
        from.placement.source_in_tick = 500_000;
        from.placement.source_out_tick = 2_500_000;
        from.playback_mode = PlaybackMode::Reverse;
        let mut to = video_clip("reverse-clip-b", "reverse-b", 2_000_000);
        to.placement.source_in_tick = 500_000;
        to.placement.source_out_tick = 2_500_000;
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("reverse-main").unwrap(),
            name: "Reverse".into(),
            hidden: false,
            muted: false,
            locked: false,
            transitions: vec![ClipTransition {
                id: TransitionId::parse("reverse-transition").unwrap(),
                from_clip_id: from.id.clone(),
                to_clip_id: to.id.clone(),
                duration_ticks: 200_000,
                kind: TransitionKind::Dissolve,
            }],
            clips: vec![from, to],
        }];
        assert!(matches!(
            composition.validate(),
            Err(CompositionError::InvalidPlaybackMode(_))
        ));
    }

    #[test]
    fn stabilization_wire_is_bounded_and_freeze_fails_closed() {
        let source = source("stabilization-source");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source.id.clone(), source);
        let mut clip = video_clip("stabilization-clip", "stabilization-source", 0);
        clip.placement.speed = 0.5;
        clip.playback_mode = PlaybackMode::Reverse;
        clip.frame_interpolation = FrameInterpolation::OpticalFlow;
        clip.stabilization = StabilizationSpec::Deshake {
            radius_x: 32,
            radius_y: 16,
        };
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("stabilization-track").unwrap(),
            name: "Stabilization".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];
        composition.validate().unwrap();
        assert!(composition.requires_stabilization());
        let wire = serde_json::to_value(&composition).unwrap();
        assert_eq!(
            wire["tracks"][0]["clips"][0]["stabilization"]["mode"],
            "deshake"
        );
        assert_eq!(
            wire["tracks"][0]["clips"][0]["stabilization"]["radiusX"],
            32
        );
        assert_eq!(
            serde_json::from_value::<Composition>(wire).unwrap(),
            composition
        );

        for invalid_radius in [0, 8, 24, 65] {
            let mut invalid = composition.clone();
            let CompositionTrack::Video { clips, .. } = &mut invalid.tracks[0] else {
                unreachable!();
            };
            clips[0].stabilization = StabilizationSpec::Deshake {
                radius_x: invalid_radius,
                radius_y: 16,
            };
            assert_eq!(
                invalid.validate(),
                Err(CompositionError::InvalidStabilization)
            );
        }

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        clips[0].frame_interpolation = FrameInterpolation::Duplicate;
        assert_eq!(
            composition.validate(),
            Err(CompositionError::InvalidStabilization)
        );
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
            muted: false,
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
                muted: false,
                locked: false,
                clips: vec![duplicate.clone()],
                transitions: Vec::new(),
            },
            CompositionTrack::Video {
                id: TrackId::parse("video-b").unwrap(),
                name: "B".into(),
                hidden: false,
                muted: false,
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
    fn mask_wire_semantics_are_normalized_and_schema_compatible() {
        let effect = VideoEffect::Mask {
            shape: MaskShape::Ellipse,
            x: AnimatableValue::constant(0.5),
            y: AnimatableValue::constant(0.25),
            width: AnimatableValue::constant(1.5),
            height: AnimatableValue::constant(0.75),
            feather: 0.2,
            inverted: true,
        };
        effect.validate().unwrap();
        let encoded = serde_json::to_value(&effect).unwrap();
        assert_eq!(encoded["kind"], "mask");
        assert_eq!(encoded["shape"], "ellipse");
        assert_eq!(encoded["x"]["mode"], "constant");
        assert_eq!(encoded["width"]["value"], 1.5);
        assert_eq!(
            serde_json::from_value::<VideoEffect>(encoded).unwrap(),
            effect
        );

        for invalid in [
            VideoEffect::Mask {
                shape: MaskShape::Rectangle,
                x: AnimatableValue::constant(-0.01),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.5),
                height: AnimatableValue::constant(0.5),
                feather: 0.0,
                inverted: false,
            },
            VideoEffect::Mask {
                shape: MaskShape::Rectangle,
                x: AnimatableValue::constant(0.5),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.0),
                height: AnimatableValue::constant(0.5),
                feather: 0.0,
                inverted: false,
            },
        ] {
            assert_eq!(invalid.validate(), Err(CompositionError::InvalidEffect));
        }
    }

    #[test]
    fn animatable_values_bound_saved_projects_to_thirty_two_points() {
        let allowed = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                DEFAULT_TIME_BASE,
                Interpolation::Linear,
                (0..MAX_KEYFRAMES_PER_VALUE)
                    .map(|index| Keyframe {
                        tick: index as u64,
                        value: index as f64,
                    })
                    .collect(),
            )
            .unwrap(),
        };
        allowed.validate().unwrap();
        let too_many = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                DEFAULT_TIME_BASE,
                Interpolation::EaseInOut,
                (0..=MAX_KEYFRAMES_PER_VALUE)
                    .map(|index| Keyframe {
                        tick: index as u64,
                        value: index as f64,
                    })
                    .collect(),
            )
            .unwrap(),
        };
        assert_eq!(too_many.validate(), Err(CompositionError::InvalidAnimation));
    }

    #[test]
    fn text_fonts_are_allowlisted_and_transitions_require_source_handles() {
        let mut composition = Composition::new(CanvasSpec::default());
        composition.tracks.push(CompositionTrack::Text {
            id: TrackId::parse("title-track").unwrap(),
            name: "Title".into(),
            hidden: false,
            locked: false,
            clips: vec![TextClip {
                id: CompositionClipId::parse("title-clip").unwrap(),
                timeline_start_tick: 0,
                timeline_end_tick: 1_000_000,
                text: "safe textfile content".into(),
                style: TextStyle {
                    font_family: "../../arbitrary.ttf".into(),
                    font_size: 48.0,
                    color: Rgba::BLACK,
                    background: Rgba::BLACK,
                    stroke: Rgba::BLACK,
                    stroke_width: 0.0,
                    shadow: Rgba::BLACK,
                    shadow_x: 0.0,
                    shadow_y: 0.0,
                },
                transform: TransformSpec::default(),
                opacity: AnimatableValue::constant(1.0),
                enabled: true,
            }],
        });
        assert_eq!(composition.validate(), Err(CompositionError::InvalidText));

        let source_a = source("source-a");
        let source_b = source("source-b");
        composition.sources.insert(source_a.id.clone(), source_a);
        composition.sources.insert(source_b.id.clone(), source_b);
        let clip_a = video_clip("clip-a", "source-a", 0);
        let clip_b = video_clip("clip-b", "source-b", 2_000_000);
        let transition_id = TransitionId::parse("transition-a-b").unwrap();
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("video-main").unwrap(),
            name: "Main".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip_a.clone(), clip_b.clone()],
            transitions: vec![ClipTransition {
                id: transition_id.clone(),
                from_clip_id: clip_a.id,
                to_clip_id: clip_b.id,
                duration_ticks: 250_000,
                kind: TransitionKind::Dissolve,
            }],
        }];
        assert_eq!(
            composition.validate(),
            Err(CompositionError::InvalidTransition(transition_id))
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
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];

        assert_eq!(composition.validate(), Err(CompositionError::OutputTooLong));
    }

    #[test]
    fn speed_ramp_wire_duration_and_bounds_are_deterministic_and_backward_compatible() {
        let legacy = placement(0);
        let legacy_wire = serde_json::to_value(&legacy).unwrap();
        assert!(legacy_wire.get("speedRamp").is_none());
        let decoded: ClipPlacement = serde_json::from_value(legacy_wire).unwrap();
        assert!(decoded.speed_ramp.is_none());
        assert_eq!(decoded.timeline_duration_ticks().unwrap(), 2_000_000);

        let mut linear = placement(0);
        linear.speed = 0.5;
        linear.speed_ramp = Some(SpeedRampSpec {
            interpolation: SpeedRampInterpolation::Linear,
            points: vec![
                SpeedRampPoint {
                    source_progress_tick: 0,
                    speed: 0.5,
                },
                SpeedRampPoint {
                    source_progress_tick: 2_000_000,
                    speed: 2.0,
                },
            ],
            audio_policy: SpeedRampAudioPolicy::PreservePitch,
        });
        assert_eq!(linear.timeline_duration_ticks().unwrap(), 1_848_392);
        let segments = linear.speed_ramp_segments().unwrap().unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].timeline_end_tick, 1_848_392);

        let mut hold = placement(0);
        hold.speed_ramp = Some(SpeedRampSpec {
            interpolation: SpeedRampInterpolation::Hold,
            points: vec![
                SpeedRampPoint {
                    source_progress_tick: 0,
                    speed: 1.0,
                },
                SpeedRampPoint {
                    source_progress_tick: 1_000_000,
                    speed: 2.0,
                },
                // Endpoint speed is intentionally metadata only for Hold.
                SpeedRampPoint {
                    source_progress_tick: 2_000_000,
                    speed: 4.0,
                },
            ],
            audio_policy: SpeedRampAudioPolicy::Mute,
        });
        assert_eq!(hold.timeline_duration_ticks().unwrap(), 1_500_000);

        for invalid in [
            SpeedRampSpec {
                interpolation: SpeedRampInterpolation::Linear,
                points: vec![
                    SpeedRampPoint {
                        source_progress_tick: 0,
                        speed: 2.0,
                    },
                    SpeedRampPoint {
                        source_progress_tick: 2_000_000,
                        speed: 1.0,
                    },
                ],
                audio_policy: SpeedRampAudioPolicy::PreservePitch,
            },
            SpeedRampSpec {
                interpolation: SpeedRampInterpolation::Linear,
                points: vec![
                    SpeedRampPoint {
                        source_progress_tick: 0,
                        speed: 1.0,
                    },
                    SpeedRampPoint {
                        source_progress_tick: 999,
                        speed: 2.0,
                    },
                    SpeedRampPoint {
                        source_progress_tick: 2_000_000,
                        speed: 1.0,
                    },
                ],
                audio_policy: SpeedRampAudioPolicy::PreservePitch,
            },
            SpeedRampSpec {
                interpolation: SpeedRampInterpolation::Linear,
                points: vec![
                    SpeedRampPoint {
                        source_progress_tick: 0,
                        speed: 1.0,
                    },
                    SpeedRampPoint {
                        source_progress_tick: 1_999_999,
                        speed: 1.0,
                    },
                ],
                audio_policy: SpeedRampAudioPolicy::PreservePitch,
            },
        ] {
            let mut placement = placement(0);
            placement.speed_ramp = Some(invalid);
            assert_eq!(
                placement.timeline_duration_ticks(),
                Err(CompositionError::InvalidSpeedRamp)
            );
        }
    }

    #[test]
    fn speed_ramp_supports_reverse_optical_and_stabilization_but_rejects_freeze_and_transitions() {
        let source_a = source("ramp-source-a");
        let source_b = source("ramp-source-b");
        let mut ramped = video_clip("ramp-clip", "ramp-source-a", 0);
        ramped.placement.speed = 0.5;
        ramped.placement.speed_ramp = Some(SpeedRampSpec {
            interpolation: SpeedRampInterpolation::Linear,
            points: vec![
                SpeedRampPoint {
                    source_progress_tick: 0,
                    speed: 0.5,
                },
                SpeedRampPoint {
                    source_progress_tick: 2_000_000,
                    speed: 1.5,
                },
            ],
            audio_policy: SpeedRampAudioPolicy::PreservePitch,
        });
        ramped.playback_mode = PlaybackMode::Reverse;
        ramped.frame_interpolation = FrameInterpolation::OpticalFlow;
        ramped.stabilization = StabilizationSpec::Deshake {
            radius_x: 16,
            radius_y: 16,
        };
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source_a.id.clone(), source_a);
        composition.sources.insert(source_b.id.clone(), source_b);
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("ramp-track").unwrap(),
            name: "Ramp".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![ramped.clone()],
            transitions: Vec::new(),
        }];
        composition.validate().unwrap();
        assert!(composition.requires_speed_ramp());
        assert!(composition.requires_speed_ramp_pitch_audio());
        assert!(composition.requires_reverse_video());
        assert!(composition.requires_optical_flow());
        assert!(composition.requires_stabilization());

        let CompositionTrack::Video { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].playback_mode = PlaybackMode::Freeze { source_tick: 0 };
        assert_eq!(
            composition.validate(),
            Err(CompositionError::InvalidSpeedRamp)
        );

        let mut first = ramped;
        first.playback_mode = PlaybackMode::Forward;
        first.frame_interpolation = FrameInterpolation::Duplicate;
        first.stabilization = StabilizationSpec::Disabled;
        let first_duration = first.placement.timeline_duration_ticks().unwrap();
        let second = video_clip("plain-clip", "ramp-source-b", first_duration);
        let transition_id = TransitionId::parse("ramp-transition").unwrap();
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("transition-track").unwrap(),
            name: "Transition".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![first.clone(), second.clone()],
            transitions: vec![ClipTransition {
                id: transition_id.clone(),
                from_clip_id: first.id,
                to_clip_id: second.id,
                duration_ticks: 100_000,
                kind: TransitionKind::Dissolve,
            }],
        }];
        assert_eq!(
            composition.validate(),
            Err(CompositionError::InvalidSpeedRampTransition(transition_id))
        );
    }
}
