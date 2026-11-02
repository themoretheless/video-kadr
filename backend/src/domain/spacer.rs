//! Filmcraft Spacer; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::{group_transform::GroupAffine, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Spacer {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub uniform: bool,
    pub radius: f32,
    pub fill: bool,
    pub color: [f32; 4],
}
impl Default for Spacer {
    fn default() -> Self {
        Self {
            left: 40.,
            top: 40.,
            right: 40.,
            bottom: 40.,
            uniform: true,
            radius: 0.,
            fill: false,
            color: [0., 0., 0., 1.],
        }
    }
}
pub(crate) fn round_rect_sdf(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let (cx, cy) = ((x0 + x1) / 2., (y0 + y1) / 2.);
    let (hx, hy) = ((x1 - x0) / 2. - r, (y1 - y0) / 2. - r);
    let (qx, qy) = ((px - cx).abs() - hx, (py - cy).abs() - hy);
    qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - r
}
impl Spacer {
    pub fn validate(&self) -> Result<()> {
        for (v, hi) in [
            (self.left, 4000.),
            (self.top, 4000.),
            (self.right, 4000.),
            (self.bottom, 4000.),
            (self.radius, 2000.),
        ] {
            ensure!(
                v.is_finite() && (0. ..=hi).contains(&v),
                "Spacer parameter out of range"
            );
        }
        ensure!(
            self.color.iter().all(|v| v.is_finite()),
            "nonfinite Spacer color"
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
            "invalid Spacer pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let k = px_scale as f64;
        let l = self.left as f64 * k;
        let (t, r, b) = if self.uniform {
            (l, l, l)
        } else {
            (
                self.top as f64 * k,
                self.right as f64 * k,
                self.bottom as f64 * k,
            )
        };
        let (wf, hf) = (w as f64, h as f64);
        let (bw, bh) = ((wf - l - r).max(1.), (hf - t - b).max(1.));
        let s = (bw / wf).min(bh / hf);
        let (dw, dh) = (wf * s, hf * s);
        let (x0, y0) = (l + (bw - dw) / 2., t + (bh - dh) / 2.);
        let matrix = super::spin::compose([1., 0., 0., 1., x0, y0], [s, 0., 0., s, 0., 0.]);
        let mut out = frame.transformed(w, h, GroupAffine(matrix), checkpoint)?;
        let rad = (self.radius as f64 * k).min(dw.min(dh) / 2.);
        if rad > 0. {
            let mut index = 0;
            out.map_premultiplied(
                &mut |mut p| {
                    let (x, y) = ((index % w) as f32 + 0.5, (index / w) as f32 + 0.5);
                    index += 1;
                    let d = round_rect_sdf(
                        x,
                        y,
                        x0 as f32,
                        y0 as f32,
                        (x0 + dw) as f32,
                        (y0 + dh) as f32,
                        rad as f32,
                    );
                    let a = (0.5 - d).clamp(0., 1.);
                    if a < 1. {
                        for v in &mut p {
                            *v *= a;
                        }
                    }
                    p
                },
                checkpoint,
            )?;
        }
        if self.fill {
            let c: [f32; 3] =
                std::array::from_fn(|i| super::basic_grade::linear(self.color[i].clamp(0., 1.)));
            out.map_premultiplied(
                &mut |mut p| {
                    let k = 1. - p[3];
                    for i in 0..3 {
                        p[i] += c[i] * k;
                    }
                    p[3] = 1.;
                    p
                },
                checkpoint,
            )?;
        }
        *frame = out;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpacerChannel {
    Left,
    Top,
    Right,
    Bottom,
    Radius,
    ColorR,
    ColorG,
    ColorB,
    ColorA,
}
pub type SpacerAnimation =
    std::collections::BTreeMap<SpacerChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &SpacerAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            SpacerChannel::Left
            | SpacerChannel::Top
            | SpacerChannel::Right
            | SpacerChannel::Bottom => (0., 4000.),
            SpacerChannel::Radius => (0., 2000.),
            _ => (-(f32::MAX as f64), f32::MAX as f64),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Spacer curve out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Spacer, a: &SpacerAnimation, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Spacer> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, time)? as f32;
        match c {
            SpacerChannel::Left => p.left = v,
            SpacerChannel::Top => p.top = v,
            SpacerChannel::Right => p.right = v,
            SpacerChannel::Bottom => p.bottom = v,
            SpacerChannel::Radius => p.radius = v,
            SpacerChannel::ColorR => p.color[0] = v,
            SpacerChannel::ColorG => p.color[1] = v,
            SpacerChannel::ColorB => p.color[2] = v,
            SpacerChannel::ColorA => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_rounding_fill_ignores_color_alpha_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [(i * 10) as u8, 64, 192, 128])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let p = Spacer {
            left: 0.,
            top: 0.,
            right: 0.,
            bottom: 0.,
            ..Default::default()
        };
        let mut a = frame.clone();
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        assert_eq!(a.pixels(), frame.pixels());
        let p = Spacer {
            left: 0.75,
            radius: 1.,
            fill: true,
            color: [0.25, 0.5, 1., 0.],
            ..p
        };
        p.apply_linear(&mut a, 1., &mut || Ok(())).unwrap();
        assert_ne!(a.pixels(), frame.pixels());
        assert!(a.pixels().iter().all(|p| p[3] == 1.));
        let mut b = frame.clone();
        Spacer {
            color: [0.25, 0.5, 1., 1.],
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
    fn persisted_nine_curves_defaults_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"spacer","parameters":{},"animation":{"left":curve(0.,2.),"top":curve(0.,4.),"right":curve(0.,6.),"bottom":curve(0.,8.),"radius":curve(0.,2.),"colorR":curve(0.,1.),"colorG":curve(0.,1.),"colorB":curve(0.,1.),"colorA":curve(0.,1.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Spacer {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Spacer")
        };
        assert_eq!(parameters, Spacer::default());
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            Spacer {
                left: 1.,
                top: 2.,
                right: 3.,
                bottom: 4.,
                radius: 1.,
                color: [0.5; 4],
                ..Default::default()
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
            parameters: Spacer,
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
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spacer.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 1152);
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
                "Spacer original working bits case{index}"
            );
        }
    }
}
