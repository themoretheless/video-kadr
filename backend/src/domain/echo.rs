//! Filmcraft temporal Echo; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Echo {
    pub time: f32,
    pub count: f32,
    pub start: f32,
    pub decay: f32,
    pub operator: u32,
}
impl Default for Echo {
    fn default() -> Self {
        Self {
            time: -0.033,
            count: 1.,
            start: 1.,
            decay: 1.,
            operator: 0,
        }
    }
}
impl Echo {
    pub fn validate(&self) -> Result<()> {
        for (v, lo, hi) in [
            (self.time, -10., 10.),
            (self.count, 0., 30.),
            (self.start, 0., 1.),
            (self.decay, 0., 2.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "Echo parameter out of range"
            );
        }
        ensure!(self.operator <= 6, "Echo operator out of range");
        Ok(())
    }
    /// None means the source effect returns without changing the current frame.
    pub fn requests(&self) -> Result<Option<Vec<(f64, f32)>>> {
        self.validate()?;
        let n = self.count.clamp(0., 30.) as usize;
        let dt = self.time as f64;
        if n == 0 || dt.abs() < 1e-6 {
            return Ok(None);
        }
        Ok(Some(
            (1..=n)
                .map(|k| (dt * k as f64, self.start * self.decay.powi(k as i32)))
                .collect(),
        ))
    }
    /// Layers are ordered current frame first, then available temporal requests.
    pub fn combine(
        &self,
        layers: &[(LinearColorFrame, f32)],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<LinearColorFrame> {
        self.validate()?;
        ensure!(!layers.is_empty(), "empty Echo layers");
        ensure!(
            layers.len() <= 31 && layers.iter().all(|(_, w)| w.is_finite() && *w >= 0.),
            "invalid Echo layers"
        );
        let dimensions = layers[0].0.dimensions();
        let mut out = layers[0].0.clone();
        let wsum = layers.iter().map(|l| l.1).sum::<f32>().max(1e-6);
        let op = self.operator;
        let mut i = 0;
        out.map_premultiplied(
            &mut |_| {
                let px = |l: &(LinearColorFrame, f32)| -> [f32; 4] {
                    if l.0.dimensions() != dimensions {
                        return [0.; 4];
                    }
                    l.0.pixels()[i].map(|v| v * l.1)
                };
                let mut acc = match op {
                    2 => [f32::INFINITY; 4],
                    3 => [1.; 4],
                    _ => [0.; 4],
                };
                if op == 4 || op == 5 {
                    let over = |acc: &mut [f32; 4], p: [f32; 4]| {
                        let k = 1. - p[3];
                        for c in 0..4 {
                            acc[c] = p[c] + acc[c] * k;
                        }
                    };
                    if op == 4 {
                        for l in layers.iter().rev() {
                            over(&mut acc, px(l));
                        }
                    } else {
                        for l in layers {
                            over(&mut acc, px(l));
                        }
                    }
                } else {
                    for l in layers {
                        let p = px(l);
                        for k in 0..4 {
                            acc[k] = match op {
                                1 => acc[k].max(p[k]),
                                2 => acc[k].min(p[k]),
                                3 => acc[k] * (1. - p[k].min(1.)),
                                6 => acc[k] + p[k] / wsum,
                                _ => acc[k] + p[k],
                            };
                        }
                    }
                    if op == 3 {
                        acc = acc.map(|v| 1. - v);
                    }
                }
                if op == 0 {
                    acc[3] = acc[3].min(1.);
                }
                i += 1;
                acc.map(|v| if v.is_finite() { v.max(0.) } else { 0. })
            },
            checkpoint,
        )?;
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn picture(t: u64, alpha: u8) -> LinearColorFrame {
        let rgba = (0..24u64)
            .flat_map(|i| {
                let k = t / 10000;
                [
                    (i * 17 + k * 11) % 256,
                    (i * 7 + k * 23) % 256,
                    (i * 31 + k * 3) % 256,
                    alpha as u64,
                ]
                .map(|v| v as u8)
            })
            .collect();
        LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba,
            },
            &mut || Ok(()),
        )
        .unwrap()
    }
    #[test]
    fn echo_original_seven_operators_and_temporal_weights() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-echo.json")).unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 112);
        for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p:Echo=serde_json::from_value(serde_json::json!({"time":c["time"],"count":c["count"],"start":c["start"],"decay":c["decay"],"operator":c["operator"]})).unwrap();
            let alpha = c["alpha"].as_u64().unwrap() as u8;
            let mut layers = vec![(picture(500000, alpha), p.start)];
            for (dt, weight) in p.requests().unwrap().unwrap() {
                let t = 500000 + (dt * 1000000.).round() as i64;
                if (0..1000000).contains(&t) {
                    layers.push((picture(t as u64, alpha), weight));
                }
            }
            let actual = p.combine(&layers, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = actual
                .pixels()
                .iter()
                .flat_map(|p| p.map(f32::to_bits))
                .collect();
            let expected: Vec<u32> = c["bits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            assert_eq!(bits, expected, "original Echo case{index}");
        }
    }
    #[test]
    fn echo_identity_request_validation_and_cancellation() {
        assert!(Echo {
            count: 0.,
            ..Echo::default()
        }
        .requests()
        .unwrap()
        .is_none());
        assert!(Echo {
            time: 0.0000001,
            ..Echo::default()
        }
        .requests()
        .unwrap()
        .is_none());
        assert_eq!(
            Echo {
                count: 3.75,
                ..Echo::default()
            }
            .requests()
            .unwrap()
            .unwrap()
            .len(),
            3
        );
        for p in [
            Echo {
                time: f32::NAN,
                ..Echo::default()
            },
            Echo {
                operator: 7,
                ..Echo::default()
            },
            Echo {
                decay: 2.01,
                ..Echo::default()
            },
        ] {
            assert!(p.validate().is_err());
        }
        assert!(Echo::default().combine(&[], &mut || Ok(())).is_err());
        assert!(Echo::default()
            .combine(&[(picture(0, 255), 1.)], &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EchoChannel {
    Time,
    Count,
    Start,
    Decay,
}
pub type EchoAnimation =
    std::collections::BTreeMap<EchoChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &EchoAnimation) -> Result<()> {
    for (k, v) in a {
        v.validate()?;
        let (lo, hi) = match k {
            EchoChannel::Time => (-10., 10.),
            EchoChannel::Count => (0., 30.),
            EchoChannel::Start => (0., 1.),
            EchoChannel::Decay => (0., 2.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "Echo animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(mut p: Echo, a: &EchoAnimation, t: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<Echo> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (k, v) in a {
        let value = super::nested_composition::sample_value(v, t)? as f32;
        match k {
            EchoChannel::Time => p.time = value,
            EchoChannel::Count => p.count = value,
            EchoChannel::Start => p.start = value,
            EchoChannel::Decay => p.decay = value,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn echo_four_curves_roundtrip_bounds_and_fractional_count() {
        let channels = [
            ("time", -0.1, 0.1),
            ("count", 1., 6.5),
            ("start", 1., 0.),
            ("decay", 0.5, 2.),
        ];
        let mut curves = serde_json::Map::new();
        for (name, from, to) in channels {
            curves.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":from},{"tick":500000,"value":to}]}}));
        }
        let animation: EchoAnimation =
            serde_json::from_value(serde_json::Value::Object(curves)).unwrap();
        let middle = sample_animation(Echo::default(), &animation, 250000).unwrap();
        assert_eq!(
            (middle.time, middle.count, middle.start, middle.decay),
            (0., 3.75, 0.5, 1.25)
        );
        assert!(middle.requests().unwrap().is_none());
        let late = sample_animation(Echo::default(), &animation, 900000).unwrap();
        assert_eq!(
            (late.time, late.count, late.start, late.decay),
            (0.1, 6.5, 0., 2.)
        );
        assert_eq!(late.requests().unwrap().unwrap().len(), 6);
        let effect = super::super::composition::VideoEffect::Echo {
            parameters: Echo::default(),
            animation: Some(animation),
        };
        let saved = serde_json::to_value(effect).unwrap();
        let restored: super::super::composition::VideoEffect =
            serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        for (channel, lo, hi) in [
            (EchoChannel::Time, -10., 10.),
            (EchoChannel::Count, 0., 30.),
            (EchoChannel::Start, 0., 1.),
            (EchoChannel::Decay, 0., 2.),
        ] {
            for value in [lo, hi] {
                assert!(validate_animation(&EchoAnimation::from([(
                    channel,
                    super::super::composition::AnimatableValue::Constant { value }
                )]))
                .is_ok());
            }
            for value in [lo - 0.01, hi + 0.01, f64::NAN] {
                assert!(validate_animation(&EchoAnimation::from([(
                    channel,
                    super::super::composition::AnimatableValue::Constant { value }
                )]))
                .is_err());
            }
        }
    }
}
