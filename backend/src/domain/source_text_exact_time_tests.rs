// Included after the typed source-text adapter is applied.
#[cfg(test)]
mod exact_clock_tests {
    use super::*;
    use crate::domain::animation_sample_time::AnimationSampleTime;
    #[test]
    fn exact_source_tick_boundary_changes_text_without_microsecond_rounding() {
        let origin = 35_000_000i64 * 254_016_000_000;
        let animation = SourceTextAnimation {
            time_base: 254_016_000_000,
            keyframes: vec![
                SourceTextKey { tick: origin - 1, value: "Before".into() },
                SourceTextKey { tick: origin + 1, value: "After".into() },
            ],
        };
        for time in [AnimationSampleTime::SourceTicks(origin), AnimationSampleTime::FilmcraftSourceTicks(origin)] {
            assert_eq!(animation.sample(time).unwrap(), "Before");
        }
        assert_eq!(animation.sample(AnimationSampleTime::FilmcraftSourceTicks(origin + 1)).unwrap(), "After");
        assert_eq!(animation.sample(AnimationSampleTime::FilmcraftSourceTicks(i64::MIN)).unwrap(), "Before");
        assert_eq!(animation.sample(AnimationSampleTime::FilmcraftSourceTicks(i64::MAX)).unwrap(), "After");
    }
    #[test]
    fn signed_preroll_and_legacy_microsecond_units_remain_distinct() {
        let animation = SourceTextAnimation {
            time_base: 1_000_000,
            keyframes: vec![
                SourceTextKey { tick: -1, value: "Preroll".into() },
                SourceTextKey { tick: 1, value: "Visible".into() },
            ],
        };
        assert_eq!(animation.sample(-1i64).unwrap(), "Preroll");
        assert_eq!(animation.sample(1u64).unwrap(), "Visible");
        assert_eq!(animation.sample(AnimationSampleTime::FilmcraftSourceTicks(1)).unwrap(), "Preroll");
        assert_eq!(animation.sample(AnimationSampleTime::FilmcraftSourceTicks(254_016)).unwrap(), "Visible");
    }
}
