//! Transition definitions and parameter authoring from the pinned Filmcraft catalog.
use anyhow::{Context,Result,ensure};
use serde_json::{Value,json};
use std::sync::OnceLock;

pub fn catalog()-> &'static Value{
 static CATALOG:OnceLock<Value>=OnceLock::new();
 CATALOG.get_or_init(||serde_json::from_str(include_str!("filmcraft_transition_catalog.json")).expect("embedded transition catalog"))
}
pub fn instance(name:&str,kind:&str)->Result<Value>{
 let name=name.trim().to_ascii_lowercase();
 catalog().as_array().unwrap().iter().find(|entry|entry["definition"]["kind"]==kind && (entry["definition"]["id"]==name||entry["definition"]["name"].as_str().is_some_and(|v|v.to_ascii_lowercase()==name)))
  .map(|entry|entry["instance"].clone()).context("unknown transition")
}
fn converted(template:&Value,value:&Value)->Result<Value>{
 let tag=template.as_object().and_then(|v|v.keys().next()).context("invalid parameter template")?;
 let payload=match tag.as_str(){
  "Float"=>json!(value.as_f64().context("expected number")?),
  "Choice"=>json!(value.as_u64().context("expected choice index")? as u32),
  "Bool"=>json!(value.as_bool().context("expected boolean")?),
  "Vec2"=>{let a=value.as_array().context("expected point")?;json!({"x":a.first().and_then(Value::as_f64).context("expected point x")?,"y":a.get(1).and_then(Value::as_f64).context("expected point y")?})},
  "Color"=>{
   let color=if let Some(text)=value.as_str(){let text=text.trim().trim_start_matches('#');ensure!(matches!(text.len(),6|8),"invalid hex color");let mut c=[0f32,0.,0.,1.];for (i,channel) in c.iter_mut().enumerate().take(text.len()/2){*channel=u8::from_str_radix(text.get(i*2..i*2+2).context("invalid hex color")?,16)? as f32/255.;}c}
   else{let a=value.as_array().context("expected color")?;[a.first().and_then(Value::as_f64).context("expected red")? as f32,a.get(1).and_then(Value::as_f64).context("expected green")? as f32,a.get(2).and_then(Value::as_f64).context("expected blue")? as f32,a.get(3).and_then(Value::as_f64).unwrap_or(1.) as f32]};json!(color)
  },
  _=>anyhow::bail!("unsupported transition parameter template"),
 };Ok(json!({tag:payload}))
}
/// Change static parameter values while retaining keyframes and unknown instance properties.
pub fn set_params(effect:&mut Value,params:&Value)->Result<()>{
 let params=params.as_object().context("params must be an object")?;
 let definition=catalog().as_array().unwrap().iter().find(|entry|entry["definition"]["id"]==effect["effect"]).context("unknown transition")?;
 let mut work=effect.clone();
 for (key,value) in params{
  let pd=definition["definition"]["params"].as_array().unwrap().iter().find(|pd|pd["id"]==key.as_str()).context("unknown transition parameter")?;
  let mut converted=if let(Some(options),Some(text))=(pd["kind"]["Choice"].as_array(),value.as_str()){
   json!({"Choice":options.iter().position(|option|option.as_str().is_some_and(|v|v.eq_ignore_ascii_case(text))).context("unknown choice")?})
  }else{converted(&pd["default"],value)?};
  if let Some(options)=pd["kind"]["Choice"].as_array(){ensure!(converted["Choice"].as_u64().context("invalid choice")? < options.len() as u64,"choice outside range");}
  if let Some(bounds)=pd["kind"].get("Float"){let f=converted["Float"].as_f64().context("invalid float")?;converted["Float"]=json!(f.clamp(bounds["min"].as_f64().context("invalid minimum")?,bounds["max"].as_f64().context("invalid maximum")?));}
  let target=work["params"].as_object_mut().context("invalid effect params")?;
  match target.get_mut(key){Some(param)=>{ensure!(param.is_object(),"invalid parameter instance");param["value"]=converted;},None=>{target.insert(key.clone(),json!({"value":converted,"keyframes":[]}));}}
 }
 *effect=work;Ok(())
}
/// Original setTransition behavior: first source match, then first matching ID per media track.
pub fn set_transition(document:&mut super::filmcraft_project_file::SourceProjectDocument,sequence:u64,id:u64,reset:bool,params:Option<&Value>,reverse:Option<bool>)->Result<Value>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing transition sequence")?;
 let current=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).find_map(|track|track["transitions"].as_array().and_then(|transitions|transitions.iter().find(|transition|transition["id"].as_u64()==Some(id)))).context("missing transition")?;
 let mut effect=current["effect"].clone();
 if reset{if let Some(entry)=catalog().as_array().unwrap().iter().find(|entry|entry["definition"]["id"]==effect["effect"]){effect=entry["instance"].clone();}}
 if let Some(params)=params{set_params(&mut effect,params)?;}
 let reverse=reverse.unwrap_or_else(||current["reverse"].as_bool().unwrap_or(false));
 let mut work=document.clone();let seq=work.envelope.pointer_mut(&path).unwrap();
 for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().context("invalid transition tracks")?{if let Some(transition)=track["transitions"].as_array_mut().context("invalid track transitions")?.iter_mut().find(|transition|transition["id"].as_u64()==Some(id)){transition["effect"]=effect.clone();transition["reverse"]=json!(reverse);}}}
 *document=work;Ok(json!({"transition":id,"effect":effect["effect"],"reverse":reverse}))
}
/// Batch selection edges: preserve existing cut transitions and allow partial per-edge success.
pub fn apply_selection(document:&mut super::filmcraft_project_file::SourceProjectDocument,sequence:u64,clips:&[u64],video_effect:&str,audio_effect:&str,video_duration:i64,audio_duration:i64)->Result<Vec<u64>>{
 use super::filmcraft_trim::{plan_transition,add_transition,Edge};
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing transition sequence")?;
 let mut edges:Vec<(&str,u64,Edge,i64,u64)>=vec![];
 for clip in clips{
  let found=["video_tracks","audio_tracks"].into_iter().find_map(|kind|seq[kind].as_array().into_iter().flatten().find_map(|track|track["items"].as_array().into_iter().flatten().find(|item|item["id"].as_u64()==Some(*clip)).map(|item|(kind,track,item))));
  let Some((kind,track,item))=found else{continue};let track_id=track["id"].as_u64().context("invalid batch track ID")?;let start=item["start"].as_i64().context("invalid clip start")?;let end=i64::try_from(start as i128+item["duration"].as_i64().context("invalid clip duration")? as i128).context("batch clock overflow")?;
  for (edge,at) in [(Edge::In,start),(Edge::Out,end)]{
   if edges.iter().any(|edge|edge.4==track_id&&edge.3==at){continue;}
   let mut existing=false;for transition in track["transitions"].as_array().context("invalid batch transitions")?{let start=transition["start"].as_i64().context("invalid transition start")? as i128;let end=start+transition["duration"].as_i64().context("invalid transition duration")? as i128;if start<=at as i128&&end>=at as i128&&(transition["from"].as_u64()==Some(*clip)||transition["to"].as_u64()==Some(*clip)){existing=true;break;}}
   if !existing{edges.push((kind,*clip,edge,at,track_id));}
  }
 }
 let mut ids=vec![];
 for (kind,clip,edge,_,_) in edges{
  let result=(||->Result<u64>{let video=kind=="video_tracks";let duration=if video{video_duration}else{audio_duration};let effect=instance(if video{video_effect}else{audio_effect},if video{"VideoTransition"}else{"AudioTransition"})?;let target=plan_transition(document,sequence,kind,&[],Some(clip),Some(edge),0,duration)?;let transition=json!({"id":0,"effect":effect,"start":target["start"],"duration":duration,"from":target["from"],"to":target["to"],"align":"CenterAtCut","reverse":false});add_transition(document,sequence,target["track"].as_u64().unwrap(),&transition)})();
  if let Ok(id)=result{ids.push(id);}
 }
 ensure!(!ids.is_empty(),"no transition could be applied to the selection");Ok(ids)
}
/// Trim-monitor edit points preserve order and duplicates, unlike selected-clip cuts.
/// Failed points are skipped; successful points remain, matching the original command.
pub fn apply_edit_points(document:&mut super::filmcraft_project_file::SourceProjectDocument,sequence:u64,points:&[(u64,super::filmcraft_trim::Edge)],video_effect:&str,audio_effect:&str,video_duration:i64,audio_duration:i64)->Result<Vec<u64>>{
 use super::filmcraft_trim::{plan_transition,add_transition};
 let path=format!("/project/items/{sequence}/kind/Sequence");
 ensure!(document.envelope.pointer(&path).is_some(),"missing transition sequence");
 let mut ids=vec![];
 for &(clip,edge) in points{
  let result=(||->Result<u64>{
   let seq=document.envelope.pointer(&path).unwrap();
   let kind=["video_tracks","audio_tracks"].into_iter().find(|kind|seq[*kind].as_array().into_iter().flatten().any(|track|track["items"].as_array().into_iter().flatten().any(|item|item["id"].as_u64()==Some(clip)))).context("missing edit-point clip")?;
   let video=kind=="video_tracks";let duration=if video{video_duration}else{audio_duration};
   let effect=instance(if video{video_effect}else{audio_effect},if video{"VideoTransition"}else{"AudioTransition"})?;
   let target=plan_transition(document,sequence,kind,&[],Some(clip),Some(edge),0,duration)?;
   let transition=json!({"id":0,"effect":effect,"start":target["start"],"duration":duration,"from":target["from"],"to":target["to"],"align":"CenterAtCut","reverse":false});
   add_transition(document,sequence,target["track"].as_u64().context("invalid edit-point track")?,&transition)
  })();
  if let Ok(id)=result{ids.push(id);}
 }
 ensure!(!ids.is_empty(),"no transition could be applied at the selected edit points");Ok(ids)
}
/// Original TimelinePrefs duration_in, with checked clocks rather than overflow/wrap.
pub fn preference_duration(value:f64,unit:&str,num:i64,den:i64)->Result<i64>{
 ensure!(value.is_finite(),"invalid transition duration");let (num,den)=if num>0&&den>0{(num as i128,den as i128)}else{(24000,1001)};let clock=254016000000i128*den;
 let tick_of=|frame:i64|->Result<i64>{i64::try_from((frame as i128*clock).div_euclid(num)).context("transition duration overflow")};
 let duration=if unit=="frames"{tick_of(value.max(1.).round() as i64)?}else{let tick=(value.max(0.)*254016000000f64).round() as i64;let frame=i64::try_from((tick as i128*num).div_euclid(clock)).context("transition frame overflow")?;let a=tick_of(frame)?;let b=tick_of(frame.checked_add(1).context("transition frame overflow")?)?;if tick as i128-a as i128<=b as i128-tick as i128{a}else{b}};
 Ok(duration.max(i64::try_from(clock/num).context("transition frame duration overflow")?))
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]
 fn original_edit_point_transitions_match(){
  use super::super::{filmcraft_project_file::SourceProjectDocument,filmcraft_trim::Edge};
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),864);
  for (i,case) in cases.iter().enumerate(){
   let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let points:Vec<_>=case["points"].as_array().unwrap().iter().map(|p|(p["clip"].as_u64().unwrap(),if p["out"].as_bool().unwrap(){Edge::Out}else{Edge::In})).collect();
   let sequence=case["sequence"].as_u64().unwrap();let rate=&doc.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"]["settings"]["frame_rate"];
   let duration=i64::try_from((case["frames"].as_i64().unwrap() as i128*254016000000i128*rate["den"].as_i64().unwrap() as i128).div_euclid(rate["num"].as_i64().unwrap() as i128)).unwrap();
   let result=apply_edit_points(&mut doc,sequence,&points,"cross_dissolve","constant_power",duration,duration);
   if case["error"]==true{assert!(result.is_err(),"edit-point refusal {i}");}else{assert_eq!(json!({"transitions":result.unwrap()}),case["result"],"edit-point IDs {i}");}
   assert_eq!(doc.envelope,case["expected"],"edit-point document {i}");
  }
 }
 #[test]
 fn original_preference_duration_matches(){let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-duration.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),408);for (i,case) in cases.iter().enumerate(){assert_eq!(preference_duration(case["value"].as_f64().unwrap(),case["unit"].as_str().unwrap(),case["num"].as_i64().unwrap(),case["den"].as_i64().unwrap()).unwrap(),case["expected"].as_i64().unwrap(),"preference duration {i}");}assert!(preference_duration(f64::NAN,"seconds",24,1).is_err());assert!(preference_duration(f64::MAX,"frames",24,1).is_err());}

 #[test]
 fn original_batch_transition_matches(){
  use super::super::filmcraft_project_file::SourceProjectDocument;
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-batch-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),864);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let clips:Vec<_>=case["clips"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();let sequence=case["sequence"].as_u64().unwrap();let seq=&doc.envelope["project"]["items"][sequence.to_string()]["kind"]["Sequence"];let rate=&seq["settings"]["frame_rate"];let duration=i64::try_from((case["frames"].as_i64().unwrap() as i128*254016000000i128*rate["den"].as_i64().unwrap() as i128).div_euclid(rate["num"].as_i64().unwrap() as i128)).unwrap();let result=apply_selection(&mut doc,sequence,&clips,"cross_dissolve","constant_power",duration,duration);
  if case["error"]==true{assert!(result.is_err(),"batch refusal {i}");}else{assert_eq!(json!({"transitions":result.unwrap()}),case["result"],"batch IDs {i}");}assert_eq!(doc.envelope,case["expected"],"batch document {i}");}
 }

 #[test]
 fn original_set_transition_matches(){
  use super::super::filmcraft_project_file::SourceProjectDocument;
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-set-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),6960);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();let request=&case["request"];let result=set_transition(&mut doc,case["sequence"].as_u64().unwrap(),request["transition"].as_u64().unwrap(),request["reset"].as_bool().unwrap(),request.get("params"),request["reverse"].as_bool());if case["error"]==true{assert!(result.is_err(),"set refusal {i}");assert_eq!(doc,before);}else{assert_eq!(result.unwrap(),case["result"],"set result {i}");assert_eq!(doc.envelope,case["expected"],"set document {i}");}}
  let case=&cases[0];let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();assert!(set_transition(&mut doc,case["sequence"].as_u64().unwrap(),1,false,None,None).is_err());assert_eq!(doc,before);
 }

 #[test]
 fn original_transition_parameter_cases(){
  let oracle:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-params.json")).unwrap();assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(catalog(),&oracle["catalog"]);assert_eq!(catalog().as_array().unwrap().len(),115);assert_eq!(oracle["cases"].as_array().unwrap().len(),4102);
  for (i,case) in oracle["cases"].as_array().unwrap().iter().enumerate(){let mut effect=case["before"].clone();let before=effect.clone();let result=set_params(&mut effect,&case["params"]);if case["error"]==true{assert!(result.is_err(),"parameter refusal {i}");assert_eq!(effect,before,"atomic parameter refusal {i}");}else{result.unwrap();assert_eq!(effect,case["expected"],"parameter {i}");}}
  for entry in catalog().as_array().unwrap(){let def=&entry["definition"];assert_eq!(instance(&format!(" {} ",def["name"].as_str().unwrap().to_uppercase()),def["kind"].as_str().unwrap()).unwrap(),entry["instance"]);}
  assert!(instance("Constant Power","VideoTransition").is_err());
 }
 #[test]
 fn parameter_edits_preserve_keyframes_and_extensions(){
  let mut effect=instance("push","VideoTransition").unwrap();effect["opaque"]=json!({"id":u64::MAX});effect["params"]["direction"]["keyframes"]=json!([{"opaque":u64::MAX}]);effect["params"]["direction"]["extension"]=json!(9007199254740993_u64);let before=effect.clone();set_params(&mut effect,&json!({"direction":"from east"})).unwrap();assert_eq!(effect["opaque"],before["opaque"]);assert_eq!(effect["params"]["direction"]["keyframes"],before["params"]["direction"]["keyframes"]);assert_eq!(effect["params"]["direction"]["extension"],before["params"]["direction"]["extension"]);
  let before=effect.clone();assert!(set_params(&mut effect,&json!({"direction":"from north","unknown":0})).is_err());assert_eq!(effect,before);
 }
}
