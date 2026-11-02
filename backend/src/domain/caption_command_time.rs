//! Exact caption command input coercion using the original Filmcraft time parser.
use super::caption_time::CaptionTime;
use filmcraft_time::{FrameRate,Tick,TICKS_PER_SECOND};
use serde::Deserialize;
#[derive(Debug,Clone,Deserialize)]
#[serde(tag="unit",content="value",rename_all="snake_case",deny_unknown_fields)]
pub enum CaptionCommandTime {Frames(i64),Seconds(f64),Timecode(String),Ticks(String)}
impl CaptionCommandTime {
 pub fn resolve(&self,rate:FrameRate,drop_frame:bool,current:CaptionTime)->Result<CaptionTime,String>{
  if rate.num<=0||rate.den<=0{return Err("invalid caption frame rate".into());}
  fn frame(index:i64,rate:FrameRate)->Result<i64,String>{
   let ticks=(index as i128).checked_mul(TICKS_PER_SECOND as i128).and_then(|v|v.checked_mul(rate.den as i128)).ok_or("caption frame overflow")?.div_euclid(rate.num as i128);
   i64::try_from(ticks).map_err(|_|"caption frame overflow".into())
  }
  let ticks=match self {
   Self::Frames(index)=>frame(*index,rate)?,
   Self::Seconds(seconds)=>{
    if !seconds.is_finite(){return Err("invalid caption seconds".into());}
    let ticks=seconds*TICKS_PER_SECOND as f64;
    if ticks<i64::MIN as f64||ticks>=i64::MAX as f64{return Err("caption seconds overflow".into());}
    Tick::from_seconds_f64(*seconds).0
   },
   Self::Ticks(text)=>text.parse::<i64>().map_err(|_|"invalid caption source ticks")?,
   Self::Timecode(text)=>{
    let point=current.source_tick().map_err(str::to_owned)?;
    let index=filmcraft_time::parse_timecode(text,rate,drop_frame,rate.frame_at(point)).map_err(|e|e.to_string())?;
    frame(index,rate)?
   },
  };
  Ok(CaptionTime::SourceTicks(ticks))
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn caption_command_frames_seconds_and_exact_ticks_retain_source_clock(){
  let rate=FrameRate::FPS_23_976;
  for input in [CaptionCommandTime::Frames(13),CaptionCommandTime::Ticks("137729592000".into())]{assert_eq!(input.resolve(rate,false,CaptionTime::ZERO).unwrap().exact_ticks(),137729592000);}
  assert_eq!(CaptionCommandTime::Seconds(0.5).resolve(rate,false,CaptionTime::ZERO).unwrap().exact_ticks(),127008000000);
  assert_eq!(CaptionCommandTime::Frames(-1).resolve(FrameRate{num:11,den:1},false,CaptionTime::ZERO).unwrap().exact_ticks(),-23092363637);
 }
 #[test]fn caption_command_timecode_supports_compact_relative_and_drop_frame(){
  let rate=FrameRate::FPS_29_97;let current=CaptionTime::SourceTicks(rate.tick_of(100).0);
  for (text,df,index) in [("00:00:10:00",false,300),("1000",false,300),("+15",false,115),("-1.00",false,70),("00;01;00;02",true,1800)]{
   assert_eq!(CaptionCommandTime::Timecode(text.into()).resolve(rate,df,current).unwrap().exact_ticks(),rate.tick_of(index).0 as i128,"{text}");
  }
 }
 #[test]fn caption_command_time_refuses_bad_inputs_and_frame_overflow(){
  let rate=FrameRate::FPS_24;
  for input in [CaptionCommandTime::Frames(i64::MAX),CaptionCommandTime::Seconds(f64::INFINITY),CaptionCommandTime::Seconds(f64::NAN),CaptionCommandTime::Ticks("9223372036854775808".into()),CaptionCommandTime::Timecode("bad".into())]{assert!(input.resolve(rate,false,CaptionTime::ZERO).is_err());}
 }
}
