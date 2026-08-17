//! Keyframed transform of the framed image: Ken Burns zoom, pan and rotation.
//! Speed ramps live next to it on `EditSpec` because they share the same
//! output-timeline keyframe representation.

use serde::{Deserialize, Serialize};

use super::edit::{keyframe_track, EditSpecError};
use super::keyframes::KeyframeTrack;
use crate::model;

/// 1.0 fits the frame; larger values punch in.
pub const MIN_ZOOM: f64 = 1.0;
pub const MAX_ZOOM: f64 = 8.0;
/// Pan is expressed in frames: -1 is a full frame left/up, 1 right/down.
pub const MAX_PAN: f64 = 1.0;
pub const MAX_ROTATION_DEGREES: f64 = 360.0;
/// Speed ramp multipliers share the clip speed range.
pub const MIN_RAMP_SPEED: f64 = 0.25;
pub const MAX_RAMP_SPEED: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MotionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) zoom: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pan_x: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pan_y: Option<KeyframeTrack<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rotation: Option<KeyframeTrack<f64>>,
}

impl MotionSpec {
    /// A motion block with no populated track means the parameter is unused,
    /// so it maps to `None` instead of an empty transform.
    pub fn from_wire(value: &model::Motion) -> Result<Option<Self>, EditSpecError> {
        let spec = Self {
            zoom: keyframe_track(&value.zoom, EditSpecError::InvalidMotion)?,
            pan_x: keyframe_track(&value.pan_x, EditSpecError::InvalidMotion)?,
            pan_y: keyframe_track(&value.pan_y, EditSpecError::InvalidMotion)?,
            rotation: keyframe_track(&value.rotation, EditSpecError::InvalidMotion)?,
        };
        if spec.is_empty() {
            return Ok(None);
        }
        spec.validate()?;
        Ok(Some(spec))
    }

    pub fn is_empty(&self) -> bool {
        self.zoom.is_none()
            && self.pan_x.is_none()
            && self.pan_y.is_none()
            && self.rotation.is_none()
    }

    pub fn zoom(&self) -> Option<&KeyframeTrack<f64>> {
        self.zoom.as_ref()
    }

    pub fn pan_x(&self) -> Option<&KeyframeTrack<f64>> {
        self.pan_x.as_ref()
    }

    pub fn pan_y(&self) -> Option<&KeyframeTrack<f64>> {
        self.pan_y.as_ref()
    }

    pub fn rotation(&self) -> Option<&KeyframeTrack<f64>> {
        self.rotation.as_ref()
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        track_within(self.zoom.as_ref(), MIN_ZOOM, MAX_ZOOM)?;
        track_within(self.pan_x.as_ref(), -MAX_PAN, MAX_PAN)?;
        track_within(self.pan_y.as_ref(), -MAX_PAN, MAX_PAN)?;
        track_within(
            self.rotation.as_ref(),
            -MAX_ROTATION_DEGREES,
            MAX_ROTATION_DEGREES,
        )
    }
}

/// Every sampled value of a track has to stay inside the documented range;
/// clamping individual keyframes would silently distort the animation instead.
fn track_within(
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
        return Err(EditSpecError::InvalidMotion);
    }
    Ok(())
}

/// Shared by `EditSpec::speed_ramps`, which stores a bare track.
pub fn validate_speed_ramps(track: &KeyframeTrack<f64>) -> Result<(), EditSpecError> {
    track_within(Some(track), MIN_RAMP_SPEED, MAX_RAMP_SPEED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyframes(values: &[(f64, f64)]) -> Vec<model::Keyframe> {
        values
            .iter()
            .map(|(t, v)| model::Keyframe {
                t: *t,
                v: *v,
                interp: model::KeyframeInterpolation::Smooth,
            })
            .collect()
    }

    fn motion(zoom: &[(f64, f64)]) -> model::Motion {
        model::Motion {
            zoom: keyframes(zoom),
            pan_x: Vec::new(),
            pan_y: Vec::new(),
            rotation: Vec::new(),
        }
    }

    #[test]
    fn an_empty_motion_block_is_not_a_transform() {
        assert!(MotionSpec::from_wire(&motion(&[])).unwrap().is_none());
    }

    #[test]
    fn zoom_keyframes_keep_their_easing_and_timing() {
        let spec = MotionSpec::from_wire(&motion(&[(0.0, 1.0), (4.0, 1.5)]))
            .unwrap()
            .unwrap();
        let zoom = spec.zoom().unwrap();
        assert_eq!(zoom.keyframes.len(), 2);
        assert_eq!(zoom.keyframes[1].tick, 4_000);
        assert_eq!(
            zoom.interpolation,
            super::super::keyframes::Interpolation::EaseInOutCubic
        );
        assert_eq!(zoom.sample_seconds(4.0).unwrap(), 1.5);
    }

    #[test]
    fn out_of_range_and_non_finite_keyframes_are_rejected() {
        assert_eq!(
            MotionSpec::from_wire(&motion(&[(0.0, 1.0), (1.0, 99.0)])),
            Err(EditSpecError::InvalidMotion)
        );
        assert_eq!(
            MotionSpec::from_wire(&motion(&[(0.0, f64::NAN)])),
            Err(EditSpecError::InvalidMotion)
        );
        assert_eq!(
            MotionSpec::from_wire(&motion(&[(-1.0, 1.0)])),
            Err(EditSpecError::InvalidMotion)
        );
    }

    #[test]
    fn keyframe_tracks_are_capped_at_sixty_four_points() {
        let long: Vec<_> = (0..=64).map(|index| (index as f64, 1.0)).collect();
        assert_eq!(
            MotionSpec::from_wire(&motion(&long)),
            Err(EditSpecError::InvalidMotion)
        );
    }
}
