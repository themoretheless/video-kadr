//! Filmcraft VR Glow. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    basic_grade::{display, linear},
    light_leaks::smoothstep,
    vr_filter::blur,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrGlow {
    pub frame_layout: u32,
    pub threshold: f32,
    pub radius: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub use_tint: bool,
    pub tint: [f32; 4],
}
impl Default for VrGlow {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            threshold: 70.,
            radius: 30.,
            brightness: 1.,
            saturation: 1.,
            use_tint: false,
            tint: [1.; 4],
        }
    }
}
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
impl VrGlow {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.frame_layout <= 2, "invalid VR layout");
        for (v, lo, hi) in [
            (self.threshold, 0., 100.),
            (self.radius, 0., 1000.),
            (self.brightness, 0., 10.),
            (self.saturation, 0., 4.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "invalid VR glow parameter"
            );
        }
        ensure!(
            self.tint
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "invalid VR tint"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(px_scale.is_finite() && px_scale > 0., "invalid pixel scale");
        checkpoint()?;
        let th = self.threshold / 100.;
        let radius = self.radius * px_scale * 0.5;
        let tint = self.use_tint.then(|| {
            [
                linear(self.tint[0]),
                linear(self.tint[1]),
                linear(self.tint[2]),
            ]
        });
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
            let mut hi = Vec::with_capacity(vw * vh);
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let mut p = input[(y0 + y) * w + x0 + x];
                    let a = p[3];
                    if a <= 1e-6 {
                        p.fill(0.);
                    } else {
                        let c = [display(p[0] / a), display(p[1] / a), display(p[2] / a)];
                        let k = smoothstep(th, th + 0.1, luma(c));
                        for v in &mut p {
                            *v *= k;
                        }
                    }
                    hi.push(p);
                }
            }
            blur(&mut hi, vw, vh, radius, checkpoint)?;
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let q = hi[y * vw + x];
                    let l = luma([q[0], q[1], q[2]]);
                    let mut g = [0, 1, 2].map(|k| (l + (q[k] - l) * self.saturation).max(0.));
                    if let Some(t) = tint {
                        g = t.map(|c| c * l);
                    }
                    let add = g.map(|c| c * self.brightness);
                    let p = &mut output[(y0 + y) * w + x0 + x];
                    let m = add[0].max(add[1]).max(add[2]).clamp(0., 1.);
                    for k in 0..3 {
                        p[k] += add[k];
                    }
                    p[3] = p[3] + (1. - p[3]) * m;
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
    #[serde(rename_all = "camelCase")]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: VrGlow,
        px_scale: f32,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_glow_working_bits() {
        let f: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-vr-glow.json"))
                .unwrap();
        assert_eq!(f.cases.len(), 648);
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
                .apply_linear(&mut frame, c.px_scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "VR glow case{i}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrGlowChannel {
    Threshold,
    Radius,
    Brightness,
    Saturation,
    TintR,
    TintG,
    TintB,
    TintA,
}
pub type VrGlowAnimation =
    std::collections::BTreeMap<VrGlowChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrGlowAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrGlowChannel::Threshold => (0., 100.),
            VrGlowChannel::Radius => (0., 1000.),
            VrGlowChannel::Brightness => (0., 10.),
            VrGlowChannel::Saturation => (0., 4.),
            VrGlowChannel::TintR => (0., 1.),
            VrGlowChannel::TintG => (0., 1.),
            VrGlowChannel::TintB => (0., 1.),
            VrGlowChannel::TintA => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR glow animation out of range"
        );
    }
    Ok(())
}
pub fn sample(mut p: VrGlow, a: &VrGlowAnimation, local: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<VrGlow> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrGlowChannel::Threshold => p.threshold = v,
            VrGlowChannel::Radius => p.radius = v,
            VrGlowChannel::Brightness => p.brightness = v,
            VrGlowChannel::Saturation => p.saturation = v,
            VrGlowChannel::TintR => p.tint[0] = v,
            VrGlowChannel::TintG => p.tint[1] = v,
            VrGlowChannel::TintB => p.tint[2] = v,
            VrGlowChannel::TintA => p.tint[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn eight_channels_roundtrip_and_bounds() {
        let mut a = serde_json::Map::new();
        for (name, value, delta) in [
            ("threshold", 37.5, 1.),
            ("radius", 12.25, 1.),
            ("brightness", 1.75, 0.1),
            ("saturation", 0.45, 0.1),
            ("tintR", 0.2, 0.05),
            ("tintG", 0.7, 0.05),
            ("tintB", 0.4, 0.05),
            ("tintA", 0.25, 0.05),
        ] {
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-delta},{"tick":750,"value":value+delta}]}}));
        }
        let a: VrGlowAnimation = serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        let p = sample(
            VrGlow {
                frame_layout: 2,
                use_tint: true,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        assert_eq!(
            [p.threshold, p.radius, p.brightness, p.saturation],
            [37.5, 12.25, 1.75, 0.45]
        );
        assert_eq!(p.tint, [0.2, 0.7, 0.4, 0.25]);
        assert_eq!((p.frame_layout, p.use_tint), (2, true));
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_glow","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrGlow {
            brightness: 11.,
            ..p
        }
        .validate()
        .is_err());
        assert!(VrGlow {
            tint: [f32::NAN; 4],
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
        let mut n = 0;
        assert!(VrGlow::default()
            .apply_linear(&mut frame, 1., &mut || {
                n += 1;
                anyhow::ensure!(n < 4, "cancelled");
                Ok(())
            })
            .is_err());
        assert_eq!(frame.pixels(), before);
    }
}
