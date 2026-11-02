//! Filmcraft intrinsic Motion import. Pixel-space anchors remain animatable.
//! This model is not yet admitted into public clip preview/export.
use super::{animation_sample_time::AnimationSampleTime, composition::AnimatableValue};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilmcraftMotion {
    pub position: Option<[AnimatableValue; 2]>,
    pub anchor: Option<[AnimatableValue; 2]>,
    pub scale: AnimatableValue,
    pub scale_width: AnimatableValue,
    pub rotation: AnimatableValue,
    pub uniform_scale: bool,
    pub anti_flicker: AnimatableValue,
}
/// Persisted intrinsic Motion on a video clip; native authoring transform is separate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipMotion {
    pub parameters: FilmcraftMotion,
    pub enabled: bool,
    pub scale_to_frame: bool,
}
impl FilmcraftMotion {
    pub fn validate(&self) -> Result<()> {
        for point in [&self.position, &self.anchor].into_iter().flatten() {
            for value in point {
                value.validate()?;
            }
        }
        for value in [
            &self.scale,
            &self.scale_width,
            &self.rotation,
            &self.anti_flicker,
        ] {
            value.validate()?;
        }
        Ok(())
    }
}
impl FilmcraftMotion {
    /// Apply original project-loader point defaults with known layer geometry.
    pub fn from_source_parameters_in_geometry(
        params: &Value,
        frame: (u32, u32),
        source_size: (u32, u32),
    ) -> Result<Self> {
        let mut resolved = params.clone();
        let map = resolved
            .as_object_mut()
            .context("invalid Motion parameters")?;
        let mut animated = Vec::new();
        for name in ["position", "anchor"] {
            if let Some(point) = map.get_mut(name) {
                *point = super::filmcraft_component_curve::resolve_point_baseline(
                    point,
                    "motion",
                    name,
                    frame,
                    source_size,
                )?;
                let keyed_auto = point
                    .get("keyframes")
                    .and_then(Value::as_array)
                    .is_some_and(|keys| {
                        keys.iter().any(|key| {
                            ["x", "y"].iter().any(|axis| {
                                key.pointer(&format!("/value/Vec2/{axis}"))
                                    .is_some_and(Value::is_null)
                            })
                        })
                    });
                if keyed_auto {
                    let size = if name == "anchor" { source_size } else { frame };
                    let fallback = [f64::from(size.0) / 2., f64::from(size.1) / 2.];
                    // Validate the complete source shape before normalising the ignored Y.
                    super::filmcraft_component_curve::point_with_runtime_fallback(point, fallback)?;
                    let mut coupled = point.clone();
                    for key in coupled["keyframes"].as_array_mut().expect("checked keys") {
                        if key.pointer("/value/Vec2/x").is_some_and(Value::is_null) {
                            key["value"]["Vec2"]["y"] = Value::Null;
                        } else {
                            ensure!(!key.pointer("/value/Vec2/y").is_some_and(Value::is_null),
                                "Motion has finite X with automatic Y: original matrix is nonfinite");
                        }
                    }
                    animated.push((
                        name,
                        super::filmcraft_component_curve::point_with_runtime_fallback(
                            &coupled, fallback,
                        )?,
                    ));
                }
            }
        }
        for (name, _) in &animated {
            map.remove(*name);
        }
        let mut motion = Self::from_source_parameters(&resolved)?;
        for (name, point) in animated {
            if name == "position" {
                motion.position = Some(point);
            } else {
                motion.anchor = Some(point);
            }
        }
        Ok(motion)
    }

    pub fn from_source_parameters(params: &Value) -> Result<Self> {
        let map = params.as_object().context("invalid Motion parameters")?;
        ensure!(
            map.keys().all(|k| matches!(
                k.as_str(),
                "position"
                    | "anchor"
                    | "scale"
                    | "scale_width"
                    | "rotation"
                    | "uniform_scale"
                    | "anti_flicker"
            )),
            "unknown Motion control"
        );
        let scalar = |name: &str, default: f64| -> Result<AnimatableValue> {
            match map.get(name) {
                Some(p) => super::filmcraft_scalar_curve::convert(p),
                None => Ok(AnimatableValue::constant(default)),
            }
        };
        let point = |name: &str| -> Result<Option<[AnimatableValue; 2]>> {
            let Some(p) = map.get(name) else {
                return Ok(None);
            };
            let auto = p.pointer("/value/Vec2/x").is_some_and(Value::is_null);
            if auto
                && p.get("keyframes")
                    .is_none_or(|k| k.as_array().is_some_and(Vec::is_empty))
            {
                let mut checked = p.clone();
                let v = checked
                    .pointer_mut("/value/Vec2")
                    .and_then(Value::as_object_mut)
                    .context("invalid Motion point")?;
                ensure!(
                    v.len() == 2 && v.contains_key("y"),
                    "unknown Motion point fields"
                );
                v.insert("x".into(), serde_json::json!(0.));
                v.insert("y".into(), serde_json::json!(0.));
                super::filmcraft_component_curve::point(&checked)?;
                return Ok(None);
            }
            Ok(Some(super::filmcraft_component_curve::point(p)?))
        };
        let uniform_scale = match map.get("uniform_scale") {
            None => true,
            Some(p) => p
                .pointer("/value/Bool")
                .and_then(Value::as_bool)
                .context("invalid Motion uniform scale")?,
        };
        Ok(Self {
            position: point("position")?,
            anchor: point("anchor")?,
            scale: scalar("scale", 100.)?,
            scale_width: scalar("scale_width", 100.)?,
            rotation: scalar("rotation", 0.)?,
            uniform_scale,
            anti_flicker: scalar("anti_flicker", 0.)?,
        })
    }
    pub fn matrix(
        &self,
        canvas: (u32, u32),
        source: (u32, u32),
        ticks: i64,
        enabled: bool,
        scale_to_frame: bool,
    ) -> Result<[f64; 6]> {
        ensure!(
            canvas.0 > 0 && canvas.1 > 0 && source.0 > 0 && source.1 > 0,
            "empty Motion dimensions"
        );
        let time = AnimationSampleTime::FilmcraftSourceTicks(ticks);
        let sample = |v: &AnimatableValue| super::nested_composition::sample_value(v, time);
        let point = |p: &Option<[AnimatableValue; 2]>, fallback: [f64; 2]| -> Result<[f64; 2]> {
            match p {
                Some([x, y]) => Ok([sample(x)?, sample(y)?]),
                None => Ok(fallback),
            }
        };
        let mut position = [canvas.0 as f64 / 2., canvas.1 as f64 / 2.];
        let mut anchor = [source.0 as f64 / 2., source.1 as f64 / 2.];
        let mut scale = [1., 1.];
        let mut rotation = 0.;
        if enabled {
            position = point(&self.position, position)?;
            anchor = point(&self.anchor, anchor)?;
            let height = sample(&self.scale)? / 100.;
            scale = [
                if self.uniform_scale {
                    height
                } else {
                    sample(&self.scale_width)? / 100.
                },
                height,
            ];
            rotation = sample(&self.rotation)?;
        }
        if scale_to_frame {
            let fit = (canvas.0 as f64 / source.0 as f64).min(canvas.1 as f64 / source.1 as f64);
            scale = scale.map(|v| v * fit);
        }
        let (s, c) = rotation.to_radians().sin_cos();
        let multiply = |a: [f64; 6], b: [f64; 6]| {
            [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ]
        };
        let m = multiply(
            multiply(
                multiply(
                    [1., 0., 0., 1., position[0], position[1]],
                    [c, s, -s, c, 0., 0.],
                ),
                [scale[0], 0., 0., scale[1], 0., 0.],
            ),
            [1., 0., 0., 1., -anchor[0], -anchor[1]],
        );
        ensure!(m.iter().all(|v| v.is_finite()), "nonfinite Motion matrix");
        Ok(m)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_2268_original_automatic_key_matrices_match_with_unequal_geometry() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-motion-auto-matrix.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2268);
        for (i, case) in cases.iter().enumerate() {
            let frame = serde_json::from_value(case["frame"].clone()).unwrap();
            let source = serde_json::from_value(case["source"].clone()).unwrap();
            let motion = FilmcraftMotion::from_source_parameters_in_geometry(
                &case["parameters"],
                frame,
                source,
            )
            .unwrap();
            let saved: FilmcraftMotion =
                serde_json::from_value(serde_json::to_value(&motion).unwrap()).unwrap();
            assert_eq!(saved, motion);
            let expected: Vec<u64> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(
                saved
                    .matrix(
                        frame,
                        source,
                        case["at"].as_str().unwrap().parse().unwrap(),
                        true,
                        false
                    )
                    .unwrap()
                    .map(f64::to_bits)
                    .as_slice(),
                expected,
                "Motion automatic key matrix {i}"
            );
        }
        let invalid = serde_json::json!({"position":{"value":{"Vec2":{"x":0.,"y":0.}},"keyframes":[{"time":0,"value":{"Vec2":{"x":0.,"y":null}},"interp":"Linear"}]}});
        assert!(
            FilmcraftMotion::from_source_parameters_in_geometry(&invalid, (8, 6), (4, 2)).is_err()
        );
    }

    #[test]
    fn original_motion_source_point_matrices_match_all_72_cases() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-motion-import.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 72);
        for c in cases {
            let motion = FilmcraftMotion::from_source_parameters(&c["sourceParameters"]).unwrap();
            let restored: FilmcraftMotion =
                serde_json::from_value(serde_json::to_value(&motion).unwrap()).unwrap();
            assert_eq!(motion, restored);
            let matrix = restored
                .matrix(
                    (4, 4),
                    (4, 4),
                    c["effectTime"].as_str().unwrap().parse().unwrap(),
                    true,
                    false,
                )
                .unwrap();
            let expected: Vec<u64> = serde_json::from_value(c["matrixBits"].clone()).unwrap();
            assert_eq!(matrix.map(f64::to_bits).as_slice(), expected);
        }
    }
    #[test]
    fn defaults_auto_points_disabled_and_fit() {
        let auto = serde_json::json!({"position":{"value":{"Vec2":{"x":null,"y":null}}}});
        let p = FilmcraftMotion::from_source_parameters(&auto).unwrap();
        assert!(p.position.is_none());
        assert_eq!(
            p.matrix((8, 8), (4, 2), 0, true, true).unwrap(),
            [2., 0., 0., 2., 0., 2.]
        );
        let mut q = p.clone();
        q.position = Some([
            AnimatableValue::constant(99.),
            AnimatableValue::constant(-99.),
        ]);
        assert_eq!(
            q.matrix((8, 8), (4, 2), 0, false, true).unwrap(),
            p.matrix((8, 8), (4, 2), 0, true, true).unwrap()
        );
        assert!(
            FilmcraftMotion::from_source_parameters(&serde_json::json!({"unknown":{}})).is_err()
        );
        assert!(p.matrix((0, 8), (4, 2), 0, true, false).is_err());
    }
}
