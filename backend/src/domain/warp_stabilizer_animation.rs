//! Persisted local animation channels for Filmcraft Warp Stabilizer.
use super::{composition::AnimatableValue, warp_stabilizer::WarpStabilizer};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Smoothness,
    MaxScale,
    ActionSafe,
    AdditionalScale,
    CropLess,
}
pub type Animation = std::collections::BTreeMap<Channel, AnimatableValue>;
fn bounds(channel: Channel) -> (f64, f64) {
    match channel {
        Channel::Smoothness => (0., 1000.),
        Channel::MaxScale => (100., 1000.),
        Channel::ActionSafe | Channel::CropLess => (0., 100.),
        Channel::AdditionalScale => (50., 400.),
    }
}
pub fn validate(animation: &Animation) -> Result<()> {
    for (&channel, value) in animation {
        value.validate()?;
        let (lo, hi) = bounds(channel);
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Warp Stabilizer animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut parameters: WarpStabilizer,
    animation: &Animation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<WarpStabilizer> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    ensure!(parameters.is_valid(), "invalid Warp Stabilizer parameters");
    validate(animation)?;
    for (&channel, value) in animation {
        let v = super::nested_composition::sample_value(value, local)? as f32;
        match channel {
            Channel::Smoothness => parameters.smoothness = v,
            Channel::MaxScale => parameters.max_scale = v,
            Channel::ActionSafe => parameters.action_safe = v,
            Channel::AdditionalScale => parameters.additional_scale = v,
            Channel::CropLess => parameters.crop_less = v,
        }
    }
    ensure!(
        parameters.is_valid(),
        "invalid sampled Warp Stabilizer parameters"
    );
    Ok(parameters)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_channels_roundtrip_and_sample_with_source_bounds() {
        let mut animation = Animation::new();
        for (channel, value) in [
            (Channel::Smoothness, 75.),
            (Channel::MaxScale, 200.),
            (Channel::ActionSafe, 12.5),
            (Channel::AdditionalScale, 125.),
            (Channel::CropLess, 25.),
        ] {
            animation.insert(
                channel,
                serde_json::from_value(serde_json::json!({"mode":"constant","value":value}))
                    .unwrap(),
            );
        }
        animation.insert(Channel::Smoothness,serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":50.},{"tick":1000,"value":100.}]}})).unwrap());
        let saved = serde_json::to_string(&animation).unwrap();
        let restored: Animation = serde_json::from_str(&saved).unwrap();
        assert_eq!(serde_json::to_string(&restored).unwrap(), saved);
        let result = sample(WarpStabilizer::default(), &restored, 500000).unwrap();
        assert_eq!(
            [
                result.smoothness,
                result.max_scale,
                result.action_safe,
                result.additional_scale,
                result.crop_less
            ],
            [75., 200., 12.5, 125., 25.]
        );
        for (channel, (lo, hi)) in [
            Channel::Smoothness,
            Channel::MaxScale,
            Channel::ActionSafe,
            Channel::AdditionalScale,
            Channel::CropLess,
        ]
        .into_iter()
        .map(|c| (c, bounds(c)))
        {
            for value in [lo - 1., hi + 1.] {
                let bad = Animation::from([(
                    channel,
                    serde_json::from_value(serde_json::json!({"mode":"constant","value":value}))
                        .unwrap(),
                )]);
                assert!(validate(&bad).is_err());
            }
        }
    }
}

