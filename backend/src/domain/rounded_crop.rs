//! Filmcraft Rounded Crop; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RoundedCrop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub radius: f32,
    pub feather: f32,
    pub border: f32,
    pub border_color: [f32; 4],
}
impl Default for RoundedCrop {
    fn default() -> Self {
        Self {
            left: 0.,
            top: 0.,
            right: 0.,
            bottom: 0.,
            radius: 40.,
            feather: 0.,
            border: 0.,
            border_color: [1.; 4],
        }
    }
}
impl RoundedCrop {
    pub fn validate(&self) -> Result<()> {
        for (v, hi) in [
            (self.left, 100.),
            (self.top, 100.),
            (self.right, 100.),
            (self.bottom, 100.),
            (self.radius, 2000.),
            (self.feather, 500.),
            (self.border, 500.),
        ] {
            ensure!(
                v.is_finite() && (0. ..=hi).contains(&v),
                "RoundedCrop parameter out of range"
            );
        }
        ensure!(
            self.border_color.iter().all(|v| v.is_finite()),
            "nonfinite RoundedCrop color"
        );
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
            "invalid RoundedCrop pixel scale"
        );
        checkpoint()?;
        let (wi, hi) = frame.dimensions();
        let (w, h) = (wi as f32, hi as f32);
        let (l, t, r, b) = (
            self.left / 100. * w,
            self.top / 100. * h,
            w - self.right / 100. * w,
            h - self.bottom / 100. * h,
        );
        let rad = (self.radius * px_scale).min(((r - l).min(b - t) / 2.).max(0.));
        let fe = self.feather * px_scale;
        let border = self.border * px_scale;
        let bc: [f32; 3] =
            std::array::from_fn(|i| super::basic_grade::linear(self.border_color[i].clamp(0., 1.)));
        let mut index = 0;
        frame.map_premultiplied(
            &mut |mut p| {
                let (x, y) = ((index % wi) as f32 + 0.5, (index / wi) as f32 + 0.5);
                index += 1;
                let d = super::spacer::round_rect_sdf(x, y, l, t, r, b, rad);
                if border > 0. {
                    let k = (0.5 - d).clamp(0., 1.) - (0.5 - d - border).clamp(0., 1.);
                    if k > 0. {
                        let src = [bc[0] * k, bc[1] * k, bc[2] * k, k];
                        let back = 1. - src[3];
                        for c in 0..4 {
                            p[c] = src[c] + p[c] * back;
                        }
                    }
                }
                let a = if fe > 0. {
                    (-d / fe).clamp(0., 1.)
                } else {
                    (0.5 - d).clamp(0., 1.)
                };
                if a < 1. {
                    for v in &mut p {
                        *v *= a;
                    }
                }
                p
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoundedCropChannel {
    Left,
    Top,
    Right,
    Bottom,
    Radius,
    Feather,
    Border,
    BorderR,
    BorderG,
    BorderB,
    BorderA,
}
pub type RoundedCropAnimation =
    std::collections::BTreeMap<RoundedCropChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &RoundedCropAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            RoundedCropChannel::Left
            | RoundedCropChannel::Top
            | RoundedCropChannel::Right
            | RoundedCropChannel::Bottom => (0., 100.),
            RoundedCropChannel::Radius => (0., 2000.),
            RoundedCropChannel::Feather | RoundedCropChannel::Border => (0., 500.),
            _ => (-(f32::MAX as f64), f32::MAX as f64),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "RoundedCrop curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: RoundedCrop,
    a: &RoundedCropAnimation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<RoundedCrop> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            RoundedCropChannel::Left => p.left = v,
            RoundedCropChannel::Top => p.top = v,
            RoundedCropChannel::Right => p.right = v,
            RoundedCropChannel::Bottom => p.bottom = v,
            RoundedCropChannel::Radius => p.radius = v,
            RoundedCropChannel::Feather => p.feather = v,
            RoundedCropChannel::Border => p.border = v,
            RoundedCropChannel::BorderR => p.border_color[0] = v,
            RoundedCropChannel::BorderG => p.border_color[1] = v,
            RoundedCropChannel::BorderB => p.border_color[2] = v,
            RoundedCropChannel::BorderA => p.border_color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_rounding_feather_border_alpha_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut a = frame.clone();
        RoundedCrop {
            radius: 0.,
            ..Default::default()
        }
        .apply_linear(&mut a, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), frame.pixels());
        let p = RoundedCrop {
            left: 10.,
            radius: 1.,
            feather: 0.75,
            border: 1.,
            border_color: [0.25, 0.5, 1., 0.],
            ..Default::default()
        };
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        assert_ne!(a.pixels(), frame.pixels());
        let mut b = frame.clone();
        RoundedCrop {
            border_color: [0.25, 0.5, 1., 1.],
            ..p
        }
        .apply_linear(&mut b, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(a.pixels(), b.pixels());
        assert!(p
            .apply_linear(&mut b, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
    #[test]
    fn persisted_eleven_curves_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"rounded_crop","parameters":{},"animation":{"left":curve(0.,20.),"top":curve(0.,40.),"right":curve(0.,60.),"bottom":curve(0.,80.),"radius":curve(0.,2.),"feather":curve(0.,2.),"border":curve(0.,2.),"borderR":curve(0.,1.),"borderG":curve(0.,1.),"borderB":curve(0.,1.),"borderA":curve(0.,1.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::RoundedCrop {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("RoundedCrop")
        };
        assert_eq!(parameters, RoundedCrop::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            RoundedCrop {
                left: 10.,
                top: 20.,
                right: 30.,
                bottom: 40.,
                radius: 1.,
                feather: 1.,
                border: 1.,
                border_color: [0.5; 4]
            }
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: RoundedCrop,
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
            "../../tests/fixtures/filmcraft-rounded-crop.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 1944);
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
                "RoundedCrop original working bits case{index}"
            );
        }
    }
}
