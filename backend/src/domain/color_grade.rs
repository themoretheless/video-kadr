//! Resolve-lite primary grade: white balance, exposure, highlight/shadow
//! recovery, lift/gamma/gain wheels and per-band HSL. It runs before the look
//! preset, curves and LUT so a creative look sits on a corrected image.

use serde::{Deserialize, Serialize};

use super::edit::{clamped, EditSpecError};
use crate::model;

pub const MAX_WHITE_BALANCE: f64 = 1.0;
pub const MAX_EXPOSURE_STOPS: f64 = 2.0;
pub const MAX_TONE_SHIFT: f64 = 1.0;
pub const MAX_LIFT: f64 = 0.5;
pub const MIN_GAMMA: f64 = 0.1;
pub const MAX_GAMMA: f64 = 4.0;
pub const MAX_GAIN: f64 = 4.0;
pub const MAX_HUE_SHIFT_DEGREES: f64 = 180.0;
pub const MAX_HSL_MULTIPLIER: f64 = 4.0;

/// A colour wheel triplet. The valid range depends on which wheel it is, so
/// the caller passes it in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RgbTriplet {
    pub(crate) r: f64,
    pub(crate) g: f64,
    pub(crate) b: f64,
}

impl RgbTriplet {
    pub fn from_wire(value: &model::Rgb, min: f64, max: f64) -> Result<Self, EditSpecError> {
        Ok(Self {
            r: clamped(value.r, min, max, EditSpecError::InvalidColorGrade)?,
            g: clamped(value.g, min, max, EditSpecError::InvalidColorGrade)?,
            b: clamped(value.b, min, max, EditSpecError::InvalidColorGrade)?,
        })
    }

    pub fn red(self) -> f64 {
        self.r
    }

    pub fn green(self) -> f64 {
        self.g
    }

    pub fn blue(self) -> f64 {
        self.b
    }

    fn within(self, min: f64, max: f64) -> bool {
        [self.r, self.g, self.b]
            .into_iter()
            .all(|channel| channel.is_finite() && (min..=max).contains(&channel))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HslBand {
    Red,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Magenta,
}

impl HslBand {
    pub const ALL: [Self; 7] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Magenta,
    ];

    pub const fn wire_id(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
            Self::Magenta => "magenta",
        }
    }

    /// Hue centre of the band in degrees, used to build the selection mask.
    pub const fn center_degrees(self) -> f64 {
        match self {
            Self::Red => 0.0,
            Self::Orange => 30.0,
            Self::Yellow => 60.0,
            Self::Green => 120.0,
            Self::Cyan => 180.0,
            Self::Blue => 240.0,
            Self::Magenta => 300.0,
        }
    }

    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        Self::ALL
            .into_iter()
            .find(|band| band.wire_id() == value)
            .ok_or(EditSpecError::InvalidColorGrade)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HslBandAdjustSpec {
    pub(crate) band: HslBand,
    pub(crate) hue_degrees: f64,
    pub(crate) saturation: f64,
    pub(crate) luminance: f64,
}

impl HslBandAdjustSpec {
    pub fn from_wire(value: &model::HslBandAdjust) -> Result<Self, EditSpecError> {
        Ok(Self {
            band: HslBand::parse(&value.band)?,
            hue_degrees: clamped(
                value.hue,
                -MAX_HUE_SHIFT_DEGREES,
                MAX_HUE_SHIFT_DEGREES,
                EditSpecError::InvalidColorGrade,
            )?,
            saturation: clamped(
                value.saturation,
                0.0,
                MAX_HSL_MULTIPLIER,
                EditSpecError::InvalidColorGrade,
            )?,
            luminance: clamped(
                value.luminance,
                0.0,
                MAX_HSL_MULTIPLIER,
                EditSpecError::InvalidColorGrade,
            )?,
        })
    }

    pub fn band(self) -> HslBand {
        self.band
    }

    pub fn hue_degrees(self) -> f64 {
        self.hue_degrees
    }

    pub fn saturation(self) -> f64 {
        self.saturation
    }

    pub fn luminance(self) -> f64 {
        self.luminance
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorAdvancedSpec {
    pub(crate) temperature: f64,
    pub(crate) tint: f64,
    pub(crate) exposure_stops: f64,
    pub(crate) highlights: f64,
    pub(crate) shadows: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lift: Option<RgbTriplet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) gamma: Option<RgbTriplet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) gain: Option<RgbTriplet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) hsl: Vec<HslBandAdjustSpec>,
}

impl ColorAdvancedSpec {
    pub fn from_wire(value: &model::ColorAdvanced) -> Result<Self, EditSpecError> {
        if value.hsl.len() > HslBand::ALL.len() {
            return Err(EditSpecError::InvalidColorGrade);
        }
        let spec = Self {
            temperature: clamped(
                value.temperature,
                -MAX_WHITE_BALANCE,
                MAX_WHITE_BALANCE,
                EditSpecError::InvalidColorGrade,
            )?,
            tint: clamped(
                value.tint,
                -MAX_WHITE_BALANCE,
                MAX_WHITE_BALANCE,
                EditSpecError::InvalidColorGrade,
            )?,
            exposure_stops: clamped(
                value.exposure,
                -MAX_EXPOSURE_STOPS,
                MAX_EXPOSURE_STOPS,
                EditSpecError::InvalidColorGrade,
            )?,
            highlights: clamped(
                value.highlights,
                -MAX_TONE_SHIFT,
                MAX_TONE_SHIFT,
                EditSpecError::InvalidColorGrade,
            )?,
            shadows: clamped(
                value.shadows,
                -MAX_TONE_SHIFT,
                MAX_TONE_SHIFT,
                EditSpecError::InvalidColorGrade,
            )?,
            lift: value
                .lift
                .as_ref()
                .map(|wheel| RgbTriplet::from_wire(wheel, -MAX_LIFT, MAX_LIFT))
                .transpose()?,
            gamma: value
                .gamma
                .as_ref()
                .map(|wheel| RgbTriplet::from_wire(wheel, MIN_GAMMA, MAX_GAMMA))
                .transpose()?,
            gain: value
                .gain
                .as_ref()
                .map(|wheel| RgbTriplet::from_wire(wheel, 0.0, MAX_GAIN))
                .transpose()?,
            hsl: value
                .hsl
                .iter()
                .map(HslBandAdjustSpec::from_wire)
                .collect::<Result<Vec<_>, _>>()?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    pub fn tint(&self) -> f64 {
        self.tint
    }

    pub fn exposure_stops(&self) -> f64 {
        self.exposure_stops
    }

    pub fn highlights(&self) -> f64 {
        self.highlights
    }

    pub fn shadows(&self) -> f64 {
        self.shadows
    }

    pub fn lift(&self) -> Option<RgbTriplet> {
        self.lift
    }

    pub fn gamma(&self) -> Option<RgbTriplet> {
        self.gamma
    }

    pub fn gain(&self) -> Option<RgbTriplet> {
        self.gain
    }

    pub fn hsl(&self) -> &[HslBandAdjustSpec] {
        &self.hsl
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        let scalars = [
            (self.temperature, -MAX_WHITE_BALANCE, MAX_WHITE_BALANCE),
            (self.tint, -MAX_WHITE_BALANCE, MAX_WHITE_BALANCE),
            (self.exposure_stops, -MAX_EXPOSURE_STOPS, MAX_EXPOSURE_STOPS),
            (self.highlights, -MAX_TONE_SHIFT, MAX_TONE_SHIFT),
            (self.shadows, -MAX_TONE_SHIFT, MAX_TONE_SHIFT),
        ];
        if scalars
            .into_iter()
            .any(|(value, min, max)| !value.is_finite() || !(min..=max).contains(&value))
        {
            return Err(EditSpecError::InvalidColorGrade);
        }
        let wheels = [
            (self.lift, -MAX_LIFT, MAX_LIFT),
            (self.gamma, MIN_GAMMA, MAX_GAMMA),
            (self.gain, 0.0, MAX_GAIN),
        ];
        for (wheel, min, max) in wheels {
            if wheel.is_some_and(|value| !value.within(min, max)) {
                return Err(EditSpecError::InvalidColorGrade);
            }
        }
        if self.hsl.len() > HslBand::ALL.len() {
            return Err(EditSpecError::InvalidColorGrade);
        }
        // One adjustment per band keeps the emitted filter chain deterministic.
        let mut seen = Vec::with_capacity(self.hsl.len());
        for adjust in &self.hsl {
            if seen.contains(&adjust.band) {
                return Err(EditSpecError::InvalidColorGrade);
            }
            seen.push(adjust.band);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade() -> model::ColorAdvanced {
        model::ColorAdvanced {
            temperature: 0.0,
            tint: 0.0,
            exposure: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            lift: None,
            gamma: None,
            gain: None,
            hsl: Vec::new(),
        }
    }

    #[test]
    fn scalars_are_clamped_into_their_documented_ranges() {
        let mut extreme = grade();
        extreme.temperature = 9.0;
        extreme.exposure = -9.0;
        extreme.highlights = 9.0;
        let spec = ColorAdvancedSpec::from_wire(&extreme).unwrap();
        assert_eq!(spec.temperature(), MAX_WHITE_BALANCE);
        assert_eq!(spec.exposure_stops(), -MAX_EXPOSURE_STOPS);
        assert_eq!(spec.highlights(), MAX_TONE_SHIFT);
    }

    #[test]
    fn wheels_use_their_own_ranges_and_reject_non_finite_channels() {
        let mut wheels = grade();
        wheels.lift = Some(model::Rgb {
            r: 9.0,
            g: -9.0,
            b: 0.0,
        });
        wheels.gamma = Some(model::Rgb {
            r: 0.0,
            g: 1.0,
            b: 9.0,
        });
        let spec = ColorAdvancedSpec::from_wire(&wheels).unwrap();
        assert_eq!(spec.lift().unwrap().red(), MAX_LIFT);
        assert_eq!(spec.lift().unwrap().green(), -MAX_LIFT);
        assert_eq!(spec.gamma().unwrap().red(), MIN_GAMMA);
        assert_eq!(spec.gamma().unwrap().blue(), MAX_GAMMA);

        let mut broken = grade();
        broken.gain = Some(model::Rgb {
            r: f64::NAN,
            g: 1.0,
            b: 1.0,
        });
        assert_eq!(
            ColorAdvancedSpec::from_wire(&broken),
            Err(EditSpecError::InvalidColorGrade)
        );
    }

    #[test]
    fn hsl_bands_are_known_and_unique() {
        let band = |name: &str| model::HslBandAdjust {
            band: name.into(),
            hue: 0.0,
            saturation: 1.0,
            luminance: 1.0,
        };
        let mut duplicated = grade();
        duplicated.hsl = vec![band("red"), band("red")];
        assert_eq!(
            ColorAdvancedSpec::from_wire(&duplicated),
            Err(EditSpecError::InvalidColorGrade)
        );

        let mut unknown = grade();
        unknown.hsl = vec![band("chartreuse")];
        assert_eq!(
            ColorAdvancedSpec::from_wire(&unknown),
            Err(EditSpecError::InvalidColorGrade)
        );

        let mut every = grade();
        every.hsl = HslBand::ALL.iter().map(|b| band(b.wire_id())).collect();
        assert_eq!(ColorAdvancedSpec::from_wire(&every).unwrap().hsl().len(), 7);
    }
}
