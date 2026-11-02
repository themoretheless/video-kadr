//! Exact exported frame time alongside the host clock used by ordinary media decoders.
use super::caption_time::{CaptionTime,SOURCE_TICKS_PER_MICROSECOND as UNIT};
#[derive(Debug,Clone,Copy)]
pub struct CaptionFrameSample {pub host_us:u64,pub source_ticks:i128}
impl CaptionFrameSample {
    pub fn new(index:u64,rate:filmcraft_time::FrameRate,offset_us:u64)->Result<Self,&'static str>{
        if rate.num<=0 || rate.den<=0 {return Err("invalid caption sample frame rate");}
        let numerator=(index as i128).checked_mul(filmcraft_time::TICKS_PER_SECOND as i128)
            .and_then(|v|v.checked_mul(rate.den as i128)).ok_or("caption frame clock overflow")?;
        let ticks=numerator/(rate.num as i128);
        let offset=CaptionTime::Microseconds(offset_us);offset.validate_timeline_point()?;
        let source_ticks=offset.exact_ticks().checked_add(ticks).ok_or("caption frame clock overflow")?;
        let host_us=offset_us.checked_add(u64::try_from(ticks/UNIT).map_err(|_|"caption frame clock overflow")?).ok_or("caption frame clock overflow")?;
        CaptionTime::Microseconds(host_us).validate_timeline_point()?;
        Ok(Self{host_us,source_ticks})
    }
    /// Native composition instances map time by integer-us translation (no speed field).
    /// Retain the frame's sub-us residue for every local source-time callback.
    pub fn local_ticks(self,local_us:u64)->Result<i128,&'static str>{
        CaptionTime::Microseconds(local_us).validate_timeline_point()?;
        Ok(self.source_ticks+(local_us as i128-self.host_us as i128)*UNIT)
    }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn fractional_frame_start_and_half_open_end_match_original_source_clock(){
  let rate=filmcraft_time::FrameRate::FPS_23_976;
  let frame13=CaptionFrameSample::new(13,rate,0).unwrap();
  let frame14=CaptionFrameSample::new(14,rate,0).unwrap();
  assert_eq!(filmcraft_time::Tick(frame13.source_ticks as i64),rate.tick_of(13));
  assert_eq!(frame13.source_ticks,137729592000);assert_eq!(frame13.host_us,542208);
  assert!(CaptionTime::Microseconds(frame13.host_us).exact_ticks()<frame13.source_ticks);
  let active=|time:i128|frame13.source_ticks<=time&&time<frame14.source_ticks;
  assert!(active(frame13.source_ticks));assert!(!active(frame14.source_ticks));assert!(!active(CaptionTime::Microseconds(frame13.host_us).exact_ticks()));
 }
 #[test]fn selected_range_and_nested_integer_translations_retain_fractional_residue(){
  for rate in [filmcraft_time::FrameRate::FPS_23_976,filmcraft_time::FrameRate::FPS_29_97,filmcraft_time::FrameRate::FPS_25]{
   let sample=CaptionFrameSample::new(13,rate,1234567).unwrap();
   assert_eq!(sample.source_ticks,1234567i128*UNIT+rate.tick_of(13).0 as i128);
   for local_us in [0,1,987654,9000000]{
    let expected=local_us as i128*UNIT+(sample.source_ticks-sample.host_us as i128*UNIT);
    assert_eq!(sample.local_ticks(local_us).unwrap(),expected);
   }
  }
 }
 #[test]fn invalid_rates_indices_and_offsets_fail_without_wrapping(){
  assert!(CaptionFrameSample::new(1,filmcraft_time::FrameRate{num:0,den:1},0).is_err());
  assert!(CaptionFrameSample::new(u64::MAX,filmcraft_time::FrameRate{num:1,den:i64::MAX},0).is_err());
  assert!(CaptionFrameSample::new(0,filmcraft_time::FrameRate::FPS_24,u64::MAX).is_err());
 }
}
#[cfg(test)]mod legacy_range_tests {
 use super::*;
 #[test]fn long_legacy_origin_retains_fraction_without_narrowing_to_i64(){
  let sample=CaptionFrameSample::new(13,filmcraft_time::FrameRate::FPS_23_976,9_000_000_000_000_000).unwrap();
  assert!(sample.source_ticks>i64::MAX as i128);
  assert_eq!(sample.source_ticks-sample.host_us as i128*UNIT,84672);
  assert_eq!(sample.local_ticks(1).unwrap(),UNIT+84672);
 }
}
