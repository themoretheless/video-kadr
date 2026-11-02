// Execute original Filmcraft mask commands; native code is not used here.
use serde_json::{json,Value};use sha2::{Digest,Sha256};use filmcraft_project::{Mask,MaskPath,Keyframe,ParamValue as V,Interpolation as I};use filmcraft_time::Tick;
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let sequence=s.state.active_sequence.unwrap();let base=s.project.clone();let clip=base.sequence(sequence).unwrap().video_tracks[0].items[0].id;
 let mut instances=Vec::new();let mut jobs:Vec<(usize,&str,Value,Value)>=Vec::new();
 for d in filmcraft_project::effect_defs(){let i=instances.len();instances.push(d.instance());jobs.push((i,"masks.add",json!({"clip":clip.0,"effect":d.id}),json!({})));}
 let mut curve=MaskPath::rect(-13.25,20.5,103.75,157.125);curve.vertices[0].t_in.x=-5.3;curve.vertices[0].t_out.y=7.7;
 for count in [0,1,3]{let mut effect=filmcraft_project::find_effect("opacity").unwrap().instance();effect.enabled=false;
 let mut mask=Mask::new("Mask (7)",curve.clone());for key in ["path","feather","opacity","expansion"]{let p=mask.param_mut(key).unwrap();for i in 0..count{let v=if key=="path"{let mut path=curve.clone();for v in &mut path.vertices{v.p.x+=i as f64*17.3;v.p.y-=i as f64*8.7;}V::Path(path)}else{V::Float(i as f64*17.25+3.)};let mut k=Keyframe::new(Tick(i*100),v);k.interp=I::Bezier;k.out_influence=0.73;k.in_influence=0.87;p.keyframes.push(k);}}
 effect.masks.push(mask);let index=instances.len();instances.push(effect);
 let ctx=json!({"playhead":40,"selectedMask":{"clip":clip.0,"effect":0,"mask":0},"selection":[clip.0]});
 for (cmd,extra) in [
 ("masks.set",json!({"name":"custom","inverted":true,"mode":"SuBtRaCt","trackMethod":"Position & Rotation","feather":-4.,"opacity":123.,"expansion":-3.5})),
 ("masks.set",json!({"path":[[1.,2.],[3.,4.],[5.,6.]]})),("masks.set",json!({"path":{"closed":false,"vertices":[]}})),("masks.set",json!({"opacity":-10.,"feather":7.25,"mode":"none"})),
 ("masks.set",json!({"mode":"invalid"})),("masks.set",json!({"trackMethod":"invalid"})),("masks.set",json!({"name":false,"opacity":"ignored","inverted":"ignored","mode":false})),
 ("masks.translate",json!({"delta":[0.1,-3.3]})),("masks.translate",json!({"delta":{"x":-25.75,"y":17.5}})),("masks.translate",json!({"delta":"invalid"})),
 ("masks.moveVertex",json!({"vertex":0,"to":[1.3,5.7]})),("masks.moveVertex",json!({"vertex":0,"handle":"in","to":[-17.3,22.7]})),("masks.moveVertex",json!({"vertex":0,"handle":"out","delta":[0.1,-0.2],"breakHandles":true})),("masks.moveVertex",json!({"vertex":1,"handle":"out","to":[23.,42.],"delta":[99.,99.]})),("masks.moveVertex",json!({"vertex":20,"delta":[1.,2.]})),("masks.moveVertex",json!({"vertex":0})),
 ("masks.addVertex",json!({"after":0,"at":[7.7,12.3]})),("masks.addVertex",json!({"after":100,"at":[17.,29.]})),("masks.removeVertex",json!({"vertex":1})),("masks.removeVertex",json!({"vertex":100})),
 ("masks.toggleVertexSmooth",json!({"vertex":0})),("masks.toggleVertexSmooth",json!({"vertex":1})),("masks.toggleVertexSmooth",json!({"vertex":99})),
 ("masks.list",json!({})),("masks.select",json!({})),("masks.select",json!({"none":true})),("masks.remove",json!({})),
 ]{let mut params=json!({"clip":clip.0,"effect":"opacity","mask":0});for(k,v)in extra.as_object().unwrap(){params[k]=v.clone();}jobs.push((index,cmd,params,ctx.clone()));}
 // Context fallback resolves selected mask or selected video clip; selection uses default opacity.
 for cmd in ["masks.set","masks.list","masks.remove","masks.select"]{jobs.push((index,cmd,if cmd=="masks.select"{json!({})}else{json!({"feather":12.})},ctx.clone()));}
 for mode in ["none","add","subtract","intersect","lighten","darken","difference"]{jobs.push((index,"masks.set",json!({"mode":mode}),ctx.clone()));}
 for method in ["position","position_rotation","Position, Scale & Rotation"]{jobs.push((index,"masks.set",json!({"trackMethod":method}),ctx.clone()));}
 for shape in ["ellipse","polygon","rectangle","rect","4-point","bezier","invalid"]{jobs.push((index,"masks.add",json!({"shape":shape,"center":[13.7,-8.1],"size":[0.,-24.5]}),ctx.clone()));}
 for path in [json!([[1.,2.],[3.,4.]]),json!({"closed":false,"vertices":[[1.,2.],[3.,4.]]}),json!([{"p":[1.,2.],"in":[3.,4.],"out":[-3.,-4.]},[3.,4.],[5.,6.]]),json!({"invalid":true})]{jobs.push((index,"masks.add",json!({"path":path}),ctx.clone()));}
 jobs.push((index,"masks.list",json!({"clip":999}),ctx.clone()));jobs.push((index,"masks.select",json!({"clip":clip.0,"effect":0,"mask":99}),ctx.clone()));
 }
 let mut double=filmcraft_project::find_effect("opacity").unwrap().instance();double.masks=vec![Mask::new("first",curve.clone()),Mask::new("second",curve.clone())];let di=instances.len();instances.push(double);
 let dc=json!({"playhead":40,"selectedMask":{"clip":clip.0,"effect":0,"mask":1},"selection":[clip.0]});
 for(cmd,p)in [("masks.select",json!({"clip":clip.0})),("masks.set",json!({"clip":false,"mask":false,"feather":4.})),("masks.list",json!({"clip":false})),("masks.remove",json!({"clip":false}))]{jobs.push((di,cmd,p,dc.clone()));}
 let mut cases=Vec::new();for(instance,command,params,context)in jobs{
 s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.history.merge_key=None;s.state.playheads.insert(sequence,Tick(context["playhead"].as_i64().unwrap_or(0)));s.state.selected_mask=serde_json::from_value(context.get("selectedMask").cloned().unwrap_or(Value::Null))?;s.state.selection=context["selection"].as_array().map(|a|a.iter().map(|v|filmcraft_project::ClipId(v.as_u64().unwrap())).collect()).unwrap_or_default();
 std::sync::Arc::make_mut(&mut s.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0].effects=vec![instances[instance].clone()];
 let result=s.execute(command,params.clone());let selected=serde_json::to_value(s.state.selected_mask)?;cases.push(json!({"instance":instance,"command":command,"params":params,"context":context,"ok":result.is_ok(),"result":result.ok(),"selectedMask":selected,"projectSha256":hash(&serde_json::to_value(s.project.as_ref())?)}));
 }
 let header=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":sequence.0,"clip":clip.0,"project":serde_json::to_value(base.as_ref())?});let mut text=serde_json::to_string(&header)?;text.pop();text.push_str(",\"instances\":[\n");for(i,v)in instances.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(&serde_json::to_value(v)?)?);}text.push_str("\n],\"cases\":[\n");for(i,v)in cases.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(v)?);}text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
