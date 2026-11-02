use filmcraft_project::{Project,Sequence,ItemId,ClipId,TrackId};use filmcraft_time::{Tick,TimeRange};use serde_json::{Value,json};use filmcraft_edit::EditCtx;
struct State{selection:Vec<ClipId>,linked_selection:bool}
struct Targeting{targeted:Vec<TrackId>}
struct Session{project:Project,sequence:ItemId,state:State,targets:Vec<TrackId>,playhead:Tick}
#[derive(Debug)]enum EngineError{NoSequence,Other(String)}
type Result<T>=std::result::Result<T,EngineError>;
mod original{use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
mod commands{use super::*;
__WITH_LINKS__
}
impl Session{
 fn active_sequence(&self)->Option<&Sequence>{self.project.sequence(self.sequence)}
 fn targeting(&self)->Targeting{Targeting{targeted:self.targets.clone()}}
 fn playhead(&self)->Tick{self.playhead}
 fn set_playhead(&mut self,t:Tick){self.playhead=t;}
 fn edit_sequence<R>(&mut self,_label:&str,f:impl FnOnce(&mut Sequence,&mut EditCtx,&mut State)->Result<R>)->Result<R>{
  let before=self.project.clone();let mut work=before.clone();let rate=before.sequence(self.sequence).ok_or(EngineError::NoSequence)?.settings.frame_rate;
  let duration=|id|original::media_duration(&before,&(),id);let start=|id|original::media_start(&before,id);let mut next=before.next_id;let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:rate.frame_duration()};
  let result=f(work.sequence_mut(self.sequence).unwrap(),&mut ctx,&mut self.state)?;work.next_id=next;self.project=work;Ok(result)
 }
}
__TO_PLAYHEAD__
fn main(){let source:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for (source_case,case) in source["cases"].as_array().unwrap().iter().enumerate().step_by(216){for selected in [vec![],vec![1001],vec![1002],vec![9999]]{for targets in [vec![],vec![3],vec![4,5]]{for ph in [0,508032000000,762048000000,1016064000000]{for next in [false,true]{for ripple in [false,true]{for linked in [false,true]{
 let mut s=Session{project:serde_json::from_value(case["document"]["project"].clone()).unwrap(),sequence:ItemId(case["sequence"].as_u64().unwrap()),state:State{selection:selected.iter().copied().map(ClipId).collect(),linked_selection:linked},targets:targets.iter().copied().map(TrackId).collect(),playhead:Tick(ph)};
 let result=to_playhead(&mut s,&json!({"side":if next{"next"}else{"previous"},"ripple":ripple}));cases.push(json!({"sourceCase":source_case,"selected":selected,"targeted":targets,"playhead":ph,"next":next,"ripple":ripple,"linked":linked,"error":result.is_err(),"result":result.ok(),"expectedPlayhead":s.playhead.0,"expectedProject":s.project}));
}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
