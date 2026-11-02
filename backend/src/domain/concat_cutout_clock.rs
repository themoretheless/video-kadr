// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original MaskStore nearest-time semantics, driven by the native source clock.
use std::sync::Arc;
use anyhow::{ensure,Result};
use super::{composition::{ClipPlacement,PlaybackMode},concat_picture::Preparation,group_surface::GroupSurface};
const REACH_MILLIS:u64=250; // 2500 / original MASK_RATE(10)
#[derive(Debug,Clone)]
pub struct MaskTimeline{times:Vec<u64>}
impl MaskTimeline{
 pub fn new(mut source_millis:Vec<u64>)->Result<Self>{
  ensure!(source_millis.len()<=360000,"source mask inventory budget exceeded");
  source_millis.sort_unstable();source_millis.dedup();Ok(Self{times:source_millis})
 }
 pub fn nearest_millis(&self,seconds:f64)->Result<Option<u64>>{
  ensure!(seconds.is_finite(),"nonfinite source mask time");
  if self.times.len()==1{return Ok(self.times.first().copied())}
  let wanted=(seconds.max(0.)*1000.).round()as u64;
  let index=self.times.partition_point(|at|*at<wanted);
  let before=index.checked_sub(1).and_then(|i|self.times.get(i)).copied();
  let after=self.times.get(index).copied();
  Ok([before,after].into_iter().flatten().min_by_key(|at|at.abs_diff(wanted)).filter(|at|at.abs_diff(wanted)<=REACH_MILLIS))
 }
}
/// A caller-resolved, corrected and feathered probability plane. The provider
/// establishes source/model/region provenance; no paths are accepted here.
pub struct ResolvedMask{pub width:usize,pub height:usize,pub probabilities:Vec<u8>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Dispatch{
 Inactive,
 Unanalysed{source_us:u64},
 Missing{source_us:u64,mask_millis:u64},
 Applied{source_us:u64,mask_millis:u64},
}
/// Missing inventory/decoded masks preserve the original picture, matching
/// CutoutJob::cut(None). Provider errors propagate instead of inventing a mask.
pub fn apply_video(frame:&mut GroupSurface,placement:&ClipPlacement,mode:PlaybackMode,timeline_us:u64,times:&MaskTimeline,geometry:&Preparation,treated:bool,provider:&mut dyn FnMut(u64)->Result<Option<Arc<ResolvedMask>>>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Dispatch>{
 checkpoint()?;
 let Some(source_us)=super::video_clock::source_tick(placement,mode,timeline_us)?else{return Ok(Dispatch::Inactive)};
 let Some(mask_millis)=times.nearest_millis(source_us as f64/1000000.)?else{return Ok(Dispatch::Unanalysed{source_us})};
 let Some(mask)=provider(mask_millis)?else{return Ok(Dispatch::Missing{source_us,mask_millis})};
 super::concat_cutout_mask::apply_for_decode(frame,super::concat_cutout_mask::MaskPlane{width:mask.width,height:mask.height,probabilities:&mask.probabilities},geometry,treated,checkpoint)?;
 Ok(Dispatch::Applied{source_us,mask_millis})
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct AnalysisRun{pub start_millis:u64,pub frames:usize}
impl MaskTimeline {
 /// Original missing-grid semantics: floor the start, include the rounded-up end,
 /// deduplicate overlapping source ranges, then reset recurrence at each gap.
 pub fn missing_runs(&self,ranges:&[(f64,f64)])->Result<Vec<AnalysisRun>>{
  ensure!(ranges.len()<=512,"too many cutout source ranges");
  let mut missing=std::collections::BTreeSet::new();
  for &(from,to) in ranges{
   ensure!(from.is_finite()&&to.is_finite(),"nonfinite cutout source range");
   let first_ms=from.max(0.)*1000.;let last_ms=(to.max(from).max(0.)*1000.).ceil();
   ensure!(first_ms<u64::MAX as f64&&last_ms<u64::MAX as f64,"cutout source clock overflow");
   let first=(first_ms as u64)/100*100;let last=last_ms as u64;
   let count=(last-first)/100+1;ensure!(count<=360000,"cutout source range budget exceeded");
   for ordinal in 0..count{let at=first.checked_add(ordinal*100).ok_or_else(||anyhow::anyhow!("cutout grid overflow"))?;
    if self.times.binary_search(&at).is_err(){missing.insert(at);ensure!(missing.len()<=360000,"cutout analysis inventory budget exceeded");}
   }
  }
  let mut runs:Vec<AnalysisRun>=vec![];
  for at in missing{if let Some(run)=runs.last_mut(){if run.start_millis.checked_add(run.frames as u64*100)==Some(at){run.frames+=1;continue}}
   runs.push(AnalysisRun{start_millis:at,frames:1});
  }Ok(runs)
 }
}
