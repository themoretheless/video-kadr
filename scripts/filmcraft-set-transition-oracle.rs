use serde_json::{Value,json};use filmcraft_project::{ParamValue,EffectKind};
type Result<T>=std::result::Result<T,String>;
fn bad(cmd:&str,msg:impl std::fmt::Display)->String{format!("{cmd}: {msg}")}
// No transition definition uses paths; this unexercised adapter only satisfies the engine helper signature.
mod masks{pub fn path_from_json(_: &serde_json::Value)->Option<filmcraft_project::mask::MaskPath>{None}}
__CONVERT__
__PARAMS__
enum EngineError{NoSequence}
impl From<EngineError> for String{fn from(_:EngineError)->Self{"no sequence".into()}}
struct Session{project:filmcraft_project::Project,sequence:filmcraft_project::ItemId}
impl Session{
 fn active_sequence(&self)->Option<&filmcraft_project::Sequence>{self.project.sequence(self.sequence)}
 fn edit_sequence<F>(&mut self,_label:&str,f:F)->Result<()> where F:FnOnce(&mut filmcraft_project::Sequence,&mut (),&())->Result<()>{f(self.project.sequence_mut(self.sequence).unwrap(),&mut (),&())}
}
__SET__
fn main(){let mut cases=vec![];
let mut effects:Vec<_>=filmcraft_project::effect::effect_defs().iter().filter(|def|matches!(def.kind,EffectKind::VideoTransition|EffectKind::AudioTransition)).map(|def|def.instance()).collect();effects.push(serde_json::from_value(json!({"effect":"unknown_future_transition","enabled":false,"params":{}})).unwrap());
for mut effect in effects{effect.enabled=false;for param in effect.params.values_mut(){param.keyframes.push(filmcraft_project::Keyframe::new(filmcraft_time::Tick(0),param.value.clone()));}for reset in [false,true]{for reverse in [None,Some(false),Some(true)]{for patch in 0..5{for scenario in 0..2{
 let mut project=filmcraft_project::Project::new("Transition controls");let sequence=project.new_sequence("Sequence",Default::default(),1,2,None);
 let id=if scenario==0{9007199254740993u64}else{18446744073709551615u64};
 let tr=filmcraft_project::Transition{id:filmcraft_project::TransitionId(id),effect:effect.clone(),start:filmcraft_time::Tick(0),duration:filmcraft_time::Tick(100),from:None,to:None,align:Default::default(),reverse:true};
 for track in project.sequence_mut(sequence).unwrap().all_tracks_mut(){track.locked=scenario==1;track.transitions=vec![tr.clone(),tr.clone()];}
 let before=project.clone();let mut s=Session{project,sequence};let mut request=json!({"transition":id,"reset":reset});if let Some(reverse)=reverse{request["reverse"]=json!(reverse);}
 match patch{1=>request["params"]=json!({}),2=>request["params"]=json!({"unknown":0}),3=>request["params"]=Value::Null,4=>{let mut params=json!({});if let Some((id,param))=effect.params.first_key_value(){params[id]=match &param.value{ParamValue::Float(_)=>json!(999999),ParamValue::Choice(_)=>json!(0),ParamValue::Bool(_)=>json!(true),ParamValue::Color(_)=>json!("#12abef80"),ParamValue::Vec2(_)=>json!([10,20]),_=>panic!("unexpected transition type")};}request["params"]=params;},_=>{}}
 let result=set_transition(&mut s,&request);let error=result.is_err();if error{s.project=before.clone();}
 cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":before},"expected":{"format":"filmcraft.project","schema_version":12,"project":s.project},"sequence":sequence,"request":request,"error":error,"result":result.ok()}));
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
