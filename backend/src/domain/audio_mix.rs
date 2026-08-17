//! Extra audio tracks laid onto the output timeline plus the master dynamics
//! chain (denoise, gate, compressor, de-esser, limiter, filters, envelope).

use serde::{Deserialize, Serialize};

use super::edit::{asset_reference, clamped, keyframe_track, EditSpecError};
use super::keyframes::KeyframeTrack;
use crate::model;

const MAX_GAIN: f64 = 4.0;
const MAX_TIMELINE_SECONDS: f64 = 86_400.0;
const MAX_FADE_SECONDS: f64 = 600.0;
/// Envelope times are milliseconds, matching `sidechaincompress`/`acompressor`.
const MIN_ENVELOPE_MS: f64 = 0.01;
const MAX_ENVELOPE_MS: f64 = 9_000.0;
const MIN_BITRATE_KBPS: u32 = 64;
const MAX_BITRATE_KBPS: u32 = 320;
/// Audible band limits for the optional high-pass/low-pass.
const MIN_FILTER_HZ: f64 = 10.0;
const MAX_FILTER_HZ: f64 = 20_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRole {
    Music,
    Voiceover,
    Sfx,
}

impl AudioRole {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "music" => Ok(Self::Music),
            "voiceover" => Ok(Self::Voiceover),
            "sfx" => Ok(Self::Sfx),
            _ => Err(EditSpecError::InvalidAudioTrack),
        }
    }
}

/// Sidechain ducking of this track against the primary dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuckingSpec {
    pub(crate) enabled: bool,
    pub(crate) threshold: f64,
    pub(crate) ratio: f64,
    pub(crate) attack_ms: f64,
    pub(crate) release_ms: f64,
}

impl DuckingSpec {
    pub fn from_wire(value: &model::Ducking) -> Result<Self, EditSpecError> {
        Ok(Self {
            enabled: value.enabled,
            threshold: clamped(
                value.threshold,
                0.000_1,
                1.0,
                EditSpecError::InvalidAudioTrack,
            )?,
            ratio: clamped(value.ratio, 1.0, 20.0, EditSpecError::InvalidAudioTrack)?,
            attack_ms: clamped(
                value.attack,
                MIN_ENVELOPE_MS,
                MAX_ENVELOPE_MS,
                EditSpecError::InvalidAudioTrack,
            )?,
            release_ms: clamped(
                value.release,
                MIN_ENVELOPE_MS,
                MAX_ENVELOPE_MS,
                EditSpecError::InvalidAudioTrack,
            )?,
        })
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn threshold(self) -> f64 {
        self.threshold
    }

    pub fn ratio(self) -> f64 {
        self.ratio
    }

    pub fn attack_ms(self) -> f64 {
        self.attack_ms
    }

    pub fn release_ms(self) -> f64 {
        self.release_ms
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioTrackSpec {
    pub(crate) asset_id: String,
    pub(crate) role: AudioRole,
    pub(crate) gain: f64,
    /// Where the track lands on the OUTPUT timeline.
    pub(crate) start_seconds: f64,
    /// In-point inside the asset.
    pub(crate) source_start_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) end_seconds: Option<f64>,
    pub(crate) looping: bool,
    pub(crate) fade_in_seconds: f64,
    pub(crate) fade_out_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ducking: Option<DuckingSpec>,
}

impl AudioTrackSpec {
    pub fn from_wire(value: &model::AudioTrack) -> Result<Self, EditSpecError> {
        let spec = Self {
            asset_id: asset_reference(&value.asset_id)?,
            role: AudioRole::parse(&value.role)?,
            gain: clamped(value.gain, 0.0, MAX_GAIN, EditSpecError::InvalidAudioTrack)?,
            start_seconds: clamped(
                value.start,
                0.0,
                MAX_TIMELINE_SECONDS,
                EditSpecError::InvalidAudioTrack,
            )?,
            source_start_seconds: clamped(
                value.source_start,
                0.0,
                MAX_TIMELINE_SECONDS,
                EditSpecError::InvalidAudioTrack,
            )?,
            end_seconds: value
                .end
                .map(|end| {
                    clamped(
                        end,
                        0.0,
                        MAX_TIMELINE_SECONDS,
                        EditSpecError::InvalidAudioTrack,
                    )
                })
                .transpose()?,
            looping: value.r#loop,
            fade_in_seconds: clamped(
                value.fade_in,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidAudioTrack,
            )?,
            fade_out_seconds: clamped(
                value.fade_out,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidAudioTrack,
            )?,
            ducking: value
                .ducking
                .as_ref()
                .map(DuckingSpec::from_wire)
                .transpose()?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }

    pub fn role(&self) -> AudioRole {
        self.role
    }

    pub fn gain(&self) -> f64 {
        self.gain
    }

    pub fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    pub fn source_start_seconds(&self) -> f64 {
        self.source_start_seconds
    }

    pub fn end_seconds(&self) -> Option<f64> {
        self.end_seconds
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn fade_in_seconds(&self) -> f64 {
        self.fade_in_seconds
    }

    pub fn fade_out_seconds(&self) -> f64 {
        self.fade_out_seconds
    }

    pub fn ducking(&self) -> Option<DuckingSpec> {
        self.ducking
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.asset_id.is_empty()
            || !(0.0..=MAX_GAIN).contains(&self.gain)
            || self.start_seconds < 0.0
            || self.source_start_seconds < 0.0
        {
            return Err(EditSpecError::InvalidAudioTrack);
        }
        if let Some(end) = self.end_seconds {
            if end <= self.start_seconds {
                return Err(EditSpecError::InvalidAudioTrack);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompressorSpec {
    pub(crate) threshold_db: f64,
    pub(crate) ratio: f64,
    pub(crate) attack_ms: f64,
    pub(crate) release_ms: f64,
    pub(crate) makeup: f64,
}

impl CompressorSpec {
    pub fn from_wire(value: &model::Compressor) -> Result<Self, EditSpecError> {
        Ok(Self {
            threshold_db: clamped(
                value.threshold,
                -60.0,
                0.0,
                EditSpecError::InvalidAudioDynamics,
            )?,
            ratio: clamped(value.ratio, 1.0, 20.0, EditSpecError::InvalidAudioDynamics)?,
            attack_ms: clamped(
                value.attack,
                MIN_ENVELOPE_MS,
                MAX_ENVELOPE_MS,
                EditSpecError::InvalidAudioDynamics,
            )?,
            release_ms: clamped(
                value.release,
                MIN_ENVELOPE_MS,
                MAX_ENVELOPE_MS,
                EditSpecError::InvalidAudioDynamics,
            )?,
            makeup: clamped(value.makeup, 1.0, 64.0, EditSpecError::InvalidAudioDynamics)?,
        })
    }

    pub fn threshold_db(self) -> f64 {
        self.threshold_db
    }

    pub fn ratio(self) -> f64 {
        self.ratio
    }

    pub fn attack_ms(self) -> f64 {
        self.attack_ms
    }

    pub fn release_ms(self) -> f64 {
        self.release_ms
    }

    pub fn makeup(self) -> f64 {
        self.makeup
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimiterSpec {
    pub(crate) ceiling_db: f64,
}

impl LimiterSpec {
    pub fn from_wire(value: &model::Limiter) -> Result<Self, EditSpecError> {
        Ok(Self {
            ceiling_db: clamped(
                value.ceiling,
                -30.0,
                0.0,
                EditSpecError::InvalidAudioDynamics,
            )?,
        })
    }

    pub fn ceiling_db(self) -> f64 {
        self.ceiling_db
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GateSpec {
    pub(crate) threshold_db: f64,
    pub(crate) ratio: f64,
}

impl GateSpec {
    pub fn from_wire(value: &model::Gate) -> Result<Self, EditSpecError> {
        Ok(Self {
            threshold_db: clamped(
                value.threshold,
                -80.0,
                0.0,
                EditSpecError::InvalidAudioDynamics,
            )?,
            ratio: clamped(value.ratio, 1.0, 20.0, EditSpecError::InvalidAudioDynamics)?,
        })
    }

    pub fn threshold_db(self) -> f64 {
        self.threshold_db
    }

    pub fn ratio(self) -> f64 {
        self.ratio
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioDynamicsSpec {
    pub(crate) denoise: f64,
    pub(crate) dereverb: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) compressor: Option<CompressorSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limiter: Option<LimiterSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) gate: Option<GateSpec>,
    pub(crate) deesser: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) highpass_hz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lowpass_hz: Option<f64>,
    pub(crate) bitrate_kbps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) volume_envelope: Option<KeyframeTrack<f64>>,
}

impl AudioDynamicsSpec {
    pub fn from_wire(value: &model::AudioDynamics) -> Result<Self, EditSpecError> {
        let spec = Self {
            denoise: clamped(value.denoise, 0.0, 1.0, EditSpecError::InvalidAudioDynamics)?,
            dereverb: value.dereverb,
            compressor: value
                .compressor
                .as_ref()
                .map(CompressorSpec::from_wire)
                .transpose()?,
            limiter: value
                .limiter
                .as_ref()
                .map(LimiterSpec::from_wire)
                .transpose()?,
            gate: value.gate.as_ref().map(GateSpec::from_wire).transpose()?,
            deesser: value.deesser,
            highpass_hz: value
                .highpass_hz
                .map(|hz| {
                    clamped(
                        hz,
                        MIN_FILTER_HZ,
                        MAX_FILTER_HZ,
                        EditSpecError::InvalidAudioDynamics,
                    )
                })
                .transpose()?,
            lowpass_hz: value
                .lowpass_hz
                .map(|hz| {
                    clamped(
                        hz,
                        MIN_FILTER_HZ,
                        MAX_FILTER_HZ,
                        EditSpecError::InvalidAudioDynamics,
                    )
                })
                .transpose()?,
            bitrate_kbps: value.bitrate_kbps.clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS),
            volume_envelope: keyframe_track(
                &value.volume_envelope,
                EditSpecError::InvalidAudioDynamics,
            )?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn denoise(&self) -> f64 {
        self.denoise
    }

    pub fn dereverb(&self) -> bool {
        self.dereverb
    }

    pub fn compressor(&self) -> Option<CompressorSpec> {
        self.compressor
    }

    pub fn limiter(&self) -> Option<LimiterSpec> {
        self.limiter
    }

    pub fn gate(&self) -> Option<GateSpec> {
        self.gate
    }

    pub fn deesser(&self) -> bool {
        self.deesser
    }

    pub fn highpass_hz(&self) -> Option<f64> {
        self.highpass_hz
    }

    pub fn lowpass_hz(&self) -> Option<f64> {
        self.lowpass_hz
    }

    pub fn bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps
    }

    pub fn volume_envelope(&self) -> Option<&KeyframeTrack<f64>> {
        self.volume_envelope.as_ref()
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if !(0.0..=1.0).contains(&self.denoise)
            || !(MIN_BITRATE_KBPS..=MAX_BITRATE_KBPS).contains(&self.bitrate_kbps)
        {
            return Err(EditSpecError::InvalidAudioDynamics);
        }
        if let (Some(high), Some(low)) = (self.highpass_hz, self.lowpass_hz) {
            if high >= low {
                return Err(EditSpecError::InvalidAudioDynamics);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> model::AudioTrack {
        model::AudioTrack {
            asset_id: "ast_0123456789abcdef".into(),
            role: "music".into(),
            gain: 1.0,
            start: 0.0,
            source_start: 0.0,
            end: None,
            r#loop: false,
            fade_in: 0.0,
            fade_out: 0.0,
            ducking: None,
        }
    }

    fn dynamics() -> model::AudioDynamics {
        model::AudioDynamics {
            denoise: 0.0,
            dereverb: false,
            compressor: None,
            limiter: None,
            gate: None,
            deesser: false,
            highpass_hz: None,
            lowpass_hz: None,
            bitrate_kbps: 128,
            volume_envelope: Vec::new(),
        }
    }

    #[test]
    fn track_gain_and_role_are_validated() {
        let mut loud = track();
        loud.gain = 99.0;
        assert_eq!(AudioTrackSpec::from_wire(&loud).unwrap().gain(), MAX_GAIN);

        let mut unknown = track();
        unknown.role = "narration".into();
        assert_eq!(
            AudioTrackSpec::from_wire(&unknown),
            Err(EditSpecError::InvalidAudioTrack)
        );

        let mut inverted = track();
        inverted.start = 10.0;
        inverted.end = Some(1.0);
        assert_eq!(
            AudioTrackSpec::from_wire(&inverted),
            Err(EditSpecError::InvalidAudioTrack)
        );
    }

    #[test]
    fn non_finite_values_never_reach_the_adapter() {
        let mut broken = track();
        broken.gain = f64::NAN;
        assert_eq!(
            AudioTrackSpec::from_wire(&broken),
            Err(EditSpecError::InvalidAudioTrack)
        );

        let mut ducked = track();
        ducked.ducking = Some(model::Ducking {
            enabled: true,
            threshold: f64::INFINITY,
            ratio: 8.0,
            attack: 20.0,
            release: 300.0,
        });
        assert_eq!(
            AudioTrackSpec::from_wire(&ducked),
            Err(EditSpecError::InvalidAudioTrack)
        );
    }

    #[test]
    fn dynamics_clamp_the_bitrate_and_reject_crossed_filters() {
        let mut low = dynamics();
        low.bitrate_kbps = 8;
        assert_eq!(
            AudioDynamicsSpec::from_wire(&low).unwrap().bitrate_kbps(),
            MIN_BITRATE_KBPS
        );

        let mut crossed = dynamics();
        crossed.highpass_hz = Some(8_000.0);
        crossed.lowpass_hz = Some(200.0);
        assert_eq!(
            AudioDynamicsSpec::from_wire(&crossed),
            Err(EditSpecError::InvalidAudioDynamics)
        );
    }

    #[test]
    fn volume_envelope_becomes_a_millisecond_keyframe_track() {
        let mut enveloped = dynamics();
        enveloped.volume_envelope = vec![
            model::Keyframe {
                t: 0.0,
                v: 1.0,
                interp: model::KeyframeInterpolation::Linear,
            },
            model::Keyframe {
                t: 2.5,
                v: 0.2,
                interp: model::KeyframeInterpolation::Linear,
            },
        ];
        let spec = AudioDynamicsSpec::from_wire(&enveloped).unwrap();
        let envelope = spec.volume_envelope().unwrap();
        assert_eq!(envelope.keyframes[1].tick, 2_500);
        assert_eq!(envelope.sample_seconds(2.5).unwrap(), 0.2);
    }
}
