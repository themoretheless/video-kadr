//! Native transient J/K/L trim controller. Documents remain lossless source projects.
use super::{filmcraft_project_file::SourceProjectDocument,filmcraft_trim::{self,TrimEditPoint,TrimKind,TrimShift}};
use anyhow::{ensure,Context,Result};
use filmcraft_time::{FrameRate,Tick};
use serde_json::{json,Value};
#[derive(Clone)]
struct Dynamic { before:SourceProjectDocument,shift_before:TrimShift,offset:i64,speed:f64,anchor_clock:f64,anchor_offset:i64,at_limit:bool }
#[derive(Clone)]
struct PlayAround{start:i64,end:i64,looping:bool,clock0:f64}
#[derive(Clone)]
pub struct DynamicTrimSession {
 pub document:SourceProjectDocument,pub sequence:u64,pub points:Vec<TrimEditPoint>,pub linked:bool,pub shift:TrimShift,pub playhead:i64,
 dynamic:Option<Dynamic>,around:Option<PlayAround>,pub undo:Vec<(String,SourceProjectDocument)>,pub redo:Vec<(String,SourceProjectDocument)>,pub history_limit:usize,
}
impl DynamicTrimSession {
 /// Serialized bytes retained by current document and every owned snapshot.
 pub fn retained_bytes(&self)->Result<usize>{
  let size=|document:&SourceProjectDocument|->Result<usize>{Ok(serde_json::to_vec(&document.envelope)?.len())};let mut bytes=size(&self.document)?;
  if let Some(dynamic)=&self.dynamic{bytes=bytes.checked_add(size(&dynamic.before)?).context("trim snapshot byte overflow")?;}
  for (_,document) in self.undo.iter().chain(self.redo.iter()){bytes=bytes.checked_add(size(document)?).context("trim snapshot byte overflow")?;}Ok(bytes)
 }
 pub fn new(document:SourceProjectDocument,sequence:u64,points:Vec<TrimEditPoint>,linked:bool)->Result<Self>{
  ensure!(document.envelope.pointer(&format!("/project/items/{sequence}/kind/Sequence")).is_some(),"missing dynamic trim sequence");
  Ok(Self{document,sequence,points,linked,shift:TrimShift::default(),playhead:0,dynamic:None,around:None,undo:vec![],redo:vec![],history_limit:32})
 }
 fn rate(&self)->Result<FrameRate>{serde_json::from_value(self.document.envelope.pointer(&format!("/project/items/{}/kind/Sequence/settings/frame_rate",self.sequence)).context("missing dynamic trim rate")?.clone()).context("invalid dynamic trim rate")}
 fn follow_edit(&mut self)->Result<()>{if let Some(point)=self.points.first(){let context=filmcraft_trim::edit_point_context(&self.document,self.sequence,point.clip,point.edge)?;if let Some(time)=context["editTime"].as_i64(){self.playhead=time;}}Ok(())}
 pub fn around_status(&self)->Option<Value>{self.around.as_ref().map(|a|json!({"startTicks":a.start.to_string(),"endTicks":a.end.to_string(),"loop":a.looping}))}
 pub fn status(&self)->Result<Value>{let rate=self.rate()?;Ok(json!({"playing":self.dynamic.is_some()||self.around.is_some(),"dynamic":self.dynamic.as_ref().map(|d|json!({"offsetFrames":rate.frame_at(Tick(d.offset)),"speed":d.speed,"atLimit":d.at_limit})),"playhead":self.playhead}))}
 pub fn shuttle(&mut self,clock:f64,direction:f64,slow:bool)->Result<Value>{
  ensure!(clock.is_finite()&&direction.is_finite(),"non-finite dynamic trim clock or direction");let dir=if direction<0.0{-1.0}else{1.0};self.around=None;
  if self.dynamic.is_some(){self.tick(clock)?;}else{self.dynamic=Some(Dynamic{before:self.document.clone(),shift_before:self.shift,offset:0,speed:0.0,anchor_clock:clock,anchor_offset:0,at_limit:false});}
  let rate=self.rate()?;let d=self.dynamic.as_mut().unwrap();let speed=if slow{0.25*dir}else if d.speed*dir>0.0&&!d.at_limit{(d.speed*2.0).clamp(-8.0,8.0)}else{dir};d.speed=speed;d.anchor_clock=clock;d.anchor_offset=d.offset;d.at_limit=false;Ok(json!({"speed":speed,"offsetFrames":rate.frame_at(Tick(d.offset))}))
 }
 pub fn tick(&mut self,clock:f64)->Result<Value>{
  ensure!(clock.is_finite(),"non-finite dynamic trim clock");if let Some(d)=self.dynamic.clone(){let secs=(clock-d.anchor_clock).max(0.0)*d.speed;ensure!(secs.is_finite(),"dynamic trim clock overflow");let rate=self.rate()?;let magnitude=rate.frame_at(Tick::from_seconds_f64(secs.abs()));let travel=rate.tick_of(if secs<0.0{-magnitude}else{magnitude}).0;let want=d.anchor_offset.checked_add(travel).context("dynamic trim offset overflow")?;
   if want!=d.offset{let got=self.apply_offset(want)?;let current=self.dynamic.as_mut().unwrap();current.offset=got;if got!=want{current.at_limit=true;current.speed=0.0;current.anchor_offset=got;current.anchor_clock=clock;}}
  }else if let Some(a)=self.around.clone(){let len=((a.end-a.start) as f64/254016000000.0).max(1e-9);let mut elapsed=(clock-a.clock0).max(0.0);if a.looping{elapsed%=len;}else if elapsed>=len{self.around=None;self.playhead=a.end.checked_sub(self.rate()?.frame_duration().0).context("play-around clock overflow")?;return self.status();}self.playhead=a.start.checked_add(Tick::from_seconds_f64(elapsed).0).context("play-around clock overflow")?;}self.status()
 }
 fn apply_offset(&mut self,offset:i64)->Result<i64>{
  let Some(d)=self.dynamic.clone()else{return Ok(0)};self.document=d.before.clone();
  let result=if offset==0{Ok(vec![0])}else{filmcraft_trim::trim_edit_points(&mut self.document,self.sequence,&self.points,offset,self.linked)};
  let applied=match result{Ok(a)=>a.first().copied().unwrap_or(0),Err(_)=>{self.document=d.before.clone();if d.offset!=0{let _=filmcraft_trim::trim_edit_points(&mut self.document,self.sequence,&self.points,d.offset,self.linked);}d.offset}};
  self.shift=filmcraft_trim::trim_shift_after(d.shift_before,self.points.first().copied(),&[applied])?;self.follow_edit()?;Ok(applied)
 }
 pub fn stop(&mut self,clock:Option<f64>)->Result<Value>{if let Some(clock)=clock{if self.dynamic.is_some(){self.tick(clock)?;}}self.around=None;self.commit()}
 pub fn commit(&mut self)->Result<Value>{let rate=self.rate()?;let Some(d)=self.dynamic.take()else{return Ok(json!({"committed":false}))};if d.offset==0{self.document=d.before;self.shift=d.shift_before;return Ok(json!({"committed":false,"offsetFrames":0}));}
  let label=match self.points.first().map(|p|p.kind){Some(TrimKind::Roll)=>"Dynamic Rolling Edit",Some(TrimKind::Ripple)=>"Dynamic Ripple Trim",_=>"Dynamic Trim"};self.undo.push((label.into(),d.before));if self.undo.len()>self.history_limit{self.undo.remove(0);}self.redo.clear();Ok(json!({"committed":true,"label":label,"offsetFrames":rate.frame_at(Tick(d.offset)),"offset":d.offset}))
 }
 pub fn play_around(&mut self,clock:f64,looping:bool,toggle:bool,center_playhead:bool,pre:f64,post:f64)->Result<Value>{
  ensure!(clock.is_finite()&&pre.is_finite()&&post.is_finite(),"non-finite play-around preferences");if self.dynamic.is_some(){self.commit()?;}if self.around.is_some()&&toggle{self.around=None;return self.status();}
  let edit=if let Some(p)=self.points.first(){filmcraft_trim::edit_point_context(&self.document,self.sequence,p.clip,p.edge)?["editTime"].as_i64()}else{None};let center=if center_playhead{self.playhead}else{edit.unwrap_or(self.playhead)};let rate=self.rate()?;
  let seq=self.document.envelope.pointer(&format!("/project/items/{}/kind/Sequence",self.sequence)).context("missing play-around sequence")?;let mut duration=0i64;
  for (key,items) in [("video_tracks","items"),("audio_tracks","items"),("caption_tracks","captions")]{for track in seq[key].as_array().into_iter().flatten(){let mut track_end:Option<i64>=None;for item in track[items].as_array().into_iter().flatten(){let end=item["start"].as_i64().context("invalid play-around start")?.checked_add(item["duration"].as_i64().context("invalid play-around duration")?).context("play-around duration overflow")?;track_end=Some(track_end.map_or(end,|old|old.max(end)));}duration=duration.max(track_end.unwrap_or(0));}}
  let start=rate.snap(Tick(center.checked_sub(Tick::from_seconds_f64(pre).0).context("play-around preroll overflow")?.max(0))).0;let minimum_end=center.checked_add(rate.frame_duration().0).context("play-around end overflow")?;
  let end=rate.snap(Tick(center.checked_add(Tick::from_seconds_f64(post).0).context("play-around postroll overflow")?.min(duration.max(minimum_end)))).0.max(start.checked_add(rate.frame_duration().0).context("play-around end overflow")?);
  self.around=Some(PlayAround{start,end,looping,clock0:clock});self.playhead=start;Ok(json!({"start":start,"end":end,"loop":looping}))
 }
 pub fn cancel(&mut self)->Result<Value>{self.around=None;if let Some(d)=self.dynamic.take(){self.document=d.before;self.shift=d.shift_before;self.follow_edit()?;}Ok(Value::Null)}
}
#[cfg(test)]mod tests{
 use super::*;use crate::domain::filmcraft_trim::Edge;
 #[test]fn original_dynamic_trim_matches(){check_original(include_str!("../../tests/fixtures/filmcraft-dynamic-trim.json"),192);}
 #[test]fn original_extended_dynamic_trim_matches(){check_original(include_str!("../../tests/fixtures/filmcraft-dynamic-trim-extended.json"),1536);}
 #[test]fn original_play_around_matches(){check_original(include_str!("../../tests/fixtures/filmcraft-play-around.json"),192);}
 fn check_original(data:&str,count:usize){
  let fixture:Value=serde_json::from_str(data).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),count);
  for (i,case) in cases.iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let points=case["points"].as_array().unwrap().iter().map(|p|TrimEditPoint{clip:p["clip"].as_u64().unwrap(),edge:if p["out"]==true{Edge::Out}else{Edge::In},kind:match p["kind"].as_str().unwrap(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,_=>TrimKind::Roll}}).collect();let mut session=DynamicTrimSession::new(doc,base["sequence"].as_u64().unwrap(),points,case["linked"].as_bool().unwrap()).unwrap();session.playhead=case["initialPlayhead"].as_i64().unwrap_or(0);if let Some(initial)=case["initialShift"].as_object(){session.shift=TrimShift{outgoing:initial["out_shift"].as_i64().unwrap(),incoming:initial["in_shift"].as_i64().unwrap()};}
   for (j,step) in case["steps"].as_array().unwrap().iter().enumerate(){let a=&step["action"];let clock=a["clock"].as_f64().unwrap_or(0.0);let result=match a["op"].as_str().unwrap(){"shuttle"=>{let direction=match &a["direction"]{Value::String(d) if d.starts_with('r')||d.starts_with('b')||d.eq_ignore_ascii_case("j")=>-1.0,v if v.as_f64().is_some_and(|v|v<0.0)=>-1.0,_=>1.0};session.shuttle(clock,direction,a["slow"]==true)},"tick"=>session.tick(clock),"stop"=>session.stop(a["clock"].as_f64()),"around"=>session.play_around(clock,a["loop"].as_bool().unwrap_or(true),a["toggle"]==true,case["centerPlayhead"]==true,case["pre"].as_f64().unwrap(),case["post"].as_f64().unwrap()),_=>session.cancel()};
    assert_eq!(result.is_err(),step["error"].as_bool().unwrap(),"dynamic refusal {i}/{j}");if let Ok(result)=result{assert_eq!(result,step["result"],"dynamic result {i}/{j}");}assert_eq!(session.status().unwrap(),step["status"],"dynamic status {i}/{j}");assert_eq!(json!({"out_shift":session.shift.outgoing,"in_shift":session.shift.incoming}),step["shift"],"dynamic shift {i}/{j}");assert_eq!(session.document.envelope["project"],step["project"],"dynamic project {i}/{j}");assert_eq!(session.undo.len(),step["undoCount"].as_u64().unwrap() as usize,"dynamic undo {i}/{j}");
   }
  }
 }
}
