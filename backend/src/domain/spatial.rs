//! Action-cam and 360 geometry: spherical reframing, stabilization and lens
//! distortion correction. These run before the ordinary geometry stage because
//! they change what the frame contains, not just how it is cropped.

use serde::{Deserialize, Serialize};

use super::edit::{clamped, keyframe_track, EditSpecError};
use super::keyframes::KeyframeTrack;
use crate::model;

const MIN_REFRAME_DIMENSION: u32 = 16;
const MAX_REFRAME_DIMENSION: u32 = 7_680;
const MIN_SMOOTHING: f64 = 1.0;
const MAX_SMOOTHING: f64 = 100.0;
const MAX_STABILIZE_ZOOM_PERCENT: f64 = 20.0;
const MAX_LENS_COEFFICIENT: f64 = 1.0;
const MAX_FOV_DEGREES: f64 = 360.0;
const MAX_ANGLE_DEGREES: f64 = 360.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputProjection {
    Equirect,
    Fisheye,
    DualFisheye,
}

impl InputProjection {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "equirect" => Ok(Self::Equirect),
            "fisheye" => Ok(Self::Fisheye),
            "dfisheye" => Ok(Self::DualFisheye),
            _ => Err(EditSpecError::InvalidSpatial),
        }
    }

    /// The `v360=input=` token. Fixed strings only, never client text.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Equirect => "e",
            Self::Fisheye => "fisheye",
            Self::DualFisheye => "dfisheye",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputProjection {
    Flat,
    Equirect,
    Fisheye,
    Stereographic,
    Pannini,
}

impl OutputProjection {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "flat" => Ok(Self::Flat),
            "equirect" => Ok(Self::Equirect),
            "fisheye" => Ok(Self::Fisheye),
            "stereographic" => Ok(Self::Stereographic),
            "pannini" => Ok(Self::Pannini),
            _ => Err(EditSpecError::InvalidSpatial),
        }
    }

    /// The `v360=output=` token.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Equirect => "e",
            Self::Fisheye => "fisheye",
            Self::Stereographic => "sg",
            Self::Pannini => "pannini",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reframe360Spec {
    pub(crate) input_projection: InputProjection,
    pub(crate) output_projection: OutputProjection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fov: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) yaw: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pitch: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) roll: Option<KeyframeTrack<f64>>,
    pub(crate) output_width: u32,
    pub(crate) output_height: u32,
    pub(crate) horizon_lock: bool,
}

impl Reframe360Spec {
    pub fn from_wire(value: &model::Reframe360) -> Result<Self, EditSpecError> {
        let spec = Self {
            input_projection: InputProjection::parse(&value.input_projection)?,
            output_projection: OutputProjection::parse(&value.output_projection)?,
            fov: keyframe_track(&value.fov, EditSpecError::InvalidSpatial)?,
            yaw: keyframe_track(&value.yaw, EditSpecError::InvalidSpatial)?,
            pitch: keyframe_track(&value.pitch, EditSpecError::InvalidSpatial)?,
            roll: keyframe_track(&value.roll, EditSpecError::InvalidSpatial)?,
            // Even dimensions keep every downstream encoder happy.
            output_width: value
                .output_width
                .clamp(MIN_REFRAME_DIMENSION, MAX_REFRAME_DIMENSION)
                & !1,
            output_height: value
                .output_height
                .clamp(MIN_REFRAME_DIMENSION, MAX_REFRAME_DIMENSION)
                & !1,
            horizon_lock: value.horizon_lock,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn input_projection(&self) -> InputProjection {
        self.input_projection
    }

    pub fn output_projection(&self) -> OutputProjection {
        self.output_projection
    }

    pub fn fov(&self) -> Option<&KeyframeTrack<f64>> {
        self.fov.as_ref()
    }

    pub fn yaw(&self) -> Option<&KeyframeTrack<f64>> {
        self.yaw.as_ref()
    }

    pub fn pitch(&self) -> Option<&KeyframeTrack<f64>> {
        self.pitch.as_ref()
    }

    pub fn roll(&self) -> Option<&KeyframeTrack<f64>> {
        self.roll.as_ref()
    }

    pub fn output_size(&self) -> (u32, u32) {
        (self.output_width, self.output_height)
    }

    pub fn horizon_lock(&self) -> bool {
        self.horizon_lock
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if !(MIN_REFRAME_DIMENSION..=MAX_REFRAME_DIMENSION).contains(&self.output_width)
            || !(MIN_REFRAME_DIMENSION..=MAX_REFRAME_DIMENSION).contains(&self.output_height)
        {
            return Err(EditSpecError::InvalidSpatial);
        }
        angles_within(self.fov.as_ref(), 1.0, MAX_FOV_DEGREES)?;
        for track in [self.yaw.as_ref(), self.pitch.as_ref(), self.roll.as_ref()] {
            angles_within(track, -MAX_ANGLE_DEGREES, MAX_ANGLE_DEGREES)?;
        }
        Ok(())
    }
}

fn angles_within(
    track: Option<&KeyframeTrack<f64>>,
    min: f64,
    max: f64,
) -> Result<(), EditSpecError> {
    let Some(track) = track else {
        return Ok(());
    };
    if track
        .keyframes
        .iter()
        .any(|keyframe| !keyframe.value.is_finite() || !(min..=max).contains(&keyframe.value))
    {
        return Err(EditSpecError::InvalidSpatial);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilizeMode {
    /// Single-pass `deshake`.
    Fast,
    /// Two-pass `vidstabdetect` + `vidstabtransform`.
    Precise,
}

impl StabilizeMode {
    /// `off` is not a mode: it maps to no stabilization at all.
    pub fn parse(value: &str) -> Result<Option<Self>, EditSpecError> {
        match value {
            "off" => Ok(None),
            "fast" => Ok(Some(Self::Fast)),
            "precise" => Ok(Some(Self::Precise)),
            _ => Err(EditSpecError::InvalidSpatial),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StabilizeSpec {
    pub(crate) mode: StabilizeMode,
    pub(crate) smoothing: f64,
    /// Extra crop-in percentage that hides the stabilized borders.
    pub(crate) zoom_percent: f64,
    pub(crate) horizon_lock: bool,
}

impl StabilizeSpec {
    pub fn from_wire(value: &model::Stabilize) -> Result<Option<Self>, EditSpecError> {
        let Some(mode) = StabilizeMode::parse(&value.mode)? else {
            return Ok(None);
        };
        let spec = Self {
            mode,
            smoothing: clamped(
                value.smoothing,
                MIN_SMOOTHING,
                MAX_SMOOTHING,
                EditSpecError::InvalidSpatial,
            )?,
            zoom_percent: clamped(
                value.zoom,
                0.0,
                MAX_STABILIZE_ZOOM_PERCENT,
                EditSpecError::InvalidSpatial,
            )?,
            horizon_lock: value.horizon_lock,
        };
        spec.validate()?;
        Ok(Some(spec))
    }

    pub fn mode(self) -> StabilizeMode {
        self.mode
    }

    pub fn smoothing(self) -> f64 {
        self.smoothing
    }

    pub fn zoom_percent(self) -> f64 {
        self.zoom_percent
    }

    pub fn horizon_lock(self) -> bool {
        self.horizon_lock
    }

    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if !(MIN_SMOOTHING..=MAX_SMOOTHING).contains(&self.smoothing)
            || !(0.0..=MAX_STABILIZE_ZOOM_PERCENT).contains(&self.zoom_percent)
        {
            return Err(EditSpecError::InvalidSpatial);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LensCorrectionSpec {
    pub(crate) k1: f64,
    pub(crate) k2: f64,
}

impl LensCorrectionSpec {
    /// All-zero coefficients are an identity correction, so they map to `None`.
    pub fn from_wire(value: &model::LensCorrection) -> Result<Option<Self>, EditSpecError> {
        let spec = Self {
            k1: clamped(
                value.k1,
                -MAX_LENS_COEFFICIENT,
                MAX_LENS_COEFFICIENT,
                EditSpecError::InvalidSpatial,
            )?,
            k2: clamped(
                value.k2,
                -MAX_LENS_COEFFICIENT,
                MAX_LENS_COEFFICIENT,
                EditSpecError::InvalidSpatial,
            )?,
        };
        if spec.k1.abs() < 1e-9 && spec.k2.abs() < 1e-9 {
            return Ok(None);
        }
        spec.validate()?;
        Ok(Some(spec))
    }

    pub fn k1(self) -> f64 {
        self.k1
    }

    pub fn k2(self) -> f64 {
        self.k2
    }

    pub(crate) fn validate(self) -> Result<(), EditSpecError> {
        if !(-MAX_LENS_COEFFICIENT..=MAX_LENS_COEFFICIENT).contains(&self.k1)
            || !(-MAX_LENS_COEFFICIENT..=MAX_LENS_COEFFICIENT).contains(&self.k2)
        {
            return Err(EditSpecError::InvalidSpatial);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reframe() -> model::Reframe360 {
        model::Reframe360 {
            input_projection: "equirect".into(),
            output_projection: "flat".into(),
            fov: Vec::new(),
            yaw: Vec::new(),
            pitch: Vec::new(),
            roll: Vec::new(),
            output_width: 1920,
            output_height: 1080,
            horizon_lock: true,
        }
    }

    #[test]
    fn projections_map_to_fixed_v360_tokens() {
        assert_eq!(
            InputProjection::parse("dfisheye").unwrap().ffmpeg_name(),
            "dfisheye"
        );
        assert_eq!(
            OutputProjection::parse("equirect").unwrap().ffmpeg_name(),
            "e"
        );
        assert_eq!(
            OutputProjection::parse("sg"),
            Err(EditSpecError::InvalidSpatial)
        );
        assert_eq!(
            InputProjection::parse("e:yaw=0'"),
            Err(EditSpecError::InvalidSpatial)
        );
    }

    #[test]
    fn reframe_dimensions_are_clamped_to_an_even_range() {
        let mut odd = reframe();
        odd.output_width = 1_921;
        odd.output_height = 99_999;
        let spec = Reframe360Spec::from_wire(&odd).unwrap();
        assert_eq!(spec.output_size(), (1_920, MAX_REFRAME_DIMENSION));

        let mut tiny = reframe();
        tiny.output_width = 1;
        assert_eq!(
            Reframe360Spec::from_wire(&tiny).unwrap().output_size().0,
            16
        );
    }

    #[test]
    fn reframe_angles_stay_inside_a_full_turn() {
        let mut spun = reframe();
        spun.yaw = vec![model::Keyframe {
            t: 0.0,
            v: 5_000.0,
            interp: model::KeyframeInterpolation::Linear,
        }];
        assert_eq!(
            Reframe360Spec::from_wire(&spun),
            Err(EditSpecError::InvalidSpatial)
        );
    }

    #[test]
    fn stabilization_off_and_identity_lens_are_no_ops() {
        let off = model::Stabilize {
            mode: "off".into(),
            smoothing: 10.0,
            zoom: 0.0,
            horizon_lock: false,
        };
        assert!(StabilizeSpec::from_wire(&off).unwrap().is_none());
        assert!(
            LensCorrectionSpec::from_wire(&model::LensCorrection { k1: 0.0, k2: 0.0 })
                .unwrap()
                .is_none()
        );

        let precise = model::Stabilize {
            mode: "precise".into(),
            smoothing: 999.0,
            zoom: 999.0,
            horizon_lock: true,
        };
        let spec = StabilizeSpec::from_wire(&precise).unwrap().unwrap();
        assert_eq!(spec.mode(), StabilizeMode::Precise);
        assert_eq!(spec.smoothing(), MAX_SMOOTHING);
        assert_eq!(spec.zoom_percent(), MAX_STABILIZE_ZOOM_PERCENT);
    }
}
