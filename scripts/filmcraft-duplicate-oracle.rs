// Actual original edit.duplicate, including every ItemKind and Session edit-cycle refusal.
use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let seq=s.state.active_sequence.unwrap();
 {let p=std::sync::Arc::make_mut(&mut s.project);let source=p.items.values().find(|i|matches!(i.kind,filmcraft_project::ItemKind::Media(_))).unwrap().clone();
 p.add_item("Subclip",source.label,filmcraft_project::ItemKind::Subclip{parent:source.id,range:filmcraft_time::TimeRange::new(filmcraft_time::Tick(0),filmcraft_time::Tick(254016000000)),restrict_trims:true},None);
 p.add_item("Graphic",source.label,filmcraft_project::ItemKind::Graphic{width:1920,height:1080,rate:filmcraft_time::FrameRate::default()},None);
 p.add_item("Adjustment",source.label,filmcraft_project::ItemKind::AdjustmentLayer{width:1920,height:1080,rate:filmcraft_time::FrameRate::default(),duration:filmcraft_time::Tick(254016000000)},None);
 for item in p.items.values_mut(){item.metadata.insert("Description".into(),"should not copy 🎬".into());item.split.video_in=Some(filmcraft_time::Tick(42));}
 }
 let base=s.project.clone();let ids:Vec<_>=base.items.keys().copied().collect();let mut selections=vec![vec![],vec![filmcraft_project::ItemId(u64::MAX)],ids.clone(),vec![ids[0],ids[0]],vec![ids[0],filmcraft_project::ItemId(base.next_id)],vec![filmcraft_project::ItemId(u64::MAX),ids[0]]];for id in &ids{selections.push(vec![*id]);}let mut cases=Vec::new();
 for profile in 0..3{for selected in &selections{for params in [json!({}),json!({"items":[ids[0].0]})]{
  s.project=base.clone();let p=std::sync::Arc::make_mut(&mut s.project);if profile>0{let target=if profile==1{filmcraft_project::ItemId(p.next_id)}else{seq};p.sequence_mut(seq).unwrap().video_tracks[0].items[0].item=target;}
  s.state.project_selection=selected.clone();s.state.active_sequence=Some(seq);let before=serde_json::to_value(s.project.as_ref())?;let undo=s.history.undo.len();let result=s.execute("edit.duplicate",params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"profile":profile,"selection":selected.iter().map(|id|id.0).collect::<Vec<_>>(),"params":params,"ok":result.is_ok(),"result":result.ok(),"afterSelection":s.state.project_selection.iter().map(|id|id.0).collect::<Vec<_>>(),"afterSha256":hash(&after),"changed":hash(&before)!=hash(&after),"undoDelta":s.history.undo.len().saturating_sub(undo)}));
 }} }
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"project":serde_json::to_value(base.as_ref())?,"cases":cases}))?)?;Ok(())
}
