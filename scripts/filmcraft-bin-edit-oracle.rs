// Actual original commands; this program does not implement expected mutations.
use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;
 let parent=std::sync::Arc::make_mut(&mut s.project).add_bin("Bin 01",None);
 std::sync::Arc::make_mut(&mut s.project).add_bin("Bin 02",Some(parent));
 let base=s.project.clone();let project=serde_json::to_value(base.as_ref())?;
 let ids:Vec<_>=base.items.keys().take(2).copied().collect();let mut cases=Vec::new();
 let mut run=|command:&str,params:Value,selected:bool|->Result<(),Box<dyn std::error::Error>>{
   s.project=base.clone();s.state.project_selection=if selected{ids.clone()}else{vec![]};
   let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
   cases.push(json!({"command":command,"params":params,"selected":selected,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?))}));Ok(())
 };
 for name in [json!(null),json!(""),json!(" New Bin "),json!("новое 🎬"),json!(false),json!(42)] {for target in [json!(null),json!(0),json!(1),json!(parent.0),json!(u64::MAX),json!(1.9)] {run("file.newBin",json!({"name":name,"parent":target}),false)?;}}
 let selections=vec![json!(null),json!([]),json!([ids[0].0]),json!([ids[0].0,ids[1].0]),json!([ids[0].0,ids[0].0]),json!([u64::MAX]),json!([ids[0].0,u64::MAX]),json!([true,1.5,ids[1].0]),json!(false)];
 for selected in [false,true] {for items in &selections {
   for name in [json!(null),json!(""),json!("Bin 01"),json!("  Group 🎬  "),json!(false)] {run("file.newBinFromSelection",json!({"items":items,"name":name}),selected)?;}
   for bin in [json!(null),json!(0),json!(1),json!(parent.0),json!(u64::MAX),json!(true)] {run("project.moveToBin",json!({"items":items,"bin":bin}),selected)?;}
 }}
 for item in [json!(ids[0].0),json!(u64::MAX),json!(null),json!(1.9)] {run("file.newBinFromSelection",json!({"item":item}),false)?;}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":project,"selection":ids.iter().map(|id|id.0).collect::<Vec<_>>(),"cases":cases}))?)?;Ok(())
}
