//! Bounded SO(3) track retiming and adaptive compression. Error is checked at
//! every supplied knot; it is not a continuous motion certificate between IMU samples.
use super::spherical_video::Orientation;
use anyhow::{ensure,Result};
const MAX_INPUT:usize=1_200_004;
fn dot(a:[f64;4],b:[f64;4])->f64{(0..4).map(|i|a[i]*b[i]).sum()}
fn normalized(q:[f64;4])->[f64;4]{let n=dot(q,q).sqrt();q.map(|v|v/n)}
pub fn slerp(a:[f64;4],mut b:[f64;4],t:f64)->[f64;4]{
 let a=normalized(a);b=normalized(b);let mut d=dot(a,b);if d<0.{b=b.map(|v|-v);d=-d;}
 if d>0.9995{return normalized(std::array::from_fn(|i|a[i]+t*(b[i]-a[i])))}
 let angle=d.clamp(-1.,1.).acos();let s=angle.sin();normalized(std::array::from_fn(|i|(a[i]*((1.-t)*angle).sin()+b[i]*(t*angle).sin())/s))
}
pub fn angular_error_degrees(a:[f64;4],b:[f64;4])->f64{(2.*dot(normalized(a),normalized(b)).abs().clamp(-1.,1.).acos()).to_degrees()}
fn validate(track:&[Orientation])->Result<()>{
 ensure!(!track.is_empty()&&track.len()<=MAX_INPUT,"orientation track size exceeds budget");
 for (i,s)in track.iter().enumerate(){ensure!(s.time_seconds.is_finite()&&s.time_seconds.abs()<=1e12&&(i==0||s.time_seconds>track[i-1].time_seconds)&&s.quaternion.iter().all(|v|v.is_finite())&&(0.99..=1.01).contains(&dot(s.quaternion,s.quaternion)),"invalid orientation track");}
 Ok(())
}
#[derive(Clone,Copy,Debug,PartialEq,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CaptureTime { pub time_seconds:f64, pub capture_seconds:f64 }
/// Shared piecewise-linear capture clock, with explicit bounded guard-band extrapolation.
pub fn padded_clock(targets:&[f64],sample_rate:f64,duration:f64,padding:f64)->Result<Vec<CaptureTime>> {
 ensure!(targets.len()>=2&&targets.len()<=20000&&sample_rate.is_finite()&&(1. ..=120.).contains(&sample_rate)&&duration.is_finite()&&duration>0.&&duration<=600.&&padding.is_finite()&&(0. ..=0.2).contains(&padding),"invalid capture clock");
 ensure!(duration*sample_rate<=19999.,"capture clock exceeds budget");
 ensure!((duration*sample_rate).ceil()as usize+1==targets.len()&&targets.iter().all(|v|v.is_finite()&&v.abs()<=1e12)&&targets.windows(2).all(|p|p[0]<p[1]),"invalid capture clock knots");
 let mut clock:Vec<_>=targets.iter().enumerate().map(|(i,v)|CaptureTime{time_seconds:(i as f64/sample_rate).min(duration)+padding,capture_seconds:*v}).collect();
 if padding>0. {
  let first=clock[0];let second=clock[1];let last=clock[clock.len()-1];let before=clock[clock.len()-2];
  clock.insert(0,CaptureTime{time_seconds:0.,capture_seconds:first.capture_seconds-padding*(second.capture_seconds-first.capture_seconds)/(second.time_seconds-first.time_seconds)});
  clock.push(CaptureTime{time_seconds:duration+2.*padding,capture_seconds:last.capture_seconds+padding*(last.capture_seconds-before.capture_seconds)/(last.time_seconds-before.time_seconds)});
 }
 Ok(clock)
}
pub fn capture_to_clip_track(fused:&[Orientation],targets:&[f64],sample_rate:f64,duration:f64)->Result<Vec<Orientation>>{capture_to_clip_track_padded(fused,targets,sample_rate,duration,0.)}
/// Invert the measured clip→capture clock without discarding native-rate motion.
/// Preserve both IMU and clock breakpoints; endpoints are interpolated exactly.
pub fn capture_to_clip_track_padded(fused:&[Orientation],targets:&[f64],sample_rate:f64,duration:f64,padding:f64)->Result<Vec<Orientation>> {
 let clock=padded_clock(targets,sample_rate,duration,padding)?;
 capture_to_clip_from_clock(fused,&clock)
}
/// Native-rate retiming against an already shifted, precise per-frame clock.
pub fn capture_to_clip_from_clock(fused:&[Orientation],clock:&[CaptureTime])->Result<Vec<Orientation>>{
 validate(fused)?;ensure!(fused.len()>=2,"too few native orientation samples");validate_clock(clock)?;
 let first=clock[0].capture_seconds;let last=clock[clock.len()-1].capture_seconds;
 ensure!(first>=fused[0].time_seconds-1e-10&&last<=fused[fused.len()-1].time_seconds+1e-10,"capture clock is not bracketed");
 let mut times:Vec<_>=fused.iter().map(|s|s.time_seconds).filter(|t|*t>first&&*t<last).chain(clock.iter().map(|s|s.capture_seconds)).collect();
 ensure!(times.len()<=MAX_INPUT,"retimed orientation exceeds budget");times.sort_by(f64::total_cmp);times.dedup_by(|a,b|(*a-*b).abs()<1e-10);
 if let Some(t)=times.first_mut(){*t=first;}if let Some(t)=times.last_mut(){*t=last;}
 let(mut fi,mut ti)=(0,0);let mut out=Vec::with_capacity(times.len());
 for capture in times {
  while fi+2<fused.len()&&fused[fi+1].time_seconds<capture{fi+=1;}
  while ti+2<clock.len()&&clock[ti+1].capture_seconds<capture{ti+=1;}
  let a=fused[fi];let b=fused[fi+1];let q=slerp(a.quaternion,b.quaternion,((capture-a.time_seconds)/(b.time_seconds-a.time_seconds)).clamp(0.,1.));
  let(ca,cb)=(clock[ti],clock[ti+1]);
  let time=if capture==first{clock[0].time_seconds}else if capture==last{clock[clock.len()-1].time_seconds}else{ca.time_seconds+(cb.time_seconds-ca.time_seconds)*(capture-ca.capture_seconds)/(cb.capture_seconds-ca.capture_seconds)};
  out.push(Orientation{time_seconds:time,quaternion:q});
 }
 validate(&out)?;Ok(out)
}
fn validate_clock(clock:&[CaptureTime])->Result<()>{
 ensure!(clock.len()>=2&&clock.len()<=145000,"invalid capture clock size");
 for(i,k)in clock.iter().enumerate(){ensure!(k.time_seconds.is_finite()&&k.time_seconds>=-0.2&&k.time_seconds<=1e9&&k.capture_seconds.is_finite()&&k.capture_seconds.abs()<=1e12&&(i==0||(k.time_seconds>clock[i-1].time_seconds&&k.capture_seconds>clock[i-1].capture_seconds)),"invalid capture clock knots");}Ok(())
}
/// Add bounded guard knots and shift clip-relative time to nonnegative storage.
/// If the caller knows real exposure events in the guard interval, include them
/// before retiming instead of substituting this affine extension.
pub fn pad_clock_knots(clock:&[CaptureTime],padding:f64)->Result<Vec<CaptureTime>>{
 validate_clock(clock)?;ensure!(clock[0].time_seconds==0.&&padding.is_finite()&&(0. ..=0.2).contains(&padding),"invalid clock padding");
 let mut out:Vec<_>=clock.iter().map(|k|CaptureTime{time_seconds:k.time_seconds+padding,capture_seconds:k.capture_seconds}).collect();
 if padding>0.{let(a,b)=(out[0],out[1]);let(c,d)=(out[out.len()-2],out[out.len()-1]);out.insert(0,CaptureTime{time_seconds:0.,capture_seconds:a.capture_seconds-padding*(b.capture_seconds-a.capture_seconds)/(b.time_seconds-a.time_seconds)});out.push(CaptureTime{time_seconds:d.time_seconds+padding,capture_seconds:d.capture_seconds+padding*(d.capture_seconds-c.capture_seconds)/(d.time_seconds-c.time_seconds)});}
 validate_clock(&out)?;Ok(out)
}
/// Compress a piecewise-linear clock with a physical-time error bound. Since
/// both clocks are affine between knots, the knot bound covers each interval.
pub fn compress_clock(clock:&[CaptureTime],tolerance_seconds:f64)->Result<Vec<CaptureTime>>{
 validate_clock(clock)?;ensure!(tolerance_seconds.is_finite()&&(1e-10..=1e-3).contains(&tolerance_seconds),"invalid clock tolerance");
 let mut keep=vec![false;clock.len()];keep[0]=true;keep[clock.len()-1]=true;let mut stack=vec![(0,clock.len()-1)];let mut work=0usize;
 while let Some((a,b))=stack.pop(){let mut worst=tolerance_seconds;let mut split=None;for i in a+1..b{work+=1;ensure!(work<=100_000_000,"capture clock compression work budget exceeded");let fraction=(clock[i].time_seconds-clock[a].time_seconds)/(clock[b].time_seconds-clock[a].time_seconds);let estimate=clock[a].capture_seconds+fraction*(clock[b].capture_seconds-clock[a].capture_seconds);let err=(estimate-clock[i].capture_seconds).abs();if err>worst{worst=err;split=Some(i)}}if let Some(worst)=split{let i=if worst-a<(b-a)/4||b-worst<(b-a)/4{a+(b-a)/2}else{worst};keep[i]=true;stack.push((a,i));stack.push((i,b));}}
 Ok(clock.iter().zip(keep).filter_map(|(k,yes)|yes.then_some(*k)).collect())
}
/// Quaternion Ramer-Douglas-Peucker with a hard work and output budget. Complex
/// footage fails explicitly instead of silently discarding rolling-shutter motion.
pub fn compress(track:&[Orientation],tolerance_degrees:f64,max_samples:usize)->Result<Vec<Orientation>>{
 validate(track)?;ensure!(tolerance_degrees.is_finite()&&(0.000001..=1.).contains(&tolerance_degrees)&&max_samples>=2&&max_samples<=20000,"invalid orientation compression parameters");
 if track.len()<3{return Ok(track.to_vec())}
 let mut keep=vec![false;track.len()];keep[0]=true;keep[track.len()-1]=true;let mut count=2;let mut stack=vec![(0,track.len()-1)];let mut work=0usize;
 while let Some((a,b))=stack.pop(){
  if b<=a+1{continue}let mut worst=tolerance_degrees;let mut split=None;
  for i in a+1..b{
   work+=1;ensure!(work<=100_000_000,"orientation compression work budget exceeded");
   let t=(track[i].time_seconds-track[a].time_seconds)/(track[b].time_seconds-track[a].time_seconds);
   let error=angular_error_degrees(slerp(track[a].quaternion,track[b].quaternion,t),track[i].quaternion);
   if error>worst{worst=error;split=Some(i)}
  }
  if let Some(i)=split{keep[i]=true;count+=1;ensure!(count<=max_samples,"native motion exceeds orientation storage budget; shorten clip or increase tolerance");stack.push((a,i));stack.push((i,b));}
 }
 Ok(track.iter().zip(keep).filter_map(|(s,keep)|keep.then_some(Orientation{time_seconds:s.time_seconds,quaternion:normalized(s.quaternion)})).collect())
}
