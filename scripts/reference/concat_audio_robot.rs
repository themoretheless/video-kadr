//! Qualification only: original pinned Robot catalogue and chain compiler.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn main(){
 let cat=Catalogue::builtin(); assert_eq!(cat.get("robot").unwrap().id(),"concat.robot");
 let mut cases=vec![];
 for signal in ["noise","steps"] {
  let mut plans:Vec<Vec<(&str,Value)>>=[1.,5.,10.,6.555,5.5].into_iter().map(|depth|vec![("concat.robot",json!({"depth":depth}))]).collect();
  plans.extend([
   vec![("concat.robot",json!({"depth":5})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
   vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.robot",json!({"depth":5}))],
   vec![("concat.bass",json!({"gain":5})),("concat.robot",json!({"depth":5}))],
   vec![("concat.robot",json!({"depth":5})),("concat.robot",json!({"depth":10}))],
  ]);
  for plan in plans {
   let filters:Vec<_>=plan.iter().map(|(id,p)|AppliedFilter{id:(*id).into(),params:p.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}).collect();
   let settings:Vec<_>=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect();
   cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));
  }
 }
 std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":{"robot":"concat.robot"},"cases":cases})).unwrap()).unwrap();
}
