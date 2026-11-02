//! Native Write-on parameter animation and high-resolution historical brush sampling.
use super::{
    composition::AnimatableValue, linear_color_frame::LinearColorFrame, write_on::WriteOn,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WriteOnChannel {
    BrushX,
    BrushY,
    ColorR,
    ColorG,
    ColorB,
    ColorA,
    Size,
    Hardness,
    Opacity,
    StrokeLength,
    Spacing,
}
pub type WriteOnAnimation = std::collections::BTreeMap<WriteOnChannel, AnimatableValue>;
pub fn validate_animation(animation: &WriteOnAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            WriteOnChannel::BrushX | WriteOnChannel::BrushY => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            WriteOnChannel::Size => (0., 500.),
            WriteOnChannel::Hardness | WriteOnChannel::Opacity => (0., 100.),
            WriteOnChannel::StrokeLength => (0., 3600.),
            WriteOnChannel::Spacing => (0.001, 1.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Write-on animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut parameters: WriteOn,
    animation: &WriteOnAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<WriteOn> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            WriteOnChannel::BrushX => parameters.brush[0] = Some(value),
            WriteOnChannel::BrushY => parameters.brush[1] = Some(value),
            WriteOnChannel::ColorR => parameters.color[0] = value as f32,
            WriteOnChannel::ColorG => parameters.color[1] = value as f32,
            WriteOnChannel::ColorB => parameters.color[2] = value as f32,
            WriteOnChannel::ColorA => parameters.color[3] = value as f32,
            WriteOnChannel::Size => parameters.size = value as f32,
            WriteOnChannel::Hardness => parameters.hardness = value as f32,
            WriteOnChannel::Opacity => parameters.opacity = value as f32,
            WriteOnChannel::StrokeLength => parameters.stroke_length = value as f32,
            WriteOnChannel::Spacing => parameters.spacing = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}
fn compiled_history(value: &AnimatableValue) -> Result<AnimatableValue> {
    value.validate()?;
    let mut compiled = value.clone();
    if let AnimatableValue::Keyframes { track } = &mut compiled {
        for key in &mut track.keyframes {
            let tick = key.tick as i128 * filmcraft_time::TICKS_PER_SECOND as i128
                / track.time_base as i128;
            key.tick = i64::try_from(tick)
                .map_err(|_| anyhow::anyhow!("Write-on key exceeds Filmcraft tick range"))?;
        }
        ensure!(
            track
                .keyframes
                .windows(2)
                .all(|pair| pair[0].tick < pair[1].tick),
            "Write-on keys collapse at Filmcraft resolution"
        );
    }
    Ok(compiled)
}
fn history_value(value: &AnimatableValue, tick: filmcraft_time::Tick) -> f64 {
    match value {
        AnimatableValue::Constant { value } => *value,
        AnimatableValue::Keyframes { track } => track.sample_tick(tick.0),
    }
}
pub fn apply(
    parameters: &WriteOn,
    animation: Option<&WriteOnAnimation>,
    frame: &mut LinearColorFrame,
    local: u64,
    scale: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    let sampled = if let Some(animation) = animation {
        sample(parameters.clone(), animation, local)?
    } else {
        parameters.clone()
    };
    let x = animation
        .and_then(|a| a.get(&WriteOnChannel::BrushX))
        .map(compiled_history)
        .transpose()?;
    let y = animation
        .and_then(|a| a.get(&WriteOnChannel::BrushY))
        .map(compiled_history)
        .transpose()?;
    let us = i64::try_from(local).map_err(|_| anyhow::anyhow!("Write-on local clock overflow"))?;
    let tick = i64::try_from(us as i128 * filmcraft_time::TICKS_PER_SECOND as i128 / 1000000)
        .map_err(|_| anyhow::anyhow!("Write-on local Filmcraft tick overflow"))?;
    sampled.apply_linear(
        frame,
        filmcraft_time::Tick(tick),
        local as f64 / 1000000.,
        scale,
        &mut |tick| {
            Ok([
                x.as_ref()
                    .map(|curve| history_value(curve, tick))
                    .or(parameters.brush[0]),
                y.as_ref()
                    .map(|curve| history_value(curve, tick))
                    .or(parameters.brush[1]),
            ])
        },
        checkpoint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_history_with_signed_millisecond_tracks() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-write-on.json"))
                .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 288);
        for (index, c) in cases.iter().enumerate() {
            let parameters: WriteOn = serde_json::from_value(c["parameters"].clone()).unwrap();
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c["width"].as_u64().unwrap() as usize,
                    height: c["height"].as_u64().unwrap() as usize,
                    rgba: serde_json::from_value(c["rgba"].clone()).unwrap(),
                },
                &mut || Ok(()),
            )
            .unwrap();
            let mut animation = serde_json::Map::new();
            if !c["keys"].as_array().unwrap().is_empty() {
                for (axis, channel) in ["brushX", "brushY"].into_iter().enumerate() {
                    let keys:Vec<_>=c["keys"].as_array().unwrap().iter().map(|k|serde_json::json!({"tick":k[0].as_i64().unwrap()/1000,"value":k[1][axis]})).collect();
                    animation.insert(channel.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":if c["hold"].as_bool().unwrap(){"hold"}else{"linear"},"keyframes":keys}}));
                }
            }
            let animation: WriteOnAnimation =
                serde_json::from_value(serde_json::Value::Object(animation)).unwrap();
            let local = (c["seconds"].as_f64().unwrap() * 1000000.).round() as u64;
            apply(
                &parameters,
                Some(&animation),
                &mut frame,
                local,
                c["pixelScale"].as_f64().unwrap() as f32,
                &mut || Ok(()),
            )
            .unwrap();
            let bits: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                bits,
                "Write-on compiled history {index}"
            );
        }
    }
}

include!("write_on_exact_time.rs");
