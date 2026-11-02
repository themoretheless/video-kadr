//! Filmcraft standalone Vignette; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VignetteEffect {
    pub amount: f32,
    pub midpoint: f32,
    pub roundness: f32,
    pub feather: f32,
    pub color: [f32; 4],
}
impl Default for VignetteEffect {
    fn default() -> Self {
        Self {
            amount: -50.,
            midpoint: 50.,
            roundness: 0.,
            feather: 50.,
            color: [0., 0., 0., 1.],
        }
    }
}
impl VignetteEffect {
    pub fn validate(&self) -> Result<()> {
        for (v, lo, hi) in [
            (self.amount, -100., 100.),
            (self.midpoint, 0., 100.),
            (self.roundness, -100., 100.),
            (self.feather, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "Vignette parameter out of range"
            );
        }
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "Vignette color out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let amt = self.amount / 100.;
        if amt.abs() < 1e-5 {
            return Ok(());
        }
        let mid = self.midpoint / 100.;
        let round = self.roundness / 100.;
        let feather = (self.feather / 100.).max(0.01);
        let target = if amt < 0. {
            [self.color[0], self.color[1], self.color[2]]
        } else {
            [1.; 3]
        };
        let (w, h) = frame.dimensions();
        let (w, h) = (w as f32, h as f32);
        let aspect = w / h;
        frame.map_signal(
            &mut |c, x, y| {
                let mut nx = (x as f32 + 0.5) / w * 2. - 1.;
                let ny = (y as f32 + 0.5) / h * 2. - 1.;
                if round > 0. {
                    nx *= 1. + (aspect - 1.) * round;
                }
                let p = if round < 0. { 2. + (-round) * 6. } else { 2. };
                let d = (nx.abs().powf(p) + ny.abs().powf(p)).powf(1. / p) / 2f32.powf(1. / p);
                let e0 = mid - feather * 0.5;
                let e1 = mid + feather * 0.5;
                let t = ((d - e0) / (e1 - e0).max(1e-6)).clamp(0., 1.);
                let edge = t * t * (3. - 2. * t);
                let k = edge * amt.abs();
                Ok(std::array::from_fn(|i| c[i] + (target[i] - c[i]) * k))
            },
            checkpoint,
        )
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface, linear_color_frame::LinearColorFrame};
    #[test]
    fn original_vignette_effect_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: VignetteEffect,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vignette-effect.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 1080);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original VignetteEffect {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VignetteEffectChannel {
    Amount,
    Midpoint,
    Roundness,
    Feather,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type VignetteEffectAnimation =
    std::collections::BTreeMap<VignetteEffectChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VignetteEffectAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VignetteEffectChannel::Amount | VignetteEffectChannel::Roundness => (-100., 100.),
            VignetteEffectChannel::Midpoint | VignetteEffectChannel::Feather => (0., 100.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Vignette animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: VignetteEffect,
    a: &VignetteEffectAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VignetteEffect> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            VignetteEffectChannel::Amount => p.amount = v,
            VignetteEffectChannel::Midpoint => p.midpoint = v,
            VignetteEffectChannel::Roundness => p.roundness = v,
            VignetteEffectChannel::Feather => p.feather = v,
            VignetteEffectChannel::ColorRed => p.color[0] = v,
            VignetteEffectChannel::ColorGreen => p.color[1] = v,
            VignetteEffectChannel::ColorBlue => p.color[2] = v,
            VignetteEffectChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn vignette_eight_curves_defaults_bounds_and_roundtrip() {
        let controls = [
            ("amount", -50., 100.),
            ("midpoint", 50., 75.),
            ("roundness", 0., -100.),
            ("feather", 50., 0.),
            ("colorRed", 0., 1.),
            ("colorGreen", 0., 0.75),
            ("colorBlue", 0., 0.125),
            ("colorAlpha", 1., 0.),
        ];
        let animation:serde_json::Map<String,serde_json::Value>=controls.iter().map(|(n,a,b)|((*n).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}))).collect();
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vignette","parameters":{},"animation":animation}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::VignetteEffect {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Vignette")
        };
        assert_eq!(parameters, VignetteEffect::default());
        assert_eq!(a.len(), 8);
        for time in [0, 250000, 500000, 750000] {
            let p = sample_animation(parameters, &a, time).unwrap();
            let values = [
                p.amount,
                p.midpoint,
                p.roundness,
                p.feather,
                p.color[0],
                p.color[1],
                p.color[2],
                p.color[3],
            ];
            for ((name, start, end), actual) in controls.into_iter().zip(values) {
                assert_eq!(
                    actual,
                    (start + (end - start) * time.min(500000) as f64 / 500000.) as f32,
                    "{name}/{time}"
                );
            }
        }
        for (c, value) in [
            (VignetteEffectChannel::Amount, 100.01),
            (VignetteEffectChannel::Roundness, -100.01),
            (VignetteEffectChannel::Midpoint, -0.01),
            (VignetteEffectChannel::Feather, 100.01),
            (VignetteEffectChannel::ColorRed, 1.01),
            (VignetteEffectChannel::ColorAlpha, -0.01),
        ] {
            assert!(validate_animation(&VignetteEffectAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value }
            )]))
            .is_err());
        }
    }
}
