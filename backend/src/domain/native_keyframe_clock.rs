//! Rational local-to-authored animation time, before authored-grid quantization.
//! Integration into persisted tracks remains explicit; legacy clocks are unchanged.
use super::keyframes::KeyframeTrack;
use anyhow::{ensure, Context, Result};
const TPS: i128 = 254_016_000_000;
const PER_US: i128 = 254_016;
const WIRE: u64 = 9_007_199_254_740_991;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "ClockWire", into = "ClockWire")]
pub struct Clock {
    num: u64,
    den: u64,
    offset: i64,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClockWire {
    rate_num: u64,
    rate_den: u64,
    offset_num: i64,
}
impl TryFrom<ClockWire> for Clock {
    type Error = anyhow::Error;
    fn try_from(value: ClockWire) -> Result<Self> {
        Self::new(value.rate_num, value.rate_den, value.offset_num)
    }
}
impl From<Clock> for ClockWire {
    fn from(clock: Clock) -> Self {
        Self {
            rate_num: clock.num,
            rate_den: clock.den,
            offset_num: clock.offset,
        }
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
impl Clock {
    pub fn identity() -> Self {
        Self {
            num: 1,
            den: 1,
            offset: 0,
        }
    }
    /// authored_us = (local_us * num + offset) / den.
    pub fn new(num: u64, den: u64, offset: i64) -> Result<Self> {
        Self::wide(num.into(), den.into(), offset.into())
    }
    fn wide(num: u128, den: u128, offset: i128) -> Result<Self> {
        ensure!(num > 0 && den > 0, "invalid animation clock rate");
        let common = gcd(gcd(num, den), offset.unsigned_abs());
        let divisor = i128::try_from(common)?;
        let num = u64::try_from(num / common)?;
        let den = u64::try_from(den / common)?;
        let offset = i64::try_from(offset / divisor)?;
        // These bounds also prove that source-TPS/key products fit i128.
        ensure!(
            num <= 86_400_000_000 && den <= 1_000_000 && offset.unsigned_abs() <= WIRE,
            "animation clock exceeds supported numeric domain"
        );
        Ok(Self { num, den, offset })
    }
    pub fn parts(self) -> (u64, u64, i64) {
        (self.num, self.den, self.offset)
    }
    /// Compose a tail/trim offset without rounding it to authored ticks.
    pub fn shifted(self, local_us: i64) -> Result<Self> {
        let offset = i128::from(local_us)
            .checked_mul(self.num.into())
            .and_then(|delta| delta.checked_add(self.offset.into()))
            .context("animation clock offset overflow")?;
        Self::wide(self.num.into(), self.den.into(), offset)
    }
    /// Replay the current authored interval over another local duration.
    pub fn retimed(self, old_us: u64, new_us: u64) -> Result<Self> {
        ensure!(old_us > 0 && new_us > 0, "invalid animation clock duration");
        Self::wide(
            u128::from(self.num) * u128::from(old_us),
            u128::from(self.den) * u128::from(new_us),
            i128::from(self.offset) * i128::from(new_us),
        )
    }
    pub(crate) fn validate_domain<T>(
        self,
        time_base: u64,
        keyframes: &[super::keyframes::Keyframe<T>],
    ) -> Result<()> {
        ensure!(
            time_base > 0 && time_base <= 100_000_000 && keyframes.len() <= 4096,
            "animation clock track budget exceeded"
        );
        ensure!(
            !keyframes.is_empty()
                && keyframes.iter().all(|key| key.tick.unsigned_abs() <= WIRE)
                && keyframes.windows(2).all(|pair| pair[0].tick < pair[1].tick),
            "invalid animation clock key order/range"
        );
        Ok(())
    }
    pub fn prepare<'a>(self, track: &'a KeyframeTrack<f64>) -> Result<Prepared<'a>> {
        self.validate_domain(track.time_base, &track.keyframes)?;
        KeyframeTrack::new(
            track.time_base,
            track.interpolation,
            track.keyframes.clone(),
        )?;
        Ok(Prepared { clock: self, track })
    }
}
#[derive(Clone, Copy)]
pub struct Prepared<'a> {
    clock: Clock,
    track: &'a KeyframeTrack<f64>,
}
fn rounded(numerator: i128, denominator: i128) -> Result<i64> {
    let whole = numerator / denominator;
    let rem = numerator % denominator;
    let value = if rem.unsigned_abs() * 2 >= denominator as u128 {
        whole + if numerator < 0 { -1 } else { 1 }
    } else {
        whole
    };
    i64::try_from(value).context("animation clock sample exceeds native tick domain")
}
impl Prepared<'_> {
    pub fn sample_seconds(self, seconds: f64) -> Result<f64> {
        ensure!(seconds.is_finite(), "nonfinite animation time");
        let seconds = (seconds * self.clock.num as f64 + self.clock.offset as f64 / 1e6)
            / self.clock.den as f64;
        let tick = seconds * self.track.time_base as f64;
        ensure!(
            tick.is_finite() && tick >= i64::MIN as f64 && tick < i64::MAX as f64,
            "animation clock sample exceeds native tick domain"
        );
        let value = self.track.sample_tick(tick.round() as i64);
        ensure!(value.is_finite(), "nonfinite animation clock sample");
        Ok(value)
    }

    pub fn sample_microseconds(self, local: i64) -> Result<f64> {
        if self.clock == Clock::identity() {
            return Ok(self.track.sample_seconds(local as f64 / 1e6)?);
        }
        let numerator = (i128::from(local) * i128::from(self.clock.num)
            + i128::from(self.clock.offset))
            * i128::from(self.track.time_base);
        let tick = rounded(numerator, i128::from(self.clock.den) * 1_000_000)?;
        let value = self.track.sample_tick(tick);
        ensure!(value.is_finite(), "nonfinite animation clock sample");
        Ok(value)
    }
    /// Exact source modes retain fractional TPS, distinct from native grid sampling.
    pub fn sample_source_ticks(self, local: i64, filmcraft: bool) -> Result<f64> {
        let numerator = (i128::from(local) * i128::from(self.clock.num)
            + i128::from(self.clock.offset) * PER_US)
            * i128::from(self.track.time_base);
        let denominator = TPS * i128::from(self.clock.den);
        let whole = numerator / denominator;
        ensure!(
            (i128::from(i64::MIN)..=i128::from(i64::MAX)).contains(&whole),
            "animation clock sample exceeds native tick domain"
        );
        let value = self
            .track
            .sample_source_ratio(numerator, denominator, filmcraft);
        ensure!(value.is_finite(), "nonfinite source animation clock sample");
        Ok(value)
    }
}
