//! Explicit time units and evaluator semantics for effect parameter sampling.
//! Legacy microsecond callers retain authored-grid rounding; exact callers opt in.
use super::keyframes::{KeyframeTrack,KeyframeError};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum AnimationSampleTime {
    Microseconds(u64),
    SignedMicroseconds(i64),
    SourceTicks(i64),
    FilmcraftSourceTicks(i64),
}
impl From<u64> for AnimationSampleTime { fn from(value:u64)->Self {Self::Microseconds(value)} }
impl From<i64> for AnimationSampleTime { fn from(value:i64)->Self {Self::SignedMicroseconds(value)} }
impl From<i32> for AnimationSampleTime { fn from(value:i32)->Self {Self::SignedMicroseconds(i64::from(value))} }
impl From<u32> for AnimationSampleTime { fn from(value:u32)->Self {Self::Microseconds(u64::from(value))} }
impl AnimationSampleTime {
    pub fn sample_track(self,track:&KeyframeTrack<f64>)->Result<f64,KeyframeError>{
        match self {
            Self::Microseconds(us)=>if let Some(clock)=track.sample_clock{clock.prepare(track).and_then(|prepared|prepared.sample_microseconds(i64::try_from(us)?)).map_err(|_|KeyframeError::InvalidClock)}else{track.sample_seconds(us as f64/1_000_000.0)},
            Self::SignedMicroseconds(us)=>if let Some(clock)=track.sample_clock{clock.prepare(track).and_then(|prepared|prepared.sample_microseconds(us)).map_err(|_|KeyframeError::InvalidClock)}else{track.sample_seconds(us as f64/1_000_000.0)},
            Self::SourceTicks(tick)=>Ok(track.sample_source_ticks(tick)),
            Self::FilmcraftSourceTicks(tick)=>Ok(track.sample_filmcraft_source_ticks(tick)),
        }
    }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn old_microsecond_callers_keep_grid_rounding_and_exact_ticks_remain_distinct(){
  for base in [24,1000,1000000]{
   let track:KeyframeTrack<f64>=serde_json::from_value(serde_json::json!({"timeBase":base,"interpolation":"linear","keyframes":[{"tick":0,"value":5.0},{"tick":base,"value":10.0}]})).unwrap();
   for us in [0u64,1,12345,250000,500001,1000000]{
    assert_eq!(AnimationSampleTime::from(us).sample_track(&track).unwrap(),track.sample_seconds(us as f64/1_000_000.).unwrap());
    assert_eq!(AnimationSampleTime::from(us as i32).sample_track(&track).unwrap(),track.sample_seconds(us as f64/1_000_000.).unwrap());
   }
   assert!(AnimationSampleTime::SourceTicks(1).sample_track(&track).unwrap()>5.);
   assert!(AnimationSampleTime::FilmcraftSourceTicks(1).sample_track(&track).unwrap()>5.);
  }
 }
 #[test]
 fn explicit_filmcraft_time_uses_original_easing_bits(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-exact-parameter-time.json")).unwrap();
  for case in oracle["cases"].as_array().unwrap(){
   let origin=case["origin"].as_str().unwrap().parse::<i64>().unwrap()/254016;
   let track:KeyframeTrack<f64>=serde_json::from_value(serde_json::json!({"timeBase":1000000,"interpolation":case["nativeInterpolation"],"keyframes":[{"tick":origin,"value":5.0},{"tick":origin+1000000,"value":10.0}]})).unwrap();
   let at=case["at"].as_str().unwrap().parse::<i64>().unwrap();
   assert_eq!(AnimationSampleTime::FilmcraftSourceTicks(at).sample_track(&track).unwrap().to_bits(),case["bits"].as_u64().unwrap());
  }
 }
}
