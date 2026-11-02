//! Filmcraft Lightning clip-local animation channels.
use super::{composition::AnimatableValue, lightning::Lightning};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LightningChannel {
    StartX,
    StartY,
    EndX,
    EndY,
    OutsideR,
    OutsideG,
    OutsideB,
    OutsideA,
    InsideR,
    InsideG,
    InsideB,
    InsideA,
    Segments,
    Amplitude,
    Detail,
    DetailAmplitude,
    Branching,
    Speed,
    Width,
    Core,
    Seed,
}
pub type LightningAnimation = std::collections::BTreeMap<LightningChannel, AnimatableValue>;
pub fn validate_animation(animation: &LightningAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            LightningChannel::StartX => (-(f32::MAX as f64), f32::MAX as f64),
            LightningChannel::StartY => (-(f32::MAX as f64), f32::MAX as f64),
            LightningChannel::EndX => (-(f32::MAX as f64), f32::MAX as f64),
            LightningChannel::EndY => (-(f32::MAX as f64), f32::MAX as f64),
            LightningChannel::OutsideR => (0., 1.),
            LightningChannel::OutsideG => (0., 1.),
            LightningChannel::OutsideB => (0., 1.),
            LightningChannel::OutsideA => (0., 1.),
            LightningChannel::InsideR => (0., 1.),
            LightningChannel::InsideG => (0., 1.),
            LightningChannel::InsideB => (0., 1.),
            LightningChannel::InsideA => (0., 1.),
            LightningChannel::Segments => (1., 100.),
            LightningChannel::Amplitude => (0., 100.),
            LightningChannel::Detail => (0., 10.),
            LightningChannel::DetailAmplitude => (0., 1.),
            LightningChannel::Branching => (0., 1.),
            LightningChannel::Speed => (0., 100.),
            LightningChannel::Width => (0., 200.),
            LightningChannel::Core => (0., 1.),
            LightningChannel::Seed => (0., 10000.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Lightning animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut parameters: Lightning,
    animation: &LightningAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<Lightning> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            LightningChannel::StartX => parameters.start[0] = Some(value),
            LightningChannel::StartY => parameters.start[1] = Some(value),
            LightningChannel::EndX => parameters.end[0] = Some(value),
            LightningChannel::EndY => parameters.end[1] = Some(value),
            LightningChannel::OutsideR => parameters.outside[0] = value as f32,
            LightningChannel::OutsideG => parameters.outside[1] = value as f32,
            LightningChannel::OutsideB => parameters.outside[2] = value as f32,
            LightningChannel::OutsideA => parameters.outside[3] = value as f32,
            LightningChannel::InsideR => parameters.inside[0] = value as f32,
            LightningChannel::InsideG => parameters.inside[1] = value as f32,
            LightningChannel::InsideB => parameters.inside[2] = value as f32,
            LightningChannel::InsideA => parameters.inside[3] = value as f32,
            LightningChannel::Segments => parameters.segments = value as f32,
            LightningChannel::Amplitude => parameters.amplitude = value as f32,
            LightningChannel::Detail => parameters.detail = value as f32,
            LightningChannel::DetailAmplitude => parameters.detail_amplitude = value as f32,
            LightningChannel::Branching => parameters.branching = value as f32,
            LightningChannel::Speed => parameters.speed = value as f32,
            LightningChannel::Width => parameters.width = value as f32,
            LightningChannel::Core => parameters.core = value as f32,
            LightningChannel::Seed => parameters.seed = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_channels_signed_clock_roundtrip_and_bounds() {
        let values = [
            ("startX", 12.25),
            ("startY", -7.75),
            ("endX", 25.125),
            ("endY", 8.5),
            ("outsideR", 0.2),
            ("outsideG", 0.4),
            ("outsideB", 0.6),
            ("outsideA", 0.8),
            ("insideR", 0.8),
            ("insideG", 0.6),
            ("insideB", 0.4),
            ("insideA", 0.2),
            ("segments", 5.75),
            ("amplitude", 37.5),
            ("detail", 3.25),
            ("detailAmplitude", 0.25),
            ("branching", 0.75),
            ("speed", 27.5),
            ("width", 1.25),
            ("core", 0.4),
            ("seed", 17.75),
        ];
        let animation:LightningAnimation=serde_json::from_value(serde_json::Value::Object(values.iter().map(|(name,value)|((*name).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":-250,"value":value-0.125},{"tick":1250,"value":value+0.125}]}}))).collect())).unwrap();
        assert_eq!(animation.len(), 21);
        let base = Lightning {
            mode: 4,
            ..Default::default()
        };
        let result = sample(base.clone(), &animation, 500000).unwrap();
        assert_eq!(result.mode, 4);
        assert_eq!(base.start, [None, None]);
        assert_eq!(result.start[0], Some(12.25));
        assert_eq!(result.start[1], Some(-7.75));
        assert_eq!(result.end[0], Some(25.125));
        assert_eq!(result.end[1], Some(8.5));
        assert_eq!(result.outside[0], 0.2f32);
        assert_eq!(result.outside[1], 0.4f32);
        assert_eq!(result.outside[2], 0.6f32);
        assert_eq!(result.outside[3], 0.8f32);
        assert_eq!(result.inside[0], 0.8f32);
        assert_eq!(result.inside[1], 0.6f32);
        assert_eq!(result.inside[2], 0.4f32);
        assert_eq!(result.inside[3], 0.2f32);
        assert_eq!(result.segments, 5.75f32);
        assert_eq!(result.amplitude, 37.5f32);
        assert_eq!(result.detail, 3.25f32);
        assert_eq!(result.detail_amplitude, 0.25f32);
        assert_eq!(result.branching, 0.75f32);
        assert_eq!(result.speed, 27.5f32);
        assert_eq!(result.width, 1.25f32);
        assert_eq!(result.core, 0.4f32);
        assert_eq!(result.seed, 17.75f32);
        assert_eq!(
            animation,
            serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap()
        );
        assert_eq!(
            result,
            serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap()
        );
        for (value, channel) in [
            (100.1, "amplitude"),
            (0., "segments"),
            (10.1, "detail"),
            (1.1, "branching"),
            (200.1, "width"),
            (10000.1, "seed"),
        ] {
            let animation: LightningAnimation = serde_json::from_value(
                serde_json::json!({channel:{"mode":"constant","value":value}}),
            )
            .unwrap();
            assert!(validate_animation(&animation).is_err());
        }
        let exact = 1.23456789012345;
        let animation: LightningAnimation =
            serde_json::from_value(serde_json::json!({"startX":{"mode":"constant","value":exact}}))
                .unwrap();
        assert_eq!(
            sample(base, &animation, 0).unwrap().start,
            [Some(exact), None]
        );
        assert!(serde_json::from_value::<LightningAnimation>(
            serde_json::json!({"unknown":{"mode":"constant","value":1.}})
        )
        .is_err());
    }
}
