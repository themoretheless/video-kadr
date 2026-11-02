//! Source-row rolling-shutter timing. Capture-clock inversion is shared with the
//! IMU retiming; output panorama rows never stand in for physical sensor rows.
use super::{orientation_track::{CaptureTime,slerp},spherical_video::Orientation};
use anyhow::{ensure,Context,Result};
use serde::{Deserialize,Serialize};
/// Proven ordinary source presentation clock; independent of output/composition FPS.
#[derive(Clone,Copy,Debug,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SourceFrameClock {pub numerator:u32,pub denominator:u32,pub source_start_seconds:f64}
impl SourceFrameClock {
 pub fn validate(&self)->Result<()>{ensure!(self.numerator>0&&self.denominator>0&&(1. ..=240.).contains(&(self.numerator as f64/self.denominator as f64))&&self.source_start_seconds.is_finite()&&(0. ..=1e9).contains(&self.source_start_seconds),"invalid source presentation clock");Ok(())}
 pub fn presentation_time(&self,clip_time:f64)->Result<f64>{self.validate()?;ensure!(clip_time.is_finite()&&clip_time>=0.&&clip_time<=1e9,"invalid source clip time");let source=self.source_start_seconds+clip_time;let index=(source*self.numerator as f64/self.denominator as f64+1e-9).floor();Ok(index*self.denominator as f64/self.numerator as f64-self.source_start_seconds)}
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ScanDirection{#[default]TopToBottom,BottomToTop}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct RollingShutter{
 pub readout_seconds:f64,
 pub reference_row:f64,
 pub scan_directions:[ScanDirection;2],
 /// Orientation-clock → physical capture seconds, including guard bands.
 pub capture_clock:Vec<CaptureTime>,
}
impl Default for RollingShutter{fn default()->Self{Self{readout_seconds:0.,reference_row:0.5,scan_directions:[ScanDirection::TopToBottom;2],capture_clock:vec![]}}}
impl RollingShutter{
 pub fn validate(&self)->Result<()>{
  ensure!(self.readout_seconds.is_finite()&&(0. ..=0.1).contains(&self.readout_seconds)&&self.reference_row.is_finite()&&(0. ..=1.).contains(&self.reference_row),"invalid sensor readout timing");
  ensure!(self.capture_clock.len()<=145000&&self.capture_clock.len()!=1,"invalid rolling shutter capture clock size");
  for(i,s)in self.capture_clock.iter().enumerate(){ensure!(s.time_seconds.is_finite()&&s.time_seconds>=0.&&s.capture_seconds.is_finite()&&s.capture_seconds.abs()<=1e12&&(i==0||(s.time_seconds>self.capture_clock[i-1].time_seconds&&s.capture_seconds>self.capture_clock[i-1].capture_seconds)),"rolling shutter clock must increase");}
  Ok(())
 }
 fn capture_at(&self,time:f64)->Result<f64>{
  if self.capture_clock.is_empty(){return Ok(time)}
  let c=&self.capture_clock;ensure!(time>=c[0].time_seconds-1e-10&&time<=c[c.len()-1].time_seconds+1e-10,"rolling shutter midpoint outside capture clock");
  let i=c.partition_point(|s|s.time_seconds<time).clamp(1,c.len()-1);let(a,b)=(c[i-1],c[i]);Ok(a.capture_seconds+(b.capture_seconds-a.capture_seconds)*(time-a.time_seconds)/(b.time_seconds-a.time_seconds))
 }
 fn time_at_capture(&self,capture:f64)->Result<f64>{
  if self.capture_clock.is_empty(){return Ok(capture)}
  let c=&self.capture_clock;ensure!(capture>=c[0].capture_seconds-1e-10&&capture<=c[c.len()-1].capture_seconds+1e-10,"rolling shutter requires more IMU guard-band data");
  let i=c.partition_point(|s|s.capture_seconds<capture).clamp(1,c.len()-1);let(a,b)=(c[i-1],c[i]);Ok(a.time_seconds+(b.time_seconds-a.time_seconds)*(capture-a.capture_seconds)/(b.capture_seconds-a.capture_seconds))
 }
 pub fn row_time(&self,reference_time:f64,row:f64,lens:usize)->Result<f64>{
  ensure!(reference_time.is_finite()&&row.is_finite()&&(0. ..=1.).contains(&row)&&lens<2,"invalid source row");
  let sign=if self.scan_directions[lens]==ScanDirection::TopToBottom{1.}else{-1.};
  self.time_at_capture(self.capture_at(reference_time)?+sign*(row-self.reference_row)*self.readout_seconds)
 }
}
pub(crate) struct RollingFrame{rotations:[Vec<[f64;4]>;2]}
fn multiply(a:[f64;4],b:[f64;4])->[f64;4]{[a[0]*b[0]-a[1]*b[1]-a[2]*b[2]-a[3]*b[3],a[0]*b[1]+a[1]*b[0]+a[2]*b[3]-a[3]*b[2],a[0]*b[2]-a[1]*b[3]+a[2]*b[0]+a[3]*b[1],a[0]*b[3]+a[1]*b[2]-a[2]*b[1]+a[3]*b[0]]}
fn pose(track:&[Orientation],time:f64)->Result<[f64;4]>{
 ensure!(track.len()>=2&&time>=track[0].time_seconds-1e-10&&time<=track[track.len()-1].time_seconds+1e-10,"rolling shutter orientation does not cover sensor readout");
 let i=track.partition_point(|s|s.time_seconds<time).clamp(1,track.len()-1);let(a,b)=(track[i-1],track[i]);Ok(slerp(a.quaternion,b.quaternion,((time-a.time_seconds)/(b.time_seconds-a.time_seconds)).clamp(0.,1.)))
}
impl RollingFrame{
 pub(crate) fn prepare(config:&RollingShutter,track:&[Orientation],reference_time:f64,height:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Self>{
  config.validate()?;ensure!(height>1&&height<=32768,"rolling shutter row table exceeds budget");
  let midpoint=pose(track,reference_time)?;let mut rotations=[Vec::with_capacity(height),Vec::with_capacity(height)];
  for(lens,table)in rotations.iter_mut().enumerate(){for row in 0..height{if row%128==0{checkpoint()?;}let time=config.row_time(reference_time,row as f64/(height-1)as f64,lens)?;let q=pose(track,time).context("import gyro with readout guard bands before enabling rolling shutter")?;table.push(multiply([q[0],-q[1],-q[2],-q[3]],midpoint));}}
  Ok(Self{rotations})
 }
 pub(crate) fn rotation(&self,lens:usize,row:f64)->[f64;4]{let table=&self.rotations[lens];let at=row.clamp(0.,1.)*(table.len()-1)as f64;let i=(at.floor()as usize).min(table.len()-2);slerp(table[i],table[i+1],at-i as f64)}
}
