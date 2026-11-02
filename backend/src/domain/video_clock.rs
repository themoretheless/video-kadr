//! Native source-frame clock shared by nested media rendering.
use super::composition::{ClipPlacement, PlaybackMode, SpeedRampInterpolation};
use anyhow::{ensure, Result};

/// Decoder timestamp ownership. Concat clocks must retain their exact source ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameTime {
    Microseconds(u64),
    SourceTicks(i64),
}
impl FrameTime {
    pub fn microseconds(self) -> Result<u64> {
        let value = match self {
            Self::Microseconds(value) => return Ok(value),
            Self::SourceTicks(value) => { ensure!(value >= 0, "negative source frame clock"); value as u64 / 254016 },
        };
        Ok(value)
    }
    pub fn legacy_microseconds(self) -> Result<u64> {
        if let Self::SourceTicks(value) = self { ensure!(value % 254016 == 0, "exact source frame requires a precision-aware decoder"); }
        self.microseconds()
    }
    pub fn last_presented_pts(self, numerator: i64, denominator: i64) -> Result<i128> {
        ensure!(numerator > 0 && denominator > 0, "invalid video presentation time base");
        let ticks = match self { Self::Microseconds(value) => i128::from(value)*254016, Self::SourceTicks(value) => i128::from(value) };
        ensure!(ticks >= 0, "negative source frame clock");
        let top = ticks.checked_mul(i128::from(denominator)).ok_or_else(||anyhow::anyhow!("video presentation clock overflow"))?;
        let bottom = 254016000000i128.checked_mul(i128::from(numerator)).ok_or_else(||anyhow::anyhow!("video presentation time base overflow"))?;
        let value = top / bottom;
        ensure!(value <= 9_007_199_254_740_991, "video PTS exceeds exact filter range");
        Ok(value)
    }
}

pub fn source_tick(
    placement: &ClipPlacement,
    mode: PlaybackMode,
    timeline_tick: u64,
) -> Result<Option<u64>> {
    if placement.concat_retime.is_some(){
        let unit=super::caption_time::SOURCE_TICKS_PER_MICROSECOND;
        return Ok(transition_source_sample_tick(placement,mode,i128::from(timeline_tick)*unit)?.map(|tick|(i128::from(tick)/unit)as u64));
    }
    let duration = placement.timeline_duration_ticks()?;
    let end = placement.timeline_end_tick()?;
    ensure!(
        end <= 9_007_199_254_740_991 && placement.source_out_tick <= 9_007_199_254_740_991,
        "video clock exceeds exact wire range"
    );
    if let PlaybackMode::Freeze { source_tick } = mode {
        ensure!(
            source_tick >= placement.source_in_tick && source_tick < placement.source_out_tick,
            "freeze source outside selected interval"
        );
    }
    if timeline_tick < placement.timeline_start_tick || timeline_tick >= end {
        return Ok(None);
    }
    if let PlaybackMode::Freeze { source_tick } = mode {
        return Ok(Some(source_tick));
    }
    let local = i128::from((timeline_tick - placement.timeline_start_tick).min(duration - 1))
        + i128::from(placement.source_timeline_offset_tick.unwrap_or(0));
    let head = placement.source_head_ticks()?;
    let (minimum, maximum) = if mode == PlaybackMode::Reverse {
        (placement.source_in_tick, placement.source_out_tick.checked_add(head).ok_or_else(|| anyhow::anyhow!("reverse source head overflow"))? - 1)
    } else {
        (placement.source_in_tick.checked_sub(head).ok_or_else(|| anyhow::anyhow!("source head before file"))?, placement.source_out_tick - 1)
    };
    let span = placement.source_span_ticks();
    let progress = if let Some(segments) = placement.speed_ramp_segments()? {
        if local < 0 { (local as f64 * placement.speed).floor() as i128 }
        else if let Some(segment) = segments
            .iter()
            .find(|segment| local < i128::from(segment.timeline_end_tick))
        {
            let time = (local - i128::from(segment.timeline_start_tick)).max(0) as f64;
            let source_span = (segment.source_end_tick - segment.source_start_tick) as f64;
            let delta = segment.end_speed - segment.start_speed;
            let offset = match segment.interpolation {
                SpeedRampInterpolation::Hold => time * segment.start_speed,
                SpeedRampInterpolation::Linear if delta.abs() < 1e-12 => time * segment.start_speed,
                SpeedRampInterpolation::Linear => {
                    let slope = delta / source_span;
                    segment.start_speed * (slope * time).exp_m1() / slope
                }
            };
            ensure!(offset.is_finite(), "invalid ramp source clock");
            i128::from(segment.source_start_tick) + offset.floor().clamp(0.0, source_span) as i128
        } else {
            i128::from(span - 1)
        }
    } else {
        // Keep sub-microsecond source precision until the presentation clock is sampled.
        let unit = super::caption_time::SOURCE_TICKS_PER_MICROSECOND;
        return Ok(transition_source_sample_tick(placement, mode, i128::from(timeline_tick) * unit)?
            .map(|tick| (i128::from(tick) / unit) as u64)
            .map(|tick| tick.clamp(minimum, maximum)));
    }
    .min(i128::from(span - 1));
    let source = match mode {
        PlaybackMode::Forward => i128::from(placement.source_in_tick) + progress,
        PlaybackMode::Reverse => i128::from(placement.source_out_tick) - 1 - progress,
        PlaybackMode::Freeze { .. } => unreachable!("handled freeze"),
    };
    Ok(Some(u64::try_from(source)?.clamp(minimum, maximum)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clocks_preserve_offsets_reverse_freeze_and_half_open_boundaries() {
        let placement = ClipPlacement {
            timeline_start_tick: 500000,
            source_in_tick: 100000,
            source_out_tick: 1100000,
            speed: 2.0,
            source_timeline_offset_tick: None, source_head_timeline_tick: None, concat_retime: None,
            timeline_duration_tick: None,
            speed_ramp: None,
        };
        assert_eq!(
            source_tick(&placement, PlaybackMode::Forward, 800000).unwrap(),
            Some(700000)
        );
        assert_eq!(
            source_tick(&placement, PlaybackMode::Reverse, 800000).unwrap(),
            Some(499999)
        );
        assert_eq!(
            source_tick(
                &placement,
                PlaybackMode::Freeze {
                    source_tick: 200000
                },
                800000
            )
            .unwrap(),
            Some(200000)
        );
        assert_eq!(
            source_tick(&placement, PlaybackMode::Forward, 1000000).unwrap(),
            None
        );
        assert_eq!(
            source_tick(&placement, PlaybackMode::Forward, 499999).unwrap(),
            None
        );
        assert!(source_tick(
            &placement,
            PlaybackMode::Freeze {
                source_tick: 1100000
            },
            800000
        )
        .is_err());
    }
    #[test]
    fn linear_source_speed_ramp_inverts_native_duration_integral() {
        let placement: ClipPlacement = serde_json::from_value(serde_json::json!({"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":1100000,"speed":1.0,"speedRamp":{"interpolation":"linear","points":[{"sourceProgressTick":0,"speed":1.0},{"sourceProgressTick":1000000,"speed":2.0}]}})).unwrap();
        let sample = source_tick(&placement, PlaybackMode::Forward, 405465)
            .unwrap()
            .unwrap();
        assert!(sample.abs_diff(600000) <= 2, "{sample}");
    }
}

/// Original transition side clock, extrapolated beyond an item's edit span.
/// Source precision is retained until the decoder's explicit microsecond boundary.
pub fn transition_source_tick(placement:&ClipPlacement,mode:PlaybackMode,timeline_source_tick:i128)->Result<Option<u64>>{
 transition_source_sample_tick(placement,mode,timeline_source_tick)?.map(|tick|u64::try_from(i128::from(tick)/super::caption_time::SOURCE_TICKS_PER_MICROSECOND)).transpose().map_err(Into::into)
}

#[cfg(test)]mod transition_clock_tests{
 use super::*;
 #[test]fn transition_clock_reads_both_source_handles_with_signed_exact_time(){
  let placement=ClipPlacement{timeline_start_tick:10000000,source_in_tick:20000000,source_out_tick:30000000,speed:1.,source_timeline_offset_tick:None,source_head_timeline_tick:None, concat_retime: None,timeline_duration_tick:None,speed_ramp:None};
  assert_eq!(transition_source_tick(&placement,PlaybackMode::Forward,9000000i128*254016).unwrap(),Some(19000000));
  assert_eq!(transition_source_tick(&placement,PlaybackMode::Forward,21000000i128*254016).unwrap(),Some(31000000));
  assert_eq!(transition_source_tick(&placement,PlaybackMode::Reverse,0).unwrap(),Some(39999999));
  assert_eq!(transition_source_tick(&placement,PlaybackMode::Forward,10000000i128*254016+1).unwrap(),Some(20000000));
 }
}

include!("transition_source_sample.rs");

#[cfg(test)]
mod trim_boundary_tests {
    use super::*;
    #[test]
    fn extending_slow_source_windows_does_not_reveal_a_future_frame_before_the_old_end() {
        for ramped in [false,true] {
            for mode in [PlaybackMode::Forward,PlaybackMode::Reverse] {
                let old:ClipPlacement=serde_json::from_value(serde_json::json!({"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":4000000,"speed":0.5})).unwrap();
                let mut old=old;
                if ramped {old.speed_ramp=Some(serde_json::from_value(serde_json::json!({"interpolation":"hold","audioPolicy":"preserve_pitch","points":[{"sourceProgressTick":0,"speed":0.5},{"sourceProgressTick":3000000,"speed":0.5}]})).unwrap());}
                let mut extended=old.clone();
                if mode==PlaybackMode::Forward {extended.source_out_tick=4250000;}else{extended.source_in_tick=750000;}
                if ramped {extended.speed_ramp=Some(serde_json::from_value(serde_json::json!({"interpolation":"hold","audioPolicy":"preserve_pitch","points":[{"sourceProgressTick":0,"speed":0.5},{"sourceProgressTick":3000000,"speed":0.5},{"sourceProgressTick":3250000,"speed":0.5}]})).unwrap());}
                let last=old.timeline_end_tick().unwrap()-1;
                assert_eq!(source_tick(&old,mode,last).unwrap(),source_tick(&extended,mode,last).unwrap(),"ramped{ramped} {mode:?}");
            }
        }
    }
}

#[cfg(test)]
mod retained_source_phase_tests {
    use super::*;
    #[test]
    fn right_fragment_keeps_full_ramp_and_exact_source_phase() {
        for ramped in [false, true] {
            let mut original: ClipPlacement = serde_json::from_value(serde_json::json!({"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":4000000,"speed":0.5})).unwrap();
            if ramped { original.speed_ramp=Some(serde_json::from_value(serde_json::json!({"interpolation":"linear","audioPolicy":"preserve_pitch","points":[{"sourceProgressTick":0,"speed":0.5},{"sourceProgressTick":3000000,"speed":2.0}]})).unwrap()); }
            let duration = original.timeline_duration_ticks().unwrap();
            let cut = 1234567;
            let mut right = original.clone();
            right.timeline_start_tick += cut;
            right.source_timeline_offset_tick = Some(cut as i64);
            assert_eq!(right.timeline_duration_ticks().unwrap(),duration-cut);
            for mode in [PlaybackMode::Forward, PlaybackMode::Reverse] {
                for local in [cut,cut+1,cut+7777,duration-1] {
                    let time = original.timeline_start_tick+local;
                    assert_eq!(source_tick(&original,mode,time).unwrap(),source_tick(&right,mode,time).unwrap(),"ramped{ramped} {mode:?} {local}");
                }
            }
            right.source_timeline_offset_tick=Some(duration as i64);
            assert!(right.timeline_duration_ticks().is_err());
        }
    }
}

#[cfg(test)]
mod negative_source_phase_tests {
    use super::*;
    #[test]
    fn extended_heads_preserve_original_source_ticks_for_fractional_speed_and_ramps() {
        for speed in [0.5, 1.37] { for ramped in [false,true] {
            let mut original:ClipPlacement=serde_json::from_value(serde_json::json!({"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":4000000,"speed":speed})).unwrap();
            if ramped {original.speed_ramp=Some(serde_json::from_value(serde_json::json!({"interpolation":"linear","audioPolicy":"preserve_pitch","points":[{"sourceProgressTick":0,"speed":speed},{"sourceProgressTick":3000000,"speed":2.11}]})).unwrap());}
            let mut extended=original.clone();extended.timeline_start_tick-=250000;extended.source_timeline_offset_tick=Some(-250000);
            assert_eq!(extended.timeline_end_tick().unwrap(),original.timeline_end_tick().unwrap());
            for mode in [PlaybackMode::Forward,PlaybackMode::Reverse] {
                for time in [2000000,2000001,2000003,2250000,original.timeline_end_tick().unwrap()-1] {
                    assert_eq!(source_tick(&original,mode,time).unwrap(),source_tick(&extended,mode,time).unwrap(),"{speed} ramped{ramped} {mode:?} {time}");
                }
                assert!(source_tick(&extended,mode,extended.timeline_start_tick).unwrap().is_some());
            }
            let mut no_head=extended.clone();no_head.source_in_tick=0;
            assert!(source_tick(&no_head,PlaybackMode::Forward,no_head.timeline_start_tick).is_err());
        }}
    }
}
