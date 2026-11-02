//! Active Filmcraft Bokeh Blur; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BokehBlur {
    pub amount: f32,
    pub shape: u32,
    pub rotation: f32,
    pub highlight: f32,
    pub threshold: f32,
}
impl Default for BokehBlur {
    fn default() -> Self {
        Self {
            amount: 10.,
            shape: 4,
            rotation: 0.,
            highlight: 50.,
            threshold: 80.,
        }
    }
}
fn poly_radius(n: u32, theta: f32, rot: f32) -> f32 {
    if n == 0 {
        return 1.;
    }
    let seg = std::f32::consts::TAU / n as f32;
    let a = (theta - rot).rem_euclid(seg) - seg / 2.;
    (std::f32::consts::PI / n as f32).cos() / a.cos()
}
impl BokehBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.shape <= 5 && self.rotation.is_finite(),
            "Bokeh Blur shape/rotation out of range"
        );
        for (v, max) in [
            (self.amount, 500.),
            (self.highlight, 100.),
            (self.threshold, 100.),
        ] {
            ensure!(
                v.is_finite() && (0. ..=max).contains(&v),
                "Bokeh Blur parameter out of range"
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
            "invalid Bokeh Blur pixel scale"
        );
        checkpoint()?;
        let r = self.amount * px_scale;
        if r < 0.5 {
            return Ok(());
        }
        let sides = [0u32, 3, 4, 5, 6, 8][self.shape as usize];
        let rot = self.rotation.to_radians();
        let boost = self.highlight / 100. * 6.;
        let th = self.threshold / 100.;
        let (fw, fh) = frame.dimensions();
        let mut work = frame.clone();
        let mut down = 1.0f32;
        while r / down > 12. && work.dimensions().0 >= 8 && work.dimensions().1 >= 8 {
            work = work.downsample2(checkpoint)?;
            down *= 2.;
        }
        let rr = r / down;
        if boost > 0. {
            work.map_premultiplied(
                &mut |mut p| {
                    if p[3] <= 1e-6 {
                        return p;
                    }
                    let c = [0, 1, 2].map(|i| super::basic_grade::display((p[i] / p[3]).max(0.)));
                    let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                    let t = ((l - th) / (1. - th).max(1e-6)).clamp(0., 1.);
                    // Preserve original smoothstep evaluation before multiplication by boost.
                    let k = 1. + boost * (t * t * (3. - 2. * t));
                    for v in &mut p[..3] {
                        *v *= k;
                    }
                    p
                },
                checkpoint,
            )?;
        }
        let rings = (rr.ceil() as usize).clamp(1, 12);
        let mut taps = vec![(0.0f32, 0.0f32)];
        for ring in 1..=rings {
            checkpoint()?;
            let rad = rr * ring as f32 / rings as f32;
            let n = (ring * 8).max(6);
            for i in 0..n {
                let theta = std::f32::consts::TAU * (i as f32 + 0.5 * (ring % 2) as f32) / n as f32;
                let pr = rad * poly_radius(sides, theta, rot);
                taps.push((theta.cos() * pr, theta.sin() * pr));
            }
        }
        let inv = 1. / taps.len() as f32;
        let src = work.clone();
        let (w, h) = work.dimensions();
        let mut index = 0;
        work.map_premultiplied(
            &mut |_| {
                let (x, y) = (index % w, index / w);
                index += 1;
                let mut acc = [0f32; 4];
                for &(dx, dy) in &taps {
                    let p = super::mosaic::sample(
                        src.pixels(),
                        w,
                        h,
                        x as f32 + 0.5 + dx,
                        y as f32 + 0.5 + dy,
                    );
                    for k in 0..4 {
                        acc[k] += p[k];
                    }
                }
                acc.map(|v| v * inv)
            },
            checkpoint,
        )?;
        *frame = if down > 1. {
            work.transformed(
                fw,
                fh,
                super::group_transform::GroupAffine([
                    fw as f64 / w as f64,
                    0.,
                    0.,
                    fh as f64 / h as f64,
                    0.,
                    0.,
                ]),
                checkpoint,
            )?
        } else {
            work
        };
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn bokeh_blur_identity_iris_highlight_downsample_and_cancellation() {
        let surface = GroupSurface {
            width: 18,
            height: 10,
            rgba: (0..180)
                .flat_map(|i| {
                    if i % 3 == 0 {
                        [255, 255, 255, 128]
                    } else {
                        [64, 128, 192, 255]
                    }
                })
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&surface, &mut || Ok(())).unwrap();
        let mut identity = frame.clone();
        BokehBlur {
            amount: 0.,
            ..Default::default()
        }
        .apply_linear(&mut identity, 1., &mut || Ok(()))
        .unwrap();
        assert_eq!(frame.pixels(), identity.pixels());
        for shape in 0..6 {
            let mut out = frame.clone();
            BokehBlur {
                amount: 500.,
                shape,
                rotation: -37.,
                ..Default::default()
            }
            .apply_linear(&mut out, 1., &mut || Ok(()))
            .unwrap();
            assert_eq!(out.dimensions(), (18, 10));
            assert_ne!(out.pixels(), frame.pixels());
        }
        assert!(BokehBlur {
            threshold: 101.,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(BokehBlur::default()
            .apply_linear(&mut identity, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_bokeh_blur_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: BokehBlur,
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
            "../../tests/fixtures/filmcraft-bokeh-blur.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 192);
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
                "original Bokeh Blur {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BokehBlurChannel {
    Amount,
    Rotation,
    Highlight,
    Threshold,
}
pub type BokehBlurAnimation =
    std::collections::BTreeMap<BokehBlurChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &BokehBlurAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (min, max) = match c {
            BokehBlurChannel::Amount => (0., 500.),
            BokehBlurChannel::Rotation => (-(f32::MAX as f64), f32::MAX as f64),
            _ => (0., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(v, min, max, true),
            "Bokeh Blur animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: BokehBlur, a: &BokehBlurAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<BokehBlur> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            BokehBlurChannel::Amount => p.amount = v,
            BokehBlurChannel::Rotation => p.rotation = v,
            BokehBlurChannel::Highlight => p.highlight = v,
            BokehBlurChannel::Threshold => p.threshold = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn bokeh_blur_defaults_four_curves_choice_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"bokeh_blur","parameters":{"shape":5},"animation":{"amount":curve(0.,26.5),"rotation":curve(-37.,74.),"highlight":curve(0.,100.),"threshold":curve(0.,100.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::BokehBlur {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Bokeh Blur")
        };
        assert_eq!(
            parameters,
            BokehBlur {
                shape: 5,
                ..Default::default()
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            BokehBlur {
                amount: 13.25,
                rotation: 18.5,
                highlight: 50.,
                threshold: 50.,
                shape: 5
            }
        );
        assert_eq!(
            sample_animation(parameters, &a, 750000).unwrap().amount,
            26.5
        );
        assert!(validate_animation(&BokehBlurAnimation::from([(
            BokehBlurChannel::Highlight,
            super::super::composition::AnimatableValue::Constant { value: 101. }
        )]))
        .is_err());
    }
}
