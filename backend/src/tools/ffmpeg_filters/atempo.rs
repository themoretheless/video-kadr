//! Bounded `atempo` chains for speeds outside a single FFmpeg filter's range.

/// Split `speed` into a chain of `atempo` filters in the supported 0.5..=2.0 band.
pub fn atempo_filter_chain(speed: f64) -> Vec<String> {
    debug_assert!(speed.is_finite() && speed > 0.0);
    let mut remaining = speed;
    let mut filters = Vec::new();
    while remaining > 2.0 {
        filters.push("atempo=2.000000".to_owned());
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        filters.push("atempo=0.500000".to_owned());
        remaining *= 2.0;
    }
    if (remaining - 1.0).abs() > 1e-9 {
        filters.push(format!("atempo={remaining:.6}"));
    }
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_speed_emits_no_filters() {
        assert!(atempo_filter_chain(1.0).is_empty());
    }

    #[test]
    fn fast_speed_chains_twos() {
        let chain = atempo_filter_chain(8.0);
        assert_eq!(
            chain,
            vec![
                "atempo=2.000000".to_owned(),
                "atempo=2.000000".to_owned(),
                "atempo=2.000000".to_owned(),
            ]
        );
    }
}
