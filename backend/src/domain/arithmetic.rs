//! Bit-exact checked arithmetic shared by media timing and chunk planning.
//!
//! These functions remain allocation-free and side-effect-free so the Kani
//! harnesses can prove their primitive overflow and range contracts.

pub fn rescale_ticks_i64(
    ticks: i64,
    source_numerator: i64,
    source_denominator: i64,
    target_numerator: i64,
    target_denominator: i64,
) -> Option<i64> {
    if source_numerator <= 0
        || source_denominator <= 0
        || target_numerator <= 0
        || target_denominator <= 0
    {
        return None;
    }
    let numerator = i128::from(ticks)
        .checked_mul(i128::from(source_numerator))?
        .checked_mul(i128::from(target_denominator))?;
    let denominator = i128::from(source_denominator).checked_mul(i128::from(target_numerator))?;
    let adjustment = denominator / 2;
    let rounded = if numerator >= 0 {
        numerator.checked_add(adjustment)? / denominator
    } else {
        numerator.checked_sub(adjustment)? / denominator
    };
    i64::try_from(rounded).ok()
}

/// Convert a bounded count through a rational rate with round-to-nearest.
pub fn rescale_count_u64(value: u64, multiplier: u64, divisor: u64) -> Option<u64> {
    if divisor == 0 {
        return None;
    }
    let numerator = u128::from(value).checked_mul(u128::from(multiplier))?;
    let rounded = numerator.checked_add(u128::from(divisor / 2))? / u128::from(divisor);
    u64::try_from(rounded).ok()
}

/// Count output frames for a tick duration and milli-frame-rate, rounding up.
pub fn frame_count_ceil(duration_ticks: u64, time_base: u32, fps_milli: u32) -> Option<u64> {
    if time_base == 0 || fps_milli == 0 {
        return None;
    }
    let numerator = u128::from(duration_ticks).checked_mul(u128::from(fps_milli))?;
    let denominator = u128::from(time_base).checked_mul(1_000)?;
    let frames = numerator.checked_add(denominator - 1)? / denominator;
    u64::try_from(frames).ok()
}

pub fn range_end(start: u64, duration: u64) -> Option<u64> {
    start.checked_add(duration)
}

/// Return the next exclusive chunk end without overflow or an empty chunk.
pub fn chunk_end(start: u64, total: u64, max_chunk_frames: u64) -> Option<u64> {
    if start >= total || max_chunk_frames == 0 {
        return None;
    }
    let remaining = total.checked_sub(start)?;
    start.checked_add(remaining.min(max_chunk_frames))
}

#[cfg(kani)]
mod verification {
    use super::*;

    #[kani::proof]
    fn bounded_tick_identity_is_exact() {
        let ticks = i64::from(kani::any::<i32>());
        assert_eq!(rescale_ticks_i64(ticks, 1, 1_000, 1, 1_000), Some(ticks));
    }

    #[kani::proof]
    fn sample_count_rescale_is_checked() {
        let samples: u64 = kani::any();
        let time_base: u64 = kani::any();
        let sample_rate: u64 = kani::any();
        let result = rescale_count_u64(samples, time_base, sample_rate);
        if sample_rate == 0 {
            assert!(result.is_none());
        }
    }

    #[kani::proof]
    fn frame_count_is_ceil_or_explicit_overflow() {
        let duration: u64 = kani::any();
        let time_base: u32 = kani::any();
        let fps_milli: u32 = kani::any();
        if let Some(frames) = frame_count_ceil(duration, time_base, fps_milli) {
            if duration > 0 && time_base > 0 && fps_milli > 0 {
                assert!(frames > 0);
            }
        }
    }

    #[kani::proof]
    fn range_end_never_wraps() {
        let start: u64 = kani::any();
        let duration: u64 = kani::any();
        if let Some(end) = range_end(start, duration) {
            assert!(end >= start);
            assert_eq!(end - start, duration);
        }
    }

    #[kani::proof]
    fn chunks_advance_without_gaps_or_overflow() {
        let start: u64 = kani::any();
        let total: u64 = kani::any();
        let maximum: u64 = kani::any();
        if let Some(end) = chunk_end(start, total, maximum) {
            assert!(start < end);
            assert!(end <= total);
            assert!(end - start <= maximum);
        } else {
            assert!(start >= total || maximum == 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_examples_are_explicit() {
        assert_eq!(rescale_ticks_i64(123, 1, 1_000, 1, 1_000), Some(123));
        assert_eq!(rescale_ticks_i64(-123, 1, 1_000, 1, 1_000), Some(-123));
        assert_eq!(rescale_ticks_i64(1, 1, 4, 1, 10), Some(3));
        assert_eq!(rescale_ticks_i64(-1, 1, 4, 1, 10), Some(-3));
        for invalid in [
            (0, 1, 1, 1),
            (-1, 1, 1, 1),
            (1, 0, 1, 1),
            (1, -1, 1, 1),
            (1, 1, 0, 1),
            (1, 1, -1, 1),
            (1, 1, 1, 0),
            (1, 1, 1, -1),
        ] {
            assert_eq!(
                rescale_ticks_i64(7, invalid.0, invalid.1, invalid.2, invalid.3),
                None
            );
        }
        assert_eq!(range_end(u64::MAX, 1), None);
        assert_eq!(range_end(7, 9), Some(16));
        assert_eq!(chunk_end(u64::MAX - 2, u64::MAX, 100), Some(u64::MAX));
        assert_eq!(chunk_end(0, 10, 3), Some(3));
        assert_eq!(chunk_end(10, 10, 3), None);
        assert_eq!(chunk_end(0, 10, 0), None);
        assert_eq!(frame_count_ceil(1, 1_000, 1_000), Some(1));
        assert_eq!(frame_count_ceil(1_000, 1_000, 1_000), Some(1));
        assert_eq!(frame_count_ceil(1, 0, 1_000), None);
        assert_eq!(frame_count_ceil(1, 1_000, 0), None);
        assert_eq!(frame_count_ceil(1_000_000, 1_000_000, 29_970), Some(30));
        assert_eq!(rescale_count_u64(5, 1, 2), Some(3));
        assert_eq!(rescale_count_u64(5, 1, 0), None);
        assert_eq!(
            rescale_count_u64(48_000, 1_000_000, 48_000),
            Some(1_000_000)
        );
    }
}
