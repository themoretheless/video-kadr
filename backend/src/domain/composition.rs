use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::keyframes::KeyframeTrack;

pub const COMPOSITION_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_TIME_BASE: u32 = 1_000_000;
pub const MAX_TRACKS: usize = 16;
pub const MAX_CLIPS: usize = 512;
pub const MAX_SOURCES: usize = 32;
pub const MAX_VISIBLE_LAYERS: usize = 8;
pub const MAX_STYLE_EFFECTS_PER_CLIP: usize = 5;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameRateRatio {
    pub numerator: u32,
    pub denominator: u32,
}

impl std::fmt::Display for FrameRateRatio {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.numerator, self.denominator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasSpec {
    pub width: u32,
    pub height: u32,
    pub fps_milli: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fps_rate: Option<FrameRateRatio>,
    pub background: Rgba,
    #[serde(default)]
    pub background_mode: CanvasBackgroundMode,
    #[serde(default = "default_canvas_blur")]
    pub background_blur: f64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasBackgroundMode {
    #[default]
    Color,
    Blur,
    Checker,
}

const fn default_canvas_blur() -> f64 {
    24.0
}

impl Default for CanvasSpec {
    fn default() -> Self {
        Self {
            width: 1_920,
            height: 1_080,
            fps_milli: 30_000,
            fps_rate: None,
            background: Rgba::BLACK,
            background_mode: CanvasBackgroundMode::Color,
            background_blur: default_canvas_blur(),
        }
    }
}

impl CanvasSpec {
    pub fn frame_rate(self) -> FrameRateRatio {
        self.fps_rate.unwrap_or(FrameRateRatio {
            numerator: self.fps_milli,
            denominator: 1000,
        })
    }

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
        if let Some(rate) = self.fps_rate {
            if rate.numerator == 0 || rate.denominator == 0 {
                return Err(CompositionError::InvalidFrameRate);
            }
            let milli = (u64::from(rate.numerator) * 1000 + u64::from(rate.denominator) / 2)
                / u64::from(rate.denominator);
            if milli != u64::from(self.fps_milli) {
                return Err(CompositionError::InvalidFrameRate);
            }
        }
        self.background.validate()?;
        if !self.background_blur.is_finite() || !(1.0..=100.0).contains(&self.background_blur) {
            return Err(CompositionError::InvalidCanvas);
        }
        Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_channels: Option<u32>,
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
        if self
            .audio_channels
            .is_some_and(|channels| !self.has_audio || channels == 0 || channels > 64)
        {
            return Err(CompositionError::InvalidSource(self.id.clone()));
        }
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
    /// Authored timeline extent, independent of the source window. Legacy
    /// documents derive it from the source span and speed curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline_duration_tick: Option<u64>,
    /// Retained phase within the complete source-speed curve after a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_timeline_offset_tick: Option<i64>,
    /// Full retained preparation head, shared by fragments cut inside that head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_head_timeline_tick: Option<u64>,
    /// Optional source-progress speed curve. The first point is anchored to
    /// `speed`, so old constant-speed documents and authoring controls retain
    /// one unambiguous baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_ramp: Option<SpeedRampSpec>,
    /// Original timeline-fraction curve; distinct from native source-progress ramps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concat_retime: Option<super::concat_retime::SourceClock>,
}

impl ClipPlacement {
    pub fn timeline_duration_ticks(&self) -> Result<u64, CompositionError> {
        if self.source_out_tick <= self.source_in_tick
            || !self.speed.is_finite()
            || !(0.05..=16.0).contains(&self.speed)
        {
            return Err(CompositionError::InvalidPlacement);
        }
        if let Some(clock)=&self.concat_retime {
            clock.validate_placement(self).map_err(|_|CompositionError::InvalidSpeedRamp)?;
            return Ok(clock.duration_ticks);
        }
        if let Some(segments) = self.speed_ramp_segments()? {
            let derived = segments
                .last()
                .map(|segment| segment.timeline_end_tick)
                .ok_or(CompositionError::InvalidSpeedRamp)?;
            return self.authored_duration_or(derived);
        }
        let duration = self.source_span_ticks() as f64 / self.speed;
        if !duration.is_finite() || duration < 1.0 || duration > u64::MAX as f64 {
            return Err(CompositionError::InvalidPlacement);
        }
        self.authored_duration_or(duration.round() as u64)
    }

    fn authored_duration_or(&self, derived: u64) -> Result<u64, CompositionError> {
        let offset = self.source_timeline_offset_tick.unwrap_or(0);
        if i128::from(offset) >= i128::from(derived) || offset.unsigned_abs() > 9_007_199_254_740_991 {
            return Err(CompositionError::InvalidPlacement);
        }
        match self.timeline_duration_tick {
            Some(0) => Err(CompositionError::InvalidPlacement),
            Some(duration) => Ok(duration),
            None => u64::try_from(i128::from(derived) - i128::from(offset)).map_err(|_| CompositionError::InvalidPlacement),
        }
    }

    /// Source material needed before the retained clock origin (after source-out in reverse).
    pub fn source_head_timeline_ticks(&self) -> Result<u64, CompositionError> {
        let phase = self.source_timeline_offset_tick.unwrap_or(0);
        let head = self.source_head_timeline_tick.unwrap_or(0).max(if phase < 0 { phase.unsigned_abs() } else { 0 });
        if head > 9_007_199_254_740_991 { return Err(CompositionError::InvalidPlacement); }
        Ok(head)
    }
    pub fn source_head_ticks(&self) -> Result<u64, CompositionError> {
        let head = self.source_head_timeline_ticks()? as f64 * self.speed;
        if !head.is_finite() || head > 9_007_199_254_740_991.0 { return Err(CompositionError::InvalidPlacement); }
        Ok(head.ceil() as u64)
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
        if let Some(clock)=&self.concat_retime {
            return clock.curve().map_err(|_|CompositionError::InvalidSpeedRamp)?.points().iter().map(|p|p.1).reduce(f64::min).ok_or(CompositionError::InvalidSpeedRamp);
        }
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

    pub(crate) fn validate(&self) -> Result<(), CompositionError> {
        match self {
            Self::Constant { value } if value.is_finite() => Ok(()),
            Self::Constant { .. } => Err(CompositionError::InvalidAnimation),
            Self::Keyframes { track } => {
                if let Some(clock)=track.sample_clock{clock.prepare(track).map_err(|_|CompositionError::InvalidAnimation)?;}
                KeyframeTrack::new(
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
            .map_err(|_| CompositionError::InvalidAnimation)
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransformSpec {
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub concat_animation:Option<super::concat_clip_animation::SourceAnimation>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub concat_preparation:Option<super::concat_picture::Preparation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<super::motion_path::PositionAnimation>,
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
            concat_animation:None,
            concat_preparation:None,
            position: None,
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
    pub(crate) fn validate(&self) -> Result<(), CompositionError> {
        if let Some(preparation)=&self.concat_preparation{preparation.validate().map_err(|_|CompositionError::InvalidTransform)?;}
        if let Some(animation)=&self.concat_animation {
            animation.validate().map_err(|_|CompositionError::InvalidAnimation)?;
            if self.position.is_some(){return Err(CompositionError::InvalidAnimation)}
        }
        if let Some(position) = &self.position {
            position.compile().map_err(|_| CompositionError::InvalidAnimation)?;
        }
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
    /// Two-pass libvidstab. Detect writes a `.trf`; transform consumes it in the
    /// export graph. Bounds match FFmpeg's documented filter ranges.
    Vidstab {
        /// 1..=10 — how aggressive motion detection is (default 5).
        shakiness: u32,
        /// 1..=15 — detection accuracy (default 15).
        accuracy: u32,
        /// 1..=30 — low-pass smoothing for the transform pass (default 10).
        smoothing: u32,
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
            Self::Vidstab {
                shakiness,
                accuracy,
                smoothing,
            } if (1..=10).contains(&shakiness)
                && (1..=15).contains(&accuracy)
                && (1..=30).contains(&smoothing) =>
            {
                Ok(())
            }
            Self::Vidstab { .. } => Err(CompositionError::InvalidStabilization),
        }
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Deshake { .. } | Self::Vidstab { .. })
    }

    pub fn is_deshake(self) -> bool {
        matches!(self, Self::Deshake { .. })
    }

    pub fn is_vidstab(self) -> bool {
        matches!(self, Self::Vidstab { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskShape {
    /// Rectangle in the clip's pre-transform local raster.
    Rectangle,
    /// Ellipse in the clip's pre-transform local raster.
    Ellipse,
    /// Reserved wire value; render remains fail-closed until direction and
    /// falloff semantics are added without changing the saved schema.
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoEffectPreset {
    Blur,
    Pixelate,
    Vignette,
    Sharpen,
    Edge,
    RgbSplit,
    Posterize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VideoEffect {
 NativeBypass {effect:Box<VideoEffect>},
 ConcatBypass {link:super::concat_effect_upgrade::AppliedFilter},
 ConcatGaussianBlur {parameters:super::concat_gaussian_blur::GaussianBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatMotionBlur {parameters:super::concat_motion_blur::MotionBlur, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_motion_blur::OriginalKeys>},
 ConcatBloomPulse {parameters:super::concat_bloom_pulse::BloomPulse, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_bloom_pulse::OriginalKeys>},
 ConcatZoomBlur {parameters:super::concat_zoom_blur::ZoomBlur, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_zoom_blur::OriginalKeys>},
 ConcatFilmGrain {parameters:super::concat_film_grain::FilmGrain, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatGlow {parameters:super::concat_glow::Glow, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatHalftone {parameters:super::concat_halftone::Halftone, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatFisheye {parameters:super::concat_fisheye::Fisheye, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatChromaticAberration {parameters:super::concat_chromatic_aberration::ChromaticAberration, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatPosterize {parameters:super::concat_posterize::Posterize},
 ConcatLumaKey {parameters:super::concat_luma_key::LumaKey},
 ConcatPixelate {parameters:super::concat_pixelate::Pixelate},
 ConcatChromaKey {parameters:super::concat_chroma_key::ChromaKey, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_chroma_key::OriginalKeys>},
 ConcatSaturation {parameters:super::concat_chroma::Saturation, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatVibrance {parameters:super::concat_chroma::Vibrance, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::concat_chroma::VibranceAnimation>},
 ConcatSharpen {parameters:super::concat_spatial_filters::Sharpen},
 ConcatVhs {parameters:super::concat_vhs::Vhs},
 ConcatFallingDust {parameters:super::concat_falling_dust::FallingDust},
 ConcatFilm {parameters:super::concat_film_camera_looks::Film},
 ConcatDigicam {parameters:super::concat_film_camera_looks::Digicam},
 ConcatMono {parameters:super::concat_timed_looks::Mono},
 ConcatMidnight {parameters:super::concat_timed_looks::Midnight},
 ConcatMarquee {parameters:super::concat_marquee::Marquee},
 ConcatRetroCartoon {parameters:super::concat_retro_cartoon::RetroCartoon},
 ConcatSuper8 {parameters:super::concat_super8::Super8},
 ConcatBleachBypass {parameters:super::concat_bleach_bypass::BleachBypass},
 ConcatAutumn {parameters:super::concat_autumn::Autumn},
 ConcatNeon {parameters:super::concat_neon_looks::Neon},
 ConcatPinkFuzz {parameters:super::concat_neon_looks::PinkFuzz},

 ConcatNight {parameters:super::concat_low_light_looks::Night},
 ConcatSpooky {parameters:super::concat_low_light_looks::Spooky},

 ConcatCool {parameters:super::concat_color_presets::Cool},
 ConcatWarm {parameters:super::concat_color_presets::Warm},
 ConcatTealOrange {parameters:super::concat_color_presets::TealOrange},

 ConcatNoir {parameters:super::concat_scene_looks::Noir},
 ConcatDreamy {parameters:super::concat_scene_looks::Dreamy},

 ConcatHighContrastMono {parameters:super::concat_fixed_looks::HighContrastMono},
 ConcatLandscapePop {parameters:super::concat_fixed_looks::LandscapePop},
 ConcatNatural {parameters:super::concat_fixed_looks::Natural},
 #[serde(rename="concat_retro_70s")]
 ConcatRetro70s {parameters:super::concat_fixed_looks::Retro70s},
 ConcatSilver {parameters:super::concat_fixed_looks::Silver},
 ConcatTropical {parameters:super::concat_fixed_looks::Tropical},
 ConcatVaporwave {parameters:super::concat_fixed_looks::Vaporwave},
 ConcatWinterCool {parameters:super::concat_fixed_looks::WinterCool},
 ConcatClassicChrome {parameters:super::concat_fixed_looks::ClassicChrome},
 ConcatCyberpunk {parameters:super::concat_fixed_looks::Cyberpunk},
 ConcatDelicious {parameters:super::concat_fixed_looks::Delicious},
 ConcatRosy {parameters:super::concat_fixed_looks::Rosy},
 ConcatSciFi {parameters:super::concat_fixed_looks::SciFi},
 ConcatMatcha {parameters:super::concat_green_looks::Matcha},
 ConcatSage {parameters:super::concat_green_looks::Sage},
 ConcatIvory {parameters:super::concat_matte_looks::Ivory},
 ConcatMilk {parameters:super::concat_matte_looks::Milk},
 ConcatBronze {parameters:super::concat_toned_looks::Bronze},
 ConcatCoolBlue {parameters:super::concat_toned_looks::CoolBlue},
 ConcatFresh {parameters:super::concat_detail_looks::Fresh},
 ConcatVivid {parameters:super::concat_detail_looks::Vivid},
 ConcatSoftLight {parameters:super::concat_atmosphere_looks::SoftLight},
 ConcatMoody {parameters:super::concat_atmosphere_looks::Moody},
 ConcatRain {parameters:super::concat_weather_looks::Rain},
 ConcatUmber {parameters:super::concat_weather_looks::Umber},
 ConcatFair {parameters:super::concat_portrait_looks::Fair},
 ConcatPeach {parameters:super::concat_portrait_looks::Peach},
 ConcatPolaroid {parameters:super::concat_retro_looks::Polaroid},
 ConcatVintage {parameters:super::concat_retro_looks::Vintage},
 ConcatCloudlessSky {parameters:super::concat_scenery_looks::CloudlessSky},
 ConcatLagoon {parameters:super::concat_scenery_looks::Lagoon},
 ConcatGoldenHour {parameters:super::concat_light_looks::GoldenHour},
 ConcatDaydream {parameters:super::concat_light_looks::Daydream},
 ConcatClean {parameters:super::concat_clean::Clean},
 ConcatContrastPop {parameters:super::concat_contrast_pop::ContrastPop},
 ConcatVignette {parameters:super::concat_spatial_filters::Vignette},
 ConcatAdjust {parameters:super::concat_adjust::Adjust, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::concat_color_wheels::Animation>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_color_wheels::OriginalKeys>},
 ConcatCurves {parameters:super::concat_curves::Curves},
 ConcatColorWheels {parameters:super::concat_color_wheels::ColorWheels, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::concat_color_wheels::Animation>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_color_wheels::OriginalKeys>},
 ConcatWhiteBalance {parameters:super::concat_white_balance::WhiteBalance, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::concat_white_balance::Animation>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<super::concat_white_balance::OriginalKeys>},
 ConcatExposure {parameters:super::concat_exposure::ConcatExposure, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 ConcatContrast {parameters:super::concat_contrast::ConcatContrast, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="concatKeys",skip_serializing_if="Option::is_none")] concat_keys:Option<Vec<super::concat_effect_upgrade::ParamKey>>},
 Lumetri {parameters:super::lumetri::Lumetri},
 ColorSharpen {parameters:super::color_sharpen::ColorSharpen, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 CreativeAdjust {parameters:super::creative_adjust::CreativeAdjust, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::creative_adjust::AdjustAnimation>},
 ProcAmp {parameters:super::proc_amp::ProcAmp, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::proc_amp::ProcAmpAnimation>},
 ColorReplace {parameters:super::color_replace::ColorReplace, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::color_replace::ReplaceColorAnimation>},
 AlphaAdjust {parameters:super::alpha_adjust::AlphaAdjust, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 UltraKey {parameters:super::ultra_key::UltraKey, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::ultra_key::UltraAnimation>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::ultra_key::UltraColorAnimation>},
 ColorKey {parameters:super::color_key::ColorKey, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_key::KeyAnimation>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::color_key::KeyColorAnimation>},
 ColorPass {parameters:super::color_pass::ColorPass, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::color_pass::PassColorAnimation>},
 ChangeToColor {parameters:super::change_to_color::ChangeToColor, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::change_to_color::ChangeAnimation>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::change_to_color::ChangeColorAnimation>},
 LeaveColor {parameters:super::leave_color::LeaveColor, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::leave_color::LeaveAnimation>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::leave_color::LeaveColorAnimation>},
 DirectionalBlur {parameters:super::directional_blur::DirectionalBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::directional_blur::DirectionalAnimation>},
 DirectionalBlurLegacy {parameters:super::directional_blur::DirectionalBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::directional_blur::DirectionalAnimation>},
 Mosaic {parameters:super::mosaic::Mosaic, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::mosaic::MosaicAnimation>},
 MosaicLegacy {parameters:super::mosaic::Mosaic, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::mosaic::MosaicAnimation>},
 Twirl {parameters:super::twirl::Twirl, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::twirl::TwirlAnimation>},
 TwirlLegacy {parameters:super::twirl::Twirl, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::twirl::TwirlAnimation>},
 WaveWarp {parameters:super::wave_warp::WaveWarp, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::wave_warp::WaveWarpAnimation>},
 LensDistortion {parameters:super::lens_distortion::LensDistortion, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::lens_distortion::LensDistortionAnimation>},
 Grid {parameters:super::grid::Grid, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::grid::GridAnimation>},
 LensFlare {parameters:super::lens_flare::LensFlare, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::lens_flare::FlareAnimation>},
 Ellipse {parameters:super::ellipse::Ellipse, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::ellipse::EllipseAnimation>},
 PaintBucket {parameters:super::paint_bucket::PaintBucket, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::paint_bucket::PaintAnimation>},
 Gradient {parameters:super::gradient::Gradient, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::gradient::GradientAnimation>},
 ColorEmboss {parameters:super::color_emboss::ColorEmboss, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_emboss::ColorEmbossAnimation>},
 BlockDissolve {parameters:super::block_dissolve::BlockDissolve, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::block_dissolve::BlockDissolveAnimation>},
 GraphicText {parameters:super::graphic_text::GraphicTextParameters, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::graphic_text_animation::GraphicTextAnimation>, #[serde(default,rename="sourceTextAnimation",skip_serializing_if="Option::is_none")] source_text_animation:Option<super::graphic_source_text_animation::SourceTextAnimation>},
 GraphicShape {parameters:super::graphic_shape::GraphicShape, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::graphic_shape_animation::GraphicShapeAnimation>},
 TrackMatte {parameters:super::track_matte::TrackMatte},
 GradientWipe {parameters:super::gradient_wipe::GradientWipe, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::gradient_wipe::GradientWipeAnimation>},
 LinearWipe {parameters:super::linear_wipe::LinearWipe, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::linear_wipe::LinearWipeAnimation>},
 Magnify {parameters:super::magnify::Magnify, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::magnify::MagnifyAnimation>},
 Mirror {parameters:super::mirror::Mirror, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::mirror::MirrorAnimation>},
 Offset {parameters:super::offset::Offset, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::offset::OffsetAnimation>},
 Transform {parameters:super::transform_effect::TransformEffect, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::transform_effect::TransformAnimation>},
 Spherize {parameters:super::spherize::Spherize, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::spherize::SpherizeAnimation>},
 CornerPin {parameters:super::corner_pin::CornerPin, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::corner_pin::CornerPinAnimation>},
 #[serde(rename="rotate_3d")]
 Rotate3D {parameters:super::rotate_3d::Rotate3D, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::rotate_3d::Rotate3DAnimation>},
 Shrink {parameters:super::grow_shrink::Shrink, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::grow_shrink::ScaleAnimation>},
 Grow {parameters:super::grow_shrink::GrowShrink, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::grow_shrink::ScaleAnimation>},
 Spin {parameters:super::spin::Spin, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::spin::SpinAnimation>},
 Move {parameters:super::move_effect::MoveEffect, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::move_effect::MoveAnimation>},
 Wiggle {parameters:super::wiggle::Wiggle, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::wiggle::WiggleAnimation>},
 #[serde(rename="camera_shake")]
 CameraShake {parameters:super::camera_shake::CameraShake, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::camera_shake::CameraShakeAnimation>},
 Spacer {parameters:super::spacer::Spacer, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::spacer::SpacerAnimation>},
 #[serde(rename="rounded_crop")]
 RoundedCrop {parameters:super::rounded_crop::RoundedCrop, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::rounded_crop::RoundedCropAnimation>},
 Clone {parameters:super::clone_effect::CloneEffect, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::clone_effect::CloneAnimation>},
 AutoAlign {parameters:super::auto_align::AutoAlign, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::auto_align::AutoAlignAnimation>},
 CompoundBlur {parameters:super::compound_blur::CompoundBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 Crop {parameters:super::crop::Crop, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::crop::CropAnimation>},
 BrightnessContrast {parameters:super::brightness_contrast::BrightnessContrast, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::brightness_contrast::BrightnessContrastAnimation>},
 AscCdl {parameters:super::asc_cdl::AscCdl, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::asc_cdl::AscCdlAnimation>},
 LogoCutout {parameters:super::logo_cutout::LogoCutout, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::logo_cutout::LogoCutoutAnimation>},
 EdgeGlow {parameters:super::edge_glow::EdgeGlow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::edge_glow::EdgeGlowAnimation>},
 WonderGlow {parameters:super::wonder_glow::WonderGlow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::wonder_glow::WonderGlowAnimation>},
 Glint {parameters:super::glint::Glint, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::glint::GlintAnimation>},
 WarpStabilizer {parameters:super::warp_stabilizer::WarpStabilizer,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::warp_stabilizer_animation::Animation>},
 VrChromaticAberrations {parameters:super::vr_chromatic::VrChromatic,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_chromatic::VrChromaticAnimation>},
 VrColorGradients {parameters:super::vr_color_gradients::VrColorGradients,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_color_gradients::VrColorGradientsAnimation>},
 Lightning {parameters:super::lightning::Lightning,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::lightning_animation::LightningAnimation>},
 WriteOn {parameters:super::write_on::WriteOn,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::write_on_animation::WriteOnAnimation>},
 Timecode {parameters:super::clock_label::Timecode,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::clock_label::TimecodeAnimation>},
 ClipName {parameters:super::clock_label::ClipName,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::clock_label::ClipNameAnimation>},
 MetadataBurnin {parameters:super::metadata_burnin::MetadataBurnin,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::metadata_burnin::MetadataBurninAnimation>},
 SimpleText {parameters:super::simple_text::SimpleText,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::simple_text::SimpleTextAnimation>},
 VrPlaneToSphere {parameters:super::vr_plane_to_sphere::VrPlaneToSphere,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_plane_to_sphere::VrPlaneAnimation>},
 VrGlow {parameters:super::vr_glow::VrGlow,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_glow::VrGlowAnimation>},
 VrFractalNoise {parameters:super::vr_fractal_noise::VrFractalNoise,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_fractal_noise::VrFractalNoiseAnimation>},
 VrDigitalGlitch {parameters:super::vr_digital_glitch::VrDigitalGlitch,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_digital_glitch::VrDigitalGlitchAnimation>},
 VrDenoise {parameters:super::vr_denoise::VrDenoise,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 VrBlur {parameters:super::vr_filter::VrBlur,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 VrSharpen {parameters:super::vr_filter::VrSharpen,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 SphericalVideo {parameters:super::spherical_video::SphericalVideo,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_sphere::VrSphereAnimation>},
 VrProjection {parameters:super::vr_sphere::VrSphere,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_sphere::VrSphereAnimation>},
 VrRotateSphere {parameters:super::vr_sphere::VrRotateSphere,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vr_sphere::VrSphereAnimation>},
 LightLeaks {parameters:super::light_leaks::LightLeaks,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::light_leaks::LightLeaksAnimation>},
 AutoReframe {parameters:super::auto_reframe::AutoReframe,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::auto_reframe::AutoReframeAnimation>},
 Echo {parameters:super::echo::Echo,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::echo::EchoAnimation>},
 PosterizeTime {parameters:super::posterize_time::PosterizeTime,#[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::posterize_time::PosterizeTimeAnimation>},
 VolumetricRays {parameters:super::volumetric_rays::VolumetricRays, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::volumetric_rays::VolumetricRaysAnimation>},
 EchoGlow {parameters:super::echo_glow::EchoGlow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::echo_glow::EchoGlowAnimation>},
 RgbSplit {parameters:super::rgb_split::RgbSplit, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::rgb_split::RgbSplitAnimation>},
 LightingEffects {parameters:super::lighting_effects::LightingEffects, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::lighting_effects::LightingAnimation>},
 #[serde(rename="vignette")] VignetteEffect {parameters:super::vignette_effect::VignetteEffect, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::vignette_effect::VignetteEffectAnimation>},
 UnsharpMask {parameters:super::unsharp_mask::UnsharpMask, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::unsharp_mask::UnsharpMaskAnimation>},
 BokehBlur {parameters:super::bokeh_blur::BokehBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::bokeh_blur::BokehBlurAnimation>},
 FocusBlur {parameters:super::focus_blur::FocusBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::focus_blur::FocusBlurAnimation>},
 EdgeFeather {parameters:super::edge_feather::EdgeFeather, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 Sharpen {parameters:super::sharpen::Sharpen, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 ReduceInterlaceFlicker {parameters:super::reduce_interlace_flicker::ReduceInterlaceFlicker, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 ChannelBlur {parameters:super::channel_blur::ChannelBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::channel_blur::ChannelBlurAnimation>},
 TurbulentDisplace {parameters:super::turbulent_displace::TurbulentDisplace, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::turbulent_displace::TurbulentDisplaceAnimation>},
 AlphaGlow {parameters:super::alpha_glow::AlphaGlow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::alpha_glow::AlphaGlowAnimation>},
 BrushStrokes {parameters:super::brush_strokes::BrushStrokes, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::brush_strokes::BrushStrokesAnimation>},
 Stroke {parameters:super::stroke::Stroke, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::stroke::StrokeAnimation>},
 LongShadow {parameters:super::long_shadow::LongShadow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::long_shadow::LongShadowAnimation>},
 RoughenEdges {parameters:super::roughen_edges::RoughenEdges, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::roughen_edges::RoughenEdgesAnimation>},
 CellPattern {parameters:super::cell_pattern::CellPattern, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::cell_pattern::CellAnimation>},
 Checkerboard {parameters:super::checkerboard::Checkerboard, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::checkerboard::CheckerAnimation>},
 Circle {parameters:super::circle::Circle, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::circle::CircleAnimation>},
 Ramp {parameters:super::ramp::Ramp, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::ramp::RampAnimation>},
 FourColorGradient {parameters:super::four_color_gradient::FourColorGradient, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::four_color_gradient::GradientAnimation>},
 BevelAlpha {parameters:super::bevel_alpha::BevelAlpha, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::bevel_alpha::BevelAnimation>},
 DropShadow {parameters:super::drop_shadow::DropShadow, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::drop_shadow::ShadowAnimation>},
 #[serde(rename="basic_3d")]
 Basic3D {parameters:super::basic_3d::Basic3D, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::basic_3d::Basic3DAnimation>},
 Strobe {parameters:super::strobe::Strobe, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::strobe::StrobeAnimation>},
 Noise {parameters:super::noise::Noise, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::noise::NoiseAnimation>},
 NoiseLegacy {parameters:super::noise::Noise, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::noise::NoiseAnimation>},
 Emboss {parameters:super::emboss::Emboss, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::emboss::EmbossAnimation>},
 CameraBlur {parameters:super::camera_blur::CameraBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 Median {parameters:super::median::Median, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 Replicate {parameters:super::replicate::Replicate, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 FindEdges {parameters:super::find_edges::FindEdges, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 GaussianBlur {parameters:super::gaussian_blur::GaussianBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 GaussianBlurLegacy {parameters:super::gaussian_blur::GaussianBlur, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 LumaKey {parameters:super::luma_key::LumaKey, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::luma_key::LumaAnimation>},
 ChannelMix {parameters:super::channel_mix::ChannelMix, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::channel_mix::MixAnimation>},
 ColorBalance {parameters:super::color_balance::ColorBalance, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_balance::BalanceAnimation>},
 ColorLevels {parameters:super::color_levels::ColorLevels, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_levels::LevelsAnimation>},
 ColorExtract {parameters:super::color_extract::ColorExtract, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_extract::ExtractAnimation>},
 HorizontalFlip,
 VerticalFlip,
 BlackWhite,
 GammaCorrection {parameters:super::gamma_correction::GammaCorrection, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 Posterize {parameters:super::posterize::Posterize, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 ColorInvert {parameters:super::color_invert::ColorInvert, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>},
 ColorTint {parameters:super::color_tint::ColorTint, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<AnimatableValue>, #[serde(default,rename="colorAnimation",skip_serializing_if="Option::is_none")] color_animation:Option<super::color_tint::TintColorAnimation>},
 CineonConverter {parameters:super::cineon_converter::CineonConverter, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::cineon_converter::CineonAnimation>},
 VideoLimiter {parameters:super::video_limiter::VideoLimiter, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::video_limiter::VideoLimiterAnimation>},
 BasicGrade {parameters:super::basic_grade::BasicGrade, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::basic_grade::BasicAnimation>},
    ColorVignette {parameters:super::color_vignette::ColorVignette, #[serde(default,skip_serializing_if="Option::is_none")] animation:Option<super::color_vignette::VignetteAnimation>},
    HslSecondary {parameters:super::hsl_secondary::HslSecondary,
        #[serde(default,skip_serializing_if="Option::is_none")]
        animation:Option<super::hsl_secondary::HslAnimation>,
    },
    ColorLut { lut: super::color_lut::ColorLut },
    CreativeGrade { parameters: super::creative_grade::CreativeGrade,
        #[serde(default, skip_serializing_if="Option::is_none")]
        animation: Option<super::creative_grade::CreativeAnimation>,
    },
    ToneCurves { curves: super::tone_curves::ToneCurves },
    ChromaMatte {
        parameters: super::chroma_surface::ChromaParameters,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        animation: Option<super::chroma_surface::ChromaAnimation>,
    },
    ChromaKey {
        color: Rgba,
        similarity: f64,
        softness: f64,
        spill: f64,
    },
    Style {
        preset: VideoEffectPreset,
        intensity: f64,
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
        /// Clockwise rotation around the mask centre. Defaults to zero
        /// for projects saved before rotated shape masks were introduced.
        #[serde(
            default = "default_mask_rotation",
            rename = "rotationDegrees",
            deserialize_with = "deserialize_animatable_or_number"
        )]
        rotation_degrees: AnimatableValue,
        /// Inward soft edge as a fraction of the mask radius/half-extent. Zero
        /// is a hard inclusive edge. For Rectangle, each axis ramps linearly
        /// from zero at its edge and the lower ramp wins; for Ellipse, radial
        /// distance ramps linearly. One feathers from the edge to the centre.
        feather: f64,
        /// Swap selected/rejected alpha, then multiply the clip's incoming
        /// alpha. Multiple masks multiply in declaration order before opacity.
        inverted: bool,
    },
    /// A half-plane mask whose boundary crosses `(x, y)`. Zero degrees keeps
    /// pixels to the left; positive degrees rotate the normal clockwise.
    LinearMask {
        x: AnimatableValue,
        y: AnimatableValue,
        #[serde(
            rename = "rotationDegrees",
            deserialize_with = "deserialize_animatable_or_number"
        )]
        rotation_degrees: AnimatableValue,
        feather: f64,
        inverted: bool,
    },
}

impl VideoEffect {
    pub(crate) fn validate(&self) -> Result<(), CompositionError> {
        match self {
            Self::NativeBypass{effect}=>{if matches!(effect.as_ref(),Self::NativeBypass{..}|Self::ConcatBypass{..}){return Err(CompositionError::InvalidEffect)}effect.validate()},
            Self::ConcatBypass{link}=>super::concat_effect_upgrade::validate_bypassed(link).map_err(|_|CompositionError::InvalidEffect),
            Self::Lumetri {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatGaussianBlur {parameters,animation,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_gaussian_blur::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if !keys.is_empty() && animation.is_some(){return Err(CompositionError::InvalidEffect);}}if let Some(a)=animation{super::concat_gaussian_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatMotionBlur {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_motion_blur::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatBloomPulse {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_bloom_pulse::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatZoomBlur {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_zoom_blur::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatFilmGrain {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_film_grain::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatGlow {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_glow::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatHalftone {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_halftone::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatFisheye {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_fisheye::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatChromaticAberration {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys {super::concat_chromatic_aberration::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatPosterize {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatLumaKey {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatPixelate {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatChromaKey {parameters,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys{super::concat_chroma_key::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatSaturation {parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys{super::concat_chroma::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if !keys.is_empty()&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::concat_chroma::validate_saturation_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatVibrance {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::concat_chroma::validate_vibrance_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatVhs {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatFallingDust {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatFilm {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatDigicam {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatMono {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatMidnight {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatMarquee {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatRetroCartoon {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatSharpen {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatCool {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatWarm {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatTealOrange {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatNight {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatSpooky {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatNeon {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatPinkFuzz {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatSuper8 {..} | Self::ConcatBleachBypass {..} | Self::ConcatAutumn {..} | Self::ConcatNoir {..} | Self::ConcatDreamy {..} | Self::ConcatHighContrastMono {..} | Self::ConcatLandscapePop {..} | Self::ConcatNatural {..} | Self::ConcatRetro70s {..} | Self::ConcatSilver {..} | Self::ConcatTropical {..} | Self::ConcatVaporwave {..} | Self::ConcatWinterCool {..} | Self::ConcatClassicChrome {..} | Self::ConcatCyberpunk {..} | Self::ConcatDelicious {..} | Self::ConcatRosy {..} | Self::ConcatSciFi {..}=>Ok(()),
            Self::ConcatMatcha {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatSage {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatIvory {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatMilk {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatBronze {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatCoolBlue {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatFresh {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatVivid {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatSoftLight {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatMoody {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatRain {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatUmber {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatFair {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatPeach {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatPolaroid {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatVintage {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatCloudlessSky {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatLagoon {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatGoldenHour {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatDaydream {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatClean {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatContrastPop {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatVignette {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatAdjust {parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys{super::concat_color_wheels::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if keys.values().any(|run|!run.is_empty())&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::concat_color_wheels::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatCurves {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::ConcatColorWheels {parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys{super::concat_color_wheels::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if keys.values().any(|run|!run.is_empty())&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::concat_color_wheels::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatWhiteBalance {parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys{super::concat_white_balance::validate_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if keys.values().any(|run|!run.is_empty())&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::concat_white_balance::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatExposure {parameters,animation,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys{super::concat_exposure::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if !keys.is_empty()&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}if let Some(a)=animation{super::concat_exposure::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ConcatContrast {parameters,animation,concat_keys}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(keys)=concat_keys{super::concat_contrast::validate_concat_keys(keys).map_err(|_|CompositionError::InvalidEffect)?;if !keys.is_empty()&&animation.is_some(){return Err(CompositionError::InvalidEffect);}}if let Some(a)=animation{super::concat_contrast::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorSharpen {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::color_sharpen::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CreativeAdjust {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::creative_adjust::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ProcAmp {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::proc_amp::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorLevels {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_levels::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorExtract {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_extract::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::HorizontalFlip|Self::VerticalFlip=>Ok(()),
            Self::BlackWhite=>Ok(()),
            Self::GammaCorrection {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::gamma_correction::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Posterize {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::posterize::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorInvert {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_invert::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorReplace {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_replace::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::color_replace::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::UltraKey {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::ultra_key::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::ultra_key::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorKey {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_key::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::color_key::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorPass {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_pass::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::color_pass::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ChangeToColor {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::change_to_color::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::change_to_color::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LeaveColor {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::leave_color::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::leave_color::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::AlphaAdjust {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::alpha_adjust::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::DirectionalBlur {parameters,animation} | Self::DirectionalBlurLegacy {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::directional_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Mosaic {parameters,animation} | Self::MosaicLegacy {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::mosaic::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Twirl {parameters,animation} | Self::TwirlLegacy {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::twirl::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::WaveWarp {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::wave_warp::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LensDistortion {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::lens_distortion::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Grid {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::grid::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LensFlare {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::lens_flare::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Ellipse {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::ellipse::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::PaintBucket {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::paint_bucket::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Gradient {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::gradient::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorEmboss {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_emboss::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BlockDissolve {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::block_dissolve::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::GraphicText {parameters,animation,source_text_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::graphic_text_animation::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=source_text_animation{a.validate().map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::GraphicShape {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::graphic_shape_animation::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::TrackMatte {parameters}=>parameters.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::GradientWipe {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::gradient_wipe::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LinearWipe {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::linear_wipe::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Magnify {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::magnify::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Mirror {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::mirror::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Offset {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::offset::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Transform {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::transform_effect::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Spherize {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::spherize::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CornerPin {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::corner_pin::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Rotate3D {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::rotate_3d::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Shrink {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::grow_shrink::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Grow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::grow_shrink::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Spin {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::spin::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Move {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::move_effect::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Wiggle {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::wiggle::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CameraShake {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::camera_shake::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Spacer {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::spacer::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::RoundedCrop {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::rounded_crop::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Clone {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::clone_effect::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::AutoAlign {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::auto_align::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CompoundBlur {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::compound_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Crop {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::crop::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BrightnessContrast {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::brightness_contrast::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::AscCdl {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::asc_cdl::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LogoCutout {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::logo_cutout::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::EdgeGlow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::edge_glow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::WonderGlow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::wonder_glow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Glint {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::glint::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::WarpStabilizer{parameters,animation}=>{if !parameters.is_valid(){return Err(CompositionError::InvalidEffect);}if let Some(a)=animation{super::warp_stabilizer_animation::validate(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrChromaticAberrations{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_chromatic::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrColorGradients{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_color_gradients::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::Lightning{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::lightning_animation::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::WriteOn{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::write_on_animation::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::Timecode{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::clock_label::validate_timecode_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::ClipName{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::clock_label::validate_clip_name_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::MetadataBurnin{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::metadata_burnin::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::SimpleText{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::simple_text::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrPlaneToSphere{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_plane_to_sphere::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrGlow{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_glow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrFractalNoise{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_fractal_noise::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrDigitalGlitch{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_digital_glitch::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrDenoise{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_denoise::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrBlur{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_filter::validate_animation(a,false).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrSharpen{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_filter::validate_animation(a,true).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::SphericalVideo{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_sphere::validate_animation(a,false).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrProjection{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_sphere::validate_animation(a,false).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::VrRotateSphere{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::vr_sphere::validate_animation(a,true).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::LightLeaks{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::light_leaks::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::AutoReframe{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::auto_reframe::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::Echo{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::echo::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
 Self::PosterizeTime{parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::posterize_time::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::VolumetricRays {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::volumetric_rays::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::EchoGlow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::echo_glow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::RgbSplit {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::rgb_split::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LightingEffects {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::lighting_effects::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::VignetteEffect {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::vignette_effect::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::UnsharpMask {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::unsharp_mask::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BokehBlur {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::bokeh_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::FocusBlur {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::focus_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::EdgeFeather {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::edge_feather::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Sharpen {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::sharpen::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ReduceInterlaceFlicker {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::reduce_interlace_flicker::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ChannelBlur {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::channel_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::TurbulentDisplace {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::turbulent_displace::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::AlphaGlow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::alpha_glow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BrushStrokes {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::brush_strokes::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Stroke {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::stroke::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LongShadow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::long_shadow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::RoughenEdges {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::roughen_edges::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CellPattern {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::cell_pattern::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Checkerboard {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::checkerboard::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Circle {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::circle::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Ramp {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::ramp::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::FourColorGradient {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::four_color_gradient::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BevelAlpha {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::bevel_alpha::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::DropShadow {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::drop_shadow::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Basic3D {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::basic_3d::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Strobe {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::strobe::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Noise {parameters,animation} | Self::NoiseLegacy {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::noise::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Emboss {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::emboss::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CameraBlur {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::camera_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Median {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::median::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::Replicate {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::replicate::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::FindEdges {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::find_edges::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::GaussianBlur {parameters,animation} | Self::GaussianBlurLegacy {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::gaussian_blur::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::LumaKey {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::luma_key::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ChannelMix {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::channel_mix::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorBalance {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_balance::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorTint {parameters,animation,color_animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::color_tint::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}if let Some(a)=color_animation {super::color_tint::validate_color_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::CineonConverter {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::cineon_converter::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::VideoLimiter {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation {super::video_limiter::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::BasicGrade {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(a)=animation{super::basic_grade::validate_animation(a).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::ColorVignette {parameters,animation}=>{parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;if let Some(animation)=animation {super::color_vignette::validate_animation(animation).map_err(|_|CompositionError::InvalidEffect)?;}Ok(())},
            Self::HslSecondary {parameters,animation}=>{
                parameters.validate().map_err(|_|CompositionError::InvalidEffect)?;
                if let Some(animation)=animation {super::hsl_secondary::validate_animation(animation).map_err(|_|CompositionError::InvalidEffect)?;}
                Ok(())
            },
            Self::ColorLut {lut} => lut.validate().map_err(|_|CompositionError::InvalidEffect),
            Self::CreativeGrade { parameters, animation } => {
                parameters.validate().map_err(|_| CompositionError::InvalidEffect)?;
                if let Some(animation)=animation {animation.validate().map_err(|_| CompositionError::InvalidEffect)?;}
                Ok(())
            },
            Self::ToneCurves { curves } => curves.validate().map_err(|_| CompositionError::InvalidEffect),
            Self::ChromaMatte { parameters, animation } => {
                parameters.validate().map_err(|_| CompositionError::InvalidEffect)?;
                if let Some(animation) = animation {
                    animation.validate().map_err(|_| CompositionError::InvalidEffect)?;
                }
                Ok(())
            },
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
            Self::Style { intensity, .. } => {
                if intensity.is_finite() && (0.01..=1.0).contains(intensity) {
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
                rotation_degrees,
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
                    && animatable_values_in(rotation_degrees, -180.0, 180.0, true)
                    && feather.is_finite()
                    && (0.0..=1.0).contains(feather)
                {
                    Ok(())
                } else {
                    Err(CompositionError::InvalidEffect)
                }
            }
            Self::LinearMask {
                x,
                y,
                rotation_degrees,
                feather,
                ..
            } => {
                x.validate()?;
                y.validate()?;
                if animatable_values_in(x, 0.0, 1.0, true)
                    && animatable_values_in(y, 0.0, 1.0, true)
                    && animatable_values_in(rotation_degrees, -180.0, 180.0, true)
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

pub(crate) fn animatable_values_in(
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
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub concat_cutout: Option<super::concat_video_binding::Binding>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub audio_stream:Option<u32>,

    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub concat_gain_keys:Vec<super::concat_effect_upgrade::ParamKey>,

    #[serde(default,skip_serializing_if="transition_duration_is_zero")]
    pub audio_fade_in_ticks:u64,
    #[serde(default,skip_serializing_if="transition_duration_is_zero")]
    pub audio_fade_out_ticks:u64,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub dsp_effects:Vec<super::filmcraft_dsp::EffectSpec>,
    #[serde(default,skip_serializing_if="std::ops::Not::not")]
    pub independent_audio:bool,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub filmcraft_motion:Option<super::filmcraft_motion::ClipMotion>,
    /// Imported Filmcraft effects use media time; missing preserves native clip-local curves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_timing: Option<super::video_effect_timing::EffectTiming>,
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
    /// ChromaKey effects followed by Rectangle/Ellipse/Linear masks; unsupported
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
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub audio_stream:Option<u32>,

    #[serde(default,skip_serializing_if="Option::is_none")]
    pub detached_from:Option<CompositionClipId>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub concat_detached_order:Option<u32>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub concat_gain_keys:Vec<super::concat_effect_upgrade::ParamKey>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dsp_effects: Vec<super::filmcraft_dsp::EffectSpec>,
    #[serde(default = "default_role_gain")]
    pub role_gain: f64,
    pub id: CompositionClipId,
    pub source_id: SourceId,
    pub placement: ClipPlacement,
    pub gain: AnimatableValue,
    pub pan: AnimatableValue,
    #[serde(default)]
    pub reversed: bool,
    pub fade_in_ticks: u64,
    pub fade_out_ticks: u64,
    #[serde(default)]
    pub voice_effect: AudioVoiceEffect,
    #[serde(default)]
    pub pitch_semitones: f64,
    #[serde(default)]
    pub tone_db: f64,
    #[serde(default)]
    pub crossfade_in_ticks: u64,
    #[serde(default)]
    pub crossfade_curve: super::audio_transition::AudioTransitionCurve,
    #[serde(default)]
    pub ducking: Option<AudioDucking>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioDucking {
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
}

impl AudioDucking {
    fn validate(self) -> Result<(), CompositionError> {
        if self.threshold_db.is_finite()
            && (-60.0..=0.0).contains(&self.threshold_db)
            && self.ratio.is_finite()
            && (1.0..=20.0).contains(&self.ratio)
            && self.attack_ms.is_finite()
            && (0.1..=500.0).contains(&self.attack_ms)
            && self.release_ms.is_finite()
            && (1.0..=5_000.0).contains(&self.release_ms)
        {
            Ok(())
        } else {
            Err(CompositionError::InvalidAudioDucking)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioVoiceEffect {
    #[default]
    None,
    Deep,
    High,
    Chipmunk,
    Echo,
    Robot,
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
    #[serde(default)]
    pub effects: Vec<VideoEffect>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment { Left, Center, Right }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<TextAlignment>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<VideoEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<BlendMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concat_title: Option<super::concat_title::TitleStyle>,
    pub id: CompositionClipId,
    pub timeline_start_tick: u64,
    pub timeline_end_tick: u64,
    pub text: String,
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_animation: Option<super::animated_color::ColorTrack>,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Source-free rectangle; color interpolation remains owned by native RGBA tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SolidColorClip {
    pub id: CompositionClipId,
    pub timeline_start_tick: u64,
    pub timeline_end_tick: u64,
    pub width: u32,
    pub height: u32,
    pub color: super::animated_color::Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_animation: Option<super::animated_color::ColorTrack>,
    pub transform: TransformSpec,
    pub opacity: AnimatableValue,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl SolidColorClip {
    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.timeline_end_tick <= self.timeline_start_tick
            || self.timeline_end_tick > 9_007_199_254_740_991
        {
            return Err(CompositionError::InvalidPlacement);
        }
        if self.width == 0 || self.height == 0 || self.width > 3840 || self.height > 2160 {
            return Err(CompositionError::InvalidCanvas);
        }
        self.transform.validate()?;
        self.opacity.validate()?;
        if !animatable_values_in(&self.opacity, 0.0, 1.0, true) {
            return Err(CompositionError::InvalidAnimation);
        }
        if let Some(track) = &self.color_animation {
            super::animated_color::PreparedColorTrack::new(super::animated_color::ColorSampleRequest {
                track: track.clone(), default: self.color, times_us: Vec::new(),
            }).map_err(|_| CompositionError::InvalidAnimation)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod solid_color_model_tests {
    use super::*;

    #[test]
    fn solid_color_shape_and_animation_roundtrip_and_reject_invalid_admission() {
        let mut clip = SolidColorClip {
            id: CompositionClipId::parse("solid").unwrap(),
            timeline_start_tick: 500_000, timeline_end_tick: 1_500_000,
            width: 63, height: 31,
            color: super::super::animated_color::Color { r: 12, g: 34, b: 56, a: 128 },
            color_animation: None, transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.75), enabled: true,
        };
        clip.validate().unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        composition.tracks.push(CompositionTrack::Color {
            id: TrackId::parse("colors").unwrap(), name: "Colors".into(), hidden: false,
            locked: false, clips: vec![clip.clone()],
        });
        composition.validate().unwrap();
        assert_eq!(composition.duration_ticks().unwrap(), 1_500_000);
        assert!(composition.sources.is_empty());
        assert_eq!(composition.tracks[0].id().as_str(), "colors");
        let wire = serde_json::to_value(&composition).unwrap();
        assert_eq!(wire["tracks"][0]["kind"], "color");
        assert_eq!(serde_json::from_value::<Composition>(wire).unwrap(), composition);
        let CompositionTrack::Color { clips, .. } = &mut composition.tracks[0] else { unreachable!() };
        clips.push(clip.clone());
        assert_eq!(composition.validate(), Err(CompositionError::DuplicateClip(clip.id.clone())));
        let wire = serde_json::to_value(&clip).unwrap();
        assert!(wire.get("colorAnimation").is_none());
        assert_eq!(serde_json::from_value::<SolidColorClip>(wire.clone()).unwrap(), clip);
        let mut invalid_wire = wire.clone();
        invalid_wire["color"]["a"] = serde_json::json!(256);
        assert!(serde_json::from_value::<SolidColorClip>(invalid_wire).is_err());
        let mut invalid_wire = wire;
        invalid_wire["sourceId"] = serde_json::json!("source");
        assert!(serde_json::from_value::<SolidColorClip>(invalid_wire).is_err());
        clip.color_animation = Some(serde_json::from_value(serde_json::json!({
            "keys":[{"t_us":-0.5,"value":{"r":255,"g":0,"b":0,"a":128},
                "in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}],
            "extrapolate":{"before":"PingPong","after":"Offset"}
        })).unwrap());
        clip.validate().unwrap();
        assert_eq!(serde_json::from_value::<SolidColorClip>(serde_json::to_value(&clip).unwrap()).unwrap(), clip);
        for (width, height) in [(0, 31), (63, 0), (3841, 31), (63, 2161)] {
            let mut invalid = clip.clone(); invalid.width = width; invalid.height = height;
            assert_eq!(invalid.validate(), Err(CompositionError::InvalidCanvas));
        }
        let mut invalid = clip.clone(); invalid.timeline_end_tick = invalid.timeline_start_tick;
        assert_eq!(invalid.validate(), Err(CompositionError::InvalidPlacement));
        let mut invalid = clip.clone(); invalid.opacity = AnimatableValue::constant(1.1);
        assert_eq!(invalid.validate(), Err(CompositionError::InvalidAnimation));
        let mut invalid = clip.clone(); invalid.color_animation.as_mut().unwrap().keys[0].t_us = f64::NAN;
        assert_eq!(invalid.validate(), Err(CompositionError::InvalidAnimation));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    ConcatDissolve,
    ConcatFadeBlack,
    ConcatFadeWhite,
    Procedural,
    Dissolve,
    FadeBlack,
    FilmDissolve,
    AdditiveDissolve,
    FadeWhite,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    SmoothLeft,
    SmoothRight,
    SmoothUp,
    SmoothDown,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    CircleOpen,
    CircleClose,
    WipeTopLeft,
    WipeTopRight,
    WipeBottomLeft,
    WipeBottomRight,
    VerticalOpen,
    VerticalClose,
    HorizontalOpen,
    HorizontalClose,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipTransition {
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub concat_overlap: Option<super::concat_project_transitions::SourceOverlap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel: Option<super::transition_spec::TransitionSpec>,
    #[serde(default, skip_serializing_if = "transition_reverse_is_false")]
    pub reverse: bool,
    pub id: TransitionId,
    #[serde(default)]
    pub from_clip_id: Option<CompositionClipId>,
    #[serde(default)]
    pub to_clip_id: Option<CompositionClipId>,
    #[serde(default, skip_serializing_if = "transition_duration_is_zero")]
    pub duration_ticks: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeline: Option<super::transition_timeline::TransitionTimeline>,
    pub kind: TransitionKind,
}
fn transition_reverse_is_false(value: &bool) -> bool { !*value }
fn transition_duration_is_zero(value: &u64) -> bool { *value == 0 }
impl ClipTransition {
    pub fn insert_explicit_gap(&self, at: i64, duration: i64, splits: &[(CompositionClipId, CompositionClipId)])
        -> Result<Option<Self>, super::transition_gap::TransitionGapError> {
        let timeline = self.timeline.ok_or(super::transition_gap::TransitionGapError::InvalidDuration)?;
        let Some(gap) = timeline.gap(self.from_clip_id.clone(), self.to_clip_id.clone())?.insert(at, duration, splits)? else { return Ok(None) };
        Ok(Some(Self { timeline: Some(super::transition_timeline::TransitionTimeline::from_gap(&gap)),
            from_clip_id: gap.from, to_clip_id: gap.to, ..self.clone() }))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompositionTrack {
    Graphic {
        id: TrackId,
        name: String,
        #[serde(default)] hidden: bool,
        #[serde(default)] locked: bool,
        #[serde(default)] clips: Vec<super::graphic_timeline::GraphicTimelineClip>,
    },
    Color {
        id: TrackId,
        name: String,
        #[serde(default)]
        hidden: bool,
        #[serde(default)]
        locked: bool,
        #[serde(default)]
        clips: Vec<SolidColorClip>,
    },
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
            Self::Graphic { id, .. } | Self::Color { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Composition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::metadata_burnin::CompositionMetadata>,
    pub schema_version: u32,
    pub time_base: u32,
    pub canvas: CanvasSpec,
    pub sources: BTreeMap<SourceId, CompositionSource>,
    pub tracks: Vec<CompositionTrack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caption_tracks: Vec<super::caption_timeline::CaptionTrack>,
}

impl Composition {
    pub fn new(canvas: CanvasSpec) -> Self {
        Self {
            metadata: None,
            schema_version: COMPOSITION_SCHEMA_VERSION,
            time_base: DEFAULT_TIME_BASE,
            canvas,
            sources: BTreeMap::new(),
            tracks: Vec::new(),
            caption_tracks: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), CompositionError> {
        self.validate_for_surface(false)
    }

    /// Native transition sampling clamps source handles and supports reverse endpoints.
    /// Keep the ordinary exporter admission unchanged until its reverse path is qualified.
    pub fn validate_native_surface(&self) -> Result<(), CompositionError> {
        self.validate_for_surface(true)
    }

    fn validate_for_surface(&self, native: bool) -> Result<(), CompositionError> {
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
        if self.tracks.len().saturating_add(self.caption_tracks.len()) > MAX_TRACKS {
            return Err(CompositionError::TooManyTracks);
        }
        super::concat_lane_groups::groups(self).map_err(|_|CompositionError::InvalidLaneGroups)?;
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
        for track in &self.caption_tracks {
            track.validate().map_err(|_|CompositionError::InvalidText)?;
            if !track_ids.insert(&track.id) {return Err(CompositionError::DuplicateTrack(track.id.clone()));}
            clip_count=clip_count.saturating_add(track.clips.len());
            if !track.hidden {visible_layers=visible_layers.saturating_add(1);}
            for cue in &track.clips {
                if !clip_ids.insert(&cue.id) {return Err(CompositionError::DuplicateClip(cue.id.clone()));}
                output_end=output_end.max(cue.timeline_end_tick.microseconds_ceil().map_err(|_|CompositionError::InvalidText)?);
            }
        }
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
                        if (clip.placement.source_timeline_offset_tick.unwrap_or(0) != 0 || clip.placement.source_head_timeline_tick.unwrap_or(0) != 0) && (!native || clip.effect_timing.is_some()) {
                            return Err(CompositionError::InvalidPlacement);
                        }
                        if let Some(binding)=&clip.concat_cutout {
                            binding.validate().map_err(|_|CompositionError::InvalidEffect)?;
                            for track in &binding.region_tracks {
                                if track.snapshot.source_id!=clip.source_id.as_str(){return Err(CompositionError::InvalidEffect);}
                                let id=SourceId::parse(track.snapshot.mask_source_id.clone())?;
                                let region=self.sources.get(&id).ok_or_else(||CompositionError::InvalidSource(id.clone()))?;
                                if region.kind!=SourceKind::Video||region.has_audio||(region.width,region.height)!=(256,256){return Err(CompositionError::InvalidEffect);}
                            }
                        }
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
                            clip.playback_mode == PlaybackMode::Reverse,
                        )?;
                        if let Some(motion)=&clip.filmcraft_motion {
                            motion.parameters.validate().map_err(|_|CompositionError::InvalidTransform)?;
                            if clip.effect_timing.is_none() || clip.transform!=TransformSpec::default() {return Err(CompositionError::InvalidTransform);}
                        }
                        clip.transform.validate()?;
                        clip.opacity.validate()?;
                        super::filmcraft_dsp::validate_specs(&clip.dsp_effects).map_err(|_|CompositionError::InvalidEffect)?;
                        if clip.audio_fade_in_ticks>MAX_OUTPUT_TICKS || clip.audio_fade_out_ticks>MAX_OUTPUT_TICKS {return Err(CompositionError::InvalidAudioFade);}
                        validate_concat_gain_keys(&clip.concat_gain_keys,&clip.audio_gain)?;
                        clip.audio_gain.validate()?;
                        clip.audio_pan.validate()?;
                        if clip
                            .effects
                            .iter()
                            .filter(|effect| matches!(effect, VideoEffect::Style { .. }))
                            .count()
                            > MAX_STYLE_EFFECTS_PER_CLIP
                        {
                            return Err(CompositionError::InvalidEffect);
                        }
                        for effect in &clip.effects {
                            effect.validate()?;
                        }
                        if !local_ids.insert(&clip.id) || !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.placement.timeline_end_tick()?);
                    }
                    validate_transitions_for_surface(transitions, clips, &self.sources, native)?;
                    if transitions.iter().any(|transition|transition.concat_overlap.is_some()){super::concat_project_transitions::validate_audio_overlaps(clips,transitions,&self.sources,&self.canvas).map_err(|_|CompositionError::InvalidTransition(transitions[0].id.clone()))?;}
                }
                CompositionTrack::Audio { clips, .. } => {
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        if !native && (clip.placement.source_timeline_offset_tick.unwrap_or(0) != 0 || clip.placement.source_head_timeline_tick.unwrap_or(0) != 0) { return Err(CompositionError::InvalidPlacement); }
                        validate_media_clip(
                            &self.sources,
                            &clip.id,
                            &clip.source_id,
                            &clip.placement,
                            false,
                            clip.reversed,
                        )?;
                        clip.gain.validate()?;
                        if !clip.role_gain.is_finite()
                            || !(0.0..=f64::MAX / 16.0).contains(&clip.role_gain)
                        {
                            return Err(CompositionError::InvalidAudioRoleGain);
                        }
                        super::filmcraft_dsp::validate_specs(&clip.dsp_effects)
                            .map_err(|_| CompositionError::InvalidEffect)?;
                        validate_concat_gain_keys(&clip.concat_gain_keys,&clip.gain)?;
                        clip.pan.validate()?;
                        if !clip.pitch_semitones.is_finite()
                            || !(-12.0..=12.0).contains(&clip.pitch_semitones)
                        {
                            return Err(CompositionError::InvalidAudioPitch);
                        }
                        if !clip.tone_db.is_finite() || !(-12.0..=12.0).contains(&clip.tone_db) {
                            return Err(CompositionError::InvalidAudioTone);
                        }
                        if clip.fade_in_ticks > MAX_OUTPUT_TICKS || clip.fade_out_ticks > MAX_OUTPUT_TICKS {
                            return Err(CompositionError::InvalidAudioFade);
                        }
                        if let Some(ducking) = clip.ducking {
                            ducking.validate()?;
                        }
                        if !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.placement.timeline_end_tick()?);
                    }
                    validate_audio_crossfades(clips, &self.sources)?;
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
                        if clip.effects.iter().filter(|effect|matches!(effect,VideoEffect::Style{..})).count()>MAX_STYLE_EFFECTS_PER_CLIP {return Err(CompositionError::InvalidEffect);}
                        for effect in &clip.effects {effect.validate()?;}
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
                CompositionTrack::Graphic { clips, hidden, .. } => {
                    if !hidden { visible_layers = visible_layers.saturating_add(1); }
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        clip.validate().map_err(|_|CompositionError::InvalidEffect)?;
                        if !clip_ids.insert(&clip.id) {return Err(CompositionError::DuplicateClip(clip.id.clone()));}
                        output_end=output_end.max(clip.timeline_end_tick);
                    }
                }
                CompositionTrack::Color { clips, hidden, .. } => {
                    if !hidden { visible_layers = visible_layers.saturating_add(1); }
                    clip_count = clip_count.saturating_add(clips.len());
                    for clip in clips {
                        clip.validate()?;
                        if !clip_ids.insert(&clip.id) {
                            return Err(CompositionError::DuplicateClip(clip.id.clone()));
                        }
                        output_end = output_end.max(clip.timeline_end_tick);
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
                        if (!clip.effects.is_empty()&&clip.concat_title.is_none()) || clip.effects.iter().filter(|effect|matches!(effect,VideoEffect::Style{..})).count()>MAX_STYLE_EFFECTS_PER_CLIP {return Err(CompositionError::InvalidEffect);}
                        for effect in &clip.effects {effect.validate()?;}
                        if let Some(title)=&clip.concat_title {title.validate().map_err(|_|CompositionError::InvalidText)?;if clip.blend_mode.is_some_and(|mode|!matches!(mode,BlendMode::Normal|BlendMode::Multiply|BlendMode::Screen|BlendMode::Addition|BlendMode::Lighten|BlendMode::Darken)){return Err(CompositionError::InvalidText);}}
                        if let Some(track) = &clip.color_animation {
                            super::animated_color::PreparedColorTrack::new(
                                super::animated_color::ColorSampleRequest {
                                    track: track.clone(),
                                    default: super::animated_color::Color { r: 0, g: 0, b: 0, a: 255 },
                                    times_us: Vec::new(),
                                },
                            ).map_err(|_| CompositionError::InvalidAnimation)?;
                        }
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
        if let Some(metadata)=&self.metadata {
            super::metadata_burnin::validate_markers(&metadata.markers).map_err(|_|CompositionError::InvalidEffect)?;
            for (clip_id,pair) in &metadata.media_versions {
                if let Some(video)=&pair.video_cutout {
                    video.validate().map_err(|_|CompositionError::InvalidEffect)?;
                    for region in &video.tracked_regions {
                        let region_id=SourceId::parse(region.mask_source_id.clone())?;
                        let source_id=SourceId::parse(region.source_id.clone())?;
                        let region_source=self.sources.get(&region_id).ok_or_else(||CompositionError::InvalidSource(region_id.clone()))?;
                        let input=self.sources.get(&source_id).ok_or_else(||CompositionError::InvalidSource(source_id.clone()))?;
                        if region_source.kind!=SourceKind::Video || region_source.has_audio || (region_source.width,region_source.height)!=(256,256) || input.kind!=SourceKind::Video || ![(video.width,video.height),(video.height,video.width)].contains(&(input.width,input.height)) || region_id==pair.copy_source_id || source_id==pair.copy_source_id {return Err(CompositionError::InvalidEffect);}
                    }

                    if pair.cutout.is_some(){return Err(CompositionError::InvalidEffect);}
                    let source=SourceId::parse(video.source_id.clone())?;let mask=SourceId::parse(video.mask_source_id.clone())?;
                    let input=self.sources.get(&source).ok_or_else(||CompositionError::InvalidSource(source.clone()))?;
                    let masks=self.sources.get(&mask).ok_or_else(||CompositionError::InvalidSource(mask.clone()))?;
                    let copy=self.sources.get(&pair.copy_source_id).ok_or_else(||CompositionError::InvalidSource(pair.copy_source_id.clone()))?;
                    let original=self.sources.get(&pair.original_source_id).ok_or_else(||CompositionError::InvalidSource(pair.original_source_id.clone()))?;
                    let current=self.tracks.iter().find_map(|track|match track {CompositionTrack::Video{clips,..}=>clips.iter().find(|c|&c.id==clip_id).map(|c|&c.source_id),CompositionTrack::Audio{clips,..}=>clips.iter().find(|c|&c.id==clip_id).map(|c|&c.source_id),_=>None}).ok_or(CompositionError::InvalidEffect)?;
                    if input.kind!=SourceKind::Video||original.kind!=SourceKind::Video||copy.kind!=SourceKind::Video||(copy.width,copy.height)!=(video.width,video.height)||mask==pair.original_source_id||masks.kind!=SourceKind::Video||masks.has_audio||![(video.width,video.height),(video.height,video.width)].contains(&(input.width,input.height))||masks.width!=video.mask_width||masks.height!=video.mask_height||source==pair.copy_source_id||mask==pair.copy_source_id||!self.sources.contains_key(&pair.original_source_id)||!self.sources.contains_key(&pair.copy_source_id)||(*current!=pair.original_source_id&&*current!=pair.copy_source_id){return Err(CompositionError::InvalidEffect);}
                }
                if let Some(cutout)=&pair.cutout {
                    cutout.validate().map_err(|_|CompositionError::InvalidEffect)?;
                    let source=SourceId::parse(cutout.source_id.clone())?;let mask=SourceId::parse(cutout.mask_source_id.clone())?;
                    let input=self.sources.get(&source).ok_or_else(||CompositionError::InvalidSource(source.clone()))?;
                    let mask_source=self.sources.get(&mask).ok_or_else(||CompositionError::InvalidSource(mask.clone()))?;
                    for region in cutout.correction.strokes.iter().filter_map(|s|s.region.as_ref()) {let id=SourceId::parse(region.mask_source_id.clone())?;let entry=self.sources.get(&id).ok_or_else(||CompositionError::InvalidSource(id.clone()))?;if entry.kind!=SourceKind::Image||entry.width!=256||entry.height!=256||id==pair.copy_source_id{return Err(CompositionError::InvalidEffect);}}
                    let image=self.tracks.iter().find_map(|track|if let CompositionTrack::Image{clips,..}=track{clips.iter().find(|c|&c.id==clip_id)}else{None}).ok_or(CompositionError::InvalidEffect)?;
                    if source==pair.copy_source_id||mask==pair.copy_source_id||input.kind!=SourceKind::Image||mask_source.kind!=SourceKind::Image||(mask_source.width,mask_source.height)!=super::cutout::mask_dimensions(&cutout.model_hash,input.width,input.height).map_err(|_|CompositionError::InvalidEffect)?||!self.sources.contains_key(&pair.original_source_id)||!self.sources.contains_key(&pair.copy_source_id)||(image.source_id!=pair.original_source_id&&image.source_id!=pair.copy_source_id){return Err(CompositionError::InvalidEffect);}
                }
            }
            let mut linked=BTreeSet::new();
            if metadata.clip_links.len()>MAX_CLIPS {return Err(CompositionError::InvalidClipLinks);}
            for (id,members) in &metadata.clip_links {
                if CompositionClipId::parse(id.clone()).is_err() || members.len()<2 || members.len()>MAX_CLIPS {return Err(CompositionError::InvalidClipLinks);}
                for member in members {
                    if !clip_ids.iter().any(|id|*id==member) || self.caption_tracks.iter().flat_map(|track|&track.clips).any(|cue|&cue.id==member) || !linked.insert(member) {return Err(CompositionError::InvalidClipLinks);}
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

    /// Whether this already-validated document needs a source-derived canvas.
    pub fn requires_canvas_blur(&self) -> bool {
        self.canvas.background_mode == CanvasBackgroundMode::Blur
    }

    pub fn required_style_effect_filters(&self) -> BTreeSet<&'static str> {
        self.tracks
            .iter()
            .filter_map(|track| match track {
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } => Some(clips),
                _ => None,
            })
            .flat_map(|clips| clips.iter().filter(|clip| clip.enabled))
            .flat_map(|clip| clip.effects.iter())
            .filter_map(|effect| match effect {
                VideoEffect::Style { preset, .. } => Some(match preset {
                    VideoEffectPreset::Blur => "gblur",
                    VideoEffectPreset::Pixelate => "pixelize",
                    VideoEffectPreset::Vignette => "vignette",
                    VideoEffectPreset::Sharpen => "unsharp",
                    VideoEffectPreset::Edge => "edgedetect",
                    VideoEffectPreset::RgbSplit => "rgbashift",
                    VideoEffectPreset::Posterize => "elbg",
                }),
                _ => None,
            })
            .collect()
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

    /// Whether an audible independent or primary embedded source-audio graph needs `areverse`.
    /// Overlay video never contributes embedded audio; muted/non-solo tracks are ignored.
    pub fn requires_reverse_audio(&self) -> bool {
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
        if self.tracks.iter().any(|track| {
            matches!(track, CompositionTrack::Audio { muted: false, solo, clips, .. }
                if (!has_solo || *solo) && clips.iter().any(|clip| clip.enabled && clip.reversed))
        }) {
            return true;
        }
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

    /// Whether any active clip uses classical `deshake`.
    pub fn requires_deshake_stabilization(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| clip.enabled && clip.stabilization.is_deshake())
            )
        })
    }

    /// Whether any active clip uses two-pass `vidstabdetect`/`vidstabtransform`.
    pub fn requires_vidstab_stabilization(&self) -> bool {
        self.tracks.iter().any(|track| {
            matches!(
                track,
                CompositionTrack::Video {
                    hidden: false,
                    clips,
                    ..
                } if clips.iter().any(|clip| clip.enabled && clip.stabilization.is_vidstab())
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

    pub fn requires_audio_ducking(&self) -> bool {
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
            matches!(track,
            CompositionTrack::Audio { muted: false, solo, clips, .. }
                if (!has_solo || *solo)
                    && clips.iter().any(|clip| clip.enabled && clip.ducking.is_some()))
        })
    }

    pub fn requires_audio_echo(&self) -> bool {
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
            matches!(track,
            CompositionTrack::Audio { muted: false, solo, clips, .. }
                if (!has_solo || *solo) && clips.iter().any(|clip| {
                    clip.enabled && clip.voice_effect == AudioVoiceEffect::Echo
                }))
        })
    }

    pub fn requires_audio_robot(&self) -> bool {
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
            matches!(track,
            CompositionTrack::Audio { muted: false, solo, clips, .. }
                if (!has_solo || *solo) && clips.iter().any(|clip| {
                    clip.enabled && clip.voice_effect == AudioVoiceEffect::Robot
                }))
        })
    }

    pub fn requires_audio_tone(&self) -> bool {
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
            matches!(track,
            CompositionTrack::Audio { muted: false, solo, clips, .. }
                if (!has_solo || *solo) && clips.iter().any(|clip| {
                    clip.enabled && clip.tone_db != 0.0
                }))
        })
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
        self.validated_duration_ticks()
    }

    pub fn native_duration_ticks(&self) -> Result<u64, CompositionError> {
        self.validate_native_surface()?;
        self.validated_duration_ticks()
    }

    fn validated_duration_ticks(&self) -> Result<u64, CompositionError> {
        let mut end = self.caption_tracks.iter().flat_map(|track|track.clips.iter().map(|cue|cue.timeline_end_tick.microseconds_ceil().expect("validated caption time"))).max().unwrap_or(0);
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
                CompositionTrack::Graphic { clips, .. } => {
                    for clip in clips {end=end.max(clip.timeline_end_tick);}
                }
                CompositionTrack::Color { clips, .. } => {
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
    reverse: bool,
) -> Result<(), CompositionError> {
    // Do not admit source curves to production until audio and edit clocks are wired.
    if placement.concat_retime.is_some(){return Err(CompositionError::InvalidSpeedRamp);}

    let source = sources
        .get(source_id)
        .ok_or_else(|| CompositionError::MissingSource(source_id.clone()))?;
    let head = placement.source_head_ticks()?;
    if (reverse && placement.source_out_tick.checked_add(head).is_none_or(|end| end > source.duration_ticks))
        || (!reverse && head > placement.source_in_tick)
        || placement.source_out_tick > source.duration_ticks
        || (needs_video && source.kind != SourceKind::Video)
        || (!needs_video && !source.has_audio)
    {
        return Err(CompositionError::InvalidClipSource(clip_id.clone()));
    }
    placement.timeline_end_tick()?;
    Ok(())
}

#[cfg(test)]
fn validate_transitions(
    transitions: &[ClipTransition],
    clips: &[VideoClip],
    sources: &BTreeMap<SourceId, CompositionSource>,
) -> Result<(), CompositionError> {
    validate_transitions_for_surface(transitions, clips, sources, false)
}

fn validate_transitions_for_surface(
    transitions: &[ClipTransition],
    clips: &[VideoClip],
    sources: &BTreeMap<SourceId, CompositionSource>,
    native: bool,
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
        if (transition.kind == TransitionKind::Procedural) != transition.kernel.is_some()
            || (transition.kernel.is_some() && transition.timeline.is_none())
            || transition.kernel.as_ref().is_some_and(|kernel|kernel.validate().is_err()) {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
        if !ids.insert(&transition.id) {
            return Err(CompositionError::DuplicateTransition(transition.id.clone()));
        }
        if let Some(timeline) = transition.timeline {
            if transition.duration_ticks != 0 || (transition.from_clip_id.is_none() && transition.to_clip_id.is_none())
                || transition.from_clip_id.as_ref().is_some_and(|id| !indexes.contains_key(id))
                || transition.to_clip_id.as_ref().is_some_and(|id| !indexes.contains_key(id))
                || (transition.from_clip_id.is_some() && transition.from_clip_id == transition.to_clip_id)
                || timeline.source_span().is_err() {
                return Err(CompositionError::InvalidTransition(transition.id.clone()));
            }
            continue;
        }
        if matches!(transition.kind,TransitionKind::ConcatDissolve|TransitionKind::ConcatFadeBlack|TransitionKind::ConcatFadeWhite|TransitionKind::FilmDissolve|TransitionKind::AdditiveDissolve|TransitionKind::FadeWhite) {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
        let (Some(from_id), Some(to_id)) = (transition.from_clip_id.as_ref(), transition.to_clip_id.as_ref()) else {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        };
        let Some(&from_index) = indexes.get(from_id) else {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        };
        let Some(&to_index) = indexes.get(to_id) else {
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        };
        if transition.duration_ticks == 0
            || from_index.checked_add(1) != Some(to_index)
            || ordered[from_index].placement.timeline_end_tick().ok()
                != Some(ordered[to_index].placement.timeline_start_tick)
            || !from_ids.insert(from_id)
            || !to_ids.insert(to_id)
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
        if !native && (from.playback_mode != PlaybackMode::Forward || to.playback_mode != PlaybackMode::Forward)
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
        if (!native || (from.playback_mode == PlaybackMode::Forward && to.playback_mode == PlaybackMode::Forward)) && (from_source
            .duration_ticks
            .saturating_sub(from.placement.source_out_tick) as f64
            + f64::EPSILON
            < from_handle
            || to.placement.source_in_tick as f64 + f64::EPSILON < to_handle)
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
                    transition.from_clip_id.as_ref() == Some(&clip.id) || transition.to_clip_id.as_ref() == Some(&clip.id)
                })
                .expect("occupied duration comes from a transition");
            return Err(CompositionError::InvalidTransition(transition.id.clone()));
        }
    }
    Ok(())
}

fn validate_audio_crossfades(
    clips: &[AudioClip],
    sources: &BTreeMap<SourceId, CompositionSource>,
) -> Result<(), CompositionError> {
    let mut ordered: Vec<_> = clips.iter().collect();
    ordered.sort_by_key(|clip| (clip.placement.timeline_start_tick, clip.id.as_str()));
    let mut head_occupancy = vec![0_u64; ordered.len()];
    let mut tail_occupancy = vec![0_u64; ordered.len()];
    for index in 0..ordered.len() {
        let clip = ordered[index];
        let duration = clip.crossfade_in_ticks;
        if duration == 0 {
            continue;
        }
        let Some(previous) = index.checked_sub(1).map(|position| ordered[position]) else {
            return Err(CompositionError::InvalidAudioCrossfade(clip.id.clone()));
        };
        if previous.placement.timeline_end_tick()? != clip.placement.timeline_start_tick
            || previous.placement.speed_ramp.is_some()
            || clip.placement.speed_ramp.is_some()
            || previous.reversed
            || clip.reversed
            || !matches!(previous.gain, AnimatableValue::Constant { .. })
            || !matches!(previous.pan, AnimatableValue::Constant { .. })
            || !matches!(clip.gain, AnimatableValue::Constant { .. })
            || !matches!(clip.pan, AnimatableValue::Constant { .. })
            || previous.fade_out_ticks > 0
            || clip.fade_in_ticks > 0
        {
            return Err(CompositionError::InvalidAudioCrossfade(clip.id.clone()));
        }
        let before = duration / 2;
        let after = duration - before;
        let previous_tail = (after as f64 * previous.placement.speed).round() as u64;
        let current_head = (before as f64 * clip.placement.speed).round() as u64;
        if sources[&previous.source_id]
            .duration_ticks
            .saturating_sub(previous.placement.source_out_tick)
            < previous_tail
            || clip.placement.source_in_tick < current_head
        {
            return Err(CompositionError::InvalidAudioCrossfade(clip.id.clone()));
        }
        tail_occupancy[index - 1] = before;
        head_occupancy[index] = after;
    }
    for (index, clip) in ordered.iter().enumerate() {
        if head_occupancy[index]
            .checked_add(tail_occupancy[index])
            .ok_or_else(|| CompositionError::InvalidAudioCrossfade(clip.id.clone()))?
            > clip.placement.timeline_duration_ticks()?
        {
            return Err(CompositionError::InvalidAudioCrossfade(clip.id.clone()));
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_role_gain() -> f64 {
    1.0
}

fn default_audio_gain() -> AnimatableValue {
    AnimatableValue::constant(1.0)
}

fn default_audio_pan() -> AnimatableValue {
    AnimatableValue::constant(0.0)
}

fn default_mask_rotation() -> AnimatableValue {
    AnimatableValue::constant(0.0)
}

fn deserialize_animatable_or_number<'de, D>(deserializer: D) -> Result<AnimatableValue, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum CompatibleValue {
        Animatable(AnimatableValue),
        Number(f64),
    }

    Ok(match CompatibleValue::deserialize(deserializer)? {
        CompatibleValue::Animatable(value) => value,
        CompatibleValue::Number(value) => AnimatableValue::constant(value),
    })
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
    InvalidClipLinks,
    InvalidLaneGroups,
    InvalidSpeedRamp,
    InvalidAnimation,
    InvalidTransform,
    InvalidEffect,
    InvalidText,
    InvalidAudioFade,
    InvalidAudioRoleGain,
    InvalidAudioCrossfade(CompositionClipId),
    InvalidAudioDucking,
    InvalidAudioPitch,
    InvalidAudioTone,
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
    #[test]
    fn exact_frame_rate_metadata_roundtrips_and_validates_legacy_milli() {
        let canvas = CanvasSpec {
            fps_milli: 29_970,
            fps_rate: Some(FrameRateRatio {
                numerator: 30_000,
                denominator: 1001,
            }),
            ..CanvasSpec::default()
        };
        assert!(canvas.validate().is_ok());
        assert_eq!(
            canvas.frame_rate(),
            FrameRateRatio {
                numerator: 30_000,
                denominator: 1001
            }
        );
        assert_eq!(
            CanvasSpec::default().frame_rate(),
            FrameRateRatio {
                numerator: 30_000,
                denominator: 1000
            }
        );
        let restored: CanvasSpec =
            serde_json::from_str(&serde_json::to_string(&canvas).unwrap()).unwrap();
        assert_eq!(restored, canvas);
        assert!(
            CanvasSpec {
                fps_milli: 30_000,
                ..canvas
            }
            .validate()
            .is_err()
        );
        assert!(
            CanvasSpec {
                fps_rate: Some(FrameRateRatio {
                    numerator: 30_000,
                    denominator: 0
                }),
                ..canvas
            }
            .validate()
            .is_err()
        );
        let encoded = serde_json::to_value(CanvasSpec::default()).unwrap();
        assert!(encoded.get("fpsRate").is_none());
        assert!(
            serde_json::from_value::<CanvasSpec>(encoded)
                .unwrap()
                .fps_rate
                .is_none()
        );
    }

    use super::*;
    use crate::domain::keyframes::{Interpolation, Keyframe};

    fn source(id: &str) -> CompositionSource {
        CompositionSource {
            audio_channels: None,
            id: SourceId::parse(id).unwrap(),
            kind: SourceKind::Video,
            duration_ticks: 10_000_000,
            width: 1_920,
            height: 1_080,
            has_audio: true,
        }
    }

    #[test]
    fn audio_channel_metadata_is_additive_and_validated() {
        let mut source = source("channel-source");
        let legacy = serde_json::to_value(&source).unwrap();
        assert!(legacy.get("audioChannels").is_none());
        let decoded: CompositionSource = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.audio_channels, None);
        for channels in [1, 2, 6, 64] {
            source.audio_channels = Some(channels);
            source.validate().unwrap();
            assert_eq!(
                serde_json::to_value(&source).unwrap()["audioChannels"],
                channels
            );
        }
        for channels in [0, 65] {
            source.audio_channels = Some(channels);
            assert!(source.validate().is_err());
        }
        source.audio_channels = Some(1);
        source.has_audio = false;
        assert!(source.validate().is_err());
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
            source_timeline_offset_tick: None, source_head_timeline_tick: None, concat_retime: None,
            timeline_duration_tick: None,
            timeline_start_tick: start,
            source_in_tick: 0,
            source_out_tick: 2_000_000,
            speed: 1.0,
            speed_ramp: None,
        }
    }

    #[test]
    fn authored_media_extent_is_independent_and_legacy_placement_stays_derived() {
        let legacy = placement(33_367);
        assert_eq!(legacy.timeline_duration_ticks().unwrap(), 2_000_000);
        let wire = serde_json::to_value(&legacy).unwrap();
        assert!(wire.get("timelineDurationTick").is_none());
        let mut authored = legacy.clone();
        authored.timeline_duration_tick = Some(1_999_999);
        assert_eq!(authored.timeline_duration_ticks().unwrap(), 1_999_999);
        assert_eq!(authored.timeline_end_tick().unwrap(), 2_033_366);
        assert_eq!(authored.source_in_tick, legacy.source_in_tick);
        assert_eq!(authored.source_out_tick, legacy.source_out_tick);
        let roundtrip: ClipPlacement =
            serde_json::from_value(serde_json::to_value(&authored).unwrap()).unwrap();
        assert_eq!(roundtrip, authored);
        authored.timeline_duration_tick = Some(0);
        assert_eq!(
            authored.timeline_duration_ticks(),
            Err(CompositionError::InvalidPlacement)
        );
        authored.timeline_duration_tick = Some(u64::MAX);
        assert_eq!(
            authored.timeline_end_tick(),
            Err(CompositionError::InvalidPlacement)
        );
        authored.timeline_duration_tick = Some(100);
        authored.speed = f64::NAN;
        assert_eq!(
            authored.timeline_duration_ticks(),
            Err(CompositionError::InvalidPlacement)
        );
    }

    fn video_clip(id: &str, source_id: &str, start: u64) -> VideoClip {
        VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            effect_timing: None,
            filmcraft_motion: None,
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
    fn explicit_transition_validation_admits_single_sides_and_checks_authority() {
        use crate::domain::{caption_time::CaptionDelta, transition_timeline::TransitionTimeline};
        let clips = vec![video_clip("a", "source", 0), video_clip("b", "source", 9_000_000)];
        let sources = BTreeMap::from([(SourceId::parse("source").unwrap(), source("source"))]);
        let base = ClipTransition { concat_overlap: None,
            kernel: None, reverse: false,
id: TransitionId::parse("explicit").unwrap(),
            from_clip_id: None, to_clip_id: Some(clips[1].id.clone()), duration_ticks: 0,
            timeline: Some(TransitionTimeline { start_tick: CaptionDelta::SourceTicks(-1), duration_tick: CaptionDelta::SourceTicks(2) }),
            kind: TransitionKind::Dissolve,
        };
        let check = |tr: ClipTransition| validate_transitions(&[tr], &clips, &sources);
        check(base.clone()).unwrap();
        let mut paired = base.clone(); paired.from_clip_id = Some(clips[0].id.clone());
        check(paired).unwrap(); // Independent windows need no adjacent cut.
        let mut outgoing = base.clone(); outgoing.from_clip_id = outgoing.to_clip_id.take();
        check(outgoing).unwrap();
        let mut invalid = base.clone(); invalid.duration_ticks = 1; assert!(check(invalid).is_err());
        let mut invalid = base.clone(); invalid.to_clip_id = None; assert!(check(invalid).is_err());
        let mut invalid = base.clone(); invalid.from_clip_id = invalid.to_clip_id.clone(); assert!(check(invalid).is_err());
        let mut invalid = base.clone(); invalid.to_clip_id = Some(CompositionClipId::parse("missing").unwrap()); assert!(check(invalid).is_err());
        let mut invalid = base.clone(); invalid.timeline.as_mut().unwrap().duration_tick = CaptionDelta::SourceTicks(0); assert!(check(invalid).is_err());
        let mut invalid = base; invalid.timeline.as_mut().unwrap().start_tick = CaptionDelta::SourceTicks(i64::MAX); assert!(check(invalid).is_err());
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
            transitions: vec![ClipTransition { concat_overlap: None,
                kernel: None, reverse: false,
timeline: None,
                id: TransitionId::parse("transition-a-b").unwrap(),
                from_clip_id: Some(CompositionClipId::parse("clip-a").unwrap()),
                to_clip_id: Some(CompositionClipId::parse("clip-b").unwrap()),
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
        wire["canvas"]
            .as_object_mut()
            .unwrap()
            .remove("backgroundMode");
        wire["canvas"]
            .as_object_mut()
            .unwrap()
            .remove("backgroundBlur");
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
        assert_eq!(decoded.canvas.background_mode, CanvasBackgroundMode::Color);
        assert_eq!(decoded.canvas.background_blur, 24.0);
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
            transitions: vec![ClipTransition { concat_overlap: None,
                kernel: None, reverse: false,
timeline: None,
                id: TransitionId::parse("reverse-transition").unwrap(),
                from_clip_id: Some(from.id.clone()),
                to_clip_id: Some(to.id.clone()),
                duration_ticks: 200_000,
                kind: TransitionKind::Dissolve,
            }],
            clips: vec![from, to],
        }];
        composition.validate_native_surface().unwrap();
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
    fn vidstab_wire_is_bounded_and_requires_vidstab_filters() {
        let source = source("vidstab-source");
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources.insert(source.id.clone(), source);
        let mut clip = video_clip("vidstab-clip", "vidstab-source", 0);
        clip.stabilization = StabilizationSpec::Vidstab {
            shakiness: 5,
            accuracy: 15,
            smoothing: 10,
        };
        composition.tracks = vec![CompositionTrack::Video {
            id: TrackId::parse("vidstab-track").unwrap(),
            name: "Vidstab".into(),
            hidden: false,
            muted: false,
            locked: false,
            clips: vec![clip],
            transitions: Vec::new(),
        }];
        composition.validate().unwrap();
        assert!(composition.requires_stabilization());
        assert!(!composition.requires_deshake_stabilization());
        assert!(composition.requires_vidstab_stabilization());
        let wire = serde_json::to_value(&composition).unwrap();
        assert_eq!(
            wire["tracks"][0]["clips"][0]["stabilization"]["mode"],
            "vidstab"
        );
        assert_eq!(
            wire["tracks"][0]["clips"][0]["stabilization"]["shakiness"],
            5
        );

        for (shakiness, accuracy, smoothing) in [(0, 15, 10), (5, 0, 10), (5, 15, 0), (11, 15, 10)]
        {
            let mut invalid = composition.clone();
            let CompositionTrack::Video { clips, .. } = &mut invalid.tracks[0] else {
                unreachable!();
            };
            clips[0].stabilization = StabilizationSpec::Vidstab {
                shakiness,
                accuracy,
                smoothing,
            };
            assert_eq!(
                invalid.validate(),
                Err(CompositionError::InvalidStabilization)
            );
        }
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
                audio_stream:None,detached_from:None,concat_detached_order:None,concat_gain_keys:Vec::new(),dsp_effects: Vec::new(),
                role_gain: 1.0,
                id: CompositionClipId::parse("clip-audio").unwrap(),
                source_id: SourceId::parse("source-a").unwrap(),
                placement: ClipPlacement {
                    source_out_tick: 99_000,
                    ..placement(0)
                },
                gain: AnimatableValue::constant(1.0),
                pan: AnimatableValue::constant(0.0),
                reversed: false,
                fade_in_ticks: 0,
                fade_out_ticks: 0,
                voice_effect: AudioVoiceEffect::None,
                pitch_semitones: 0.0,
                tone_db: 0.0,
                crossfade_in_ticks: 0,
                crossfade_curve: crate::domain::audio_transition::AudioTransitionCurve::ConstantGain,
                ducking: None,
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
            rotation_degrees: AnimatableValue::constant(30.0),
            feather: 0.2,
            inverted: true,
        };
        effect.validate().unwrap();
        let encoded = serde_json::to_value(&effect).unwrap();
        assert_eq!(encoded["kind"], "mask");
        assert_eq!(encoded["shape"], "ellipse");
        assert_eq!(encoded["x"]["mode"], "constant");
        assert_eq!(encoded["width"]["value"], 1.5);
        assert_eq!(encoded["rotationDegrees"]["mode"], "constant");
        assert_eq!(encoded["rotationDegrees"]["value"], 30.0);
        assert_eq!(
            serde_json::from_value::<VideoEffect>(encoded.clone()).unwrap(),
            effect
        );
        let mut legacy = encoded;
        legacy["rotationDegrees"] = serde_json::json!(30.0);
        let VideoEffect::Mask {
            rotation_degrees, ..
        } = serde_json::from_value::<VideoEffect>(legacy).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(rotation_degrees, AnimatableValue::constant(30.0));
        let mut missing_rotation = serde_json::to_value(&effect).unwrap();
        missing_rotation
            .as_object_mut()
            .unwrap()
            .remove("rotationDegrees");
        let VideoEffect::Mask {
            rotation_degrees, ..
        } = serde_json::from_value::<VideoEffect>(missing_rotation).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(rotation_degrees, AnimatableValue::constant(0.0));

        for invalid in [
            VideoEffect::Mask {
                shape: MaskShape::Rectangle,
                x: AnimatableValue::constant(-0.01),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.5),
                height: AnimatableValue::constant(0.5),
                rotation_degrees: AnimatableValue::constant(0.0),
                feather: 0.0,
                inverted: false,
            },
            VideoEffect::Mask {
                shape: MaskShape::Ellipse,
                x: AnimatableValue::constant(0.5),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.5),
                height: AnimatableValue::constant(0.5),
                rotation_degrees: AnimatableValue::constant(181.0),
                feather: 0.0,
                inverted: false,
            },
            VideoEffect::Mask {
                shape: MaskShape::Rectangle,
                x: AnimatableValue::constant(0.5),
                y: AnimatableValue::constant(0.5),
                width: AnimatableValue::constant(0.0),
                height: AnimatableValue::constant(0.5),
                rotation_degrees: AnimatableValue::constant(0.0),
                feather: 0.0,
                inverted: false,
            },
        ] {
            assert_eq!(invalid.validate(), Err(CompositionError::InvalidEffect));
        }
    }

    #[test]
    fn style_effect_wire_is_bounded_and_canonical() {
        for preset in [
            VideoEffectPreset::Blur,
            VideoEffectPreset::Pixelate,
            VideoEffectPreset::Vignette,
            VideoEffectPreset::Sharpen,
            VideoEffectPreset::Edge,
            VideoEffectPreset::RgbSplit,
            VideoEffectPreset::Posterize,
        ] {
            let effect = VideoEffect::Style {
                preset,
                intensity: 0.75,
            };
            effect.validate().unwrap();
            let encoded = serde_json::to_value(&effect).unwrap();
            assert_eq!(encoded["kind"], "style");
            assert_eq!(
                serde_json::from_value::<VideoEffect>(encoded).unwrap(),
                effect
            );
        }
        assert_eq!(
            VideoEffect::Style {
                preset: VideoEffectPreset::Blur,
                intensity: 0.0
            }
            .validate(),
            Err(CompositionError::InvalidEffect)
        );
        assert_eq!(
            VideoEffect::Style {
                preset: VideoEffectPreset::Edge,
                intensity: f64::NAN
            }
            .validate(),
            Err(CompositionError::InvalidEffect)
        );
    }

    #[test]
    fn animatable_values_bound_saved_projects_to_thirty_two_points() {
        let allowed = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                u64::from(DEFAULT_TIME_BASE),
                Interpolation::Linear,
                (0..MAX_KEYFRAMES_PER_VALUE)
                    .map(|index| Keyframe {
                        tangents: None,
                        interpolation: None,
                        tick: index as i64,
                        value: index as f64,
                    })
                    .collect(),
            )
            .unwrap(),
        };
        allowed.validate().unwrap();
        let too_many = AnimatableValue::Keyframes {
            track: KeyframeTrack::new(
                u64::from(DEFAULT_TIME_BASE),
                Interpolation::EaseInOut,
                (0..=MAX_KEYFRAMES_PER_VALUE)
                    .map(|index| Keyframe {
                        tangents: None,
                        interpolation: None,
                        tick: index as i64,
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
                effects: vec![],
                blend_mode: None,
                concat_title: None,
                color_animation: None,
                id: CompositionClipId::parse("title-clip").unwrap(),
                timeline_start_tick: 0,
                timeline_end_tick: 1_000_000,
                text: "safe textfile content".into(),
                style: TextStyle {
                    align: None,
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

        let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].style.font_family = "Noto Sans".into();
        let static_wire = serde_json::to_value(&clips[0]).unwrap();
        assert!(static_wire.get("colorAnimation").is_none());
        clips[0].color_animation = Some(serde_json::from_value(serde_json::json!({
            "keys":[
                {"t_us":-0.5,"value":{"r":255,"g":0,"b":0,"a":128},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}},
                {"t_us":1000000.5,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}
            ],"extrapolate":{"before":"PingPong","after":"Offset"}
        })).unwrap());
        let wire = serde_json::to_value(&clips[0]).unwrap();
        assert_eq!(serde_json::from_value::<TextClip>(wire.clone()).unwrap(), clips[0]);
        let mut invalid_wire = wire;
        invalid_wire["colorAnimation"]["keys"][0]["value"]["r"] = serde_json::json!(256);
        assert!(serde_json::from_value::<TextClip>(invalid_wire).is_err());
        assert!(composition.validate().is_ok());
        let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].color_animation.as_mut().unwrap().keys[1].t_us = -1.0;
        assert_eq!(composition.validate(), Err(CompositionError::InvalidAnimation));
        let CompositionTrack::Text { clips, .. } = &mut composition.tracks[0] else {
            unreachable!();
        };
        clips[0].color_animation = None;

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
            transitions: vec![ClipTransition { concat_overlap: None,
                kernel: None, reverse: false,
timeline: None,
                id: transition_id.clone(),
                from_clip_id: Some(clip_a.id),
                to_clip_id: Some(clip_b.id),
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
            transitions: vec![ClipTransition { concat_overlap: None,
                kernel: None, reverse: false,
timeline: None,
                id: transition_id.clone(),
                from_clip_id: Some(first.id),
                to_clip_id: Some(second.id),
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

#[cfg(test)]
mod motion_position_model_tests {
    use super::*;
    use crate::domain::motion_path::PositionAnimation;
    #[test]
    fn transform_position_is_optional_legacy_compatible_and_admitted() {
        let original=TransformSpec::default();
        let wire=serde_json::to_value(&original).unwrap();
        assert!(wire.get("position").is_none());
        assert_eq!(serde_json::from_value::<TransformSpec>(wire).unwrap(),original);
        let positioned=TransformSpec{position:Some(PositionAnimation::Xy{x:AnimatableValue::constant(10.0),y:AnimatableValue::constant(20.0)}),..original};
        positioned.validate().unwrap();
        let wire=serde_json::to_value(&positioned).unwrap();
        assert_eq!(serde_json::from_value::<TransformSpec>(wire).unwrap(),positioned);
        let invalid=TransformSpec{position:Some(PositionAnimation::Xy{x:AnimatableValue::constant(f64::NAN),y:AnimatableValue::constant(20.0)}),..positioned};
        assert!(invalid.validate().is_err());
    }
}

#[cfg(test)]
mod warp_stabilizer_project_tests {
    #[test]
    fn persisted_effect_contract_and_bounds() {
        let value = serde_json::json!({"kind":"warp_stabilizer","parameters":{},"animation":{"smoothness":{"mode":"constant","value":75.}}});
        let effect: super::VideoEffect = serde_json::from_value(value).unwrap();
        effect.validate().unwrap();
        let saved = serde_json::to_value(&effect).unwrap();
        assert_eq!(saved["parameters"]["framing"], 2);
        assert_eq!(saved["animation"]["smoothness"]["value"], 75.);
        let restored: super::VideoEffect =
            serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        let invalid: super::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"warp_stabilizer","parameters":{"method":4}}),
        )
        .unwrap();
        assert!(invalid.validate().is_err());
    }
}

fn validate_concat_gain_keys(keys:&[super::concat_effect_upgrade::ParamKey],gain:&AnimatableValue)->Result<(),CompositionError>{
    if keys.len()>4096 || (!keys.is_empty()&&!matches!(gain,AnimatableValue::Constant{..})) || keys.iter().any(|k|!k.at.is_finite()||!(0.0..=1.0).contains(&k.at)||!k.value.is_finite()||(!k.derived&&k.value<0.)||!k.ease.iter().all(|v|v.is_finite())||!(0.0..=1.0).contains(&k.ease[0])||!(0.0..=1.0).contains(&k.ease[2])) {return Err(CompositionError::InvalidAnimation)}
    Ok(())
}
