//! Source Float/Choice/Bool scalar lowering for effect parameters and mixer lanes.
//! Filmcraft Param::f64_at and Param::scalar_at differ on mixed-type segments.
use super::composition::AnimatableValue;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evaluation {
    EffectParameter,
    MixerLane,
}
fn scalar(value: &Value) -> Result<(f64, bool)> {
    let object = value.as_object().context("invalid scalar value")?;
    ensure!(object.len() == 1, "ambiguous scalar value");
    let (number, float) = if let Some(value) = object.get("Float") {
        (value.as_f64().context("invalid Float")?, true)
    } else if let Some(value) = object.get("Choice") {
        let value = value.as_u64().context("invalid Choice")?;
        ensure!(value <= u32::MAX as u64, "Choice overflow");
        (value as f64, false)
    } else if let Some(value) = object.get("Bool") {
        (
            if value.as_bool().context("invalid Bool")? {
                1.
            } else {
                0.
            },
            false,
        )
    } else {
        anyhow::bail!("non-scalar Filmcraft value")
    };
    ensure!(number.is_finite(), "nonfinite scalar");
    Ok((number, float))
}
pub fn convert(source: &Value, evaluation: Evaluation) -> Result<AnimatableValue> {
    let mut lowered = source.clone();
    let baseline = scalar(source.get("value").context("missing scalar value")?)?.0;
    lowered["value"] = json!({"Float":baseline});
    let mut hold = Vec::new();
    if let Some(keys) = source.get("keyframes") {
        let keys = keys.as_array().context("invalid scalar keys")?;
        let values = keys
            .iter()
            .map(|key| scalar(key.get("value").context("missing scalar key value")?))
            .collect::<Result<Vec<_>>>()?;
        for (i, (value, is_float)) in values.iter().enumerate() {
            lowered["keyframes"][i]["value"] = json!({"Float":value});
            let next_float = values.get(i + 1).map_or(true, |(_, float)| *float);
            hold.push(!is_float || evaluation == Evaluation::EffectParameter && !next_float);
        }
    }
    // Derive easing before lowering outgoing holds: a discrete key's original
    // incoming interpolation still affects the preceding mixer Float segment.
    let mut curve = super::filmcraft_scalar_curve::convert(&lowered)?;
    if let AnimatableValue::Keyframes { track } = &mut curve {
        for (key, hold) in track.keyframes.iter_mut().zip(hold) {
            if hold {
                key.interpolation = Some(super::keyframes::Interpolation::Hold);
            }
        }
    }
    Ok(curve)
}
pub fn sample(curve: &AnimatableValue, tick: i64) -> f64 {
    match curve {
        AnimatableValue::Constant { value } => *value,
        AnimatableValue::Keyframes { track } => track.sample_filmcraft_source_ticks(tick),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_float_choice_bool_effect_and_mixer_evaluators_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-effect-scalar.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 23814);
        for (i, case) in cases.iter().enumerate() {
            let master=super::super::filmcraft_master_volume::SourceMasterVolume::read(&json!({"master_volume_db":-13.25,"master_mixer":{"mode":"Read","lanes":{"volume":case["parameter"]}}})).unwrap();
            assert_eq!(master.sample(case["at"].as_str().unwrap().parse().unwrap()).to_bits(),case["mixerBits"].as_u64().unwrap(),"mixed master {i}");
            for (mode, field) in [
                (Evaluation::EffectParameter, "effectBits"),
                (Evaluation::MixerLane, "mixerBits"),
            ] {
                let curve = convert(&case["parameter"], mode).unwrap();
                let curve: AnimatableValue =
                    serde_json::from_value(serde_json::to_value(curve).unwrap()).unwrap();
                assert_eq!(
                    sample(&curve, case["at"].as_str().unwrap().parse().unwrap()).to_bits(),
                    case[field].as_u64().unwrap(),
                    "scalar {i} {mode:?}"
                );
            }
        }
    }
}
