//! Filmcraft export/job.rs output-grid range and pipeline frame clocks.
//! MIT; pinned reference 5231852443363f001c3f6b396dd9b1e6461ae2be.
use super::composition::FrameRateRatio;
use anyhow::{ensure, Context, Result};
const T: i128 = 254_016_000_000;

fn divisor(rate: FrameRateRatio) -> Result<i128> {
    ensure!(
        rate.numerator > 0 && rate.denominator > 0,
        "invalid output frame rate"
    );
    Ok(T * i128::from(rate.denominator))
}
fn tick_of(rate: FrameRateRatio, frame: i128) -> Result<i64> {
    let den = divisor(rate)?;
    Ok(i64::try_from(
        frame
            .checked_mul(den)
            .context("output frame clock overflow")?
            .div_euclid(i128::from(rate.numerator)),
    )?)
}
/// Original floor(start) .. floor(end-1)+1, including negative starts.
pub fn span(start: i64, end: i64, rate: FrameRateRatio) -> Result<(u64, i64)> {
    ensure!(end > start, "empty output frame range");
    let den = divisor(rate)?;
    let f0 = (i128::from(start) * i128::from(rate.numerator)).div_euclid(den);
    let f1 = (i128::from(end - 1) * i128::from(rate.numerator)).div_euclid(den) + 1;
    let count = u64::try_from(f1 - f0)?;
    ensure!((1..=360000).contains(&count), "invalid output frame count");
    tick_of(rate, f1)?;
    Ok((count, tick_of(rate, f0)?))
}
/// Recover f0 from a quantized tick_of(f0). Rounding down a second time would
/// incorrectly select the previous frame when a rational period is not integral.
pub fn first_frame(rate: FrameRateRatio, aligned_start: i64) -> Result<i64> {
    let den = divisor(rate)?;
    let n = i128::from(aligned_start) * i128::from(rate.numerator);
    Ok(i64::try_from(-(-n).div_euclid(den))?)
}
pub fn sample(rate: FrameRateRatio, aligned_start: i64, index: u64) -> Result<i64> {
    let frame = i128::from(first_frame(rate, aligned_start)?) + i128::from(index);
    tick_of(rate, frame)
}

/// GIF stores a whole number of centiseconds; match image::GifEncoder's floor.
pub fn gif_delay_ms(rate: FrameRateRatio) -> Result<u32> {
    divisor(rate)?;
    Ok(u32::try_from(
        (u64::from(rate.denominator) * 100 / u64::from(rate.numerator)).min(u64::from(u16::MAX))
            * 10,
    )?)
}
