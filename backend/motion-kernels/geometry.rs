//! Filmcraft Warp Stabilizer path correction math. MIT attribution: docs/licenses/filmcraft-MIT.txt.
//! Tracking, project dispatch and framing integration remain separate.
use std::ops::{Add, Mul, Sub};
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
        Vec2::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
}
impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

/// Affine transform `[a c e; b d f; 0 0 1]` mapping (x, y) → (a x + c y + e, b x + d y + f).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    pub fn translate(x: f64, y: f64) -> Self {
        Affine {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Affine {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }
    /// Rotation by `deg` degrees, clockwise on screen (y down), matching Premiere's Rotation.
    pub fn rotate_deg(deg: f64) -> Self {
        let (s, c) = deg.to_radians().sin_cos();
        Affine {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }
    /// `self * o`: apply `o` first, then `self`.
    pub fn motion(position: Vec2, scale: Vec2, rotation_deg: f64, anchor: Vec2) -> Affine {
        Self::translate(position.x, position.y)
            .then_apply(&Self::rotate_deg(rotation_deg))
            .then_apply(&Self::scale(scale.x, scale.y))
            .then_apply(&Self::translate(-anchor.x, -anchor.y))
    }
    pub fn then_apply(&self, o: &Affine) -> Affine {
        // result = self ∘ o
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }
    pub fn apply(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }
    pub fn inverse(&self) -> Option<Affine> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        let a = self.d * inv;
        let b = -self.b * inv;
        let c = -self.c * inv;
        let d = self.a * inv;
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }
}
