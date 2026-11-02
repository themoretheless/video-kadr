//! Filmcraft Edge Glow; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct EdgeGlow {
    pub threshold: f32,
    pub width: f32,
    pub radius: f32,
    pub intensity: f32,
    pub color: [f32; 4],
    pub only: bool,
}
impl Default for EdgeGlow {
    fn default() -> Self {
        Self {
            threshold: 20.,
            width: 2.,
            radius: 10.,
            intensity: 150.,
            color: [0.3, 0.8, 1., 1.],
            only: false,
        }
    }
}
impl EdgeGlow {
    pub fn validate(&self) -> Result<()> {
        for (v, max) in [
            (self.threshold, 100.),
            (self.width, 50.),
            (self.radius, 500.),
            (self.intensity, 1000.),
        ] {
            ensure!(
                v.is_finite() && (0.0..=max).contains(&v),
                "Edge Glow parameter out of range"
            );
        }
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "Edge Glow color out of range"
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
        ensure!(scale.is_finite() && scale > 0., "invalid Edge Glow scale");
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let source = frame.pixels();
        let mut mask = Vec::with_capacity(w * h);
        let threshold = self.threshold / 100.;
        for y in 0..h {
            checkpoint()?;
            for x in 0..w {
                let l = |dx: isize, dy: isize| {
                    let p = source[(y as isize + dy).clamp(0, h as isize - 1) as usize * w
                        + (x as isize + dx).clamp(0, w as isize - 1) as usize];
                    super::basic_grade::display(
                        (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).clamp(0., 1.),
                    ) * p[3].min(1.)
                        + 0.
                };
                let gx = l(1, -1) + 2. * l(1, 0) + l(1, 1) - l(-1, -1) - 2. * l(-1, 0) - l(-1, 1);
                let gy = l(-1, 1) + 2. * l(0, 1) + l(1, 1) - l(-1, -1) - 2. * l(0, -1) - l(1, -1);
                let v = (gx * gx + gy * gy).sqrt() / 4.;
                let t = ((v - threshold) / ((threshold + 0.1) - threshold).max(1e-6)).clamp(0., 1.);
                mask.push(t * t * (3. - 2. * t));
            }
        }
        let width = self.width * scale;
        let radius = self.radius * scale * 0.5;
        ensure!(
            width.is_finite() && radius.is_finite(),
            "Edge Glow scale overflow"
        );
        if width > 1. {
            mask =
                super::gaussian_blur::blur_plane(&mask, w, h, width * 0.5, 0, false, checkpoint)?
                    .into_iter()
                    .map(|v| (v * 3.).min(1.))
                    .collect();
        }
        let halo = super::gaussian_blur::blur_plane(&mask, w, h, radius, 0, false, checkpoint)?;
        let color = std::array::from_fn::<f32, 3, _>(|k| super::basic_grade::linear(self.color[k]));
        let intensity = self.intensity / 100.;
        let mut index = 0;
        frame.map_premultiplied(
            &mut |original| {
                let mut p = if self.only { [0.; 4] } else { original };
                let v = (mask[index] * 0.6 + halo[index] * 1.6) * intensity;
                index += 1;
                let add = color.map(|c| c * v);
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
    fn edge_glow_original_working_bits() {
        #[derive(Deserialize)]
        struct Case {
            parameters: EdgeGlow,
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
            "../../tests/fixtures/filmcraft-edge-glow.json"
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
                "Edge Glow original {i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeGlowChannel {
    Threshold,
    Width,
    Radius,
    Intensity,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type EdgeGlowAnimation =
    std::collections::BTreeMap<EdgeGlowChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &EdgeGlowAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let hi = match c {
            EdgeGlowChannel::Threshold => 100.,
            EdgeGlowChannel::Width => 50.,
            EdgeGlowChannel::Radius => 500.,
            EdgeGlowChannel::Intensity => 1000.,
            _ => 1.,
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., hi, true),
            "Edge Glow animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: EdgeGlow, a: &EdgeGlowAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<EdgeGlow> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            EdgeGlowChannel::Threshold => p.threshold = v,
            EdgeGlowChannel::Width => p.width = v,
            EdgeGlowChannel::Radius => p.radius = v,
            EdgeGlowChannel::Intensity => p.intensity = v,
            EdgeGlowChannel::ColorRed => p.color[0] = v,
            EdgeGlowChannel::ColorGreen => p.color[1] = v,
            EdgeGlowChannel::ColorBlue => p.color[2] = v,
            EdgeGlowChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn edge_glow_eight_curves_midpoint_hold_bounds_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let a:EdgeGlowAnimation=serde_json::from_value(serde_json::json!({"threshold":curve(20.,40.),"width":curve(2.,12.),"radius":curve(10.,30.),"intensity":curve(150.,350.),"colorRed":curve(0.3,0.7),"colorGreen":curve(0.8,0.2),"colorBlue":curve(1.,0.),"colorAlpha":curve(1.,0.)})).unwrap();
        let p = EdgeGlow {
            only: true,
            ..EdgeGlow::default()
        };
        let mid = sample_animation(p, &a, 250000).unwrap();
        assert_eq!(
            (
                mid.threshold,
                mid.width,
                mid.radius,
                mid.intensity,
                mid.only
            ),
            (30., 7., 20., 250., true)
        );
        assert_eq!(mid.color, [0.5; 4]);
        assert_eq!(
            sample_animation(p, &a, 750000).unwrap().color,
            [0.7, 0.2, 0., 0.]
        );
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"edge_glow","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        for (c, v) in [
            (EdgeGlowChannel::Threshold, 101.),
            (EdgeGlowChannel::Width, 51.),
            (EdgeGlowChannel::Radius, 501.),
            (EdgeGlowChannel::Intensity, 1001.),
            (EdgeGlowChannel::ColorRed, 1.01),
            (EdgeGlowChannel::ColorAlpha, -1.),
        ] {
            assert!(validate_animation(&EdgeGlowAnimation::from([(
                c,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
