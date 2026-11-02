//! Filmcraft Corner Pin CPU projective warp; MIT attribution in docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CornerPin {
    pub upper_left: Option<[Option<f64>; 2]>,
    pub upper_right: Option<[Option<f64>; 2]>,
    pub lower_left: Option<[Option<f64>; 2]>,
    pub lower_right: Option<[Option<f64>; 2]>,
}
type Matrix = [f64; 9];
fn square_to_quad(q: [[f64; 2]; 4]) -> Matrix {
    let [p0, p1, p2, p3] = q;
    let sx = p0[0] - p1[0] + p2[0] - p3[0];
    let sy = p0[1] - p1[1] + p2[1] - p3[1];
    if sx.abs() < 1e-12 && sy.abs() < 1e-12 {
        return [
            p1[0] - p0[0],
            p2[0] - p1[0],
            p0[0],
            p1[1] - p0[1],
            p2[1] - p1[1],
            p0[1],
            0.,
            0.,
            1.,
        ];
    }
    let (dx1, dx2, dy1, dy2) = (p1[0] - p2[0], p3[0] - p2[0], p1[1] - p2[1], p3[1] - p2[1]);
    let det = dx1 * dy2 - dx2 * dy1;
    let g = (sx * dy2 - dx2 * sy) / det;
    let h = (dx1 * sy - sx * dy1) / det;
    [
        p1[0] - p0[0] + g * p1[0],
        p3[0] - p0[0] + h * p3[0],
        p0[0],
        p1[1] - p0[1] + g * p1[1],
        p3[1] - p0[1] + h * p3[1],
        p0[1],
        g,
        h,
        1.,
    ]
}
fn inverse(m: Matrix) -> Option<Matrix> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let co = [
        e * i - f * h,
        -(d * i - f * g),
        d * h - e * g,
        -(b * i - c * h),
        a * i - c * g,
        -(a * h - b * g),
        b * f - c * e,
        -(a * f - c * d),
        a * e - b * d,
    ];
    let det = a * co[0] + b * co[1] + c * co[2];
    if det.abs() < 1e-18 {
        return None;
    }
    Some([
        co[0] / det,
        co[3] / det,
        co[6] / det,
        co[1] / det,
        co[4] / det,
        co[7] / det,
        co[2] / det,
        co[5] / det,
        co[8] / det,
    ])
}
impl CornerPin {
    pub fn validate(&self) -> Result<()> {
        for p in [
            self.upper_left,
            self.upper_right,
            self.lower_left,
            self.lower_right,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                p.into_iter().flatten().all(f64::is_finite),
                "nonfinite Corner Pin coordinate"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Corner Pin pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let defaults = [
            [0., 0.],
            [w as f64, 0.],
            [w as f64, h as f64],
            [0., h as f64],
        ];
        let points = [
            self.upper_left,
            self.upper_right,
            self.lower_right,
            self.lower_left,
        ];
        let q = std::array::from_fn(|i| {
            std::array::from_fn(|axis| {
                points[i].unwrap_or([None, None])[axis]
                    .map(|v| v * px_scale as f64)
                    .unwrap_or(defaults[i][axis])
            })
        });
        if q.iter()
            .zip(defaults)
            .all(|(p, d)| (p[0] - d[0]).hypot(p[1] - d[1]) < 1e-6)
        {
            return Ok(());
        }
        let Some(m) = inverse(square_to_quad(q)) else {
            return Ok(());
        };
        let src = frame.pixels().to_vec();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let (x, y) = ((index % w) as f64 + 0.5, (index / w) as f64 + 0.5);
                index += 1;
                let den = m[6] * x + m[7] * y + m[8];
                if den.abs() < 1e-12 {
                    return [0.; 4];
                }
                let u = (m[0] * x + m[1] * y + m[2]) / den;
                let v = (m[3] * x + m[4] * y + m[5]) / den;
                if !(-1e-3..=1. + 1e-3).contains(&u) || !(-1e-3..=1. + 1e-3).contains(&v) {
                    return [0.; 4];
                }
                super::twirl::sample(&src, w, h, (u * w as f64) as f32, (v * h as f64) as f32)
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn identity_translation_singular_and_cancellation() {
        let surface = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255],
        };
        let original = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
        let mut frame = original.clone();
        CornerPin::default()
            .apply_linear(&mut frame, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels(), original.pixels());
        let shifted = CornerPin {
            upper_left: Some([Some(1.), None]),
            upper_right: Some([Some(3.), None]),
            lower_left: Some([Some(1.), None]),
            lower_right: Some([Some(3.), None]),
        };
        shifted
            .apply_linear(&mut frame, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels()[0], [0.; 4]);
        assert_eq!(frame.pixels()[1], original.pixels()[0]);
        let point = Some([Some(0.), Some(0.)]);
        let singular = CornerPin {
            upper_left: point,
            upper_right: point,
            lower_left: point,
            lower_right: point,
        };
        frame = original.clone();
        singular
            .apply_linear(&mut frame, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(frame.pixels(), original.pixels());
        assert!(CornerPin::default()
            .apply_linear(&mut frame, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
        assert!(CornerPin {
            upper_left: Some([Some(f64::NAN), None]),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CornerPinChannel {
    UpperLeftX,
    UpperLeftY,
    UpperRightX,
    UpperRightY,
    LowerLeftX,
    LowerLeftY,
    LowerRightX,
    LowerRightY,
}
pub type CornerPinAnimation =
    std::collections::BTreeMap<CornerPinChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &CornerPinAnimation) -> Result<()> {
    for value in animation.values() {
        value.validate()?;
        ensure!(
            super::composition::animatable_values_in(value, -f64::MAX, f64::MAX, true),
            "nonfinite Corner Pin animation"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: CornerPin, a: &CornerPinAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<CornerPin> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (channel, value) in a {
        use CornerPinChannel::*;
        let (point, axis) = match channel {
            UpperLeftX => (&mut p.upper_left, 0),
            UpperLeftY => (&mut p.upper_left, 1),
            UpperRightX => (&mut p.upper_right, 0),
            UpperRightY => (&mut p.upper_right, 1),
            LowerLeftX => (&mut p.lower_left, 0),
            LowerLeftY => (&mut p.lower_left, 1),
            LowerRightX => (&mut p.lower_right, 0),
            LowerRightY => (&mut p.lower_right, 1),
        };
        point.get_or_insert([None, None])[axis] =
            Some(super::nested_composition::sample_value(value, time)?);
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn eight_local_curves_roundtrip_and_independent_auto_axes() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let mut channels = serde_json::Map::new();
        for (i, name) in [
            "upperLeftX",
            "upperLeftY",
            "upperRightX",
            "upperRightY",
            "lowerLeftX",
            "lowerLeftY",
            "lowerRightX",
            "lowerRightY",
        ]
        .into_iter()
        .enumerate()
        {
            channels.insert(name.into(), curve(i as f64, i as f64 + 2.));
        }
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"corner_pin","parameters":{},"animation":channels}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::CornerPin {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Corner Pin")
        };
        let p = sample_animation(parameters, &a, 250000).unwrap();
        assert_eq!(p.upper_left, Some([Some(1.), Some(2.)]));
        assert_eq!(p.lower_right, Some([Some(7.), Some(8.)]));
        let precise = 3.123456789012;
        let a = CornerPinAnimation::from([(
            CornerPinChannel::UpperLeftX,
            serde_json::from_value(curve(precise, precise)).unwrap(),
        )]);
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap().upper_left,
            Some([Some(precise), None])
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Case {
        parameters: CornerPin,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        scale: f32,
        bits: Vec<u32>,
    }
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[test]
    fn original_working_frame_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-corner-pin.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 48);
        for (index, case) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: case.width,
                    height: case.height,
                    rgba: case.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            case.parameters
                .apply_linear(&mut frame, case.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                case.bits,
                "original Corner Pin {index}"
            );
        }
    }
}
