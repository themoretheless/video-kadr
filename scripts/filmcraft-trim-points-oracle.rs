use filmcraft_project::{Project,Sequence,ItemId,ClipId};use filmcraft_time::Tick;use serde::Serialize;use serde_json::{Value,json};use filmcraft_edit::{EditCtx,Edge,TrimMode};
#[derive(Clone,Copy,PartialEq,Eq,Serialize)]#[serde(rename_all="camelCase")]enum TrimKind{Trim,Ripple,Roll}
#[derive(Clone,Copy,Serialize)]struct EditPoint{clip:ClipId,out:bool,kind:TrimKind}
struct State{edit_points:Vec<EditPoint>}
struct Session{project:Project,sequence:ItemId,state:State,linked:bool}
#[derive(Debug)]enum EngineError{NoSequence,Other(String)}
type Result<T>=std::result::Result<T,EngineError>;
mod original{use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
impl Session{
 fn active_sequence(&self)->Option<&Sequence>{self.project.sequence(self.sequence)}
 fn execute(&mut self,cmd:&str,p:Value)->Result<Value>{
  let before=self.project.clone();let mut work=before.clone();let rate=before.sequence(self.sequence).ok_or(EngineError::NoSequence)?.settings.frame_rate;
  let duration=|id|original::media_duration(&before,&(),id);let start=|id|original::media_start(&before,id);let mut next=before.next_id;let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:rate.frame_duration()};
  let result=if cmd=="timeline.roll"{filmcraft_edit::roll(work.sequence_mut(self.sequence).unwrap(),ClipId(p["left"].as_u64().unwrap()),ClipId(p["right"].as_u64().unwrap()),Tick(p["delta"].as_i64().unwrap()),&mut ctx)}else{
   let clip=ClipId(p["clip"].as_u64().unwrap());let edge=if p["edge"]=="in"{Edge::In}else{Edge::Out};let mode=if p["mode"]=="ripple"{TrimMode::Ripple}else{TrimMode::Regular};let mut ids=vec![clip];
   if self.linked{let q=before.sequence(self.sequence).unwrap();if let Some(link)=q.find_item(clip).and_then(|(_,it)|it.link){for track in q.all_tracks(){for item in &track.items{if item.link==Some(link)&&!ids.contains(&item.id){ids.push(item.id);}}}}}
   (||->filmcraft_edit::Result<Tick>{let q=work.sequence_mut(self.sequence).unwrap();let mut d=Tick(p["delta"].as_i64().unwrap());for id in &ids{let x=filmcraft_edit::clamp_trim(q,*id,edge,mode,d,&ctx)?;if x.abs()<d.abs(){d=x;}}
    if mode==TrimMode::Ripple{d=filmcraft_edit::ripple_trim_group(q,&ids,edge,d,&mut ctx)?;}else{for id in &ids{filmcraft_edit::trim(q,*id,edge,mode,d,&mut ctx)?;}}Ok(d)})()
  };
  let d=result.map_err(|e|EngineError::Other(e.to_string()))?;work.next_id=next;self.project=work;Ok(json!({"delta":d.0}))
 }
}
__HELPERS__
__RAW__
fn main(){let source:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for (source_case,case) in source["cases"].as_array().unwrap().iter().enumerate().step_by(216){for kind in [TrimKind::Trim,TrimKind::Ripple,TrimKind::Roll]{for out in [false,true]{for clips in [vec![1001,1002],vec![1001,9999],vec![9999,1001]]{for delta in [-254016000000i64,0,254016000000]{for linked in [false,true]{
let points:Vec<_>=clips.iter().map(|id|EditPoint{clip:ClipId(*id),out,kind}).collect();let mut s=Session{project:serde_json::from_value(case["document"]["project"].clone()).unwrap(),sequence:ItemId(case["sequence"].as_u64().unwrap()),state:State{edit_points:points.clone()},linked};let result=trim_all_raw(&mut s,Tick(delta));cases.push(json!({"sourceCase":source_case,"points":points,"delta":delta,"linked":linked,"error":result.is_err(),"applied":result.ok(),"expectedProject":s.project}));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
