use std::sync::Arc;
use filmcraft_project::{Project,Sequence,ItemId,ClipId};use filmcraft_time::Tick;use serde::Serialize;use serde_json::{Value,json};use filmcraft_edit::{EditCtx,Edge,TrimMode};
#[derive(Clone,Copy,PartialEq,Eq,Serialize)]#[serde(rename_all="camelCase")]enum TrimKind{Trim,Ripple,Roll}
#[derive(Clone,Copy,Serialize)]struct EditPoint{clip:ClipId,out:bool,kind:TrimKind}
#[derive(Clone,Copy,Debug,Default,Serialize)]struct TrimShift{out_shift:Tick,in_shift:Tick}
struct State{edit_points:Vec<EditPoint>,trim_shift:TrimShift}
struct Session{project:Arc<Project>,sequence:ItemId,state:State,linked:bool,playhead:Tick,trim_play:TrimPlayback,history:History,journal:Vec<Value>,revision:u64,events:Vec<Event>,prefs:Preferences}
#[derive(Debug)]enum EngineError{NoSequence,Other(String)}
type Result<T>=std::result::Result<T,EngineError>;
mod original{use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
impl Session{
 fn set_playhead(&mut self,t:Tick){self.playhead=t;}
 fn sequence_rate(&self)->filmcraft_time::FrameRate{self.active_sequence().unwrap().settings.frame_rate}
 fn fix_state(&mut self){}
 fn playhead(&self)->Tick{self.playhead}
 fn active_sequence(&self)->Option<&Sequence>{self.project.sequence(self.sequence)}
 fn execute(&mut self,cmd:&str,p:Value)->Result<Value>{
  let before=(*self.project).clone();let mut work=before.clone();let rate=before.sequence(self.sequence).ok_or(EngineError::NoSequence)?.settings.frame_rate;
  let duration=|id|original::media_duration(&before,&(),id);let start=|id|original::media_start(&before,id);let mut next=before.next_id;let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:rate.frame_duration()};
  let result=if cmd=="timeline.roll"{filmcraft_edit::roll(work.sequence_mut(self.sequence).unwrap(),ClipId(p["left"].as_u64().unwrap()),ClipId(p["right"].as_u64().unwrap()),Tick(p["delta"].as_i64().unwrap()),&mut ctx)}else{
   let clip=ClipId(p["clip"].as_u64().unwrap());let edge=if p["edge"]=="in"{Edge::In}else{Edge::Out};let mode=if p["mode"]=="ripple"{TrimMode::Ripple}else{TrimMode::Regular};let mut ids=vec![clip];
   if self.linked{let q=before.sequence(self.sequence).unwrap();if let Some(link)=q.find_item(clip).and_then(|(_,it)|it.link){for track in q.all_tracks(){for item in &track.items{if item.link==Some(link)&&!ids.contains(&item.id){ids.push(item.id);}}}}}
   (||->filmcraft_edit::Result<Tick>{let q=work.sequence_mut(self.sequence).unwrap();let mut d=Tick(p["delta"].as_i64().unwrap());for id in &ids{let x=filmcraft_edit::clamp_trim(q,*id,edge,mode,d,&ctx)?;if x.abs()<d.abs(){d=x;}}
    if mode==TrimMode::Ripple{d=filmcraft_edit::ripple_trim_group(q,&ids,edge,d,&mut ctx)?;}else{for id in &ids{filmcraft_edit::trim(q,*id,edge,mode,d,&mut ctx)?;}}Ok(d)})()
  };
  let d=result.map_err(|e|EngineError::Other(e.to_string()))?;work.next_id=next;self.project=Arc::new(work);Ok(json!({"delta":d.0}))
 }
}
__EDIT_TIME__
__HELPERS__
__RAW__
__TRIM_ALL__
__ADD_SHIFT__
__EXTEND__

struct History{undo:Vec<(String,Arc<Project>)>,redo:Vec<(String,Arc<Project>)>,limit:usize}
enum Event{ProjectChanged{revision:u64}}
__DYNAMIC__

struct Preferences{trim:TrimPreferences,playback:PlaybackPreferences}
struct TrimPreferences{playhead_determines_loop:bool}
struct PlaybackPreferences{preroll_seconds:f64,postroll_seconds:f64}
__AROUND__
fn main(){let source:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for (source_case,case) in source["cases"].as_array().unwrap().iter().enumerate().step_by(216){for selected in [false,true]{for center_playhead in [false,true]{for looping in [false,true]{for (pre,post) in [(0.0,0.0),(0.5,1.0),(10.0,10.0)]{for dynamic_first in [false,true]{
 let points=if selected{vec![EditPoint{clip:ClipId(1001),out:true,kind:TrimKind::Trim}]}else{vec![]};let mut s=Session{project:Arc::new(serde_json::from_value(case["document"]["project"].clone()).unwrap()),sequence:ItemId(case["sequence"].as_u64().unwrap()),state:State{edit_points:points.clone(),trim_shift:TrimShift::default()},linked:true,playhead:Tick(762048000000),trim_play:TrimPlayback::default(),history:History{undo:vec![],redo:vec![],limit:32},journal:vec![],revision:0,events:vec![],prefs:Preferences{trim:TrimPreferences{playhead_determines_loop:center_playhead},playback:PlaybackPreferences{preroll_seconds:pre,postroll_seconds:post}}};
 let mut actions=vec![];if dynamic_first{actions.extend([json!({"op":"shuttle","clock":0.0}),json!({"op":"tick","clock":0.1})]);}actions.extend([json!({"op":"around","clock":1.0,"loop":looping}),json!({"op":"tick","clock":1.2}),json!({"op":"tick","clock":100.0}),json!({"op":"around","clock":101.0,"toggle":true}),json!({"op":"stop"})]);let mut steps=vec![];
 for action in &actions{let result=match action["op"].as_str().unwrap(){"shuttle"=>shuttle(&mut s,action),"tick"=>tick(&mut s,action["clock"].as_f64().unwrap()),"around"=>play_around(&mut s,action),_=>stop(&mut s,action)};steps.push(json!({"action":action,"error":result.is_err(),"result":result.ok(),"project":&*s.project,"shift":s.state.trim_shift,"status":status(&s),"undoCount":s.history.undo.len()}));}
 cases.push(json!({"sourceCase":source_case,"points":points,"linked":true,"initialPlayhead":762048000000i64,"centerPlayhead":center_playhead,"pre":pre,"post":post,"steps":steps}));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
