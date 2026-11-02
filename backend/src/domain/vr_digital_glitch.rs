//! Filmcraft VR Digital Glitch. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    light_leaks::hash01,
    vr_sphere::{sample_wrap, Image},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrDigitalGlitch {
    pub frame_layout: u32,
    pub amplitude: f32,
    pub distortion: f32,
    pub rate: f32,
    pub color: f32,
    pub scanlines: f32,
    pub seed: f32,
}
impl Default for VrDigitalGlitch {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            amplitude: 0.,
            distortion: 20.,
            rate: 10.,
            color: 50.,
            scanlines: 0.,
            seed: 0.,
        }
    }
}
impl VrDigitalGlitch {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.frame_layout <= 2, "invalid VR layout");
        for (v, lo, hi) in [
            (self.amplitude, 0., 100.),
            (self.distortion, 1., 100.),
            (self.rate, 0., 60.),
            (self.color, 0., 100.),
            (self.scanlines, 0., 100.),
            (self.seed, 0., 10000.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "invalid VR glitch parameter"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        local: u64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.apply_linear_at_seconds(frame, local as f64 / 1000000., checkpoint)
    }
    pub fn apply_linear_at_seconds(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(seconds.is_finite(), "invalid VR glitch local time");
        checkpoint()?;
        let amp = self.amplitude / 100.;
        if amp <= 0. {
            return Ok(());
        }
        let color = self.color / 100.;
        let scan = self.scanlines / 100.;
        let seed = self.seed.max(0.) as u64;
        let tick = (seconds * self.rate as f64).floor() as i64;
        let (w, h) = frame.dimensions();
        let input = frame.pixels();
        let mut output = input.to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            checkpoint()?;
            let mut px = Vec::with_capacity(vw * vh);
            for y in 0..vh {
                px.extend_from_slice(&input[(y0 + y) * w + x0..(y0 + y) * w + x0 + vw]);
            }
            let src = Image { w: vw, h: vh, px };
            let band = (vh as f32 / self.distortion.max(1.)).max(1.);
            for y in 0..vh {
                checkpoint()?;
                let b = (y as f32 / band) as i64;
                let active = hash01(b, tick, 0, seed) < amp;
                let shift = if active {
                    (hash01(b, tick, 1, seed) - 0.5) * vw as f32 * 0.3 * amp
                } else {
                    0.
                };
                let cs = color * amp * 12.;
                for x in 0..vw {
                    let (fx, fy) = (x as f64 + 0.5 + shift as f64, y as f64 + 0.5);
                    let mut o = sample_wrap(&src, fx, fy);
                    if active && cs > 0. {
                        o[0] = sample_wrap(&src, fx + cs as f64, fy)[0];
                        o[2] = sample_wrap(&src, fx - cs as f64, fy)[2];
                    }
                    if scan > 0. && y % 2 == 1 {
                        for c in &mut o[..3] {
                            *c *= 1. - scan * 0.6;
                        }
                    }
                    output[(y0 + y) * w + x0 + x] = o;
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
        tick: u64,
        parameters: VrDigitalGlitch,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_digital_glitch_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-digital-glitch.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 432);
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
                .apply_linear(&mut frame, c.tick, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "VR glitch case{i}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrDigitalGlitchChannel {
    Amplitude,
    Distortion,
    Rate,
    Color,
    Scanlines,
    Seed,
}
pub type VrDigitalGlitchAnimation =
    std::collections::BTreeMap<VrDigitalGlitchChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrDigitalGlitchAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrDigitalGlitchChannel::Amplitude => (0., 100.),
            VrDigitalGlitchChannel::Distortion => (1., 100.),
            VrDigitalGlitchChannel::Rate => (0., 60.),
            VrDigitalGlitchChannel::Color => (0., 100.),
            VrDigitalGlitchChannel::Scanlines => (0., 100.),
            VrDigitalGlitchChannel::Seed => (0., 10000.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR glitch animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut p: VrDigitalGlitch,
    a: &VrDigitalGlitchAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrDigitalGlitch> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrDigitalGlitchChannel::Amplitude => p.amplitude = v,
            VrDigitalGlitchChannel::Distortion => p.distortion = v,
            VrDigitalGlitchChannel::Rate => p.rate = v,
            VrDigitalGlitchChannel::Color => p.color = v,
            VrDigitalGlitchChannel::Scanlines => p.scanlines = v,
            VrDigitalGlitchChannel::Seed => p.seed = v,
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
            ("amplitude", 37.5),
            ("distortion", 12.25),
            ("rate", 7.5),
            ("color", 45.),
            ("scanlines", 12.5),
            ("seed", 17.75),
        ] {
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-1.},{"tick":750,"value":value+1.}]}}));
        }
        let a: VrDigitalGlitchAnimation =
            serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        let p = sample(
            VrDigitalGlitch {
                frame_layout: 2,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        assert_eq!(
            [
                p.amplitude,
                p.distortion,
                p.rate,
                p.color,
                p.scanlines,
                p.seed
            ],
            [37.5, 12.25, 7.5, 45., 12.5, 17.75]
        );
        assert_eq!(p.frame_layout, 2);
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_digital_glitch","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrDigitalGlitch { rate: 61., ..p }.validate().is_err());
        assert!(VrDigitalGlitch {
            distortion: 0.,
            ..p
        }
        .validate()
        .is_err());
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
        let mut count = 0;
        assert!(VrDigitalGlitch {
            amplitude: 100.,
            ..Default::default()
        }
        .apply_linear(&mut frame, 500000, &mut || {
            count += 1;
            anyhow::ensure!(count < 4, "cancelled");
            Ok(())
        })
        .is_err());
        assert_eq!(frame.pixels(), before);
    }
}
