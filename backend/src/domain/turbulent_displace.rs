//! Active Filmcraft Turbulent Displace CPU warp; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{linear_color_frame::LinearColorFrame, roughen_edges::fbm};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TurbulentDisplace {
    pub displacement: u32,
    pub amount: f32,
    pub size: f32,
    pub offset: Option<[Option<f64>; 2]>,
    pub complexity: f32,
    pub evolution: f32,
    pub cycle: bool,
    pub cycle_revs: f32,
    pub seed: f32,
    pub pinning: u32,
    pub antialias: u32,
}
impl Default for TurbulentDisplace {
    fn default() -> Self {
        Self {
            displacement: 0,
            amount: 50.,
            size: 100.,
            offset: None,
            complexity: 1.,
            evolution: 0.,
            cycle: false,
            cycle_revs: 1.,
            seed: 0.,
            pinning: 1,
            antialias: 0,
        }
    }
}
impl TurbulentDisplace {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.displacement <= 8 && self.pinning <= 3 && self.antialias <= 1,
            "Turbulent Displace choice out of range"
        );
        for (v, lo, hi) in [
            (self.amount, -1000., 1000.),
            (self.size, 2., 1000.),
            (self.complexity, 1., 10.),
            (self.cycle_revs, 1., 100.),
            (self.seed, 0., 10000.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "Turbulent Displace parameter out of range"
            );
        }
        ensure!(self.evolution.is_finite(), "nonfinite evolution");
        if let Some(p) = self.offset {
            ensure!(
                p.into_iter().flatten().all(f64::is_finite),
                "nonfinite offset"
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
            "invalid Turbulent Displace pixel scale"
        );
        checkpoint()?;
        let kind = self.displacement;
        let amount = self.amount * px_scale;
        if amount.abs() < 1e-4 {
            return Ok(());
        }
        let size = (self.size * px_scale).max(0.5);
        let (wi, hi) = frame.dimensions();
        let off = self.offset.unwrap_or([None, None]);
        let off = film_point(off, wi, hi, px_scale);
        let (ox, oy) = (
            ((off.0 - wi as f64 / 2.0) / size as f64) as f32,
            ((off.1 - hi as f64 / 2.0) / size as f64) as f32,
        );
        let mut cplx = self.complexity.clamp(1.0, 10.0);
        if (3..=5).contains(&kind) {
            cplx = (cplx * 0.5).max(1.0);
        }
        let seed = self.seed.max(0.0) as u64;
        let evo = self.evolution / 360.0;
        let (ex, ey, ez) = if self.cycle {
            let revs = self.cycle_revs.max(1.0);
            let th = (evo.rem_euclid(revs) / revs) * std::f32::consts::TAU;
            (th.cos() * 0.8, th.sin() * 0.8, 0.0)
        } else {
            (0.0, 0.0, evo)
        };
        let pin = self.pinning;
        let (w, h) = (wi as f32, hi as f32);
        let n =
            |x: f32, y: f32, ch: f32| fbm(x + ex + ch * 31.7, y + ey, ez + ch * 3.1, cplx, seed);
        let src = frame.pixels().to_vec();
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let (x, y) = ((index % wi) as f64 + 0.5, (index / wi) as f64 + 0.5);
                index += 1;
                let (u, v) = (x as f32 / size + ox, y as f32 / size + oy);
                let (dx, dy) = match kind {
                    0 | 3 => (n(u, v, 0.0), n(u, v, 1.0)),
                    1 | 4 | 2 | 5 => {
                        let eps = 0.05;
                        let gx = (n(u + eps, v, 0.0) - n(u - eps, v, 0.0)) / (2.0 * eps);
                        let gy = (n(u, v + eps, 0.0) - n(u, v - eps, 0.0)) / (2.0 * eps);
                        if kind == 1 || kind == 4 {
                            (gx * 0.3, gy * 0.3)
                        } else {
                            (-gy * 0.3, gx * 0.3)
                        }
                    }
                    6 => (0.0, n(u, v, 0.0)),
                    7 => (n(u, v, 0.0), 0.0),
                    _ => {
                        let q = n(u, v, 0.0);
                        (q, q)
                    }
                };
                let (px, py) = (x as f32, y as f32);
                let edge = |d: f32, len: f32| smoothstep(1.0, (len * 0.1).max(2.0), d);
                let (kx, ky) = match pin {
                    1 => {
                        let k = edge(px.min(w - px), w).min(edge(py.min(h - py), h));
                        (k, k)
                    }
                    2 => {
                        let k = edge(px.min(w - px), w);
                        (k, k)
                    }
                    3 => {
                        let k = edge(py.min(h - py), h);
                        (k, k)
                    }
                    _ => (1.0, 1.0),
                };
                let (u, v) = (x - (dx * amount * kx) as f64, y - (dy * amount * ky) as f64);
                super::twirl::sample(&src, wi, hi, u as f32, v as f32)
            },
            checkpoint,
        )
    }
}
fn film_point(p: [Option<f64>; 2], w: usize, h: usize, k: f32) -> (f64, f64) {
    (
        p[0].map(|v| v * k as f64).unwrap_or(w as f64 / 2.),
        p[1].map(|v| v * k as f64).unwrap_or(h as f64 / 2.),
    )
}
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0).max(1e-6)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn turbulent_displace_identity_modes_cycle_and_cancellation() {
        let frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: 18,
                height: 10,
                rgba: (0..180)
                    .flat_map(|i| [(i % 256) as u8, 128, 192, 128])
                    .collect(),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut identity = frame.clone();
        TurbulentDisplace {
            amount: 0.,
            ..Default::default()
        }
        .apply_linear(&mut identity, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(identity.pixels(), frame.pixels());
        for displacement in 0..9 {
            for pinning in 0..4 {
                let mut output = frame.clone();
                TurbulentDisplace {
                    displacement,
                    pinning,
                    amount: 2.5,
                    size: 3.25,
                    complexity: 3.75,
                    ..Default::default()
                }
                .apply_linear(&mut output, 0.5, &mut || Ok(()))
                .unwrap();
                assert!(output.pixels().iter().flatten().all(|v| v.is_finite()));
            }
        }
        let mut a = frame.clone();
        let mut b = frame.clone();
        let p = TurbulentDisplace {
            cycle: true,
            cycle_revs: 2.,
            evolution: 90.,
            amount: 2.5,
            ..Default::default()
        };
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        TurbulentDisplace {
            evolution: 810.,
            ..p
        }
        .apply_linear(&mut b, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), b.pixels());
        assert!(p
            .apply_linear(&mut b, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurbulentDisplaceChannel {
    Amount,
    Size,
    OffsetX,
    OffsetY,
    Complexity,
    Evolution,
    CycleRevs,
    Seed,
}
pub type TurbulentDisplaceAnimation =
    std::collections::BTreeMap<TurbulentDisplaceChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &TurbulentDisplaceAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            TurbulentDisplaceChannel::Amount => (-1000., 1000.),
            TurbulentDisplaceChannel::Size => (2., 1000.),
            TurbulentDisplaceChannel::Complexity => (1., 10.),
            TurbulentDisplaceChannel::CycleRevs => (1., 100.),
            TurbulentDisplaceChannel::Seed => (0., 10000.),
            TurbulentDisplaceChannel::Evolution => (-(f32::MAX as f64), f32::MAX as f64),
            _ => (-f64::MAX, f64::MAX),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Turbulent Displace animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: TurbulentDisplace,
    a: &TurbulentDisplaceAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<TurbulentDisplace> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)?;
        match c {
            TurbulentDisplaceChannel::Amount => p.amount = v as f32,
            TurbulentDisplaceChannel::Size => p.size = v as f32,
            TurbulentDisplaceChannel::Complexity => p.complexity = v as f32,
            TurbulentDisplaceChannel::Evolution => p.evolution = v as f32,
            TurbulentDisplaceChannel::CycleRevs => p.cycle_revs = v as f32,
            TurbulentDisplaceChannel::Seed => p.seed = v as f32,
            TurbulentDisplaceChannel::OffsetX | TurbulentDisplaceChannel::OffsetY => {
                p.offset.get_or_insert([None, None])[if *c == TurbulentDisplaceChannel::OffsetX {
                    0
                } else {
                    1
                }] = Some(v)
            }
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn turbulent_displace_eight_curves_precise_auto_axis_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let precise = 3.123456789012;
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"turbulent_displace","parameters":{},"animation":{"amount":curve(-2.,4.),"size":curve(2.,4.),"offsetX":curve(precise,precise),"offsetY":curve(0.,2.),"complexity":curve(1.,6.5),"evolution":curve(-360.,540.),"cycleRevs":curve(1.,3.),"seed":curve(0.,42.75)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::TurbulentDisplace {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Turbulent Displace")
        };
        assert_eq!(parameters, TurbulentDisplace::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            TurbulentDisplace {
                amount: 1.,
                size: 3.,
                offset: Some([Some(precise), Some(1.)]),
                complexity: 3.75,
                evolution: 90.,
                cycle_revs: 2.,
                seed: 21.375,
                ..Default::default()
            }
        );
        let one = TurbulentDisplaceAnimation::from([(
            TurbulentDisplaceChannel::OffsetX,
            a[&TurbulentDisplaceChannel::OffsetX].clone(),
        )]);
        assert_eq!(
            sample_animation(parameters, &one, 250000).unwrap().offset,
            Some([Some(precise), None])
        );
        let invalid = TurbulentDisplaceAnimation::from([(
            TurbulentDisplaceChannel::Size,
            super::super::composition::AnimatableValue::Constant { value: 1. },
        )]);
        assert!(validate_animation(&invalid).is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: TurbulentDisplace,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-turbulent-displace.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 3456);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            c.parameters
                .apply_linear(&mut frame, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Turbulent Displace original working bits case{index}"
            );
        }
    }
}

#[cfg(test)]
mod antialias_tests {
    use super::*;
    #[test]
    fn turbulent_displace_antialias_persisted_choice_cpu_identity() {
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"turbulent_displace","parameters":{"antialias":1}}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::TurbulentDisplace { parameters, .. } = e else {
            panic!("Turbulent Displace")
        };
        assert_eq!(parameters.antialias, 1);
        let frame = LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: (0..24)
                    .flat_map(|i| [(i * 10) as u8, 128, 192, 128])
                    .collect(),
            },
            &mut || Ok(()),
        )
        .unwrap();
        let mut a = frame.clone();
        let mut b = frame.clone();
        parameters.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        TurbulentDisplace {
            antialias: 0,
            ..parameters
        }
        .apply_linear(&mut b, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), b.pixels());
        assert!(TurbulentDisplace {
            antialias: 2,
            ..parameters
        }
        .validate()
        .is_err());
    }
}
