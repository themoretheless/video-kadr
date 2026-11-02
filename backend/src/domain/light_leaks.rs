//! Filmcraft Light Leaks. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LightLeaks {
    pub c1: [f32; 4],
    pub c2: [f32; 4],
    pub intensity: f32,
    pub scale: f32,
    pub direction: f32,
    pub speed: f32,
    pub seed: f32,
    pub mode: u32,
}
impl Default for LightLeaks {
    fn default() -> Self {
        Self {
            c1: [1., 0.45, 0.1, 1.],
            c2: [1., 0.15, 0.35, 1.],
            intensity: 70.,
            scale: 100.,
            direction: 30.,
            speed: 1.,
            seed: 0.,
            mode: 0,
        }
    }
}
impl LightLeaks {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.mode <= 2 && self.direction.is_finite(),
            "invalid Light Leaks mode/direction"
        );
        for (value, lo, hi) in [
            (self.intensity, 0., 100.),
            (self.scale, 10., 1000.),
            (self.speed, 0., 20.),
            (self.seed, 0., 10000.),
        ] {
            ensure!(
                value.is_finite() && (lo..=hi).contains(&value),
                "Light Leaks parameter out of range"
            );
        }
        ensure!(
            self.c1
                .into_iter()
                .chain(self.c2)
                .all(|v| v.is_finite() && (0. ..=1.).contains(&v)),
            "invalid Light Leaks color"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        image: &mut super::linear_color_frame::LinearColorFrame,
        tick: u64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_linear_at_seconds(image, tick as f64 / 1000000., checkpoint)
    }
    pub fn apply_linear_at_seconds(
        &self,
        image: &mut super::linear_color_frame::LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(seconds.is_finite(), "invalid Light Leaks local time");
        checkpoint()?;
        let (width, height) = image.dimensions();
        let c1 = [self.c1[0], self.c1[1], self.c1[2]].map(super::basic_grade::linear);
        let c2 = [self.c2[0], self.c2[1], self.c2[2]].map(super::basic_grade::linear);
        let inten = self.intensity / 100.0;
        let scale = (self.scale / 100.0).max(0.05) * width.max(height) as f32 * 0.6;
        let dir = self.direction.to_radians();
        let t = (seconds * self.speed as f64) as f32;
        let seed = self.seed.max(0.0) as u64;
        let mode = self.mode;
        let (dx, dy) = (dir.cos(), dir.sin());
        let (w, h) = (width as f32, height as f32);
        let mut index = 0;
        image.map_premultiplied(
            &mut |p| {
                let (x, y) = ((index % width) as f32 + 0.5, (index / width) as f32 + 0.5);
                index += 1;
                let (u, v) = (x / scale, y / scale);
                // stronger towards the edge the leak enters from
                let along = ((x / w - 0.5) * dx + (y / h - 0.5) * dy) * 2.0;
                let bias = smoothstep(-0.2, 1.0, -along);
                let n = fbm(u + dx * t * 0.3, v + dy * t * 0.3, t * 0.15, 3.0, seed) * 0.5 + 0.5;
                let m = fbm(u * 0.7 + 5.2, v * 0.7, t * 0.1 + 9.0, 2.0, seed) * 0.5 + 0.5;
                let k = smoothstep(0.35, 0.85, n) * bias * inten;
                let light = lerp3(c1, c2, m).map(|c| c * k);
                let base = if p[3] <= 1e-6 {
                    [0.; 3]
                } else {
                    [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
                };
                let o = match mode {
                    1 => [base[0] + light[0], base[1] + light[1], base[2] + light[2]],
                    2 => blend_simple(4, base, lerp3(base, light.map(|q| q + 0.5), k.min(1.0))),
                    _ => blend_simple(2, base, light),
                };
                let a = p[3] + (1.0 - p[3]) * k.min(1.0);
                [o[0] * a, o[1] * a, o[2] * a, a]
            },
            checkpoint,
        )
    }
}
pub(crate) fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
#[inline]
pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub(crate) fn blend_simple(mode: u32, base: [f32; 3], src: [f32; 3]) -> [f32; 3] {
    let mut o = [0.0; 3];
    for k in 0..3 {
        let (b, s) = (base[k], src[k]);
        o[k] = match mode {
            1 => b + s,
            2 => 1.0 - (1.0 - b.min(1.0)) * (1.0 - s.min(1.0)),
            3 => b * s,
            4 => {
                if b < 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b.min(1.0)) * (1.0 - s.min(1.0))
                }
            }
            _ => s,
        };
    }
    o
}

pub(crate) fn hash_u(x: i64, y: i64, z: i64, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
        ^ seed.wrapping_mul(0x27D4_EB2F_1656_67C5);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 32)
}
/// Uniform 0..1 from integer coordinates.
#[inline]
pub(crate) fn hash01(x: i64, y: i64, z: i64, seed: u64) -> f32 {
    (hash_u(x, y, z, seed) >> 40) as f32 / (1u64 << 24) as f32
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
fn grad3(h: u64, x: f32, y: f32, z: f32) -> f32 {
    // 12 edge directions of a cube (Perlin 2002)
    match h % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        _ => -y - z,
    }
}

/// 3-D gradient noise in about −1..1 (our own hash; Perlin's interpolant and gradient set).
pub(crate) fn noise3(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (xf, yf, zf) = (x - xi, y - yi, z - zi);
    let (xi, yi, zi) = (xi as i64, yi as i64, zi as i64);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |dx: i64, dy: i64, dz: i64| {
        grad3(
            hash_u(xi + dx, yi + dy, zi + dz, seed),
            xf - dx as f32,
            yf - dy as f32,
            zf - dz as f32,
        )
    };
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = l(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = l(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = l(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = l(g(0, 1, 1), g(1, 1, 1), u);
    l(l(x00, x10, v), l(x01, x11, v), w) * 0.9
}

/// Fractal (fBm) noise with `octaves` (fractional octaves fade in), about −1..1.
pub(crate) fn fbm(x: f32, y: f32, z: f32, octaves: f32, seed: u64) -> f32 {
    let n = octaves.clamp(1.0, 10.0);
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    let mut i = 0;
    while (i as f32) < n {
        let k = (n - i as f32).min(1.0);
        sum += noise3(
            x * freq,
            y * freq,
            z * freq + i as f32 * 17.13,
            seed.wrapping_add(i as u64),
        ) * amp
            * k;
        norm += amp * k;
        amp *= 0.5;
        freq *= 2.0;
        i += 1;
    }
    sum / norm.max(1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface, linear_color_frame::LinearColorFrame};
    #[test]
    fn original_light_leaks_working_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-light-leaks.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 216);
        for (index, case) in cases.iter().enumerate() {
            let parameters: LightLeaks =
                serde_json::from_value(case["parameters"].clone()).unwrap();
            let surface = GroupSurface {
                width: case["width"].as_u64().unwrap() as usize,
                height: case["height"].as_u64().unwrap() as usize,
                rgba: serde_json::from_value(case["rgba"].clone()).unwrap(),
            };
            let mut frame = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
            parameters
                .apply_linear(&mut frame, case["tick"].as_u64().unwrap(), &mut || Ok(()))
                .unwrap();
            let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected,
                "original Light Leaks {index}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LightLeaksChannel {
    Intensity,
    Scale,
    Direction,
    Speed,
    Seed,
    C1Red,
    C1Green,
    C1Blue,
    C1Alpha,
    C2Red,
    C2Green,
    C2Blue,
    C2Alpha,
}
pub type LightLeaksAnimation =
    std::collections::BTreeMap<LightLeaksChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &LightLeaksAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            LightLeaksChannel::Intensity => (0., 100.),
            LightLeaksChannel::Scale => (10., 1000.),
            LightLeaksChannel::Direction => (-(f32::MAX as f64), f32::MAX as f64),
            LightLeaksChannel::Speed => (0., 20.),
            LightLeaksChannel::Seed => (0., 10000.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Light Leaks animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: LightLeaks,
    animation: &LightLeaksAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<LightLeaks> {
    let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(animation)?;
    for (channel, value) in animation {
        let v = super::nested_composition::sample_value(value, local)? as f32;
        match channel {
            LightLeaksChannel::Intensity => p.intensity = v,
            LightLeaksChannel::Scale => p.scale = v,
            LightLeaksChannel::Direction => p.direction = v,
            LightLeaksChannel::Speed => p.speed = v,
            LightLeaksChannel::Seed => p.seed = v,
            LightLeaksChannel::C1Red => p.c1[0] = v,
            LightLeaksChannel::C1Green => p.c1[1] = v,
            LightLeaksChannel::C1Blue => p.c1[2] = v,
            LightLeaksChannel::C1Alpha => p.c1[3] = v,
            LightLeaksChannel::C2Red => p.c2[0] = v,
            LightLeaksChannel::C2Green => p.c2[1] = v,
            LightLeaksChannel::C2Blue => p.c2[2] = v,
            LightLeaksChannel::C2Alpha => p.c2[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn all_channels_sample_local_midpoints_and_roundtrip() {
        let values = [
            ("intensity", 75.),
            ("scale", 120.),
            ("direction", -45.),
            ("speed", 2.),
            ("seed", 321.25),
            ("c1Red", 0.25),
            ("c1Green", 0.5),
            ("c1Blue", 0.75),
            ("c1Alpha", 0.125),
            ("c2Red", 0.8),
            ("c2Green", 0.6),
            ("c2Blue", 0.4),
            ("c2Alpha", 0.2),
        ];
        let mut curves = serde_json::Map::new();
        for (name, value) in values {
            let d = if name.starts_with('c') { 0.05 } else { 1. };
            curves.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":value-d},{"tick":1000,"value":value+d}]}}));
        }
        let animation: LightLeaksAnimation =
            serde_json::from_value(serde_json::Value::Object(curves)).unwrap();
        let saved = serde_json::to_string(&animation).unwrap();
        let restored: LightLeaksAnimation = serde_json::from_str(&saved).unwrap();
        assert_eq!(serde_json::to_string(&restored).unwrap(), saved);
        let p = sample_animation(
            LightLeaks {
                mode: 2,
                ..Default::default()
            },
            &animation,
            500000,
        )
        .unwrap();
        assert_eq!(
            [p.intensity, p.scale, p.direction, p.speed, p.seed],
            [75., 120., -45., 2., 321.25]
        );
        assert_eq!(p.c1, [0.25, 0.5, 0.75, 0.125]);
        assert_eq!(p.c2, [0.8, 0.6, 0.4, 0.2]);
        assert_eq!(p.mode, 2);
        let bad: LightLeaksAnimation =
            serde_json::from_value(serde_json::json!({"speed":{"mode":"constant","value":21.}}))
                .unwrap();
        assert!(validate_animation(&bad).is_err());
    }
}
