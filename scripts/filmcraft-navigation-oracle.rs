// Native code is not linked: expectations come from original MIT Session.execute.
use filmcraft_project::*;
use filmcraft_time::{Tick,FrameRate,TimeRange};
use serde_json::json;
use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let seq=s.state.active_sequence.unwrap();let p=std::sync::Arc::make_mut(&mut s.project);
 let media=p.items.values().find(|i|matches!(i.kind,ItemKind::Media(_))).unwrap().clone();
 for restrict in [false,true]{p.add_item("Subclip",media.label,ItemKind::Subclip{parent:media.id,range:TimeRange::new(Tick(123456789000),Tick(2540160000000)),restrict_trims:restrict},None);}
 let mut still=media.kind.clone();if let ItemKind::Media(m)=&mut still{m.info.kind=filmcraft_media::MediaKind::Still;m.info.duration=Tick(0);m.interpret.frame_rate=Some(FrameRate::new(1001,17));}p.add_item("Still",media.label,still,None);
 p.add_item("Graphic",media.label,ItemKind::Graphic{width:1920,height:1080,rate:FrameRate::new(1001,17)},None);
 p.add_item("Adjustment",media.label,ItemKind::AdjustmentLayer{width:1920,height:1080,rate:FrameRate::FPS_25,duration:Tick(100000000000)},None);
 for item in p.items.values_mut(){item.split=SplitMarks{video_in:Some(Tick(-17)),video_out:None,audio_in:None,audio_out:Some(Tick(9007199254740997))};match &mut item.kind{ItemKind::Media(m)=>{m.mark_in=Some(Tick(33));m.mark_out=Some(Tick(70000000000));},ItemKind::Sequence(q)=>{q.mark_in=Some(Tick(123456789000));q.mark_out=Some(Tick(70000000000));q.split=item.split;},_=>{}}}
 let base=s.project.clone();let ids:Vec<_>=base.items.keys().copied().collect();let mut sources=vec![None,Some(ItemId(u64::MAX))];sources.extend(ids.iter().copied().map(Some));
 let commands=["source.open","source.setPlayhead","markers.goToIn","markers.goToOut","markers.goToSplitVideoIn","markers.goToSplitVideoOut","markers.goToSplitAudioIn","markers.goToSplitAudioOut"];
 let mut cases=vec![];
 for profile in 0..5 {for active in [None,Some(seq),Some(ItemId(u64::MAX))]{for source in &sources{for command in commands{if profile>=3 && (active!=Some(seq)||*source!=Some(media.id)){continue;}for params in [json!({}),json!({"target":"source","time":-300}),json!({"target":"source","frame":13,"seconds":3.5}),json!({"target":"program","seconds":1e15}),json!({"target":"source","time":false,"frame":1.25}),json!({"item":ids[0].0,"time":i64::MAX})]{
  s.project=base.clone();if profile==1{let p=std::sync::Arc::make_mut(&mut s.project);for item in p.items.values_mut(){item.split=SplitMarks::default();if let ItemKind::Sequence(q)=&mut item.kind{q.split=SplitMarks::default();}}}
  s.history.undo.clear();s.history.redo.clear();s.events.clear();s.state.active_sequence=active;s.state.source_item=*source;s.state.source_playhead=Tick(55);s.state.playheads.clear();if let Some(id)=active{s.state.playheads.insert(id,Tick(77));}
  s.state.selection=vec![ClipId(u64::MAX)];s.state.project_selection=vec![media.id,ItemId(u64::MAX),media.id];s.state.selection_follows_playhead=profile>=2;s.state.linked_selection=profile!=1 && profile!=4;s.state.targeting.clear();if profile>=3{s.state.targeting.insert(seq,filmcraft_engine::Targeting{targeted:if profile==3{vec![]}else{vec![base.sequence(seq).unwrap().video_tracks[0].id]},..Default::default()});}
  let mut context=json!({"activeSequence":active,"sourceItem":source,"sourcePlayhead":55,"playhead":77,"selection":s.state.selection,"projectSelection":s.state.project_selection,"selectionFollowsPlayhead":s.state.selection_follows_playhead,"linkedSelection":s.state.linked_selection});
  if profile>=3{context["targetedTrackIds"]=json!(s.state.targeting[&seq].targeted);}
  let before=serde_json::to_value(s.project.as_ref())?;let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  let events:Vec<_>=s.events.iter().filter_map(|e|if let filmcraft_engine::Event::OpenSource(id)=e{Some(json!({"type":"openSource","item":id.0}))}else{None}).collect();
  cases.push(json!({"profile":profile,"command":command,"params":params,"context":context,"ok":result.is_ok(),"result":result.ok(),"afterContext":{"activeSequence":s.state.active_sequence,"sourceItem":s.state.source_item,"sourcePlayhead":s.state.source_playhead,"playhead":s.playhead(),"playheadFrames":s.sequence_rate().frame_at(s.playhead()),"selection":s.state.selection,"projectSelection":s.state.project_selection},"events":events,"unchanged":before==after,"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undo":s.history.undo.len()}));
 }}}}}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"cases":cases}))?)?;Ok(())
}
