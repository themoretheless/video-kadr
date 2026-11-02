//! Authored graphic shape parameters and original Filmcraft evaluation (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    graphic_shape_path::GraphicShapePath,
    graphic_shape_render::{ShapeAppearance, ShapeLayer, ShapeShadow},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicShape {
    pub name: String,
    pub shape: u32,
    pub size: [f64; 2],
    pub sides: f32,
    pub corner_radius: f32,
    pub points: Vec<[f32; 2]>,
    pub fill: bool,
    pub fill_color: [f32; 4],
    pub stroke: bool,
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_type: u32,
    pub stroke2: bool,
    pub stroke2_color: [f32; 4],
    pub stroke2_width: f32,
    pub stroke2_type: u32,
    pub background: bool,
    pub background_color: [f32; 4],
    pub background_opacity: f32,
    pub background_size: f32,
    pub background_radius: f32,
    pub shadow: bool,
    pub shadow_color: [f32; 4],
    pub shadow_opacity: f32,
    pub shadow_angle: f32,
    pub shadow_distance: f32,
    pub shadow_size: f32,
    pub shadow_blur: f32,
    pub position: Option<[f64; 2]>,
    pub anchor: [f64; 2],
    pub scale: f32,
    pub scale_width: f32,
    pub uniform_scale: bool,
    pub rotation: f32,
    pub opacity: f32,
}
impl Default for GraphicShape {
    fn default() -> Self {
        Self {
            name: String::new(),
            shape: 0,
            size: [400., 200.],
            sides: 6.,
            corner_radius: 0.,
            points: vec![],
            fill: true,
            fill_color: [1.; 4],
            stroke: false,
            stroke_color: [0., 0., 0., 1.],
            stroke_width: 4.,
            stroke_type: 0,
            stroke2: false,
            stroke2_color: [1., 0.85, 0.2, 1.],
            stroke2_width: 10.,
            stroke2_type: 0,
            background: false,
            background_color: [0., 0., 0., 1.],
            background_opacity: 75.,
            background_size: 12.,
            background_radius: 0.,
            shadow: false,
            shadow_color: [0., 0., 0., 1.],
            shadow_opacity: 75.,
            shadow_angle: 135.,
            shadow_distance: 10.,
            shadow_size: 0.,
            shadow_blur: 40.,
            position: None,
            anchor: [0.; 2],
            scale: 100.,
            scale_width: 100.,
            uniform_scale: true,
            rotation: 0.,
            opacity: 100.,
        }
    }
}
impl GraphicShape {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.shape <= 3 && self.stroke_type <= 2 && self.stroke2_type <= 2,
            "invalid graphic shape choice"
        );
        ensure!(
            self.size
                .iter()
                .chain(self.anchor.iter())
                .chain(self.position.iter().flatten())
                .all(|v| v.is_finite()),
            "invalid graphic shape point"
        );
        ensure!(
            self.points.iter().flatten().all(|v| v.is_finite()),
            "invalid graphic path"
        );
        for color in [
            &self.fill_color,
            &self.stroke_color,
            &self.stroke2_color,
            &self.background_color,
            &self.shadow_color,
        ] {
            ensure!(
                color
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
                "invalid graphic shape color"
            );
        }
        for (v, lo, hi) in [
            (self.sides, 3., 64.),
            (self.corner_radius, 0., 10000.),
            (self.stroke_width, 0., 1000.),
            (self.stroke2_width, 0., 1000.),
            (self.background_opacity, 0., 100.),
            (self.background_size, 0., 1000.),
            (self.background_radius, 0., 1000.),
            (self.shadow_opacity, 0., 100.),
            (self.shadow_distance, 0., 1000.),
            (self.shadow_size, 0., 1000.),
            (self.shadow_blur, 0., 1000.),
            (self.scale, 0., 100000.),
            (self.scale_width, 0., 100000.),
            (self.opacity, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "graphic shape scalar out of range"
            );
        }
        ensure!(
            self.rotation.is_finite() && self.shadow_angle.is_finite(),
            "invalid graphic shape angle"
        );
        Ok(())
    }
    pub fn evaluate(
        &self,
        enabled: bool,
        canvas: (u32, u32),
    ) -> Result<(ShapeLayer, filmcraft_text::Xform)> {
        self.validate()?;
        let mut strokes = vec![];
        for (on, color, width, kind) in [
            (
                self.stroke,
                self.stroke_color,
                self.stroke_width,
                self.stroke_type,
            ),
            (
                self.stroke2,
                self.stroke2_color,
                self.stroke2_width,
                self.stroke2_type,
            ),
        ] {
            if on && width > 0. {
                strokes.push((color, width, kind));
            }
        }
        let background = self.background.then(|| {
            let mut color = self.background_color;
            color[3] *= (self.background_opacity / 100.).clamp(0., 1.);
            (
                color,
                self.background_size.max(0.),
                self.background_radius.max(0.),
            )
        });
        let shadow = self.shadow.then(|| {
            let mut color = self.shadow_color;
            color[3] *= (self.shadow_opacity / 100.).clamp(0., 1.);
            let angle = (self.shadow_angle - 90.).to_radians();
            ShapeShadow {
                color,
                offset: (
                    angle.cos() * self.shadow_distance,
                    angle.sin() * self.shadow_distance,
                ),
                size: self.shadow_size.max(0.),
                blur: self.shadow_blur.max(0.),
            }
        });
        let position = self
            .position
            .unwrap_or([canvas.0 as f64 / 2., canvas.1 as f64 / 2.]);
        let s = self.scale as f64 / 100.;
        let sw = if self.uniform_scale {
            s
        } else {
            self.scale_width as f64 / 100.
        };
        let (sin, cos) = (self.rotation as f64).to_radians().sin_cos();
        // Preserve the original affine multiply order before the final f32 conversion.
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
        let matrix = multiply(
            multiply(
                multiply(
                    [1., 0., 0., 1., position[0], position[1]],
                    [cos, sin, -sin, cos, 0., 0.],
                ),
                [sw, 0., 0., s, 0., 0.],
            ),
            [1., 0., 0., 1., -self.anchor[0], -self.anchor[1]],
        )
        .map(|v| v as f32);
        ensure!(matrix.iter().all(|v|v.is_finite()), "graphic transform overflow");
        Ok((
            ShapeLayer {
                enabled,
                shape: GraphicShapePath {
                    shape: self.shape,
                    size: self.size.map(|v| v.max(0.) as f32),
                    sides: self.sides.round().clamp(3., 64.) as u32,
                    corner_radius: self.corner_radius.max(0.),
                    points: self.points.clone(),
                },
                appearance: ShapeAppearance {
                    fill: self.fill.then_some(self.fill_color),
                    strokes,
                    background,
                    shadow,
                },
                opacity: (self.opacity / 100.).clamp(0., 1.),
            },
            matrix,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_authored_parameter_evaluation() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-shape.json"
        ))
        .unwrap();
        for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let mut parameters = serde_json::Map::new();
            for (name, param) in c["effect"]["params"].as_object().unwrap() {
                let mut upper = false;
                let key: String = name
                    .chars()
                    .filter_map(|ch| {
                        if ch == '_' {
                            upper = true;
                            None
                        } else {
                            let c = if upper { ch.to_ascii_uppercase() } else { ch };
                            upper = false;
                            Some(c)
                        }
                    })
                    .collect();
                let value = param["value"].as_object().unwrap();
                let (kind, value) = value.iter().next().unwrap();
                let value = if kind == "Vec2" {
                    serde_json::json!([value["x"], value["y"]])
                } else {
                    value.clone()
                };
                parameters.insert(key, value);
            }
            let parameters: GraphicShape = serde_json::from_value(parameters.into()).unwrap();
            assert_eq!(
                parameters,
                serde_json::from_value(serde_json::to_value(&parameters).unwrap()).unwrap()
            );
            let (layer, matrix) = parameters
                .evaluate(c["enabled"].as_bool().unwrap(), (64, 48))
                .unwrap();
            let expected: ShapeLayer =
                serde_json::from_value(c["evaluated"]["layer"].clone()).unwrap();
            let expected_matrix: filmcraft_text::Xform =
                serde_json::from_value(c["evaluated"]["matrix"].clone()).unwrap();
            assert_eq!(layer, expected, "layer {index}");
            assert_eq!(matrix, expected_matrix, "matrix {index}");
        }
    }
    #[test]
    fn auto_center_and_invalid_parameters() {
        let p = GraphicShape::default();
        let (_, matrix) = p.evaluate(true, (96, 48)).unwrap();
        assert_eq!(matrix, [1., 0., 0., 1., 48., 24.]);
        assert!(GraphicShape {
            shape: 4,
            ..p.clone()
        }
        .validate()
        .is_err());
        assert!(GraphicShape {
            shadow_blur: f32::NAN,
            ..p.clone()
        }
        .validate()
        .is_err());
        assert!(GraphicShape {
            position: Some([f64::INFINITY, 0.]),
            ..p.clone()
        }
        .validate()
        .is_err());
        assert!(GraphicShape {
            stroke2_color: [0., 0., 0., 2.],
            ..p
        }
        .validate()
        .is_err());
    }
}
