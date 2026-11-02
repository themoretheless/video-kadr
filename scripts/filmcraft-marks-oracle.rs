// Original MIT engine executes every case; no native implementation is linked.
use filmcraft_project::*;
use filmcraft_time::Tick;
use serde_json::json;
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let mut s = filmcraft_engine::Session::default(); s.execute("file.openDemoProject",json!({}))?;
 let seq=s.state.active_sequence.unwrap();
 let p=std::sync::Arc::make_mut(&mut s.project);
 let media=p.items.values().find(|i|matches!(i.kind,ItemKind::Media(_))).unwrap().clone();
 p.add_item("Subclip",media.label,ItemKind::Subclip{parent:media.id,range:filmcraft_time::TimeRange::new(Tick(0),Tick(1000)),restrict_trims:true},None);
 p.add_item("Graphic",media.label,ItemKind::Graphic{width:1920,height:1080,rate:filmcraft_time::FrameRate::default()},None);
 p.add_item("Adjustment",media.label,ItemKind::AdjustmentLayer{width:1920,height:1080,rate:filmcraft_time::FrameRate::default(),duration:Tick(1000)},None);
 for item in p.items.values_mut(){item.split=SplitMarks{video_in:Some(Tick(200)),audio_in:Some(Tick(250)),video_out:Some(Tick(400)),audio_out:Some(Tick(450))};match &mut item.kind{ItemKind::Media(m)=>{m.mark_in=Some(Tick(100));m.mark_out=Some(Tick(500));},ItemKind::Sequence(q)=>{q.mark_in=Some(Tick(100));q.mark_out=Some(Tick(500));q.split=item.split;},_=>{}}}
 let base=s.project.clone();let mut sources=vec![None,Some(ItemId(u64::MAX))];sources.extend(base.items.keys().copied().map(Some));
 let commands=["project.setMarks","markers.markIn","markers.markOut","markers.clearIn","markers.clearOut","markers.clearInOut","markers.markSplitVideoIn","markers.markSplitVideoOut","markers.markSplitAudioIn","markers.markSplitAudioOut"];
 let params=[json!({}),json!({"in":null,"out":null}),json!({"in":600,"out":-17,"time":600}),json!({"in":true,"out":"invalid","time":false,"frame":2}),json!({"time":-30,"frame":9,"seconds":1.2}),json!({"seconds":1.2}),json!({"timecode":"+15"}),json!({"timecode":"bad"}),json!({"time":i64::MAX,"in":i64::MAX}),json!({"time":i64::MIN,"out":i64::MIN}),json!({"in":1.25,"out":false}),json!({"item":media.id.0,"in":77,"out":99})];
 let mut cases=vec![];
 for active in [None,Some(seq),Some(ItemId(u64::MAX))]{for source in &sources{for target in ["program","source"]{for command in commands{for input in &params{
  s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.state.active_sequence=active;s.state.source_item=*source;s.state.source_playhead=Tick(333);s.state.playheads.clear();if let Some(active)=active{s.state.playheads.insert(active,Tick(123));}
  let mut params=input.clone();params["target"]=json!(target);
  let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":command,"params":params,"context":{"activeSequence":active,"sourceItem":source,"sourcePlayhead":333,"playhead":123},"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undoDelta":s.history.undo.len()}));
 }}}}}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"cases":cases}))?)?;Ok(())
}
