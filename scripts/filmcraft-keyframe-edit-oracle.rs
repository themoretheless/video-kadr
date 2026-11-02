// Execute the pinned original engine, not native implementation logic.
use serde_json::{json, Value};
use sha2::{Digest,Sha256};
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn sample(v:&filmcraft_project::ParamValue)->Value {use filmcraft_project::ParamValue as V;match v {
 V::Float(v)=>json!(v+0.125),V::Vec2(_)=>json!([17.25,-9.5]),V::Color(_)=>json!([0.1,0.2,0.3,0.4]),V::Bool(v)=>json!(!v),V::Choice(_)=>json!(0),V::Text(_)=>json!("changed source"),V::Curve(_)=>json!([[0.,0.],[1.,0.75]]),V::Path(_)=>json!([[1.,2.],[3.,4.],[5.,6.]])}}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;
 let sequence=s.state.active_sequence.unwrap();let base=s.project.clone();let clip=base.sequence(sequence).unwrap().video_tracks[0].items[0].id;
 let mut instances=Vec::new();let mut jobs=Vec::new();
 for d in filmcraft_project::effect_defs(){let mut e=d.instance();e.enabled=false;
  for p in e.params.values_mut(){p.keyframes=vec![filmcraft_project::Keyframe::new(filmcraft_time::Tick(0),p.value.clone()),filmcraft_project::Keyframe::new(filmcraft_time::Tick(100),p.value.clone())];}
  let i=instances.len();for(name,p)in &e.params{jobs.push((i,"effects.setKeyframe",json!({"clip":clip.0,"effect":d.id,"param":name,"mediaTime":0,"value":sample(&p.value),"inInfluence":-1.,"outInfluence":2.})));}instances.push(e);
 }
 let mut motion=filmcraft_project::find_effect("motion").unwrap().instance();motion.enabled=false;
 motion.params.get_mut("scale").unwrap().keyframes=vec![filmcraft_project::Keyframe::new(filmcraft_time::Tick(0),filmcraft_project::ParamValue::Float(25.)),filmcraft_project::Keyframe::new(filmcraft_time::Tick(100),filmcraft_project::ParamValue::Float(75.))];
 let i=instances.len();instances.push(motion.clone());
 for (command,extras) in [
 ("effects.deleteKeyframe",json!({"mediaTime":0})),("effects.deleteKeyframe",json!({"mediaTime":50})),("effects.deleteKeyframe",json!({})),
 ("effects.moveKeyframe",json!({"mediaTime":0,"to":100})),("effects.moveKeyframe",json!({"mediaTime":100,"to":-50})),("effects.moveKeyframe",json!({"mediaTime":50,"to":0})),("effects.moveKeyframe",json!({"mediaTime":0})),
 ("effects.setKeyframe",json!({"mediaTime":0})),("effects.setKeyframe",json!({"mediaTime":50})),("effects.setKeyframe",json!({"mediaTime":0,"value":null})),("effects.setKeyframe",json!({"mediaTime":0,"value":"wrong"})),("effects.setKeyframe",json!({"mediaTime":0,"inInfluence":0.5,"outInfluence":"ignored"})),
 ] {let mut p=json!({"clip":clip.0,"effect":"motion","param":"scale"});for(k,v)in extras.as_object().unwrap(){p[k]=v.clone();}jobs.push((i,command,p));}
 for kind in ["linear","BeZiEr","autoBezier","continuousBezier","hold","easeIn","easeOut","unknown"] {jobs.push((i,"effects.setInterpolation",json!({"clip":clip.0,"effect":0,"param":"scale","mediaTime":0,"interpolation":kind})));}
 for p in [json!({"mediaTime":50}),json!({"time":0}),json!({"mediaTime":100,"interpolation":false})] {let mut q=json!({"clip":clip.0,"effect":"motion","param":"scale"});for(k,v)in p.as_object().unwrap(){q[k]=v.clone();}jobs.push((i,"effects.setInterpolation",q));}
 // Removing the final key retains the removed key value as the static base.
 motion.params.get_mut("scale").unwrap().keyframes.remove(0);let last=instances.len();instances.push(motion);jobs.push((last,"effects.deleteKeyframe",json!({"clip":clip.0,"param":"scale","mediaTime":100})));
 let mut opacity=filmcraft_project::find_effect("opacity").unwrap().instance();let mut mask=filmcraft_project::mask::Mask::new("Test",filmcraft_project::mask::MaskPath::rect(0.,0.,10.,10.));
 for name in ["path","feather","opacity","expansion"] {let p=mask.param_mut(name).unwrap();p.keyframes=vec![filmcraft_project::Keyframe::new(filmcraft_time::Tick(0),p.value.clone())];}
 opacity.masks.push(mask);let mi=instances.len();instances.push(opacity.clone());
 for name in ["path","feather","opacity","expansion"] {jobs.push((mi,"effects.setKeyframe",json!({"clip":clip.0,"effect":"opacity","mask":0,"param":name,"mediaTime":0,"value":sample(&opacity.masks[0].param(name).unwrap().value)})));}
 let mut cases=Vec::new();for(instance,command,params)in jobs{
  s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.history.merge_key=None;
  std::sync::Arc::make_mut(&mut s.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0].effects=vec![instances[instance].clone()];
  let result=s.execute(command,params.clone());cases.push(json!({"instance":instance,"command":command,"params":params,"ok":result.is_ok(),"projectSha256":hash(&serde_json::to_value(s.project.as_ref())?)}));
 }
 let header=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":sequence.0,"clip":clip.0,"project":serde_json::to_value(base.as_ref())?});let mut text=serde_json::to_string(&header)?;text.pop();text.push_str(",\"instances\":[\n");
 for(i,v)in instances.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(&serde_json::to_value(v)?)?);}text.push_str("\n],\"cases\":[\n");for(i,v)in cases.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(v)?);}text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
