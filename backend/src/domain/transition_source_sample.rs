// Included in video_clock: preserve exact source time until decoder conversion.
pub fn transition_source_sample_tick(placement:&ClipPlacement,mode:PlaybackMode,timeline_source_tick:i128)->Result<Option<i64>>{
 if let Some(clock)=&placement.concat_retime{
  ensure!(mode==PlaybackMode::Forward,"source retime playback direction is not migrated");
  clock.validate_placement(placement)?;
  return clock.sample(placement.timeline_start_tick,placement.source_in_tick,timeline_source_tick);
 }
 ensure!(placement.speed_ramp.is_none(),"transition source speed-ramp clock is not migrated");
 let unit=super::caption_time::SOURCE_TICKS_PER_MICROSECOND;
 if let PlaybackMode::Freeze{source_tick}=mode{return Ok(Some(i64::try_from(i128::from(source_tick)*unit)?))}
 let rel=timeline_source_tick-i128::from(placement.timeline_start_tick)*unit+i128::from(placement.source_timeline_offset_tick.unwrap_or(0))*unit;
 let scaled=(rel as f64*placement.speed.abs()).round();
 ensure!(scaled.is_finite()&&scaled>=i64::MIN as f64&&scaled < -(i64::MIN as f64),"transition source clock overflow");
 let source=match mode{
  PlaybackMode::Forward=>i128::from(placement.source_in_tick)*unit+scaled as i128,
  PlaybackMode::Reverse=>i128::from(placement.source_out_tick)*unit-scaled as i128-1,
  PlaybackMode::Freeze{..}=>unreachable!(),
 };
 if source<0{return Ok(None)}
 Ok(Some(i64::try_from(source)?))
}
#[cfg(test)]
mod exact_transition_source_tests {
 use super::*;
 #[test]
 fn source_tick_retains_reverse_offset_and_one_tick_boundary_for_metadata(){
  let p=ClipPlacement{concat_source_eof: false,
timeline_start_tick:10_000_000,source_in_tick:20_000_000,source_out_tick:30_000_000,speed:1.,source_timeline_offset_tick:None,source_head_timeline_tick:None, concat_retime: None,timeline_duration_tick:None,speed_ramp:None};
  let at=10_000_000i128*254_016;
  assert_eq!(transition_source_sample_tick(&p,PlaybackMode::Forward,at+1).unwrap(),Some(20_000_000*254_016+1));
  assert_eq!(transition_source_sample_tick(&p,PlaybackMode::Reverse,at).unwrap(),Some(30_000_000*254_016-1));
  assert_eq!(transition_source_sample_tick(&p,PlaybackMode::Freeze{source_tick:25_000_000},at-1).unwrap(),Some(25_000_000*254_016));
  let before=ClipPlacement{concat_source_eof: false,
timeline_start_tick:1,source_in_tick:0,source_out_tick:1,..p};
  assert_eq!(transition_source_sample_tick(&before,PlaybackMode::Forward,0).unwrap(),None);
 }
}
