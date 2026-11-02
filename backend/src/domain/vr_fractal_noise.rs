//! Filmcraft VR Fractal Noise. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    basic_grade::linear,
    light_leaks::{blend_simple, fbm, lerp3},
    vr_sphere::{dir, px_to_lonlat},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrFractalNoise {
    pub frame_layout: u32,
    pub fractal_type: u32,
    pub invert: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub scale: f32,
    pub complexity: f32,
    pub evolution: f32,
    pub opacity: f32,
    pub mode: u32,
}
impl Default for VrFractalNoise {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            fractal_type: 0,
            invert: false,
            contrast: 100.,
            brightness: 0.,
            scale: 100.,
            complexity: 6.,
            evolution: 0.,
            opacity: 100.,
            mode: 0,
        }
    }
}
impl VrFractalNoise {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_layout <= 2 && self.fractal_type <= 2 && self.mode <= 4,
            "invalid VR noise choice"
        );
        ensure!(self.evolution.is_finite(), "invalid VR evolution");
        for (v, lo, hi) in [
            (self.contrast, 0., 1000.),
            (self.brightness, -200., 200.),
            (self.scale, 10., 10000.),
            (self.complexity, 1., 10.),
            (self.opacity, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "VR noise parameter out of range"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let contrast = self.contrast / 100.;
        let bright = self.brightness / 100.;
        let scale = (self.scale / 100.).max(0.01);
        let evo = self.evolution / 360.;
        let op = self.opacity / 100.;
        let f = 4. / scale;
        let (w, h) = frame.dimensions();
        let mut output = frame.pixels().to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            checkpoint()?;
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let (lon, lat) = px_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, vw, vh);
                    let d = dir(lon, lat);
                    let mut n = fbm(
                        d[0] as f32 * f + evo * 0.31,
                        d[1] as f32 * f + evo * 0.17,
                        d[2] as f32 * f + evo * 0.11,
                        self.complexity,
                        3,
                    );
                    n = match self.fractal_type {
                        1 => n.abs().sqrt(),
                        2 => n.abs(),
                        _ => n * 0.5 + 0.5,
                    };
                    let mut g = ((n - 0.5) * contrast + 0.5 + bright).clamp(0., 1.);
                    if self.invert {
                        g = 1. - g;
                    }
                    let gl = linear(g);
                    let p = &mut output[(y0 + y) * w + x0 + x];
                    let base = if p[3] <= 1e-6 {
                        [0.; 3]
                    } else {
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
                    };
                    let o = lerp3(base, blend_simple(self.mode, base, [gl; 3]), op);
                    let a = p[3] + (1. - p[3]) * op;
                    *p = [o[0] * a, o[1] * a, o[2] * a, a];
                }
            }
        }
        let mut i = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = output[i];
                i += 1;
                p
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: VrFractalNoise,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_fractal_noise_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-fractal-noise.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 3240);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
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
                "VR fractal case{i}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrFractalNoiseChannel {
    Contrast,
    Brightness,
    Scale,
    Complexity,
    Evolution,
    Opacity,
}
pub type VrFractalNoiseAnimation =
    std::collections::BTreeMap<VrFractalNoiseChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrFractalNoiseAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrFractalNoiseChannel::Contrast => (0., 1000.),
            VrFractalNoiseChannel::Brightness => (-200., 200.),
            VrFractalNoiseChannel::Scale => (10., 10000.),
            VrFractalNoiseChannel::Complexity => (1., 10.),
            VrFractalNoiseChannel::Opacity => (0., 100.),
            VrFractalNoiseChannel::Evolution => (-(f32::MAX as f64), f32::MAX as f64),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR fractal animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut p: VrFractalNoise,
    a: &VrFractalNoiseAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrFractalNoise> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrFractalNoiseChannel::Contrast => p.contrast = v,
            VrFractalNoiseChannel::Brightness => p.brightness = v,
            VrFractalNoiseChannel::Scale => p.scale = v,
            VrFractalNoiseChannel::Complexity => p.complexity = v,
            VrFractalNoiseChannel::Evolution => p.evolution = v,
            VrFractalNoiseChannel::Opacity => p.opacity = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn six_channels_roundtrip_and_bounds() {
        let mut a = serde_json::Map::new();
        for (name, value) in [
            ("contrast", 137.5),
            ("brightness", -12.25),
            ("scale", 37.5),
            ("complexity", 4.25),
            ("evolution", 123.75),
            ("opacity", 45.),
        ] {
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-1.},{"tick":750,"value":value+1.}]}}));
        }
        let a: VrFractalNoiseAnimation =
            serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        let p = sample(
            VrFractalNoise {
                frame_layout: 2,
                fractal_type: 2,
                mode: 4,
                invert: true,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        assert_eq!(
            [
                p.contrast,
                p.brightness,
                p.scale,
                p.complexity,
                p.evolution,
                p.opacity
            ],
            [137.5, -12.25, 37.5, 4.25, 123.75, 45.]
        );
        assert_eq!(
            (p.frame_layout, p.fractal_type, p.mode, p.invert),
            (2, 2, 4, true)
        );
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_fractal_noise","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrFractalNoise { scale: 9., ..p }.validate().is_err());
        assert!(VrFractalNoise {
            evolution: f32::INFINITY,
            ..p
        }
        .validate()
        .is_err());
        assert!(VrFractalNoise {
            fractal_type: 3,
            ..p
        }
        .validate()
        .is_err());
        let invalid: VrFractalNoiseAnimation = serde_json::from_value(
            serde_json::json!({"evolution":{"mode":"constant","value":1e100}}),
        )
        .unwrap();
        assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn cancellation_preserves_input() {
        let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: vec![128; 96],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let before = frame.pixels().to_vec();
        let mut n = 0;
        assert!(VrFractalNoise::default()
            .apply_linear(&mut frame, &mut || {
                n += 1;
                anyhow::ensure!(n < 4, "cancelled");
                Ok(())
            })
            .is_err());
        assert_eq!(frame.pixels(), before);
    }
}
