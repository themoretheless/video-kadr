//! Filmcraft Echo Glow; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct EchoGlow {
    pub threshold: f32,
    pub echoes: f32,
    pub radius: f32,
    pub spread: f32,
    pub intensity: f32,
    pub decay: f32,
    pub color: [f32; 4],
}
impl Default for EchoGlow {
    fn default() -> Self {
        Self {
            threshold: 60.,
            echoes: 4.,
            radius: 12.,
            spread: 1.8,
            intensity: 100.,
            decay: 50.,
            color: [1., 0.85, 0.6, 1.],
        }
    }
}
impl EchoGlow {
    pub fn validate(&self) -> Result<()> {
        for (v, lo, hi) in [
            (self.threshold, 0., 100.),
            (self.echoes, 1., 12.),
            (self.radius, 0., 500.),
            (self.spread, 1., 4.),
            (self.intensity, 0., 1000.),
            (self.decay, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "Echo Glow parameter out of range"
            );
        }
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "Echo Glow color out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(scale.is_finite() && scale > 0., "invalid Echo Glow scale");
        checkpoint()?;
        let mut hi = frame.clone();
        let threshold = self.threshold / 100.;
        hi.map_premultiplied(
            &mut |p| {
                let a = p[3];
                if a <= 1e-6 {
                    return [0.; 4];
                }
                let rgb = std::array::from_fn::<f32, 3, _>(|k| {
                    super::basic_grade::display((p[k] / a).max(0.))
                });
                let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let t = ((l - threshold) / ((threshold + 0.1) - threshold).max(1e-6)).clamp(0., 1.);
                let k = t * t * (3. - 2. * t);
                p.map(|v| v * k)
            },
            checkpoint,
        )?;
        let r = self.radius * scale * 0.5;
        ensure!(r.is_finite(), "Echo Glow radius overflow");
        let decay = 1. - self.decay / 100.;
        let mut acc = vec![[0.; 4]; frame.pixels().len()];
        let mut sum = 0.;
        for i in 0..self.echoes.clamp(1., 12.) as usize {
            checkpoint()?;
            let mut layer = hi.clone();
            super::gaussian_blur::GaussianBlur::default().apply_sigma(
                &mut layer,
                r * self.spread.max(1.).powi(i as i32),
                checkpoint,
            )?;
            let weight = decay.powi(i as i32);
            sum += weight;
            for (a, b) in acc.iter_mut().zip(layer.pixels()) {
                for k in 0..4 {
                    a[k] += b[k] * weight;
                }
            }
        }
        let norm = (self.intensity / 100.) / sum.max(1e-6) * 2.;
        let color = std::array::from_fn::<f32, 3, _>(|k| super::basic_grade::linear(self.color[k]));
        for p in &mut acc {
            let l = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
            for k in 0..3 {
                p[k] += (l * color[k] * 1.5 - p[k]) * 1.;
            }
        }
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let light = acc[index];
                index += 1;
                let add = [light[0] * norm, light[1] * norm, light[2] * norm];
                let m = add[0].max(add[1]).max(add[2]).clamp(0., 1.);
                for k in 0..3 {
                    p[k] += add[k];
                }
                p[3] = p[3] + (1. - p[3]) * m;
                p
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn echo_glow_original_working_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: EchoGlow,
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
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-echo-glow.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 64);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
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
                "Echo Glow original {i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EchoGlowChannel {
    Threshold,
    Echoes,
    Radius,
    Spread,
    Intensity,
    Decay,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type EchoGlowAnimation =
    std::collections::BTreeMap<EchoGlowChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &EchoGlowAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            EchoGlowChannel::Threshold | EchoGlowChannel::Decay => (0., 100.),
            EchoGlowChannel::Echoes => (1., 12.),
            EchoGlowChannel::Radius => (0., 500.),
            EchoGlowChannel::Spread => (1., 4.),
            EchoGlowChannel::Intensity => (0., 1000.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Echo Glow animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: EchoGlow, a: &EchoGlowAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<EchoGlow> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            EchoGlowChannel::Threshold => p.threshold = v,
            EchoGlowChannel::Echoes => p.echoes = v,
            EchoGlowChannel::Radius => p.radius = v,
            EchoGlowChannel::Spread => p.spread = v,
            EchoGlowChannel::Intensity => p.intensity = v,
            EchoGlowChannel::Decay => p.decay = v,
            EchoGlowChannel::ColorRed => p.color[0] = v,
            EchoGlowChannel::ColorGreen => p.color[1] = v,
            EchoGlowChannel::ColorBlue => p.color[2] = v,
            EchoGlowChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn echo_glow_ten_curves_fractional_echoes_midpoint_hold_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let a:EchoGlowAnimation=serde_json::from_value(serde_json::json!({"threshold":curve(60.,20.),"echoes":curve(4.,3.5),"radius":curve(12.,30.),"spread":curve(1.8,2.5),"intensity":curve(100.,350.),"decay":curve(50.,100.),"colorRed":curve(1.,0.),"colorGreen":curve(0.85,0.15),"colorBlue":curve(0.6,0.4),"colorAlpha":curve(1.,0.)})).unwrap();
        let p = EchoGlow::default();
        let mid = sample_animation(p, &a, 250000).unwrap();
        assert_eq!(
            (
                mid.threshold,
                mid.echoes,
                mid.radius,
                mid.spread,
                mid.intensity,
                mid.decay
            ),
            (40., 3.75, 21., 2.15, 225., 75.)
        );
        assert_eq!(mid.color, [0.5; 4]);
        assert_eq!(sample_animation(p, &a, 750000).unwrap().echoes, 3.5);
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"echo_glow","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        for (c, v) in [
            (EchoGlowChannel::Echoes, 0.),
            (EchoGlowChannel::Echoes, 12.1),
            (EchoGlowChannel::Spread, 0.99),
            (EchoGlowChannel::Spread, 4.01),
            (EchoGlowChannel::Decay, 101.),
            (EchoGlowChannel::ColorAlpha, -1.),
        ] {
            assert!(validate_animation(&EchoGlowAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
