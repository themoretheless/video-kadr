//! Filmcraft ASC CDL; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AscCdl {
    pub r_slope: f32,
    pub r_offset: f32,
    pub r_power: f32,
    pub g_slope: f32,
    pub g_offset: f32,
    pub g_power: f32,
    pub b_slope: f32,
    pub b_offset: f32,
    pub b_power: f32,
    pub saturation: f32,
}
impl Default for AscCdl {
    fn default() -> Self {
        Self {
            r_slope: 1.,
            r_offset: 0.,
            r_power: 1.,
            g_slope: 1.,
            g_offset: 0.,
            g_power: 1.,
            b_slope: 1.,
            b_offset: 0.,
            b_power: 1.,
            saturation: 1.,
        }
    }
}
impl AscCdl {
    pub fn validate(&self) -> Result<()> {
        for v in [
            self.r_slope,
            self.g_slope,
            self.b_slope,
            self.r_power,
            self.g_power,
            self.b_power,
            self.saturation,
        ] {
            ensure!(
                v.is_finite() && (0. ..=10.).contains(&v),
                "ASC CDL parameter out of range"
            );
        }
        for v in [self.r_offset, self.g_offset, self.b_offset] {
            ensure!(
                v.is_finite() && (-1. ..=1.).contains(&v),
                "ASC CDL offset out of range"
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
        let s = [self.r_slope, self.g_slope, self.b_slope];
        let o = [self.r_offset, self.g_offset, self.b_offset];
        let p = [self.r_power, self.g_power, self.b_power];
        frame.map_signal(
            &mut |c, _, _| {
                let mut v = [0.; 3];
                for k in 0..3 {
                    v[k] = (c[k] * s[k] + o[k]).clamp(0., 1.).powf(p[k].max(0.));
                }
                let l = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
                Ok(v.map(|q| (l + self.saturation * (q - l)).clamp(0., 1.)))
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
    fn original_asc_cdl_working_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: AscCdl,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-asc-cdl.json"))
                .unwrap();
        assert_eq!(fixture.cases.len(), 136);
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
                "original AscCdl {index}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AscCdlChannel {
    RSlope,
    ROffset,
    RPower,
    GSlope,
    GOffset,
    GPower,
    BSlope,
    BOffset,
    BPower,
    Saturation,
}
pub type AscCdlAnimation =
    std::collections::BTreeMap<AscCdlChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &AscCdlAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let offset = matches!(
            c,
            AscCdlChannel::ROffset | AscCdlChannel::GOffset | AscCdlChannel::BOffset
        );
        ensure!(
            super::composition::animatable_values_in(
                v,
                if offset { -1. } else { 0. },
                if offset { 1. } else { 10. },
                true
            ),
            "ASC CDL animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: AscCdl, a: &AscCdlAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<AscCdl> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            AscCdlChannel::RSlope => p.r_slope = v,
            AscCdlChannel::ROffset => p.r_offset = v,
            AscCdlChannel::RPower => p.r_power = v,
            AscCdlChannel::GSlope => p.g_slope = v,
            AscCdlChannel::GOffset => p.g_offset = v,
            AscCdlChannel::GPower => p.g_power = v,
            AscCdlChannel::BSlope => p.b_slope = v,
            AscCdlChannel::BOffset => p.b_offset = v,
            AscCdlChannel::BPower => p.b_power = v,
            AscCdlChannel::Saturation => p.saturation = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn asc_cdl_ten_independent_curves_bounds_and_roundtrip() {
        let names = [
            "rSlope",
            "rOffset",
            "rPower",
            "gSlope",
            "gOffset",
            "gPower",
            "bSlope",
            "bOffset",
            "bPower",
            "saturation",
        ];
        let mut animation = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let offset = name.ends_with("Offset");
            let a = if offset { -0.75 } else { i as f64 / 10. };
            let b = if offset { 0.75 } else { 2. + i as f64 / 10. };
            animation.insert((*name).into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}}));
        }
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"asc_cdl","parameters":{},"animation":animation}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::AscCdl {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("ASC CDL")
        };
        assert_eq!(parameters, AscCdl::default());
        assert_eq!(a.len(), 10);
        for time in [0, 250000, 500000, 750000] {
            let sampled =
                serde_json::to_value(sample_animation(parameters, &a, time).unwrap()).unwrap();
            for (i, name) in names.iter().enumerate() {
                let offset = name.ends_with("Offset");
                let start = if offset { -0.75 } else { i as f64 / 10. };
                let end = if offset { 0.75 } else { 2. + i as f64 / 10. };
                let expected = (start + (end - start) * (time.min(500000) as f64 / 500000.)) as f32;
                assert_eq!(
                    sampled[*name].as_f64().unwrap(),
                    expected as f64,
                    "{name}/{time}"
                );
            }
        }
        for (c, values) in [
            (AscCdlChannel::RSlope, [-0.01, 10.01]),
            (AscCdlChannel::ROffset, [-1.01, 1.01]),
        ] {
            for value in values {
                assert!(validate_animation(&AscCdlAnimation::from([(
                    c,
                    super::super::composition::AnimatableValue::Constant { value }
                )]))
                .is_err());
            }
        }
    }
}
