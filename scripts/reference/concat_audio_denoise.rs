//! Qualification only: original pinned Denoise catalogue and chain compiler.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn main(){
 let cat=Catalogue::builtin();assert!(cat.get("denoise").is_none()); assert_eq!(cat.get("concat.denoise").unwrap().id(),"concat.denoise");
 let mut cases=vec![];
 for signal in ["noise","steps"] {
  let mut plans:Vec<Vec<(&str,Value)>>=[0.,50.,100.,6.555,37.5,60.].into_iter().map(|strength|vec![("concat.denoise",json!({"strength":strength}))]).collect();
  plans.extend([
   vec![("concat.denoise",json!({"strength":50})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
   vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.denoise",json!({"strength":50}))],
   vec![("concat.denoise",json!({"strength":50})),("concat.bass",json!({"gain":5}))],
   vec![("concat.bass",json!({"gain":5})),("concat.denoise",json!({"strength":50}))],
   vec![("concat.denoise",json!({"strength":50})),("concat.denoise",json!({"strength":100}))],
   vec![("concat.denoise",json!({"strength":50})),("concat.distorted",json!({"drive":50}))],
   vec![("concat.distorted",json!({"drive":50})),("concat.denoise",json!({"strength":50}))],
   vec![("concat.denoise",json!({"strength":50})),("concat.enhance-voice",json!({"amount":60}))],
   vec![("concat.enhance-voice",json!({"amount":60})),("concat.denoise",json!({"strength":50}))],
  ]);
  for plan in plans {
   let filters:Vec<_>=plan.iter().map(|(id,p)|AppliedFilter{id:(*id).into(),params:p.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}).collect();
   let settings:Vec<_>=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect();
   cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));
  }
 }
 std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":{"hasShortAlias":false},"cases":cases})).unwrap()).unwrap();
}
