// Actual original edit.selectLabelGroup; expected selections are emitted by Session.execute.
use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn reverse(bin:&mut filmcraft_project::Bin){bin.children.reverse();for entry in &mut bin.children{if let filmcraft_project::BinEntry::Bin(b)=entry{reverse(b)}}}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let seq=s.state.active_sequence.unwrap();
 {let p=std::sync::Arc::make_mut(&mut s.project);for (i,item) in p.items.values_mut().enumerate(){item.label=if i%2==0{filmcraft_project::Label::Green}else{filmcraft_project::Label::Rose}}for (i,clip) in p.sequence_mut(seq).unwrap().all_tracks_mut().flat_map(|t|t.items.iter_mut()).enumerate(){clip.label=if i%2==0{filmcraft_project::Label::Green}else{filmcraft_project::Label::Rose}}reverse(&mut p.root);}
 let base=s.project.clone();let clips:Vec<_>=base.sequence(seq).unwrap().all_tracks().flat_map(|t|t.items.iter()).map(|c|c.id).take(2).collect();let items:Vec<_>=base.items.keys().take(2).copied().collect();
 let mut cases=Vec::new();
 let selections=vec![vec![],vec![clips[0]],clips.clone(),vec![filmcraft_project::ClipId(u64::MAX)],vec![clips[0],clips[0]]];let projects=vec![vec![],vec![items[0]],items.clone(),vec![filmcraft_project::ItemId(u64::MAX)],vec![items[0],items[0]]];
 for locked in [false,true]{for active in [false,true]{for selection in &selections{for project in &projects{for params in [json!({}),json!({"items":[items[0].0],"clips":[clips[0].0]}),json!({"items":[],"clips":[]})]{
  s.project=base.clone();for track in std::sync::Arc::make_mut(&mut s.project).sequence_mut(seq).unwrap().all_tracks_mut(){track.locked=locked;}
  s.state.active_sequence=active.then_some(seq);s.state.selection=selection.clone();s.state.project_selection=project.clone();let before=serde_json::to_value(s.project.as_ref())?;
  let result=s.execute("edit.selectLabelGroup",params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"locked":locked,"active":active,"selection":selection.iter().map(|id|id.0).collect::<Vec<_>>(),"projectSelection":project.iter().map(|id|id.0).collect::<Vec<_>>(),"params":params,"ok":result.is_ok(),"result":result.ok(),"afterSelection":s.state.selection.iter().map(|id|id.0).collect::<Vec<_>>(),"afterProjectSelection":s.state.project_selection.iter().map(|id|id.0).collect::<Vec<_>>(),"unchanged":hash(&before)==hash(&after)}));
 }}}}}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"project":serde_json::to_value(base.as_ref())?,"cases":cases}))?)?;Ok(())
}
