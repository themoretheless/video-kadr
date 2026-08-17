//! Immutable edit semantics shared by planning and media adapters.
//!
//! Transport identifiers and wire defaults stop before this module. Values are
//! grouped by the part of the media pipeline they affect and validated again
//! after deserialization so persisted plans cannot bypass domain invariants.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::audio_mix::{AudioDynamicsSpec, AudioTrackSpec};
use super::color_grade::ColorAdvancedSpec;
use super::composition::{CompositionSpec, TransitionSpec};
use super::keyframes::{Interpolation, Keyframe, KeyframeTrack, OUTPUT_TIME_BASE};
use super::motion::MotionSpec;
use super::overlay::{OverlaySpec, SubtitlesSpec, TitleSpec};
use super::spatial::{LensCorrectionSpec, Reframe360Spec, StabilizeSpec};
use crate::model;

pub const EDIT_SPEC_SCHEMA_VERSION: u32 = 1;

/// Collection caps from the feature contract.
pub const MAX_CLIPS: usize = 200;
pub const MAX_OVERLAYS: usize = 32;
pub const MAX_TITLES: usize = 32;
pub const MAX_AUDIO_TRACKS: usize = 8;
pub const MAX_KEYFRAMES: usize = 64;
pub const MAX_TEXT_CHARS: usize = 512;
/// Asset references are opaque ids; the store applies the real allow-list.
const MAX_ASSET_REFERENCE_CHARS: usize = 128;

/// Reject non-finite values, then clamp into the documented range. Clamping
/// (rather than rejecting) keeps a slightly out-of-range slider usable while
/// still guaranteeing every value the FFmpeg adapter sees is bounded.
pub fn clamped(value: f64, min: f64, max: f64, error: EditSpecError) -> Result<f64, EditSpecError> {
    if !value.is_finite() {
        return Err(error);
    }
    Ok(value.clamp(min, max))
}

/// Length-cap a user string and reject control characters. Newlines survive
/// because titles are allowed to wrap.
pub fn bounded_text(
    value: &str,
    max_chars: usize,
    error: EditSpecError,
) -> Result<String, EditSpecError> {
    if value.chars().count() > max_chars
        || value
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err(error);
    }
    Ok(value.to_owned())
}

/// Validate an opaque asset reference. The same rule `LutGrade` applies: no
/// separators, no dots, so an id can never become a path fragment on its own.
pub fn asset_reference(value: &str) -> Result<String, EditSpecError> {
    if value.is_empty()
        || value.len() > MAX_ASSET_REFERENCE_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(EditSpecError::InvalidAssetReference);
    }
    Ok(value.to_owned())
}

/// A validated `#RRGGBB` colour. Stored uppercase so equal colours produce
/// equal plan fingerprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor([u8; 3]);

impl HexColor {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        let digits = value
            .strip_prefix('#')
            .filter(|rest| rest.len() == 6 && rest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or(EditSpecError::InvalidColor)?;
        let mut channels = [0_u8; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
                .map_err(|_| EditSpecError::InvalidColor)?;
        }
        Ok(Self(channels))
    }

    pub fn as_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }

    /// The `0xRRGGBB` form every FFmpeg colour option accepts. It contains only
    /// hex digits, so it is safe inside a filter string without escaping.
    pub fn ffmpeg_color(self) -> String {
        format!("0x{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }
}

impl TryFrom<String> for HexColor {
    type Error = EditSpecError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<HexColor> for String {
    fn from(value: HexColor) -> Self {
        value.as_hex()
    }
}

/// Convert a wire keyframe list into a validated domain track. An empty list
/// means "parameter unused" and yields `None`.
///
/// The wire carries `interp` per keyframe while the domain track carries one
/// interpolation for the whole track, so the first keyframe decides the mode.
pub fn keyframe_track(
    points: &[model::Keyframe],
    error: EditSpecError,
) -> Result<Option<KeyframeTrack<f64>>, EditSpecError> {
    if points.is_empty() {
        return Ok(None);
    }
    if points.len() > MAX_KEYFRAMES {
        return Err(error);
    }
    let interpolation = Interpolation::from_wire(points[0].interp.as_token()).ok_or(error)?;
    let mut keyframes = Vec::with_capacity(points.len());
    for point in points {
        if !point.t.is_finite() || point.t < 0.0 || !point.v.is_finite() {
            return Err(error);
        }
        keyframes.push(Keyframe {
            tick: (point.t * f64::from(OUTPUT_TIME_BASE)).round() as u64,
            value: point.v,
        });
    }
    KeyframeTrack::new(OUTPUT_TIME_BASE, interpolation, keyframes)
        .map(Some)
        .map_err(|_| error)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeRange {
    start_seconds: f64,
    end_seconds: f64,
}

impl TimeRange {
    pub(crate) fn new(start_seconds: f64, end_seconds: f64) -> Result<Self, EditSpecError> {
        let value = Self {
            start_seconds,
            end_seconds,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn start_seconds(self) -> f64 {
        self.start_seconds
    }

    pub fn end_seconds(self) -> f64 {
        self.end_seconds
    }

    pub fn duration_seconds(self) -> f64 {
        self.end_seconds - self.start_seconds
    }

    fn validate(self) -> Result<(), EditSpecError> {
        if !self.start_seconds.is_finite()
            || !self.end_seconds.is_finite()
            || self.start_seconds < 0.0
            || self.end_seconds - self.start_seconds <= 0.01
        {
            return Err(EditSpecError::InvalidTimeRange);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    fn validate(self) -> Result<(), EditSpecError> {
        if self.width == 0 || self.height == 0 {
            return Err(EditSpecError::InvalidRectangle);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputScale {
    pub width: i32,
    pub height: i32,
}

impl OutputScale {
    fn validate(self) -> Result<(), EditSpecError> {
        fn valid_dimension(value: i32) -> bool {
            matches!(value, -2 | -1) || (2..=7680).contains(&value)
        }

        if !valid_dimension(self.width)
            || !valid_dimension(self.height)
            || (self.width < 0 && self.height < 0)
        {
            return Err(EditSpecError::InvalidScale);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl Rotation {
    pub(crate) fn from_degrees(value: i32) -> Result<Self, EditSpecError> {
        match value.rem_euclid(360) {
            0 => Ok(Self::None),
            90 => Ok(Self::Clockwise90),
            180 => Ok(Self::Clockwise180),
            270 => Ok(Self::Clockwise270),
            _ => Err(EditSpecError::InvalidRotation),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AspectRatio {
    pub width: u32,
    pub height: u32,
}

impl AspectRatio {
    pub(crate) fn parse(value: &str) -> Result<Self, EditSpecError> {
        let (width, height) = value
            .split_once(':')
            .ok_or(EditSpecError::InvalidAspectRatio)?;
        let value = Self {
            width: width
                .trim()
                .parse()
                .map_err(|_| EditSpecError::InvalidAspectRatio)?,
            height: height
                .trim()
                .parse()
                .map_err(|_| EditSpecError::InvalidAspectRatio)?,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(self) -> Result<(), EditSpecError> {
        if self.width == 0 || self.height == 0 || self.width > 100 || self.height > 100 {
            return Err(EditSpecError::InvalidAspectRatio);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensorColor {
    Black,
    White,
    Gray,
    Red,
}

impl CensorColor {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, EditSpecError> {
        match value.unwrap_or("black") {
            "black" => Ok(Self::Black),
            "white" => Ok(Self::White),
            "gray" | "grey" => Ok(Self::Gray),
            "red" => Ok(Self::Red),
            _ => Err(EditSpecError::InvalidCensorColor),
        }
    }

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::White => "white",
            Self::Gray => "gray",
            Self::Red => "red",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookPreset {
    Grayscale,
    Sepia,
    Warm,
    Cold,
    TealOrange,
    Faded,
    Noir,
    Vintage,
}

impl LookPreset {
    pub(crate) const ALL: [Self; 8] = [
        Self::Grayscale,
        Self::Sepia,
        Self::Warm,
        Self::Cold,
        Self::TealOrange,
        Self::Faded,
        Self::Noir,
        Self::Vintage,
    ];

    pub(crate) const fn wire_id(self) -> &'static str {
        match self {
            Self::Grayscale => "grayscale",
            Self::Sepia => "sepia",
            Self::Warm => "warm",
            Self::Cold => "cold",
            Self::TealOrange => "teal-orange",
            Self::Faded => "faded",
            Self::Noir => "noir",
            Self::Vintage => "vintage",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, EditSpecError> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.wire_id() == value)
            .ok_or(EditSpecError::InvalidLook)
    }
}

impl Serialize for LookPreset {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.wire_id())
    }
}

impl<'de> Deserialize<'de> for LookPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = String::deserialize(deserializer)?;
        Self::parse(&id).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToneCurvePoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl ToneCurvePoint {
    pub(crate) fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn x(self) -> f64 {
        self.x
    }

    pub fn y(self) -> f64 {
        self.y
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToneCurve {
    pub(crate) points: Vec<ToneCurvePoint>,
}

// The FFmpeg adapter evaluates curves in a 16-bit RGB working format. Points
// closer than one 16-bit code value are rejected by the curves filter even
// though their floating-point x coordinates are strictly increasing.
const MIN_TONE_CURVE_X_GAP: f64 = 1.0 / 65_535.0;

impl ToneCurve {
    pub(crate) fn new(points: Vec<ToneCurvePoint>) -> Result<Self, EditSpecError> {
        let mut value = Self { points };
        value.validate()?;
        // Canonical endpoints keep plan fingerprints and emitted FFmpeg strings
        // stable when clients send tiny floating point noise around 0 and 1.
        value.points.first_mut().expect("validated curve").x = 0.0;
        value.points.last_mut().expect("validated curve").x = 1.0;
        Ok(value)
    }

    pub fn points(&self) -> &[ToneCurvePoint] {
        &self.points
    }

    fn validate(&self) -> Result<(), EditSpecError> {
        if !(2..=16).contains(&self.points.len()) {
            return Err(EditSpecError::InvalidToneCurve);
        }
        let endpoint_epsilon = 1e-9;
        if self.points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || !(0.0..=1.0).contains(&point.x)
                || !(0.0..=1.0).contains(&point.y)
        }) || self
            .points
            .windows(2)
            .any(|pair| pair[1].x - pair[0].x < MIN_TONE_CURVE_X_GAP)
            || self.points[0].x.abs() > endpoint_epsilon
            || (self.points[self.points.len() - 1].x - 1.0).abs() > endpoint_epsilon
        {
            return Err(EditSpecError::InvalidToneCurve);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToneCurves {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) master: Option<ToneCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) red: Option<ToneCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) green: Option<ToneCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) blue: Option<ToneCurve>,
}

impl ToneCurves {
    pub(crate) fn new(
        master: Option<ToneCurve>,
        red: Option<ToneCurve>,
        green: Option<ToneCurve>,
        blue: Option<ToneCurve>,
    ) -> Option<Self> {
        (master.is_some() || red.is_some() || green.is_some() || blue.is_some()).then_some(Self {
            master,
            red,
            green,
            blue,
        })
    }

    fn validate(&self) -> Result<(), EditSpecError> {
        if self.master.is_none()
            && self.red.is_none()
            && self.green.is_none()
            && self.blue.is_none()
        {
            return Err(EditSpecError::InvalidToneCurve);
        }
        for curve in [&self.master, &self.red, &self.green, &self.blue]
            .into_iter()
            .flatten()
        {
            curve.validate()?;
        }
        Ok(())
    }

    pub fn master(&self) -> Option<&ToneCurve> {
        self.master.as_ref()
    }

    pub fn red(&self) -> Option<&ToneCurve> {
        self.red.as_ref()
    }

    pub fn green(&self) -> Option<&ToneCurve> {
        self.green.as_ref()
    }

    pub fn blue(&self) -> Option<&ToneCurve> {
        self.blue.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LutGrade {
    pub(crate) id: String,
    pub(crate) intensity: f64,
}

impl LutGrade {
    pub(crate) fn new(id: String, intensity: f64) -> Result<Option<Self>, EditSpecError> {
        let value = Self { id, intensity };
        value.validate()?;
        Ok((intensity > 1e-9).then_some(value))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn intensity(&self) -> f64 {
        self.intensity
    }

    fn validate(&self) -> Result<(), EditSpecError> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || !self.intensity.is_finite()
            || !(0.0..=1.0).contains(&self.intensity)
        {
            return Err(EditSpecError::InvalidLut);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimingSpec {
    pub(crate) trim: Option<TimeRange>,
    pub(crate) segments: Vec<TimeRange>,
    pub(crate) speed: f64,
    pub(crate) reverse: bool,
    pub(crate) fade_in_seconds: f64,
    pub(crate) fade_out_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensorSpec {
    pub(crate) rect: PixelRect,
    pub(crate) color: CensorColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeometrySpec {
    pub(crate) crop: Option<PixelRect>,
    pub(crate) scale: Option<OutputScale>,
    pub(crate) rotation: Rotation,
    pub(crate) flip_horizontal: bool,
    pub(crate) flip_vertical: bool,
    pub(crate) pad_aspect: Option<AspectRatio>,
    pub(crate) censor: Option<CensorSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoEffects {
    pub(crate) brightness: f64,
    pub(crate) contrast: f64,
    pub(crate) saturation: f64,
    pub(crate) look: Option<LookPreset>,
    pub(crate) vignette: bool,
    pub(crate) denoise: bool,
    pub(crate) sharpen: f64,
    pub(crate) grain: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) curves: Option<ToneCurves>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lut: Option<LutGrade>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioEffects {
    pub(crate) muted: bool,
    pub(crate) volume: f64,
    pub(crate) normalize: bool,
    pub(crate) highpass: bool,
}

/// Everything the parity wave adds to an edit. Absent by default, skipped on
/// serialization when empty, so an untouched `EditSpec` keeps the exact plan
/// fingerprint (and therefore the render-cache key) it has today.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditExtensions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) composition: Option<CompositionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) segment_transition: Option<TransitionSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) overlays: Vec<OverlaySpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) titles: Vec<TitleSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subtitles: Option<SubtitlesSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) audio_tracks: Vec<AudioTrackSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio_dynamics: Option<AudioDynamicsSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) motion: Option<MotionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) speed_ramps: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reframe360: Option<Reframe360Spec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stabilize: Option<StabilizeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lens_correction: Option<LensCorrectionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) color_advanced: Option<ColorAdvancedSpec>,
}

impl EditExtensions {
    /// True when nothing in this edit uses a parity-wave feature.
    pub fn is_empty(&self) -> bool {
        self.composition.is_none()
            && self.segment_transition.is_none()
            && self.overlays.is_empty()
            && self.titles.is_empty()
            && self.subtitles.is_none()
            && self.audio_tracks.is_empty()
            && self.audio_dynamics.is_none()
            && self.motion.is_none()
            && self.speed_ramps.is_none()
            && self.reframe360.is_none()
            && self.stabilize.is_none()
            && self.lens_correction.is_none()
            && self.color_advanced.is_none()
    }

    /// Validate the optional wire payload of one edit request. A request that
    /// sets none of the new fields returns the empty value.
    pub fn from_request(request: &model::EditRequest) -> Result<Self, EditSpecError> {
        let overlays = request.overlays.as_deref().unwrap_or_default();
        if overlays.len() > MAX_OVERLAYS {
            return Err(EditSpecError::InvalidOverlay);
        }
        let titles = request.titles.as_deref().unwrap_or_default();
        if titles.len() > MAX_TITLES {
            return Err(EditSpecError::InvalidTitle);
        }
        let audio_tracks = request.audio_tracks.as_deref().unwrap_or_default();
        if audio_tracks.len() > MAX_AUDIO_TRACKS {
            return Err(EditSpecError::InvalidAudioTrack);
        }
        Ok(Self {
            composition: request
                .clips
                .as_deref()
                .map(CompositionSpec::from_wire)
                .transpose()?
                .flatten(),
            segment_transition: request
                .segment_transition
                .as_ref()
                .map(TransitionSpec::from_wire)
                .transpose()?,
            overlays: overlays
                .iter()
                .map(OverlaySpec::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            titles: titles
                .iter()
                .map(TitleSpec::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            subtitles: request
                .subtitles
                .as_ref()
                .map(SubtitlesSpec::from_wire)
                .transpose()?,
            audio_tracks: audio_tracks
                .iter()
                .map(AudioTrackSpec::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
            audio_dynamics: request
                .audio_dynamics
                .as_ref()
                .map(AudioDynamicsSpec::from_wire)
                .transpose()?,
            motion: request
                .motion
                .as_ref()
                .map(MotionSpec::from_wire)
                .transpose()?
                .flatten(),
            speed_ramps: keyframe_track(
                request.speed_ramps.as_deref().unwrap_or_default(),
                EditSpecError::InvalidMotion,
            )?,
            reframe360: request
                .reframe360
                .as_ref()
                .map(Reframe360Spec::from_wire)
                .transpose()?,
            stabilize: request
                .stabilize
                .as_ref()
                .map(StabilizeSpec::from_wire)
                .transpose()?
                .flatten(),
            lens_correction: request
                .lens_correction
                .as_ref()
                .map(LensCorrectionSpec::from_wire)
                .transpose()?
                .flatten(),
            color_advanced: request
                .color_advanced
                .as_ref()
                .map(ColorAdvancedSpec::from_wire)
                .transpose()?,
        })
    }

    fn validate(&self) -> Result<(), EditSpecError> {
        if let Some(composition) = &self.composition {
            composition.validate()?;
        }
        if let Some(transition) = &self.segment_transition {
            transition.validate()?;
        }
        if self.overlays.len() > MAX_OVERLAYS || self.titles.len() > MAX_TITLES {
            return Err(EditSpecError::InvalidOverlay);
        }
        for overlay in &self.overlays {
            overlay.validate()?;
        }
        for title in &self.titles {
            title.validate()?;
        }
        if let Some(subtitles) = &self.subtitles {
            subtitles.validate()?;
        }
        if self.audio_tracks.len() > MAX_AUDIO_TRACKS {
            return Err(EditSpecError::InvalidAudioTrack);
        }
        for track in &self.audio_tracks {
            track.validate()?;
        }
        if let Some(dynamics) = &self.audio_dynamics {
            dynamics.validate()?;
        }
        if let Some(motion) = &self.motion {
            motion.validate()?;
        }
        if let Some(ramps) = &self.speed_ramps {
            if ramps.keyframes.len() > MAX_KEYFRAMES {
                return Err(EditSpecError::InvalidMotion);
            }
            super::motion::validate_speed_ramps(ramps)?;
        }
        if let Some(reframe) = &self.reframe360 {
            reframe.validate()?;
        }
        if let Some(stabilize) = &self.stabilize {
            stabilize.validate()?;
        }
        if let Some(lens) = &self.lens_correction {
            lens.validate()?;
        }
        if let Some(color) = &self.color_advanced {
            color.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditSpec {
    pub schema_version: u32,
    pub(crate) timing: TimingSpec,
    pub(crate) geometry: GeometrySpec,
    pub(crate) video: VideoEffects,
    pub(crate) audio: AudioEffects,
    #[serde(default, skip_serializing_if = "EditExtensions::is_empty")]
    pub(crate) extensions: EditExtensions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditSpecWire {
    schema_version: u32,
    timing: TimingSpec,
    geometry: GeometrySpec,
    video: VideoEffects,
    audio: AudioEffects,
    #[serde(default)]
    extensions: EditExtensions,
}

impl<'de> Deserialize<'de> for EditSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EditSpecWire::deserialize(deserializer)?;
        if wire.schema_version != EDIT_SPEC_SCHEMA_VERSION {
            return Err(D::Error::custom("unsupported edit specification schema"));
        }
        Self::new(wire.timing, wire.geometry, wire.video, wire.audio)
            .and_then(|spec| spec.with_extensions(wire.extensions))
            .map_err(D::Error::custom)
    }
}

impl EditSpec {
    pub(crate) fn new(
        timing: TimingSpec,
        geometry: GeometrySpec,
        video: VideoEffects,
        audio: AudioEffects,
    ) -> Result<Self, EditSpecError> {
        let value = Self {
            schema_version: EDIT_SPEC_SCHEMA_VERSION,
            timing,
            geometry,
            video,
            audio,
            extensions: EditExtensions::default(),
        };
        value.validate()?;
        Ok(value)
    }

    /// Attach the validated parity-wave payload. Kept separate from `new` so
    /// the four-section constructor its existing callers use stays unchanged.
    pub fn with_extensions(mut self, extensions: EditExtensions) -> Result<Self, EditSpecError> {
        extensions.validate()?;
        self.extensions = extensions;
        Ok(self)
    }

    pub fn timing(&self) -> &TimingSpec {
        &self.timing
    }

    pub fn geometry(&self) -> &GeometrySpec {
        &self.geometry
    }

    pub fn video(&self) -> &VideoEffects {
        &self.video
    }

    pub fn audio(&self) -> &AudioEffects {
        &self.audio
    }

    /// True when a parity-wave feature other than the timeline itself is
    /// active. Stage ordering depends on it: as soon as one of those stages
    /// runs, the plain `segments` timeline has to be built inside the same
    /// `filter_complex` instead of by the legacy concat path, otherwise the
    /// cuts would be silently dropped.
    pub fn uses_non_timeline_extensions(&self) -> bool {
        let extensions = &self.extensions;
        !extensions.overlays.is_empty()
            || !extensions.titles.is_empty()
            || extensions.subtitles.is_some()
            || !extensions.audio_tracks.is_empty()
            || extensions.audio_dynamics.is_some()
            || extensions.motion.is_some()
            || extensions.speed_ramps.is_some()
            || extensions.reframe360.is_some()
            || extensions.stabilize.is_some()
            || extensions.lens_correction.is_some()
            || extensions.color_advanced.is_some()
    }

    /// Multi-source timeline, or `None` when `segments`/`trim` still drive it.
    pub fn composition(&self) -> Option<&CompositionSpec> {
        self.extensions.composition.as_ref()
    }

    /// Transition inserted between plain `segments`.
    pub fn segment_transition(&self) -> Option<&TransitionSpec> {
        self.extensions.segment_transition.as_ref()
    }

    pub fn overlays(&self) -> &[OverlaySpec] {
        &self.extensions.overlays
    }

    pub fn titles(&self) -> &[TitleSpec] {
        &self.extensions.titles
    }

    pub fn subtitles(&self) -> Option<&SubtitlesSpec> {
        self.extensions.subtitles.as_ref()
    }

    pub fn audio_tracks(&self) -> &[AudioTrackSpec] {
        &self.extensions.audio_tracks
    }

    pub fn audio_dynamics(&self) -> Option<&AudioDynamicsSpec> {
        self.extensions.audio_dynamics.as_ref()
    }

    pub fn motion(&self) -> Option<&MotionSpec> {
        self.extensions.motion.as_ref()
    }

    pub fn speed_ramps(&self) -> Option<&KeyframeTrack<f64>> {
        self.extensions.speed_ramps.as_ref()
    }

    pub fn reframe360(&self) -> Option<&Reframe360Spec> {
        self.extensions.reframe360.as_ref()
    }

    pub fn stabilize(&self) -> Option<&StabilizeSpec> {
        self.extensions.stabilize.as_ref()
    }

    pub fn lens_correction(&self) -> Option<&LensCorrectionSpec> {
        self.extensions.lens_correction.as_ref()
    }

    pub fn color_advanced(&self) -> Option<&ColorAdvancedSpec> {
        self.extensions.color_advanced.as_ref()
    }

    pub fn validate(&self) -> Result<(), EditSpecError> {
        if self.schema_version != EDIT_SPEC_SCHEMA_VERSION {
            return Err(EditSpecError::UnsupportedSchema(self.schema_version));
        }
        if !self.timing.speed.is_finite() || !(0.5..=2.0).contains(&self.timing.speed) {
            return Err(EditSpecError::InvalidSpeed);
        }
        for fade in [self.timing.fade_in_seconds, self.timing.fade_out_seconds] {
            if !fade.is_finite() || fade < 0.0 {
                return Err(EditSpecError::InvalidFade);
            }
        }
        if let Some(trim) = self.timing.trim {
            trim.validate()?;
        }
        for segment in &self.timing.segments {
            segment.validate()?;
        }
        for pair in self.timing.segments.windows(2) {
            if pair[1].start_seconds < pair[0].end_seconds {
                return Err(EditSpecError::OverlappingSegments);
            }
        }
        if let Some(crop) = self.geometry.crop {
            crop.validate()?;
        }
        if let Some(scale) = self.geometry.scale {
            scale.validate()?;
        }
        if let Some(aspect) = self.geometry.pad_aspect {
            aspect.validate()?;
        }
        if let Some(censor) = &self.geometry.censor {
            censor.rect.validate()?;
        }
        if !self.video.brightness.is_finite()
            || !(-1.0..=1.0).contains(&self.video.brightness)
            || !self.video.contrast.is_finite()
            || !(0.0..=3.0).contains(&self.video.contrast)
            || !self.video.saturation.is_finite()
            || !(0.0..=3.0).contains(&self.video.saturation)
            || !self.video.sharpen.is_finite()
            || !(0.0..=5.0).contains(&self.video.sharpen)
            || !self.video.grain.is_finite()
            || !(0.0..=100.0).contains(&self.video.grain)
        {
            return Err(EditSpecError::InvalidVideoEffect);
        }
        if let Some(curves) = &self.video.curves {
            curves.validate()?;
        }
        if let Some(lut) = &self.video.lut {
            lut.validate()?;
        }
        if !self.audio.volume.is_finite() || !(0.0..=4.0).contains(&self.audio.volume) {
            return Err(EditSpecError::InvalidAudioEffect);
        }
        self.extensions.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSpecError {
    UnsupportedSchema(u32),
    InvalidTimeRange,
    OverlappingSegments,
    InvalidRectangle,
    InvalidScale,
    InvalidRotation,
    InvalidAspectRatio,
    InvalidCensorColor,
    InvalidLook,
    InvalidSpeed,
    InvalidFade,
    InvalidVideoEffect,
    InvalidToneCurve,
    InvalidLut,
    InvalidAudioEffect,
    InvalidAssetReference,
    InvalidColor,
    InvalidClip,
    InvalidTransition,
    InvalidOverlay,
    InvalidTitle,
    InvalidSubtitles,
    InvalidAudioTrack,
    InvalidAudioDynamics,
    InvalidMotion,
    InvalidSpatial,
    InvalidColorGrade,
}

impl fmt::Display for EditSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid edit specification: {self:?}")
    }
}

impl std::error::Error for EditSpecError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec() -> EditSpec {
        EditSpec::new(
            TimingSpec {
                trim: Some(TimeRange::new(1.0, 2.0).unwrap()),
                segments: Vec::new(),
                speed: 1.0,
                reverse: false,
                fade_in_seconds: 0.0,
                fade_out_seconds: 0.0,
            },
            GeometrySpec {
                crop: None,
                scale: None,
                rotation: Rotation::None,
                flip_horizontal: false,
                flip_vertical: false,
                pad_aspect: None,
                censor: None,
            },
            VideoEffects {
                brightness: 0.0,
                contrast: 1.0,
                saturation: 1.0,
                look: None,
                vignette: false,
                denoise: false,
                sharpen: 0.0,
                grain: 0.0,
                curves: None,
                lut: None,
            },
            AudioEffects {
                muted: false,
                volume: 1.0,
                normalize: false,
                highpass: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn untouched_extensions_do_not_change_the_spec_serialization() {
        // The plan fingerprint hashes this JSON, so an edit that uses no
        // parity-wave feature must serialize exactly as it did before them.
        let value = serde_json::to_value(valid_spec()).unwrap();
        assert!(value.get("extensions").is_none(), "{value}");

        let request: crate::model::EditRequest =
            serde_json::from_value(serde_json::json!({ "videoId": "x" })).unwrap();
        assert!(EditExtensions::from_request(&request).unwrap().is_empty());
    }

    #[test]
    fn populated_extensions_round_trip_through_the_spec() {
        let request: crate::model::EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "x",
            "overlays": [{
                "assetId": "ast_0123456789abcdef",
                "kind": "image",
                "x": 0.05, "y": 0.05, "width": 0.25
            }],
            "colorAdvanced": { "temperature": 0.2 },
            "stabilize": { "mode": "fast" },
            "segmentTransition": { "kind": "wipeleft", "duration": 0.5 }
        }))
        .unwrap();
        let extensions = EditExtensions::from_request(&request).unwrap();
        assert!(!extensions.is_empty());

        let spec = valid_spec().with_extensions(extensions).unwrap();
        assert_eq!(spec.overlays().len(), 1);
        assert_eq!(spec.overlays()[0].asset_id(), "ast_0123456789abcdef");
        assert_eq!(spec.color_advanced().unwrap().temperature(), 0.2);
        assert!(spec.stabilize().is_some());
        assert_eq!(
            spec.segment_transition().unwrap().kind().ffmpeg_name(),
            "wipeleft"
        );
        assert!(spec.motion().is_none() && spec.composition().is_none());
        assert!(spec.titles().is_empty() && spec.audio_tracks().is_empty());

        let value = serde_json::to_value(&spec).unwrap();
        assert!(value.get("extensions").is_some());
        assert_eq!(serde_json::from_value::<EditSpec>(value).unwrap(), spec);
    }

    #[test]
    fn serde_cannot_bypass_extension_invariants() {
        let request: crate::model::EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "x",
            "titles": [{ "text": "Заголовок", "x": 0.5, "y": 0.85 }]
        }))
        .unwrap();
        let spec = valid_spec()
            .with_extensions(EditExtensions::from_request(&request).unwrap())
            .unwrap();
        let mut tampered = serde_json::to_value(spec).unwrap();
        tampered["extensions"]["titles"][0]["fontSize"] = serde_json::json!(100_000.0);
        assert!(serde_json::from_value::<EditSpec>(tampered).is_err());
    }

    #[test]
    fn speed_ramps_are_range_checked_like_every_other_track() {
        let ramp = |value: f64| serde_json::json!({ "videoId": "x", "speedRamps": [{ "t": 0.0, "v": value }] });
        let ok: crate::model::EditRequest = serde_json::from_value(ramp(2.0)).unwrap();
        let extensions = EditExtensions::from_request(&ok).unwrap();
        assert!(valid_spec().with_extensions(extensions).is_ok());

        let hot: crate::model::EditRequest = serde_json::from_value(ramp(99.0)).unwrap();
        let extensions = EditExtensions::from_request(&hot).unwrap();
        assert_eq!(
            valid_spec().with_extensions(extensions),
            Err(EditSpecError::InvalidMotion)
        );
    }

    #[test]
    fn collection_caps_are_enforced_at_the_wire_boundary() {
        let overlay = serde_json::json!({
            "assetId": "ast_0123456789abcdef",
            "kind": "image",
            "x": 0.0, "y": 0.0, "width": 0.1
        });
        let request: crate::model::EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "x",
            "overlays": vec![overlay; MAX_OVERLAYS + 1]
        }))
        .unwrap();
        assert_eq!(
            EditExtensions::from_request(&request),
            Err(EditSpecError::InvalidOverlay)
        );
    }

    #[test]
    fn serde_cannot_bypass_edit_invariants() {
        let value = serde_json::to_value(valid_spec()).unwrap();
        let mut invalid = value.clone();
        invalid["timing"]["speed"] = serde_json::json!(0.0);
        assert!(serde_json::from_value::<EditSpec>(invalid).is_err());

        let mut invalid = value;
        invalid["geometry"]["scale"] = serde_json::json!({"width": -1, "height": -2});
        assert!(serde_json::from_value::<EditSpec>(invalid).is_err());
    }

    #[test]
    fn overlapping_segments_are_rejected() {
        let mut spec = valid_spec();
        spec.timing.trim = None;
        spec.timing.segments = vec![
            TimeRange::new(0.0, 2.0).unwrap(),
            TimeRange::new(1.0, 3.0).unwrap(),
        ];
        assert_eq!(spec.validate(), Err(EditSpecError::OverlappingSegments));
    }

    #[test]
    fn tone_curve_requires_normalized_ordered_endpoints() {
        let valid = ToneCurve::new(vec![
            ToneCurvePoint::new(0.0, 0.1),
            ToneCurvePoint::new(0.4, 0.5),
            ToneCurvePoint::new(1.0, 0.9),
        ])
        .unwrap();
        assert_eq!(valid.points().len(), 3);

        for points in [
            vec![ToneCurvePoint::new(0.0, 0.0)],
            vec![ToneCurvePoint::new(0.1, 0.0), ToneCurvePoint::new(1.0, 1.0)],
            vec![ToneCurvePoint::new(0.0, 0.0), ToneCurvePoint::new(0.0, 1.0)],
            vec![
                ToneCurvePoint::new(0.0, 0.0),
                ToneCurvePoint::new(0.5, 0.2),
                ToneCurvePoint::new(0.500001, 0.8),
                ToneCurvePoint::new(1.0, 1.0),
            ],
            vec![ToneCurvePoint::new(0.0, 0.0), ToneCurvePoint::new(1.0, 1.1)],
        ] {
            assert_eq!(ToneCurve::new(points), Err(EditSpecError::InvalidToneCurve));
        }
    }

    #[test]
    fn lut_grade_rejects_paths_and_invalid_intensity() {
        assert!(LutGrade::new("asset-1".into(), 0.5).unwrap().is_some());
        assert!(LutGrade::new("asset-1".into(), 0.0).unwrap().is_none());
        assert_eq!(
            LutGrade::new("../look.cube".into(), 1.0),
            Err(EditSpecError::InvalidLut)
        );
        assert_eq!(
            LutGrade::new("asset".into(), 1.1),
            Err(EditSpecError::InvalidLut)
        );
    }

    #[test]
    fn look_preset_wire_contract_rejects_arbitrary_filters() {
        let expected = [
            (LookPreset::Grayscale, "grayscale"),
            (LookPreset::Sepia, "sepia"),
            (LookPreset::Warm, "warm"),
            (LookPreset::Cold, "cold"),
            (LookPreset::TealOrange, "teal-orange"),
            (LookPreset::Faded, "faded"),
            (LookPreset::Noir, "noir"),
            (LookPreset::Vintage, "vintage"),
        ];

        assert_eq!(LookPreset::ALL, expected.map(|(preset, _)| preset));
        for (preset, id) in expected {
            assert_eq!(preset.wire_id(), id);
            assert_eq!(LookPreset::parse(id), Ok(preset));
            assert_eq!(serde_json::to_value(preset).unwrap(), serde_json::json!(id));
            assert_eq!(
                serde_json::from_value::<LookPreset>(serde_json::json!(id)).unwrap(),
                preset
            );
        }

        for arbitrary_filter in ["hue=s=0", "noir,eq=contrast=2", "scale"] {
            assert_eq!(
                LookPreset::parse(arbitrary_filter),
                Err(EditSpecError::InvalidLook)
            );
        }
    }
}
