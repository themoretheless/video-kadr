//! Exact presentation-order lookup. Decoder frame ordinals never travel through floats.
use anyhow::{ensure,Context,Result};
use serde_json::Value;
const T:i128=254_016_000_000;
#[derive(Debug)]pub struct VideoFrameIndex{pub num:i64,pub den:i64,pub origin:i64,pub width:u32,pub height:u32,pub color:filmcraft_color::ColorInfo,pts:Vec<i64>,end:i64}
fn integer(v:&Value)->Option<i64>{v.as_i64().or_else(||v.as_str()?.parse().ok())}
impl VideoFrameIndex{
 pub fn from_probe(probe:&Value)->Result<Self>{
  let stream=probe["streams"].as_array().and_then(|v|v.first()).context("missing video stream")?;
  let (num,den)=stream["time_base"].as_str().and_then(|s|s.split_once('/')).context("missing video time base")?;let num=num.parse::<i64>()?;let den=den.parse::<i64>()?;ensure!(num>0&&den>0,"invalid video time base");
  let width=u32::try_from(integer(&stream["width"]).context("missing video width")?)?;let height=u32::try_from(integer(&stream["height"]).context("missing video height")?)?;ensure!(width>0&&height>0&&width<=32768&&height<=32768,"invalid video dimensions");
  let frames=probe["frames"].as_array().context("missing video frames")?;ensure!(!frames.is_empty()&&frames.len()<=32768,"video index frame budget exceeded");
  let pts=frames.iter().map(|f|integer(&f["best_effort_timestamp"]).context("missing video PTS")).collect::<Result<Vec<_>>>()?;ensure!(pts.windows(2).all(|w|w[0]<=w[1]),"video presentation order is not monotonic");
  let origin=integer(&stream["start_pts"]).unwrap_or(pts[0]);
  let last=frames.last().unwrap();let duration=integer(&last["duration"]).filter(|v|*v>0).or_else(||integer(&last["pkt_duration"]).filter(|v|*v>0));
  let end=if let Some(duration)=duration{pts.last().unwrap().checked_add(duration)}else{integer(&stream["duration_ts"]).filter(|v|*v>0).and_then(|d|origin.checked_add(d))}.context("missing video end clock")?;ensure!(end>*pts.last().unwrap(),"invalid video end clock");
  Ok(Self{num,den,origin,width,height,color:probe_color(stream)?,pts,end})
 }
 pub fn select(&self,media_ticks:i64)->Result<Option<usize>>{
  let numerator=(media_ticks as i128).checked_mul(self.den as i128).context("video query overflow")?;let denominator=T*self.num as i128;
  let target=numerator.div_euclid(denominator).checked_add(self.origin as i128).context("video query overflow")?;
  if target<self.pts[0] as i128||target>=self.end as i128{return Ok(None)}
  Ok(self.pts.partition_point(|p|(*p as i128)<=target).checked_sub(1))
 }
 pub fn pts(&self,ordinal:usize)->Result<i64>{self.pts.get(ordinal).copied().context("invalid video frame ordinal")}
 pub fn tick_floor(&self,ordinal:usize)->Result<i64>{let delta=self.pts(ordinal)? as i128-self.origin as i128;let n=delta.checked_mul(self.num as i128).and_then(|v|v.checked_mul(T)).context("video PTS overflow")?;i64::try_from(n.div_euclid(self.den as i128)).context("video PTS outside project clock")}
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::json;
 #[test]fn variable_frame_boundaries_are_exact_even_above_js_integer_precision(){
  let origin=9_007_199_254_740_993i64;let index=VideoFrameIndex::from_probe(&json!({"streams":[{"width":8,"height":4,"time_base":"1/1000","start_pts":origin}],"frames":[{"best_effort_timestamp":origin,"pkt_duration":40},{"best_effort_timestamp":origin+80,"pkt_duration":40},{"best_effort_timestamp":origin+200,"pkt_duration":40}]})).unwrap();
  for (tick,want) in [(-1,None),(0,Some(0)),(20_321_280_000-1,Some(0)),(20_321_280_000,Some(1)),(50_803_200_000-1,Some(1)),(50_803_200_000,Some(2)),(60_963_840_000,None)]{assert_eq!(index.select(tick).unwrap(),want);}
  assert_eq!(index.pts(1).unwrap(),origin+80);assert_eq!(index.tick_floor(1).unwrap(),20_321_280_000);
 }
}

/// Decoder conversion to RGBA expands range and resolves YUV matrix, but leaves
/// transfer and primaries intact. Preserve them for the managed input stage.
fn probe_color(stream:&Value)->Result<filmcraft_color::ColorInfo>{
 use filmcraft_color::{ColorInfo,Matrix,Transfer,Primaries,Range};
 let field=|name:&str|stream[name].as_str().filter(|s|!matches!(*s,"unknown"|"unspecified"));
 let transfer=match field("color_transfer"){None|Some("bt709"|"smpte170m"|"bt2020-10"|"bt2020-12")=>Transfer::Bt709,Some("iec61966-2-1")=>Transfer::Srgb,Some("linear")=>Transfer::Linear,Some("smpte2084")=>Transfer::Pq,Some("arib-std-b67")=>Transfer::Hlg,Some(v)=>anyhow::bail!("unsupported video transfer {v}")};
 let primaries=match field("color_primaries"){None|Some("bt709")=>Primaries::Bt709,Some("bt470bg")=>Primaries::Bt601_625,Some("smpte170m"|"smpte240m")=>Primaries::Bt601_525,Some("bt2020")=>Primaries::Bt2020,Some("smpte432")=>Primaries::P3D65,Some(v)=>anyhow::bail!("unsupported video primaries {v}")};
 Ok(ColorInfo{matrix:Matrix::Bt709,transfer,primaries,range:Range::Full})
}
#[cfg(test)]mod color_tests{
 use super::*;use serde_json::json;use filmcraft_color::{Primaries,Transfer,Range};
 #[test]fn decoder_rgb_metadata_preserves_transfer_and_primaries(){
  for(trc,transfer)in [("bt709",Transfer::Bt709),("iec61966-2-1",Transfer::Srgb),("linear",Transfer::Linear),("smpte2084",Transfer::Pq),("arib-std-b67",Transfer::Hlg)]{let c=probe_color(&json!({"color_transfer":trc,"color_primaries":"bt2020","color_range":"tv"})).unwrap();assert_eq!(c.transfer,transfer);assert_eq!(c.primaries,Primaries::Bt2020);assert_eq!(c.range,Range::Full);}
  assert!(probe_color(&json!({"color_transfer":"unsupported"})).is_err());assert!(probe_color(&json!({"color_primaries":"unsupported"})).is_err());assert_eq!(probe_color(&json!({})).unwrap().transfer,Transfer::Bt709);
 }
}
