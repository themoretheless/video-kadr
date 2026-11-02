//! Filmcraft Posterize Time temporal request contract; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PosterizeTime {
    pub rate: f32,
}
impl Default for PosterizeTime {
    fn default() -> Self {
        Self { rate: 12. }
    }
}
impl PosterizeTime {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.rate.is_finite() && (0.1..=99.).contains(&self.rate),
            "Posterize Time rate out of range"
        );
        Ok(())
    }
    /// Offset requested from the temporal clip environment. None means retain the current frame.
    pub fn frame_offset(&self, seconds: f64) -> Result<Option<f64>> {
        self.validate()?;
        ensure!(seconds.is_finite(), "nonfinite Posterize Time clock");
        let rate = self.rate.max(0.01) as f64;
        let q = (seconds * rate + 1e-6).floor() / rate;
        let dt = q - seconds;
        ensure!(dt.is_finite(), "Posterize Time clock overflow");
        Ok(if dt.abs() < 1e-6 { None } else { Some(dt) })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posterize_time_original_temporal_requests() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-posterize-time.json"
        ))
        .unwrap();
        assert_eq!(f["cases"].as_array().unwrap().len(), 70);
        for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let p = PosterizeTime {
                rate: c["rate"].as_f64().unwrap() as f32,
            };
            let actual = p.frame_offset(c["seconds"].as_f64().unwrap()).unwrap();
            let expected = c["dt"].as_f64();
            assert_eq!(
                actual.map(f64::to_bits),
                expected.map(f64::to_bits),
                "source temporal request {i}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PosterizeTimeChannel {
    Rate,
}
pub type PosterizeTimeAnimation =
    std::collections::BTreeMap<PosterizeTimeChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &PosterizeTimeAnimation) -> Result<()> {
    for v in a.values() {
        v.validate()?;
        ensure!(
            super::composition::animatable_values_in(v, 0.1, 99., true),
            "Posterize Time rate animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: PosterizeTime,
    a: &PosterizeTimeAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<PosterizeTime> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    if let Some(v) = a.get(&PosterizeTimeChannel::Rate) {
        p.rate = super::nested_composition::sample_value(v, t)? as f32;
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn posterize_time_rate_curve_bounds_midpoint_hold_roundtrip() {
        let a:PosterizeTimeAnimation=serde_json::from_value(serde_json::json!({"rate":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":12.0},{"tick":500000,"value":24.0}]}}})).unwrap();
        assert_eq!(
            sample_animation(PosterizeTime::default(), &a, 250000)
                .unwrap()
                .rate,
            18.
        );
        assert_eq!(
            sample_animation(PosterizeTime::default(), &a, 900000)
                .unwrap()
                .rate,
            24.
        );
        let effect = super::super::composition::VideoEffect::PosterizeTime {
            parameters: PosterizeTime::default(),
            animation: Some(a),
        };
        let saved = serde_json::to_value(effect).unwrap();
        let restored: super::super::composition::VideoEffect =
            serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), saved);
        for v in [0.1, 99.] {
            assert!(validate_animation(&PosterizeTimeAnimation::from([(
                PosterizeTimeChannel::Rate,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_ok());
        }
        for v in [0., 99.01] {
            assert!(validate_animation(&PosterizeTimeAnimation::from([(
                PosterizeTimeChannel::Rate,
                super::super::composition::AnimatableValue::Constant { value: v }
            )]))
            .is_err());
        }
    }
}
