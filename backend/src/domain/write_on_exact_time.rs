// Included in write_on_animation after exact-time history qualification.
/// Evaluate brush parameters/history in their authored clock and stroke age in generator time.
pub fn apply_at_time(
    parameters: &WriteOn,
    animation: Option<&WriteOnAnimation>,
    frame: &mut LinearColorFrame,
    time: super::animation_sample_time::AnimationSampleTime,
    generator_seconds: f64,
    scale: f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    use super::animation_sample_time::AnimationSampleTime;
    let tick = match time {
        AnimationSampleTime::SourceTicks(tick) | AnimationSampleTime::FilmcraftSourceTicks(tick) => tick,
        AnimationSampleTime::Microseconds(us) => i64::try_from(i128::from(us)*254_016)
            .map_err(|_|anyhow::anyhow!("Write-on parameter clock overflow"))?,
        AnimationSampleTime::SignedMicroseconds(us) => i64::try_from(i128::from(us)*254_016)
            .map_err(|_|anyhow::anyhow!("Write-on parameter clock overflow"))?,
    };
    let sampled=if let Some(animation)=animation {sample(parameters.clone(),animation,time)?}else{parameters.clone()};
    // Legacy history retains its original key conversion and interpolation ordering.
    let legacy=matches!(time,AnimationSampleTime::Microseconds(_)|AnimationSampleTime::SignedMicroseconds(_));
    let history_curve=|channel|->Result<Option<AnimatableValue>>{
        animation.and_then(|a|a.get(&channel)).map(|value|if legacy {compiled_history(value)}else{Ok(value.clone())}).transpose()
    };
    let x=history_curve(WriteOnChannel::BrushX)?;
    let y=history_curve(WriteOnChannel::BrushY)?;
    let history=|value:&AnimatableValue,at:filmcraft_time::Tick|->f64 {
        match value {
            AnimatableValue::Constant{value}=>*value,
            AnimatableValue::Keyframes{track}=>match time {
                AnimationSampleTime::FilmcraftSourceTicks(_)=>track.sample_filmcraft_source_ticks(at.0),
                AnimationSampleTime::SourceTicks(_)=>track.sample_source_ticks(at.0),
                _=>track.sample_tick(at.0),
            },
        }
    };
    sampled.apply_linear(frame,filmcraft_time::Tick(tick),generator_seconds,scale,&mut|at|Ok([
        x.as_ref().map(|curve|history(curve,at)).or(parameters.brush[0]),
        y.as_ref().map(|curve|history(curve,at)).or(parameters.brush[1]),
    ]),checkpoint)
}
#[cfg(test)]
mod exact_history_tests {
    use super::*;
    use crate::domain::{animation_sample_time::AnimationSampleTime,group_surface::GroupSurface};
    #[test]
    fn original_write_on_split_parameter_and_generator_clock_frames() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-write-on-split-clock.json")).unwrap();
        assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),720);
        for (index,case) in cases.iter().enumerate() {
            let parameters:WriteOn=serde_json::from_value(case["parameters"].clone()).unwrap();
            let mut channels=serde_json::Map::new();
            if !case["keys"].as_array().unwrap().is_empty() {
                for (axis,name) in ["brushX","brushY"].iter().enumerate() {
                    let keys:Vec<_>=case["keys"].as_array().unwrap().iter().map(|key|serde_json::json!({"tick":key[0],"value":key[1][axis]})).collect();
                    channels.insert((*name).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":if case["hold"].as_bool().unwrap(){"hold"}else{"linear"},"keyframes":keys}}));
                }
            }
            let animation:WriteOnAnimation=serde_json::from_value(serde_json::Value::Object(channels)).unwrap();
            let source=GroupSurface{width:case["width"].as_u64().unwrap()as usize,height:case["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};
            let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            let at=case["parameterTick"].as_str().unwrap().parse().unwrap();
            apply_at_time(&parameters,Some(&animation),&mut frame,AnimationSampleTime::FilmcraftSourceTicks(at),case["seconds"].as_f64().unwrap(),case["pixelScale"].as_f64().unwrap()as f32,&mut||Ok(())).unwrap();
            let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(frame.pixels().iter().flatten().map(|value|value.to_bits()).collect::<Vec<_>>(),expected,"WriteOn split clock original frame {index}");
        }
    }
}
