//! Original source-text keyframes hold the preceding value, including before/after edges.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTextKey {
    pub tick: i64,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTextAnimation {
    pub time_base: u64,
    pub keyframes: Vec<SourceTextKey>,
}
impl SourceTextAnimation {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.time_base > 0, "source text time base must be positive");
        ensure!(
            !self.keyframes.is_empty(),
            "source text keyframes must not be empty"
        );
        ensure!(
            self.keyframes.windows(2).all(|p| p[0].tick < p[1].tick),
            "source text keys must increase strictly"
        );
        Ok(())
    }
    /// Author a key using the editor's exact integer, half-away-from-zero clock rounding.
    pub fn set_at(&self, local_us: i128, value: String) -> Result<Self> {
        self.validate()?;
        let scaled = local_us.checked_mul(i128::from(self.time_base))
            .ok_or_else(||anyhow::anyhow!("source text key clock overflow"))?;
        let whole = scaled / 1_000_000;
        let remainder = scaled % 1_000_000;
        let rounded = whole + if remainder.abs() >= 500_000 {scaled.signum()} else {0};
        let tick = i64::try_from(rounded).map_err(|_|anyhow::anyhow!("source text key time overflow"))?;
        let mut next = self.clone();
        match next.keyframes.binary_search_by_key(&tick, |key| key.tick) {
            Ok(index) => next.keyframes[index].value = value,
            Err(index) => next.keyframes.insert(index, SourceTextKey {tick,value}),
        }
        Ok(next)
    }
    pub fn sample(&self, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<&str> {
        use super::animation_sample_time::AnimationSampleTime;
        match time.into() {
            AnimationSampleTime::Microseconds(us) => self.sample_signed(i128::from(us)),
            AnimationSampleTime::SignedMicroseconds(us) => self.sample_signed(i128::from(us)),
            AnimationSampleTime::SourceTicks(ticks) | AnimationSampleTime::FilmcraftSourceTicks(ticks) => self.sample_original_ticks(i128::from(ticks)),
        }
    }
    pub fn sample_original_ticks(&self, ticks: i128) -> Result<&str> {
        self.validate()?;
        let time = ticks.checked_mul(i128::from(self.time_base))
            .ok_or_else(|| anyhow::anyhow!("source text clock overflow"))?;
        let upper = self.keyframes.partition_point(|key| i128::from(key.tick) * 254_016_000_000 <= time);
        Ok(&self.keyframes[upper.saturating_sub(1)].value)
    }
    pub fn sample_signed(&self, local_us: i128) -> Result<&str> {
        self.validate()?;
        // Exact rational comparison; no f64 rounding at large clocks or fractional key times.
        let time = local_us
            .checked_mul(i128::from(self.time_base))
            .ok_or_else(|| anyhow::anyhow!("source text clock overflow"))?;
        let upper = self
            .keyframes
            .partition_point(|k| i128::from(k.tick) * 1_000_000 <= time);
        Ok(&self.keyframes[upper.saturating_sub(1)].value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_fractional_clock_holds_until_exact_key_boundary() {
        let a = SourceTextAnimation {
            time_base: 3,
            keyframes: vec![
                SourceTextKey {
                    tick: -1,
                    value: "Привет".into(),
                },
                SourceTextKey {
                    tick: 1,
                    value: "مرحبا".into(),
                },
                SourceTextKey {
                    tick: 4,
                    value: "Final".into(),
                },
            ],
        };
        assert_eq!(a.sample(0).unwrap(), "Привет");
        assert_eq!(a.sample(333333).unwrap(), "Привет");
        assert_eq!(a.sample(333334).unwrap(), "مرحبا");
        assert_eq!(a.sample(1333333).unwrap(), "مرحبا");
        assert_eq!(a.sample(1333334).unwrap(), "Final");
        assert_eq!(a.sample(u64::MAX).unwrap(), "Final");
        let round: SourceTextAnimation =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(round, a);
    }
}

#[cfg(test)]
#[test]
fn authored_source_text_keys_preserve_clock_order_and_replace_existing() {
    let animation=SourceTextAnimation {time_base:3,keyframes:vec![SourceTextKey {tick:-1,value:"Б".into()},SourceTextKey {tick:4,value:"End".into()}]};
    let added=animation.set_at(500000,"🙂".into()).unwrap();
    assert_eq!(added.keyframes.iter().map(|key|key.tick).collect::<Vec<_>>(),vec![-1,2,4]);
    assert_eq!(added.set_at(500000,"New".into()).unwrap().keyframes[1].value,"New");
    assert_eq!(animation.set_at(-500000,"Start".into()).unwrap().keyframes[0].tick,-2);
    assert_eq!(animation.keyframes.len(),2);
    assert!(animation.set_at(i128::MAX,"Overflow".into()).is_err());
}

include!("source_text_exact_time_tests.rs");
