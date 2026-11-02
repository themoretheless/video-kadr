//! Concat Decoder::next_paced source selection, without floating timestamp rounding.
//! The adapter supplies decoded presentation-order pictures and stream start ticks.
use anyhow::{ensure,Result};
use super::{media_probe::Rational,concat_cutout_clock::AnalysisRun};
#[derive(Debug,Clone,Copy)]
pub struct FrameTimestamp{pub ticks:i64,pub time_base:Rational,pub start_ticks:i64}
impl FrameTimestamp{
 fn clock(self,millis:u64)->Result<(i128,i128)>{
  ensure!(self.time_base.numerator>0&&self.time_base.denominator>0,"invalid decoded frame time base");
  let left=(i128::from(self.ticks)-i128::from(self.start_ticks)).checked_mul(i128::from(self.time_base.numerator)).and_then(|v|v.checked_mul(1000)).ok_or_else(||anyhow::anyhow!("decoded timestamp overflow"))?;
  let right=i128::from(millis).checked_mul(i128::from(self.time_base.denominator)).ok_or_else(||anyhow::anyhow!("analysis timestamp overflow"))?;Ok((left,right))
 }
 pub fn before_millis(self,millis:u64)->Result<bool>{let(a,b)=self.clock(millis)?;Ok(a<b)}
 fn at_or_before(self,millis:u64)->Result<bool>{let(a,b)=self.clock(millis)?;Ok(a<=b)}
 fn ended(self,millis:u64)->Result<bool>{let(a,b)=self.clock(millis)?;let end=a.checked_add(i128::from(self.time_base.denominator)*100).ok_or_else(||anyhow::anyhow!("frame horizon overflow"))?;Ok(end<=b)}
}
pub struct DecodedFrame<T>{pub timestamp:Option<FrameTimestamp>,pub picture:T}
pub struct SampledFrame<T>{pub source_millis:u64,pub source_ordinal:usize,pub picture:T}
pub struct VideoPacer<T,I>{input:I,run:AnalysisRun,tick:usize,next_ordinal:usize,current:Option<(usize,DecodedFrame<T>)>,pending:Option<(usize,DecodedFrame<T>)>,done:bool}
impl<T:Clone,I:Iterator<Item=Result<DecodedFrame<T>>>> VideoPacer<T,I>{
 pub fn new(input:I,run:AnalysisRun)->Result<Self>{ensure!(run.start_millis%100==0&&run.frames>0&&run.frames<=360000,"invalid analysis run");run.start_millis.checked_add((run.frames as u64-1)*100).ok_or_else(||anyhow::anyhow!("analysis run overflow"))?;Ok(Self{input,run,tick:0,next_ordinal:0,current:None,pending:None,done:false})}
 pub fn next_sample(&mut self)->Result<Option<SampledFrame<T>>>{
  if self.tick>=self.run.frames{return Ok(None)}let target=self.run.start_millis+self.tick as u64*100;
  loop{
   let ready=match self.pending.as_ref().and_then(|(_,f)|f.timestamp){None=>true,Some(time)=>time.at_or_before(target)?};
   if self.done||!ready{break}
   if let Some(frame)=self.pending.take(){self.current=Some(frame)}
   match self.input.next(){Some(frame)=>{self.pending=Some((self.next_ordinal,frame?));self.next_ordinal=self.next_ordinal.checked_add(1).ok_or_else(||anyhow::anyhow!("decoded ordinal overflow"))?},None=>self.done=true}
  }
  if self.current.is_none(){self.current=self.pending.take()}
  let Some((ordinal,frame))=&self.current else{return Ok(None)};
  if self.done&&self.pending.is_none()&&frame.timestamp.map(|time|time.ended(target)).transpose()?.unwrap_or(false){return Ok(None)}
  self.tick+=1;Ok(Some(SampledFrame{source_millis:target,source_ordinal:*ordinal,picture:frame.picture.clone()}))
 }
}
