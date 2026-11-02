//! Convert Filmcraft's serialized scalar Param without rounding authored ticks.
//! Source easing contract: pinned Filmcraft, MIT attribution docs/licenses/filmcraft-MIT.txt.
//! Evaluation requires the Filmcraft source clock, including its Bezier solver.
use super::{
    composition::AnimatableValue,
    keyframes::{Interpolation, Keyframe, KeyframeTrack, ProceduralInterpolation},
};
use anyhow::{ensure, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceParam {
    value: SourceValue,
    #[serde(default)]
    keyframes: Vec<SourceKey>,
}
#[derive(Deserialize)]
enum SourceValue {
    Float(f64),
}
impl SourceValue {
    fn number(&self) -> f64 {
        let Self::Float(value) = self;
        *value
    }
}
#[derive(Clone, Copy, Deserialize)]
enum SourceInterpolation {
    Linear,
    Hold,
    Bezier,
    AutoBezier,
    ContinuousBezier,
    EaseIn,
    EaseOut,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceKey {
    time: i64,
    value: SourceValue,
    interp: SourceInterpolation,
    #[serde(default = "default_influence")]
    out_influence: f64,
    #[serde(default = "default_influence")]
    in_influence: f64,
}
fn default_influence() -> f64 {
    1.0 / 3.0
}
fn segment(a: &SourceKey, b: &SourceKey) -> Interpolation {
    use SourceInterpolation::*;
    if matches!(a.interp, Hold) {
        return Interpolation::Hold;
    }
    let out_ease = matches!(a.interp, Bezier | AutoBezier | ContinuousBezier | EaseOut);
    let in_ease = matches!(b.interp, Bezier | AutoBezier | ContinuousBezier | EaseIn)
        || matches!(a.interp, Bezier | ContinuousBezier);
    if !out_ease && !in_ease {
        return Interpolation::Linear;
    }
    Interpolation::Procedural(ProceduralInterpolation::Bezier {
        p1: [
            if out_ease {
                a.out_influence.clamp(0.01, 1.0)
            } else {
                1.0 / 3.0
            },
            if out_ease { 0.0 } else { 1.0 / 3.0 },
        ],
        p2: [
            if in_ease {
                1.0 - b.in_influence.clamp(0.01, 1.0)
            } else {
                2.0 / 3.0
            },
            if in_ease { 1.0 } else { 2.0 / 3.0 },
        ],
    })
}

/// Convert a source Float Param. Other value types and malformed/unsupported data
/// are errors; no source fields, keyframes or precision are silently discarded.
/// Existing native per-value keyframe admission limits still apply.
pub fn convert(source: &serde_json::Value) -> Result<AnimatableValue> {
    let source: SourceParam = serde_json::from_value(source.clone())?;
    ensure!(
        source.value.number().is_finite(),
        "nonfinite Filmcraft scalar value"
    );
    ensure!(
        source.keyframes.len() <= super::composition::MAX_KEYFRAMES_PER_VALUE,
        "Filmcraft curve exceeds native keyframe admission limit"
    );
    if source.keyframes.is_empty() {
        return Ok(AnimatableValue::constant(source.value.number()));
    }
    ensure!(
        source
            .keyframes
            .windows(2)
            .all(|pair| pair[0].time < pair[1].time),
        "Filmcraft keys must be strictly ordered"
    );
    ensure!(
        source
            .keyframes
            .iter()
            .all(|key| key.value.number().is_finite()
                && key.in_influence.is_finite()
                && key.out_influence.is_finite()),
        "nonfinite Filmcraft keyframe"
    );
    let keys = source
        .keyframes
        .iter()
        .enumerate()
        .map(|(i, key)| Keyframe {
            tick: key.time,
            value: key.value.number(),
            tangents: None,
            interpolation: source.keyframes.get(i + 1).map(|next| segment(key, next)),
        })
        .collect();
    Ok(AnimatableValue::Keyframes {
        track: KeyframeTrack::new(254016000000, Interpolation::Linear, keys)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_source_params_match_all_2646_mixed_segment_samples() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-scalar-param.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2646);
        for (index, case) in cases.iter().enumerate() {
            let converted = convert(&case["parameter"]).unwrap();
            converted.validate().unwrap();
            let restored: AnimatableValue =
                serde_json::from_value(serde_json::to_value(&converted).unwrap()).unwrap();
            assert_eq!(converted, restored);
            let AnimatableValue::Keyframes { track } = restored else {
                panic!("source curve lost")
            };
            let at = case["at"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                track.sample_filmcraft_source_ticks(at).to_bits(),
                case["bits"].as_u64().unwrap(),
                "source scalar sample {index}"
            );
        }
    }
    #[test]
    fn static_single_key_and_unsupported_data_are_explicit() {
        assert_eq!(
            convert(&serde_json::json!({"value":{"Float":2.5}})).unwrap(),
            AnimatableValue::constant(2.5)
        );
        let key = serde_json::json!({"time":17,"value":{"Float":7.},"interp":"Linear"});
        let converted =
            convert(&serde_json::json!({"value":{"Float":2.5},"keyframes":[key.clone()]})).unwrap();
        let AnimatableValue::Keyframes { track } = converted else {
            panic!("single key lost")
        };
        assert_eq!(track.sample_filmcraft_source_ticks(i64::MIN), 7.);
        assert_eq!(track.sample_filmcraft_source_ticks(i64::MAX), 7.);
        for invalid in [
            serde_json::json!({"value":{"Choice":2}}),
            serde_json::json!({"value":{"Float":1.},"unknown":true}),
            serde_json::json!({"value":{"Float":1.},"keyframes":[key.clone(),key.clone()]}),
            serde_json::json!({"value":{"Float":1.},"keyframes":[{"time":17,"value":{"Float":7.},"interp":"Unknown"}]}),
            serde_json::json!({"value":{"Float":1.},"keyframes":vec![key;33]}),
        ] {
            assert!(
                convert(&invalid).is_err(),
                "unsupported source data was accepted"
            );
        }
    }
}
