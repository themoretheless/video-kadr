//! Filmcraft Logo Cutout CPU reference; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LogoCutout {
    pub background: u8,
    pub color: [f32; 4],
    pub threshold: f32,
    pub softness: f32,
    pub unmultiply: bool,
    pub invert: bool,
}
impl Default for LogoCutout {
    fn default() -> Self {
        Self {
            background: 0,
            color: [1.; 4],
            threshold: 5.,
            softness: 10.,
            unmultiply: true,
            invert: false,
        }
    }
}
impl LogoCutout {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.background <= 2, "Logo Cutout background out of range");
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
            "Logo Cutout color out of range"
        );
        ensure!(
            [self.threshold, self.softness]
                .iter()
                .all(|v| v.is_finite() && (0. ..=100.).contains(v)),
            "Logo Cutout threshold/softness out of range"
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
        let bg = match self.background {
            0 => [1.; 3],
            1 => [0.; 3],
            _ => [self.color[0], self.color[1], self.color[2]],
        };
        let th = self.threshold / 100.;
        let soft = (self.softness / 100.).max(1e-3);
        frame.map_premultiplied(
            &mut |p| {
                if p[3] <= 1e-6 {
                    return p;
                }
                let v = [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
                    .map(|v| super::basic_grade::display(v.max(0.)));
                let mut a0 = 0f32;
                for k in 0..3 {
                    let d = v[k] - bg[k];
                    let room = if d >= 0. { 1. - bg[k] } else { bg[k] };
                    if room > 1e-6 {
                        a0 = a0.max(d.abs() / room);
                    }
                }
                let a0 = a0.clamp(0., 1.);
                let mut a = ((a0 - th) / ((1. - th).max(1e-3) * soft)).clamp(0., 1.);
                if self.invert {
                    a = 1. - a;
                }
                let col = if self.unmultiply && a0 > 1e-4 && !self.invert {
                    [0, 1, 2].map(|k| (bg[k] + (v[k] - bg[k]) / a0).clamp(0., 1.))
                } else {
                    v
                };
                let l = col.map(|v| super::basic_grade::linear(v.clamp(0., 1.)));
                let na = a * p[3];
                [l[0] * na, l[1] * na, l[2] * na, na]
            },
            checkpoint,
        )
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn logo_cutout_original_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            parameters: LogoCutout,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-logo-cutout.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 864);
        for (index, c) in fixture.cases.into_iter().enumerate() {
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
                "original Logo Cutout {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogoCutoutChannel {
    Threshold,
    Softness,
    ColorRed,
    ColorGreen,
    ColorBlue,
    ColorAlpha,
}
pub type LogoCutoutAnimation =
    std::collections::BTreeMap<LogoCutoutChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &LogoCutoutAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let hi = if matches!(
            c,
            LogoCutoutChannel::Threshold | LogoCutoutChannel::Softness
        ) {
            100.
        } else {
            1.
        };
        ensure!(
            super::composition::animatable_values_in(v, 0., hi, true),
            "Logo Cutout animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: LogoCutout, a: &LogoCutoutAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<LogoCutout> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, curve) in a {
        let v = super::nested_composition::sample_value(curve, t)? as f32;
        match c {
            LogoCutoutChannel::Threshold => p.threshold = v,
            LogoCutoutChannel::Softness => p.softness = v,
            LogoCutoutChannel::ColorRed => p.color[0] = v,
            LogoCutoutChannel::ColorGreen => p.color[1] = v,
            LogoCutoutChannel::ColorBlue => p.color[2] = v,
            LogoCutoutChannel::ColorAlpha => p.color[3] = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn logo_cutout_six_curves_bounds_midpoint_hold_and_roundtrip() {
        let curve = |a: f64, b: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let animation:LogoCutoutAnimation=serde_json::from_value(serde_json::json!({"threshold":curve(5.,75.),"softness":curve(10.,100.),"colorRed":curve(1.,0.25),"colorGreen":curve(1.,0.75),"colorBlue":curve(1.,0.125),"colorAlpha":curve(1.,0.)})).unwrap();
        let p = LogoCutout {
            background: 2,
            unmultiply: false,
            invert: true,
            ..LogoCutout::default()
        };
        let mid = sample_animation(p, &animation, 250000).unwrap();
        assert_eq!(mid.threshold, 40.);
        assert_eq!(mid.softness, 55.);
        assert_eq!(mid.color, [0.625, 0.875, 0.5625, 0.5]);
        assert_eq!(
            (mid.background, mid.unmultiply, mid.invert),
            (2, false, true)
        );
        let end = sample_animation(p, &animation, 750000).unwrap();
        assert_eq!(end.color, [0.25, 0.75, 0.125, 0.]);
        assert_eq!((end.threshold, end.softness), (75., 100.));
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"logo_cutout","parameters":p,"animation":animation}),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<super::super::composition::VideoEffect>(
                serde_json::to_value(&e).unwrap()
            )
            .unwrap(),
            e
        );
        for (channel, value) in [("threshold", 101.), ("softness", -1.), ("colorRed", 1.01)] {
            let invalid: LogoCutoutAnimation = serde_json::from_value(
                serde_json::json!({channel:{"mode":"constant","value":value}}),
            )
            .unwrap();
            assert!(validate_animation(&invalid).is_err());
        }
    }
}
