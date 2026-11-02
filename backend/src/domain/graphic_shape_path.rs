//! Graphic shape contours from Filmcraft's vector renderer (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicShapePath {
    pub shape: u32,
    pub size: [f32; 2],
    pub sides: u32,
    pub corner_radius: f32,
    pub points: Vec<[f32; 2]>,
}
impl Default for GraphicShapePath {
    fn default() -> Self {
        Self {
            shape: 0,
            size: [400., 200.],
            sides: 6,
            corner_radius: 0.,
            points: Vec::new(),
        }
    }
}
impl GraphicShapePath {
    pub fn path(&self) -> Result<filmcraft_text::Path> {
        ensure!(
            self.size.iter().all(|value| *value >= 0.),
            "invalid graphic shape dimensions"
        );
        self.path_with_signed_size()
    }
    /// Evaluated responsive pins may invert dimensions after authored size admission.
    pub(crate) fn path_with_signed_size(&self) -> Result<filmcraft_text::Path> {
        ensure!(
            self.shape <= 3 && (3..=64).contains(&self.sides),
            "invalid graphic shape choice"
        );
        ensure!(
            self.size.iter().all(|v| v.is_finite())
                && self.corner_radius.is_finite()
                && (0.0..=10000.).contains(&self.corner_radius),
            "invalid graphic shape dimensions"
        );
        ensure!(
            self.points.iter().flatten().all(|v| v.is_finite()),
            "invalid graphic path point"
        );
        let (w, h) = (self.size[0] / 2., self.size[1] / 2.);
        Ok(match self.shape {
            1 => filmcraft_text::Path::ellipse(0., 0., w, h),
            2 => {
                let n = self.sides.max(3);
                let points: Vec<_> = (0..n)
                    .map(|i| {
                        let a = std::f32::consts::TAU * i as f32 / n as f32
                            - std::f32::consts::FRAC_PI_2;
                        (a.cos() * w, a.sin() * h)
                    })
                    .collect();
                filmcraft_text::Path::polygon(&points)
            }
            3 if self.points.len() >= 3 => filmcraft_text::Path::polygon(
                &self.points.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>(),
            ),
            _ => filmcraft_text::Path::round_rect(-w, -h, w, h, self.corner_radius),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_contours_and_coverage_bits() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-shape-path.json"
        ))
        .unwrap();
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 72);
        for (index, c) in cases.iter().enumerate() {
            let shape: GraphicShapePath = serde_json::from_value(c["parameters"].clone()).unwrap();
            let path = shape.path().unwrap();
            let expected: Vec<Vec<(f32, f32)>> =
                serde_json::from_value(c["contours"].clone()).unwrap();
            assert_eq!(path.contours, expected, "contours{index}");
            let matrix: filmcraft_text::Xform =
                serde_json::from_value(c["matrix"].clone()).unwrap();
            let mut mask = filmcraft_text::Mask::new(64, 48);
            filmcraft_text::raster::fill_into(&path.transformed(&matrix), &mut mask, 0., 0.);
            let expected: Vec<u32> = serde_json::from_value(c["coverageBits"].clone()).unwrap();
            assert_eq!(
                mask.a.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected,
                "coverage{index}"
            );
        }
    }
}
