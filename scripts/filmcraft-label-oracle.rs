// Execute actual original label commands; expected project mutations are not computed here.
use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;
 let seq=s.state.active_sequence.unwrap();let clip=s.project.sequence(seq).unwrap().video_tracks[0].items[0].id;let item=s.project.sequence(seq).unwrap().video_tracks[0].items[0].item;
 let base=s.project.clone();let project=serde_json::to_value(base.as_ref())?;let mut cases=Vec::new();
 let variants=vec![json!({}),json!({"items":[item.0]}),json!({"clips":[clip.0]}),json!({"items":[]}),json!({"clips":[]}),json!({"items":null}),json!({"clips":null}),json!({"items":null,"clips":null}),json!({"items":[item.0,u64::MAX],"clips":[clip.0,clip.0,u64::MAX]}),json!({"items":[true,1.5,item.0],"clips":[false,1.5,clip.0]}),json!({"items":[u64::MAX]}),json!({"clips":[u64::MAX]})];
 let commands:Vec<_>=std::iter::once("edit.label".to_string()).chain(filmcraft_project::Label::ALL.iter().map(|l|format!("edit.label.{}",l.name().to_ascii_lowercase()))).collect();
 for command in &commands{for locked in [false,true]{for context in 0..4{for variant in &variants{
  s.project=base.clone();std::sync::Arc::make_mut(&mut s.project).sequence_mut(seq).unwrap().video_tracks[0].locked=locked;
  s.state.active_sequence=if context==1{None}else{Some(seq)};
  s.state.project_selection=if context==1||context==3{vec![item]}else{vec![]};s.state.selection=if context==2||context==3{vec![clip]}else{vec![]};
  let mut params=variant.clone();params["label"]=json!(if command=="edit.label"{"rOsE"}else{"ignored"});
  let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":command,"locked":locked,"context":context,"params":params,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?))}));
 }}}}
 for label in [json!(" violet "),json!(null),json!(42),json!("unknown"),json!("VIOLET")]{
  s.project=base.clone();s.state.active_sequence=Some(seq);s.state.selection=vec![clip];s.state.project_selection=vec![item];let params=json!({"label":label});let result=s.execute("edit.label",params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":"edit.label","locked":false,"context":3,"params":params,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?))}));
 }
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"clip":clip.0,"item":item.0,"project":project,"palette":filmcraft_project::Label::ALL.iter().map(|l|json!({"name":l.name(),"rgb":l.rgb()})).collect::<Vec<_>>(),"cases":cases}))?)?;Ok(())
}
