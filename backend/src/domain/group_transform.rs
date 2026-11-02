//! Native child-canvas to parent-canvas affine mapping for group surfaces.
use anyhow::{ensure, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GroupAffine(pub [f64; 6]);
impl GroupAffine {
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    pub fn new(
        parent: [f64; 2],
        child: [f64; 2],
        anchor: [f64; 2],
        offset: [f64; 2],
        scale: [f64; 2],
        degrees: f64,
    ) -> Result<Self> {
        ensure!(
            parent
                .into_iter()
                .chain(child)
                .chain(anchor)
                .chain(offset)
                .chain(scale)
                .chain([degrees])
                .all(f64::is_finite),
            "group transform requires finite values"
        );
        ensure!(
            parent.into_iter().chain(child).all(|value| value > 0.0),
            "group canvas dimensions must be positive"
        );
        ensure!(
            anchor.into_iter().all(|value| (0.0..=1.0).contains(&value)),
            "group anchor must lie in canvas"
        );
        let (sin, cos) = degrees.to_radians().sin_cos();
        let a = cos * scale[0];
        let b = sin * scale[0];
        let c = -sin * scale[1];
        let d = cos * scale[1];
        let x = child[0] * anchor[0];
        let y = child[1] * anchor[1];
        Ok(Self([
            a,
            b,
            c,
            d,
            parent[0] * 0.5 + offset[0] - a * x - c * y,
            parent[1] * 0.5 + offset[1] - b * x - d * y,
        ]))
    }
    pub fn point(self, point: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, x, y] = self.0;
        [
            a * point[0] + c * point[1] + x,
            b * point[0] + d * point[1] + y,
        ]
    }
    pub fn compose(self, child: Self) -> Result<Self> {
        let [a, b, c, d, x, y] = self.0;
        let [e, f, g, h, u, v] = child.0;
        let values = [
            a * e + c * f,
            b * e + d * f,
            a * g + c * h,
            b * g + d * h,
            a * u + c * v + x,
            b * u + d * v + y,
        ];
        ensure!(
            values.into_iter().all(f64::is_finite),
            "nested transform overflow"
        );
        Ok(Self(values))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchor_rotation_and_nested_matrix_preserve_coordinates() {
        let outer = GroupAffine::new(
            [200.0, 100.0],
            [100.0, 50.0],
            [0.5, 0.5],
            [10.0, 5.0],
            [2.0, 1.0],
            90.0,
        )
        .unwrap();
        let center = outer.point([50.0, 25.0]);
        assert!((center[0] - 110.0).abs() < 1e-12 && (center[1] - 55.0).abs() < 1e-12);
        let inner = GroupAffine::new(
            [100.0, 50.0],
            [20.0, 10.0],
            [0.0, 0.0],
            [0.0, 0.0],
            [1.0, 1.0],
            0.0,
        )
        .unwrap();
        let expected = outer.point(inner.point([3.0, 4.0]));
        let actual = outer.compose(inner).unwrap().point([3.0, 4.0]);
        assert!((actual[0] - expected[0]).abs() < 1e-12 && (actual[1] - expected[1]).abs() < 1e-12);
        assert!(GroupAffine::new(
            [0.0, 10.0],
            [20.0, 10.0],
            [0.5, 0.5],
            [0.0, 0.0],
            [1.0, 1.0],
            0.0
        )
        .is_err());
    }
}
