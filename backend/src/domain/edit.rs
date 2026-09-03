//! Immutable edit semantics shared by planning and media adapters.
//!
//! Transport identifiers and wire defaults stop before this module. Values are
//! grouped by the part of the media pipeline they affect and validated again
//! after deserialization so persisted plans cannot bypass domain invariants.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const EDIT_SPEC_SCHEMA_VERSION: u32 = 1;
pub const MAX_TIMELINE_SEGMENTS: usize = 512;
pub const MAX_TIMELINE_OUTPUT_SECONDS: f64 = 24.0 * 60.0 * 60.0;

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

        if !valid_dimension(self.width) || !valid_dimension(self.height) {
            return Err(EditSpecError::InvalidScale);
        }
        let width_is_automatic = matches!(self.width, -2 | -1);
        let height_is_automatic = matches!(self.height, -2 | -1);
        if width_is_automatic && height_is_automatic {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RgbColor {
    red: u8,
    green: u8,
    blue: u8,
}

impl RgbColor {
    fn parse(value: &str) -> Result<Self, EditSpecError> {
        let hex = value.strip_prefix('#').unwrap_or(value);
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(EditSpecError::InvalidChromaKey);
        }
        let packed = u32::from_str_radix(hex, 16).map_err(|_| EditSpecError::InvalidChromaKey)?;
        Ok(Self {
            red: ((packed >> 16) & 0xff) as u8,
            green: ((packed >> 8) & 0xff) as u8,
            blue: (packed & 0xff) as u8,
        })
    }

    fn ffmpeg_hex(self) -> String {
        format!("0x{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    fn spill_screen(self) -> &'static str {
        if self.green >= self.blue {
            "green"
        } else {
            "blue"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaKeySpec {
    key_color: RgbColor,
    similarity: f64,
    blend: f64,
    spill_suppression: f64,
}

impl ChromaKeySpec {
    pub(crate) fn new(
        key_color: &str,
        similarity: f64,
        blend: f64,
        spill_suppression: f64,
    ) -> Result<Self, EditSpecError> {
        let value = Self {
            key_color: RgbColor::parse(key_color)?,
            similarity,
            blend,
            spill_suppression,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn ffmpeg_key_color(&self) -> String {
        self.key_color.ffmpeg_hex()
    }

    pub fn similarity(&self) -> f64 {
        self.similarity
    }

    pub fn blend(&self) -> f64 {
        self.blend
    }

    pub fn spill_suppression(&self) -> f64 {
        self.spill_suppression
    }

    pub fn ffmpeg_spill_screen(&self) -> &'static str {
        self.key_color.spill_screen()
    }

    fn validate(&self) -> Result<(), EditSpecError> {
        if !self.similarity.is_finite()
            || !(0.00001..=1.0).contains(&self.similarity)
            || !self.blend.is_finite()
            || !(0.0..=1.0).contains(&self.blend)
            || !self.spill_suppression.is_finite()
            || !(0.0..=1.0).contains(&self.spill_suppression)
        {
            return Err(EditSpecError::InvalidChromaKey);
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

pub const MIN_EDIT_SPEED: f64 = 0.05;
pub const MAX_EDIT_SPEED: f64 = 16.0;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslBand {
    pub(crate) hue: f64,
    pub(crate) saturation: f64,
    pub(crate) lightness: f64,
}

impl HslBand {
    fn validate(self) -> Result<(), EditSpecError> {
        if self.hue.is_finite()
            && (-180.0..=180.0).contains(&self.hue)
            && self.saturation.is_finite()
            && (-1.0..=1.0).contains(&self.saturation)
            && self.lightness.is_finite()
            && (-1.0..=1.0).contains(&self.lightness)
        {
            Ok(())
        } else {
            Err(EditSpecError::InvalidHslAdjustment)
        }
    }

    pub(crate) fn is_identity(self) -> bool {
        self.hue.abs() <= 1e-9 && self.saturation.abs() <= 1e-9 && self.lightness.abs() <= 1e-9
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslAdjustments {
    pub(crate) red: HslBand,
    pub(crate) yellow: HslBand,
    pub(crate) green: HslBand,
    pub(crate) cyan: HslBand,
    pub(crate) blue: HslBand,
    pub(crate) magenta: HslBand,
}

impl HslAdjustments {
    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        for band in [
            self.red,
            self.yellow,
            self.green,
            self.cyan,
            self.blue,
            self.magenta,
        ] {
            band.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorWheel {
    pub(crate) red: f64,
    pub(crate) green: f64,
    pub(crate) blue: f64,
}

impl ColorWheel {
    fn validate(self) -> Result<(), EditSpecError> {
        if [self.red, self.green, self.blue]
            .into_iter()
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
        {
            Ok(())
        } else {
            Err(EditSpecError::InvalidColorWheels)
        }
    }

    pub(crate) fn is_identity(self) -> bool {
        self.red.abs() <= 1e-9 && self.green.abs() <= 1e-9 && self.blue.abs() <= 1e-9
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorWheels {
    pub(crate) shadows: ColorWheel,
    pub(crate) midtones: ColorWheel,
    pub(crate) highlights: ColorWheel,
    pub(crate) preserve_luminosity: bool,
}

impl ColorWheels {
    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        self.shadows.validate()?;
        self.midtones.validate()?;
        self.highlights.validate()
    }

    pub(crate) fn is_identity(&self) -> bool {
        self.shadows.is_identity() && self.midtones.is_identity() && self.highlights.is_identity()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoEffects {
    pub(crate) brightness: f64,
    pub(crate) contrast: f64,
    pub(crate) saturation: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hsl: Option<HslAdjustments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) color_wheels: Option<ColorWheels>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chroma_key: Option<ChromaKeySpec>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioEq {
    pub(crate) low_gain_db: f64,
    pub(crate) mid_gain_db: f64,
    pub(crate) high_gain_db: f64,
}

impl AudioEq {
    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if [self.low_gain_db, self.mid_gain_db, self.high_gain_db]
            .into_iter()
            .all(|gain| gain.is_finite() && (-24.0..=24.0).contains(&gain))
        {
            Ok(())
        } else {
            Err(EditSpecError::InvalidAudioEq)
        }
    }

    pub(crate) fn is_identity(self) -> bool {
        self.low_gain_db.abs() <= 1e-9
            && self.mid_gain_db.abs() <= 1e-9
            && self.high_gain_db.abs() <= 1e-9
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioCompressor {
    pub(crate) threshold_db: f64,
    pub(crate) ratio: f64,
    pub(crate) attack_ms: f64,
    pub(crate) release_ms: f64,
    pub(crate) makeup_gain_db: f64,
}

impl AudioCompressor {
    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if self.threshold_db.is_finite()
            && (-60.0..=0.0).contains(&self.threshold_db)
            && self.ratio.is_finite()
            && (1.0..=20.0).contains(&self.ratio)
            && self.attack_ms.is_finite()
            && (0.01..=2_000.0).contains(&self.attack_ms)
            && self.release_ms.is_finite()
            && (0.01..=9_000.0).contains(&self.release_ms)
            && self.makeup_gain_db.is_finite()
            && (-36.0..=36.0).contains(&self.makeup_gain_db)
        {
            Ok(())
        } else {
            Err(EditSpecError::InvalidAudioCompressor)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioLimiter {
    pub(crate) ceiling_db: f64,
    pub(crate) release_ms: f64,
}

impl AudioLimiter {
    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if self.ceiling_db.is_finite()
            && (-24.0..=0.0).contains(&self.ceiling_db)
            && self.release_ms.is_finite()
            && (1.0..=8_000.0).contains(&self.release_ms)
        {
            Ok(())
        } else {
            Err(EditSpecError::InvalidAudioLimiter)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioEffects {
    pub(crate) muted: bool,
    pub(crate) volume: f64,
    pub(crate) normalize: bool,
    pub(crate) highpass: bool,
    #[serde(default)]
    pub(crate) pan: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) eq: Option<AudioEq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) compressor: Option<AudioCompressor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limiter: Option<AudioLimiter>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditSpec {
    pub schema_version: u32,
    pub(crate) timing: TimingSpec,
    pub(crate) geometry: GeometrySpec,
    pub(crate) video: VideoEffects,
    pub(crate) audio: AudioEffects,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditSpecWire {
    schema_version: u32,
    timing: TimingSpec,
    geometry: GeometrySpec,
    video: VideoEffects,
    audio: AudioEffects,
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
        Self::new(wire.timing, wire.geometry, wire.video, wire.audio).map_err(D::Error::custom)
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
        };
        value.validate()?;
        Ok(value)
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

    pub fn validate(&self) -> Result<(), EditSpecError> {
        if self.schema_version != EDIT_SPEC_SCHEMA_VERSION {
            return Err(EditSpecError::UnsupportedSchema(self.schema_version));
        }
        if !self.timing.speed.is_finite()
            || !(MIN_EDIT_SPEED..=MAX_EDIT_SPEED).contains(&self.timing.speed)
        {
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
        if self.timing.segments.len() > MAX_TIMELINE_SEGMENTS {
            return Err(EditSpecError::TooManyTimelineSegments);
        }
        let timeline_output_seconds = self
            .timing
            .segments
            .iter()
            .map(|segment| segment.duration_seconds())
            .sum::<f64>()
            / self.timing.speed;
        if !timeline_output_seconds.is_finite()
            || timeline_output_seconds > MAX_TIMELINE_OUTPUT_SECONDS
        {
            return Err(EditSpecError::TimelineTooLong);
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
        if let Some(hsl) = &self.video.hsl {
            hsl.validate()?;
        }
        if let Some(color_wheels) = &self.video.color_wheels {
            color_wheels.validate()?;
        }
        if let Some(chroma_key) = &self.video.chroma_key {
            chroma_key.validate()?;
        }
        if let Some(lut) = &self.video.lut {
            lut.validate()?;
        }
        if !self.audio.volume.is_finite()
            || !(0.0..=4.0).contains(&self.audio.volume)
            || !self.audio.pan.is_finite()
            || !(-1.0..=1.0).contains(&self.audio.pan)
        {
            return Err(EditSpecError::InvalidAudioEffect);
        }
        if let Some(eq) = self.audio.eq {
            eq.validate()?;
        }
        if let Some(compressor) = self.audio.compressor {
            compressor.validate()?;
        }
        if let Some(limiter) = self.audio.limiter {
            limiter.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditSpecError {
    UnsupportedSchema(u32),
    InvalidTimeRange,
    InvalidRectangle,
    InvalidScale,
    InvalidRotation,
    InvalidAspectRatio,
    InvalidCensorColor,
    InvalidLook,
    InvalidSpeed,
    InvalidFade,
    TooManyTimelineSegments,
    TimelineTooLong,
    InvalidVideoEffect,
    InvalidHslAdjustment,
    InvalidColorWheels,
    InvalidChromaKey,
    InvalidToneCurve,
    InvalidLut,
    InvalidAudioEffect,
    InvalidAudioEq,
    InvalidAudioCompressor,
    InvalidAudioLimiter,
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
                hsl: None,
                color_wheels: None,
                chroma_key: None,
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
                pan: 0.0,
                eq: None,
                compressor: None,
                limiter: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn serde_cannot_bypass_edit_invariants() {
        let value = serde_json::to_value(valid_spec()).unwrap();
        let mut invalid = value.clone();
        invalid["timing"]["speed"] = serde_json::json!(0.0);
        assert!(serde_json::from_value::<EditSpec>(invalid).is_err());

        let mut invalid = value.clone();
        invalid["geometry"]["scale"] = serde_json::json!({"width": -1, "height": -2});
        assert!(serde_json::from_value::<EditSpec>(invalid).is_err());

        let mut invalid = value;
        invalid["timing"]["segments"] = serde_json::to_value(vec![
            TimeRange::new(0.0, 1.0).unwrap();
            MAX_TIMELINE_SEGMENTS + 1
        ])
        .unwrap();
        assert!(serde_json::from_value::<EditSpec>(invalid).is_err());
    }

    #[test]
    fn primitive_boundaries_are_independent_and_exact() {
        assert!(TimeRange::new(0.0, 0.011).is_ok());
        assert!(TimeRange::new(1.0, 1.011).is_ok());
        for (start, end) in [
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (-0.1, 1.0),
            (0.0, 0.01),
            (1.0, 1.005),
        ] {
            assert_eq!(
                TimeRange::new(start, end),
                Err(EditSpecError::InvalidTimeRange)
            );
        }

        for scale in [
            OutputScale {
                width: -2,
                height: 2,
            },
            OutputScale {
                width: 7_680,
                height: -1,
            },
        ] {
            assert_eq!(scale.validate(), Ok(()));
        }
        for scale in [
            OutputScale {
                width: -1,
                height: -2,
            },
            OutputScale {
                width: 0,
                height: 2,
            },
            OutputScale {
                width: 2,
                height: 1,
            },
            OutputScale {
                width: 7_681,
                height: 2,
            },
            OutputScale {
                width: 2,
                height: 7_681,
            },
        ] {
            assert_eq!(scale.validate(), Err(EditSpecError::InvalidScale));
        }
    }

    #[test]
    fn edit_effect_and_timeline_boundaries_fail_one_at_a_time() {
        let mut spec = valid_spec();
        spec.timing.segments = vec![TimeRange::new(0.0, 1.0).unwrap(); MAX_TIMELINE_SEGMENTS];
        spec.timing.trim = None;
        assert_eq!(spec.validate(), Ok(()));

        spec.timing.segments = vec![TimeRange::new(0.0, 3_600.0).unwrap(); 24];
        assert_eq!(spec.validate(), Ok(()));
        spec.timing
            .segments
            .push(TimeRange::new(0.0, 0.011).unwrap());
        assert_eq!(spec.validate(), Err(EditSpecError::TimelineTooLong));

        for speed in [f64::NAN, 0.049, 16.01] {
            let mut invalid = valid_spec();
            invalid.timing.speed = speed;
            assert_eq!(invalid.validate(), Err(EditSpecError::InvalidSpeed));
        }
        for speed in [MIN_EDIT_SPEED, MAX_EDIT_SPEED] {
            let mut valid = valid_spec();
            valid.timing.speed = speed;
            assert_eq!(valid.validate(), Ok(()));
        }
        for fade in [f64::NAN, -0.1] {
            let mut invalid = valid_spec();
            invalid.timing.fade_in_seconds = fade;
            assert_eq!(invalid.validate(), Err(EditSpecError::InvalidFade));
        }

        for (field, value) in [
            ("brightness", f64::NAN),
            ("brightness", 1.01),
            ("contrast", f64::NAN),
            ("contrast", -0.01),
            ("saturation", f64::NAN),
            ("saturation", 3.01),
            ("sharpen", f64::NAN),
            ("sharpen", 5.01),
            ("grain", f64::NAN),
            ("grain", 100.01),
        ] {
            let mut invalid = valid_spec();
            match field {
                "brightness" => invalid.video.brightness = value,
                "contrast" => invalid.video.contrast = value,
                "saturation" => invalid.video.saturation = value,
                "sharpen" => invalid.video.sharpen = value,
                "grain" => invalid.video.grain = value,
                _ => unreachable!(),
            }
            assert_eq!(invalid.validate(), Err(EditSpecError::InvalidVideoEffect));
        }

        for (field, value) in [
            ("volume", f64::NAN),
            ("volume", 4.01),
            ("pan", f64::NAN),
            ("pan", 1.01),
        ] {
            let mut invalid = valid_spec();
            match field {
                "volume" => invalid.audio.volume = value,
                "pan" => invalid.audio.pan = value,
                _ => unreachable!(),
            }
            assert_eq!(invalid.validate(), Err(EditSpecError::InvalidAudioEffect));
        }
    }

    #[test]
    fn validates_selective_hsl_and_color_wheel_bounds() {
        let mut spec = valid_spec();
        spec.video.hsl = Some(HslAdjustments {
            red: HslBand {
                hue: 180.0,
                saturation: -1.0,
                lightness: 1.0,
            },
            ..HslAdjustments::default()
        });
        spec.video.color_wheels = Some(ColorWheels {
            shadows: ColorWheel {
                red: -1.0,
                green: 0.0,
                blue: 1.0,
            },
            midtones: ColorWheel::default(),
            highlights: ColorWheel::default(),
            preserve_luminosity: true,
        });
        assert_eq!(spec.validate(), Ok(()));

        spec.video.hsl.as_mut().unwrap().cyan.hue = 180.01;
        assert_eq!(spec.validate(), Err(EditSpecError::InvalidHslAdjustment));
        spec.video.hsl.as_mut().unwrap().cyan.hue = 0.0;
        spec.video.color_wheels.as_mut().unwrap().midtones.green = f64::NAN;
        assert_eq!(spec.validate(), Err(EditSpecError::InvalidColorWheels));
    }

    #[test]
    fn validates_deterministic_audio_dsp_bounds() {
        let mut spec = valid_spec();
        spec.audio.pan = -1.0;
        spec.audio.eq = Some(AudioEq {
            low_gain_db: -24.0,
            mid_gain_db: 0.0,
            high_gain_db: 24.0,
        });
        spec.audio.compressor = Some(AudioCompressor {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 200.0,
            makeup_gain_db: 3.0,
        });
        spec.audio.limiter = Some(AudioLimiter {
            ceiling_db: -1.0,
            release_ms: 50.0,
        });
        assert_eq!(spec.validate(), Ok(()));

        spec.audio.eq.as_mut().unwrap().low_gain_db = -24.01;
        assert_eq!(spec.validate(), Err(EditSpecError::InvalidAudioEq));
        spec.audio.eq.as_mut().unwrap().low_gain_db = 0.0;
        spec.audio.compressor.as_mut().unwrap().ratio = 20.01;
        assert_eq!(spec.validate(), Err(EditSpecError::InvalidAudioCompressor));
        spec.audio.compressor.as_mut().unwrap().ratio = 4.0;
        spec.audio.limiter.as_mut().unwrap().ceiling_db = -24.01;
        assert_eq!(spec.validate(), Err(EditSpecError::InvalidAudioLimiter));
    }

    #[test]
    fn ordered_duplicate_and_overlapping_segments_are_allowed() {
        let mut spec = valid_spec();
        spec.timing.trim = None;
        spec.timing.segments = vec![
            TimeRange::new(4.0, 5.0).unwrap(),
            TimeRange::new(1.0, 3.0).unwrap(),
            TimeRange::new(1.0, 3.0).unwrap(),
            TimeRange::new(2.0, 4.0).unwrap(),
        ];
        assert_eq!(spec.validate(), Ok(()));
    }

    #[test]
    fn timeline_caps_are_domain_invariants_and_account_for_speed() {
        let mut spec = valid_spec();
        spec.timing.trim = None;
        spec.timing.segments = vec![TimeRange::new(0.0, 1.0).unwrap(); MAX_TIMELINE_SEGMENTS + 1];
        assert_eq!(spec.validate(), Err(EditSpecError::TooManyTimelineSegments));

        spec.timing.segments = vec![TimeRange::new(0.0, 3600.0).unwrap(); 25];
        spec.timing.speed = 2.0;
        assert_eq!(spec.validate(), Ok(()));

        spec.timing.speed = 0.5;
        assert_eq!(spec.validate(), Err(EditSpecError::TimelineTooLong));
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
    fn chroma_key_canonicalizes_hex_and_validates_controls() {
        let key = ChromaKeySpec::new("#00Ff20", 0.2, 0.08, 0.6).unwrap();
        assert_eq!(key.ffmpeg_key_color(), "0x00ff20");
        assert_eq!(key.ffmpeg_spill_screen(), "green");
        assert_eq!(key.similarity(), 0.2);
        assert_eq!(key.blend(), 0.08);
        assert_eq!(key.spill_suppression(), 0.6);

        let blue = ChromaKeySpec::new("0000ff", 0.1, 0.0, 1.0).unwrap();
        assert_eq!(blue.ffmpeg_spill_screen(), "blue");

        for invalid in [
            ChromaKeySpec::new("green", 0.1, 0.0, 0.0),
            ChromaKeySpec::new("#00ff00;scale=1:1", 0.1, 0.0, 0.0),
            ChromaKeySpec::new("#00ff00", 0.0, 0.0, 0.0),
            ChromaKeySpec::new("#00ff00", 0.1, 1.1, 0.0),
            ChromaKeySpec::new("#00ff00", 0.1, 0.0, -0.1),
        ] {
            assert_eq!(invalid, Err(EditSpecError::InvalidChromaKey));
        }
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
