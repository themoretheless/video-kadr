use std::fmt;

use serde::{Deserialize, Serialize};

const EPSILON: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Result<Self, GeometryError> {
        ensure_finite(&[x, y])?;
        Ok(Self { x, y })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, GeometryError> {
        ensure_finite(&[x, y, width, height])?;
        if width < 0.0 || height < 0.0 {
            return Err(GeometryError::NegativeSize);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.right()
            && point.y >= self.y
            && point.y <= self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn clamp_within(self, bounds: Self, minimum: (f64, f64)) -> Self {
        let minimum_width = minimum.0.max(0.0).min(bounds.width);
        let minimum_height = minimum.1.max(0.0).min(bounds.height);
        let width = self.width.max(minimum_width).min(bounds.width);
        let height = self.height.max(minimum_height).min(bounds.height);
        let x = self.x.max(bounds.x).min(bounds.right() - width);
        let y = self.y.max(bounds.y).min(bounds.bottom() - height);
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn transform(self, transform: Transform2D) -> Result<Self, GeometryError> {
        let corners = [
            transform.apply(Point::new(self.x, self.y)?)?,
            transform.apply(Point::new(self.right(), self.y)?)?,
            transform.apply(Point::new(self.x, self.bottom())?)?,
            transform.apply(Point::new(self.right(), self.bottom())?)?,
        ];
        let min_x = corners
            .iter()
            .map(|point| point.x)
            .fold(f64::INFINITY, f64::min);
        let min_y = corners
            .iter()
            .map(|point| point.y)
            .fold(f64::INFINITY, f64::min);
        let max_x = corners
            .iter()
            .map(|point| point.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = corners
            .iter()
            .map(|point| point.y)
            .fold(f64::NEG_INFINITY, f64::max);
        Self::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

/// Immutable affine matrix `[a, b, c, d, tx, ty]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform2D {
    pub fn new(values: [f64; 6]) -> Result<Self, GeometryError> {
        ensure_finite(&values)?;
        Ok(Self {
            a: values[0],
            b: values[1],
            c: values[2],
            d: values[3],
            tx: values[4],
            ty: values[5],
        })
    }

    pub const fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn scale(x: f64, y: f64) -> Result<Self, GeometryError> {
        Self::new([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    pub fn apply(self, point: Point) -> Result<Point, GeometryError> {
        Point::new(
            self.a * point.x + self.c * point.y + self.tx,
            self.b * point.x + self.d * point.y + self.ty,
        )
    }

    /// Apply `self`, then `next`.
    pub fn then(self, next: Self) -> Result<Self, GeometryError> {
        Self::new([
            next.a * self.a + next.c * self.b,
            next.b * self.a + next.d * self.b,
            next.a * self.c + next.c * self.d,
            next.b * self.c + next.d * self.d,
            next.a * self.tx + next.c * self.ty + next.tx,
            next.b * self.tx + next.d * self.ty + next.ty,
        ])
    }

    pub fn inverse(self) -> Result<Self, GeometryError> {
        let determinant = self.a * self.d - self.b * self.c;
        if !determinant.is_finite() || determinant.abs() <= EPSILON {
            return Err(GeometryError::SingularTransform);
        }
        Self::new([
            self.d / determinant,
            -self.b / determinant,
            -self.c / determinant,
            self.a / determinant,
            (self.c * self.ty - self.d * self.tx) / determinant,
            (self.b * self.tx - self.a * self.ty) / determinant,
        ])
    }
}

fn ensure_finite(values: &[f64]) -> Result<(), GeometryError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(GeometryError::NonFinite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    NonFinite,
    NegativeSize,
    SingularTransform,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid geometry: {self:?}")
    }
}

impl std::error::Error for GeometryError {}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Corpus {
        schema_version: u32,
        transform_cases: Vec<TransformCase>,
        clamp_cases: Vec<ClampCase>,
    }

    #[derive(Deserialize)]
    struct TransformCase {
        matrix: [f64; 6],
        point: [f64; 2],
        expected: [f64; 2],
    }

    #[derive(Deserialize)]
    struct ClampCase {
        rect: [f64; 4],
        bounds: [f64; 4],
        minimum: [f64; 2],
        expected: [f64; 4],
    }

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "{left} != {right}");
    }

    #[test]
    fn shared_transform_and_clamp_corpus() {
        let corpus: Corpus = serde_json::from_str(include_str!(
            "../../../fixtures/geometry/transform-cases.json"
        ))
        .unwrap();
        assert_eq!(corpus.schema_version, 1);
        for case in corpus.transform_cases {
            let matrix = Transform2D::new(case.matrix).unwrap();
            let point = matrix
                .apply(Point::new(case.point[0], case.point[1]).unwrap())
                .unwrap();
            close(point.x, case.expected[0]);
            close(point.y, case.expected[1]);
            let round_trip = matrix.inverse().unwrap().apply(point).unwrap();
            close(round_trip.x, case.point[0]);
            close(round_trip.y, case.point[1]);
        }
        for case in corpus.clamp_cases {
            let rect = Rect::new(case.rect[0], case.rect[1], case.rect[2], case.rect[3]).unwrap();
            let bounds = Rect::new(
                case.bounds[0],
                case.bounds[1],
                case.bounds[2],
                case.bounds[3],
            )
            .unwrap();
            let result = rect.clamp_within(bounds, (case.minimum[0], case.minimum[1]));
            for (actual, expected) in [result.x, result.y, result.width, result.height]
                .into_iter()
                .zip(case.expected)
            {
                close(actual, expected);
            }
        }
    }

    #[test]
    fn fuzz_like_inputs_stay_finite_and_contained() {
        let bounds = Rect::new(0.0, 0.0, 1920.0, 1080.0).unwrap();
        let mut seed = 0x5eed_u64;
        for _ in 0..2_000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = |shift| ((seed >> shift) as i32 as f64) / 10_000.0;
            let rect = Rect::new(value(0), value(8), value(16).abs(), value(24).abs()).unwrap();
            let clamped = rect.clamp_within(bounds, (2.0, 2.0));
            assert!([clamped.x, clamped.y, clamped.width, clamped.height]
                .iter()
                .all(|value| value.is_finite()));
            assert!(bounds.contains(Point::new(clamped.x, clamped.y).unwrap()));
            assert!(bounds.contains(Point::new(clamped.right(), clamped.bottom()).unwrap()));
        }
    }

    #[test]
    fn rejects_nan_and_singular_matrices() {
        assert_eq!(
            Transform2D::new([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            Transform2D::scale(0.0, 1.0).unwrap().inverse(),
            Err(GeometryError::SingularTransform)
        );
    }
}
