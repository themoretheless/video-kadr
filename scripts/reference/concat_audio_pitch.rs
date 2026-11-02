//! Qualification only: original pinned Pitch catalogue and chain compiler.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn main(){
 let cat=Catalogue::builtin();assert!(cat.get("pitch").is_none()); assert_eq!(cat.get("concat.pitch").unwrap().id(),"concat.pitch");
 let mut cases=vec![];
 for signal in ["noise","steps"] {
  let plans:Vec<Vec<(&str,Value)>>=[-12.,-7.,-3.,0.,3.,7.,12.,6.555,-6.555].into_iter().map(|semitones|vec![("concat.pitch",json!({"semitones":semitones}))]).collect();
  for plan in plans {
   let filters:Vec<_>=plan.iter().map(|(id,p)|AppliedFilter{id:(*id).into(),params:p.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}).collect();
   let settings:Vec<_>=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect();
   cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));
  }
 }
 std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":{"hasShortAlias":false},"cases":cases})).unwrap()).unwrap();
}
