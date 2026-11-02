//! Component-preserving import of Filmcraft Vec2 and Color Params.
use super::composition::AnimatableValue;
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;

/// Finite source coordinates retain f64 precision. An unused auto/null baseline
/// is accepted when all keyed coordinates are finite. Sampled auto coordinates
/// still require layer geometry and are refused here.
pub fn point(source: &Value) -> Result<[AnimatableValue; 2]> {
    // The original evaluator uses only key values when keys exist, including
    // before the first key. An unused auto baseline needs no layer geometry.
    let mut keyed = source.clone();
    if let Some(first) = source
        .get("keyframes")
        .and_then(Value::as_array)
        .and_then(|keys| keys.first())
    {
        let value = source
            .get("value")
            .and_then(Value::as_object)
            .context("invalid Filmcraft point value")?;
        ensure!(value.len() == 1, "invalid Filmcraft point tag");
        let point = value
            .get("Vec2")
            .and_then(Value::as_object)
            .context("expected Filmcraft Vec2")?;
        ensure!(
            point.len() == 2 && point.contains_key("x") && point.contains_key("y"),
            "unknown Filmcraft point fields"
        );
        ensure!(
            point
                .values()
                .all(|v| v.is_null() || v.as_f64().is_some_and(f64::is_finite)),
            "invalid point baseline"
        );
        if point.values().any(Value::is_null) {
            keyed["value"] = first
                .get("value")
                .context("missing Filmcraft key value")?
                .clone();
        }
    }
    split(&keyed, |value| {
        let object = value.as_object().context("invalid Filmcraft point value")?;
        ensure!(object.len() == 1, "invalid Filmcraft point tag");
        let point = object
            .get("Vec2")
            .and_then(Value::as_object)
            .context("expected Filmcraft Vec2")?;
        ensure!(point.len() == 2, "unknown Filmcraft point fields");
        Ok([
            point
                .get("x")
                .and_then(Value::as_f64)
                .context("point x is auto or invalid")?,
            point
                .get("y")
                .and_then(Value::as_f64)
                .context("point y is auto or invalid")?,
        ])
    })
}

/// Lower original automatic keyed coordinates onto the integer source-tick grid.
/// An interpolated NaN is resolved after interpolation, so a finite-to-auto
/// segment switches to fallback one source tick after its finite endpoint.
/// Explicit Hold segments retain the finite endpoint until the next key.
/// The caller supplies the effect's runtime fallback in full-resolution pixels.
pub fn point_with_runtime_fallback(
    source: &Value,
    fallback: [f64; 2],
) -> Result<[AnimatableValue; 2]> {
    use super::keyframes::{Interpolation, Keyframe};
    ensure!(
        fallback.iter().all(|v| v.is_finite()),
        "invalid automatic point fallback"
    );
    let replace = |value: &mut Value| -> Result<[bool; 2]> {
        let tagged = value
            .as_object_mut()
            .context("invalid Filmcraft point value")?;
        ensure!(tagged.len() == 1, "invalid Filmcraft point tag");
        let point = tagged
            .get_mut("Vec2")
            .and_then(Value::as_object_mut)
            .context("expected Filmcraft Vec2")?;
        ensure!(
            point.len() == 2 && point.contains_key("x") && point.contains_key("y"),
            "unknown Filmcraft point fields"
        );
        let mut auto = [false; 2];
        for (i, name) in ["x", "y"].into_iter().enumerate() {
            let value = point.get_mut(name).expect("checked coordinate");
            auto[i] = value.is_null();
            if auto[i] {
                *value = serde_json::json!(fallback[i]);
            } else {
                ensure!(
                    value.as_f64().is_some_and(f64::is_finite),
                    "invalid point coordinate"
                );
            }
        }
        Ok(auto)
    };
    let mut finite = source.clone();
    replace(finite.get_mut("value").context("missing Filmcraft value")?)?;
    let mut automatic = Vec::new();
    if let Some(keys) = finite.get_mut("keyframes") {
        for key in keys.as_array_mut().context("invalid Filmcraft keyframes")? {
            automatic.push(replace(
                key.get_mut("value")
                    .context("missing Filmcraft key value")?,
            )?);
        }
    }
    let mut curves = point(&finite)?;
    for (axis, curve) in curves.iter_mut().enumerate() {
        if let AnimatableValue::Keyframes { track } = curve {
            let original = track.keyframes.clone();
            let mut keys = Vec::new();
            for (i, key) in original.iter().enumerate() {
                let mut key = key.clone();
                if let Some(next) = original.get(i + 1) {
                    let hold = key.interpolation == Some(Interpolation::Hold);
                    if automatic[i][axis] || automatic[i + 1][axis] {
                        key.interpolation = Some(Interpolation::Hold);
                        let switch = key
                            .tick
                            .checked_add(1)
                            .context("automatic point tick overflow")?;
                        if !automatic[i][axis]
                            && automatic[i + 1][axis]
                            && !hold
                            && switch < next.tick
                        {
                            keys.push(key);
                            keys.push(Keyframe {
                                tick: switch,
                                value: fallback[axis],
                                tangents: None,
                                interpolation: Some(Interpolation::Hold),
                            });
                            continue;
                        }
                    }
                }
                keys.push(key);
            }
            track.keyframes = keys;
        }
        curve.validate()?;
    }
    Ok(curves)
}

/// Resolve automatic baseline coordinates exactly as the original project loader.
/// Keys are deliberately untouched: resolving them before interpolation changes
/// the original NaN propagation semantics.
pub fn resolve_point_baseline(
    source: &Value,
    effect: &str,
    parameter: &str,
    frame: (u32, u32),
    source_size: (u32, u32),
) -> Result<Value> {
    ensure!(
        frame.0 > 0 && frame.1 > 0 && source_size.0 > 0 && source_size.1 > 0,
        "invalid point geometry"
    );
    let mut resolved = source.clone();
    let tagged = resolved
        .get_mut("value")
        .and_then(Value::as_object_mut)
        .context("invalid Filmcraft point value")?;
    ensure!(tagged.len() == 1, "invalid Filmcraft point tag");
    let point = tagged
        .get_mut("Vec2")
        .and_then(Value::as_object_mut)
        .context("expected Filmcraft Vec2")?;
    ensure!(
        point.len() == 2 && point.contains_key("x") && point.contains_key("y"),
        "unknown Filmcraft point fields"
    );
    let size = if parameter == "anchor" {
        source_size
    } else {
        frame
    };
    let (fx, fy) = match (effect, parameter) {
        ("corner_pin", "upper_left") => (0., 0.),
        ("corner_pin", "upper_right") => (1., 0.),
        ("corner_pin", "lower_left") => (0., 1.),
        ("corner_pin", "lower_right") => (1., 1.),
        ("gradient", "start") => (0.5, 0.),
        ("gradient", "end") => (0.5, 1.),
        ("lightning", "start") => (0.1, 0.1),
        ("lightning", "end") => (0.9, 0.9),
        ("checkerboard", "anchor") => (0., 0.),
        ("write_on", "brush") => (0.5, 0.5),
        ("volumetric_rays", "center") => (0.5, 0.25),
        _ => (0.5, if parameter == "end" { 1. } else { 0.5 }),
    };
    for (axis, fallback) in [("x", f64::from(size.0) * fx), ("y", f64::from(size.1) * fy)] {
        let value = point.get_mut(axis).expect("checked coordinate");
        if value.is_null() {
            *value = serde_json::json!(fallback);
        } else {
            ensure!(
                value.as_f64().is_some_and(f64::is_finite),
                "invalid point coordinate"
            );
        }
    }
    Ok(resolved)
}

/// Preserve all RGBA channels, including alpha. Original Color components are
/// f32 before interpolation; promote that exact value to native f64 curves.
pub fn color(source: &Value) -> Result<[AnimatableValue; 4]> {
    split(source, |value| {
        let object = value.as_object().context("invalid Filmcraft color value")?;
        ensure!(object.len() == 1, "invalid Filmcraft color tag");
        let values = object
            .get("Color")
            .and_then(Value::as_array)
            .context("expected Filmcraft Color")?;
        ensure!(values.len() == 4, "expected four Filmcraft color channels");
        let mut channels = [0.; 4];
        for (channel, value) in channels.iter_mut().zip(values) {
            let original = value.as_f64().context("invalid color channel")? as f32;
            ensure!(original.is_finite(), "nonfinite original color channel");
            *channel = f64::from(original);
        }
        Ok(channels)
    })
}

fn split<const N: usize>(
    source: &Value,
    extract: impl Fn(&Value) -> Result<[f64; N]>,
) -> Result<[AnimatableValue; N]> {
    let object = source.as_object().context("invalid Filmcraft Param")?;
    let baseline = extract(object.get("value").context("missing Filmcraft value")?)?;
    ensure!(
        baseline.iter().all(|value| value.is_finite()),
        "nonfinite source components"
    );
    let mut slots: Vec<_> = (0..N).map(|_| source.clone()).collect();
    for (slot, value) in slots.iter_mut().zip(baseline) {
        slot["value"] = serde_json::json!({"Float":value});
    }
    if let Some(keys) = object.get("keyframes") {
        let keys = keys.as_array().context("invalid Filmcraft keyframes")?;
        for (i, key) in keys.iter().enumerate() {
            let values = extract(key.get("value").context("missing Filmcraft key value")?)?;
            ensure!(
                values.iter().all(|value| value.is_finite()),
                "nonfinite source key components"
            );
            for (slot, value) in slots.iter_mut().zip(values) {
                slot["keyframes"][i]["value"] = serde_json::json!({"Float":value});
            }
        }
    }
    let converted: Vec<_> = slots
        .iter()
        .map(super::filmcraft_scalar_curve::convert)
        .collect::<Result<_>>()?;
    match converted.try_into() {
        Ok(components) => Ok(components),
        Err(_) => bail!("invalid source component count"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::animation_sample_time::AnimationSampleTime;
    use super::*;
    #[test]
    fn all_3528_source_point_and_rgba_samples_match_original() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-component-param.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 3528);
        for (index, case) in cases.iter().enumerate() {
            let is_color = case["kind"] == "color";
            let curves = if is_color {
                color(&case["parameter"]).unwrap().to_vec()
            } else {
                point(&case["parameter"]).unwrap().to_vec()
            };
            let at = case["at"].as_str().unwrap().parse().unwrap();
            let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
            for (channel, (curve, expected)) in curves.iter().zip(expected).enumerate() {
                let restored: AnimatableValue =
                    serde_json::from_value(serde_json::to_value(curve).unwrap()).unwrap();
                assert_eq!(*curve, restored);
                let value = super::super::nested_composition::sample_value(
                    &restored,
                    AnimationSampleTime::FilmcraftSourceTicks(at),
                )
                .unwrap();
                let bits = if is_color {
                    (value as f32).to_bits() as u64
                } else {
                    value.to_bits()
                };
                assert_eq!(
                    bits, expected,
                    "source component case{index} channel{channel}"
                );
            }
        }
    }
    #[test]
    fn all_28224_automatic_key_samples_match_post_interpolation_resolution() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-key-point.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 28224);
        for (index, case) in cases.iter().enumerate() {
            let fallback = serde_json::from_value(case["fallback"].clone()).unwrap();
            let curves = point_with_runtime_fallback(&case["parameter"], fallback).unwrap();
            let saved: [AnimatableValue; 2] =
                serde_json::from_value(serde_json::to_value(&curves).unwrap()).unwrap();
            assert_eq!(saved, curves);
            let at = case["at"].as_str().unwrap().parse().unwrap();
            let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
            for (axis, (curve, bits)) in saved.iter().zip(expected).enumerate() {
                assert_eq!(
                    super::super::nested_composition::sample_value(
                        curve,
                        AnimationSampleTime::FilmcraftSourceTicks(at)
                    )
                    .unwrap()
                    .to_bits(),
                    bits,
                    "auto key case {index} axis {axis}"
                );
            }
        }
    }

    #[test]
    fn all_1272_original_geometry_resolutions_reach_typed_imports() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-point-geometry.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle["controls"], 53);
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1272);
        let adapters: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-video-default-adapters.json"
        ))
        .unwrap();
        for case in cases {
            let effect = case["effect"].as_str().unwrap();
            let control = case["parameter"].as_str().unwrap();
            let frame: (u32, u32) = serde_json::from_value(case["frame"].clone()).unwrap();
            let source: (u32, u32) = serde_json::from_value(case["source"].clone()).unwrap();
            let resolved =
                resolve_point_baseline(&case["before"], effect, control, frame, source).unwrap();
            assert_eq!(resolved, case["after"], "{effect}.{control}");
            assert_eq!(point(&resolved).unwrap(), point(&case["after"]).unwrap());
            if effect == "motion" {
                let actual = super::super::filmcraft_motion::FilmcraftMotion::from_source_parameters_in_geometry(
                    &serde_json::json!({control: case["before"]}), frame, source,
                ).unwrap();
                let expected =
                    super::super::filmcraft_motion::FilmcraftMotion::from_source_parameters(
                        &serde_json::json!({control: case["after"]}),
                    )
                    .unwrap();
                assert_eq!(actual, expected);
                let saved: super::super::filmcraft_motion::FilmcraftMotion =
                    serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
                assert_eq!(saved, expected);
            } else {
                let native = adapters["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["source"] == effect)
                    .unwrap()["native"]
                    .clone();
                let native: super::super::composition::VideoEffect =
                    serde_json::from_value(native).unwrap();
                let actual =
                    super::super::filmcraft_video_project::with_source_point_animation_in_geometry(
                        native.clone(),
                        effect,
                        control,
                        &case["before"],
                        frame,
                        source,
                    )
                    .unwrap();
                let expected = super::super::filmcraft_video_project::with_source_point_animation(
                    native,
                    control,
                    &case["after"],
                )
                .unwrap();
                assert_eq!(actual, expected, "{effect}.{control}");
                let saved: super::super::composition::VideoEffect =
                    serde_json::from_value(serde_json::to_value(&actual).unwrap()).unwrap();
                assert_eq!(saved, expected);
            }
        }
        assert!(resolve_point_baseline(
            &serde_json::json!({"value":{"Vec2":{"x":null,"y":null}}}),
            "motion",
            "position",
            (0, 4),
            (4, 4)
        )
        .is_err());
    }

    #[test]
    fn unused_auto_baselines_preserve_all_5292_original_keyed_points() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-auto-baseline-point.json"
        ))
        .unwrap();
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 5292);
        for c in cases {
            let curves = point(&c["parameter"]).unwrap();
            let saved: [AnimatableValue; 2] =
                serde_json::from_value(serde_json::to_value(&curves).unwrap()).unwrap();
            assert_eq!(curves, saved);
            let at = c["at"].as_str().unwrap().parse().unwrap();
            let expected: Vec<u64> = serde_json::from_value(c["bits"].clone()).unwrap();
            for (curve, bits) in saved.iter().zip(expected) {
                assert_eq!(
                    super::super::nested_composition::sample_value(
                        curve,
                        AnimationSampleTime::FilmcraftSourceTicks(at)
                    )
                    .unwrap()
                    .to_bits(),
                    bits
                );
            }
        }
        assert!(point(&serde_json::json!({"value":{"Vec2":{"x":null,"y":0}},"keyframes":[{"time":0,"value":{"Vec2":{"x":null,"y":0}}}]})).is_err());
    }
    #[test]
    fn alpha_and_static_point_survive_and_auto_is_explicit() {
        let p = point(&serde_json::json!({"value":{"Vec2":{"x":0.123456789012345,"y":-0.25}}}))
            .unwrap();
        assert_eq!(p[0], AnimatableValue::constant(0.123456789012345));
        let c = color(&serde_json::json!({"value":{"Color":[0.1,0.2,0.3,0.4]}})).unwrap();
        assert_eq!(c[3], AnimatableValue::constant(f64::from(0.4f32)));
        assert!(point(&serde_json::json!({"value":{"Vec2":{"x":null,"y":0.}}})).is_err());
        assert!(color(&serde_json::json!({"value":{"Color":[0.1,0.2,0.3]}})).is_err());
        assert!(
            point(&serde_json::json!({"value":{"Vec2":{"x":0.,"y":0.}},"unknown":true})).is_err()
        );
    }
}
