// Original replacement admission, media capabilities, match frame and short-media report.
use filmcraft_project::*;use filmcraft_time::{Tick,TimeRange,FrameRate};use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let sequence=s.state.active_sequence.unwrap();let p=std::sync::Arc::make_mut(&mut s.project);
 let media=p.items.values().find(|i|i.as_media().is_some_and(|m|m.info.has_video())).unwrap().clone();let audio=p.items.values().find(|i|i.as_media().is_some_and(|m|!m.info.has_video())).unwrap().id;
 let mut short=media.kind.clone();if let ItemKind::Media(m)=&mut short{m.info.duration=Tick(254016000000/2);m.mark_in=Some(Tick(13));m.interpret.frame_rate=Some(FrameRate::FPS_25);}
 let short=p.add_item("Short",Label::Mango,short,None);
 let sub=p.add_item("Restricted",Label::Rose,ItemKind::Subclip{parent:short,range:TimeRange::new(Tick(100),Tick(100000000000)),restrict_trims:true},None);
 let unrestricted=p.add_item("Unrestricted",Label::Yellow,ItemKind::Subclip{parent:short,range:TimeRange::new(Tick(25),Tick(100000000000)),restrict_trims:false},None);
 let broken=p.add_item("Broken",Label::Rose,ItemKind::Subclip{parent:ItemId(u64::MAX),range:TimeRange::new(Tick(100),Tick(100000000000)),restrict_trims:true},None);
 let graphic=p.add_item("Graphic",Label::Blue,ItemKind::Graphic{width:320,height:240,rate:FrameRate::FPS_25},None);
 let adjust=p.add_item("Adjustment",Label::Blue,ItemKind::AdjustmentLayer{width:320,height:240,rate:FrameRate::FPS_25,duration:Tick(100000000000)},None);
 let mut still=media.kind.clone();if let ItemKind::Media(m)=&mut still{m.info.kind=filmcraft_media::MediaKind::Still;m.info.duration=Tick(0);m.mark_in=Some(Tick(7));}let still=p.add_item("Still",Label::Teal,still,None);
 p.item_mut(audio).unwrap().as_media_mut().unwrap().interpret.audio_channels=Some(AudioChannelMap{format:AudioChannels::Stereo,clips:vec![vec![1,0],vec![0]]});
 let q=p.sequence_mut(sequence).unwrap();let first=q.video_tracks[0].items[0].id;let second=q.video_tracks[1].items[0].id;let sound=q.audio_tracks[0].items[0].id;
 q.video_tracks[0].locked=true;q.video_tracks[0].items[0].link=Some(777);q.audio_tracks[0].items[0].link=Some(777);
 for t in q.video_tracks.iter_mut().chain(q.audio_tracks.iter_mut()){for c in &mut t.items{c.source_channels=vec![0];c.source_in=Tick(200000000000);c.speed=-1.25;c.reverse=true;c.frame_hold=Some(Tick(17));}}
 let base=s.project.clone();let sources=[None,Some(media.id),Some(short),Some(sub),Some(unrestricted),Some(broken),Some(graphic),Some(adjust),Some(still),Some(audio),Some(sequence),Some(ItemId(u64::MAX))];
 let choices:Vec<Value>=vec![json!({}),json!({"clips":[first.0]}),json!({"clips":[sound.0]}),json!({"clips":[first.0,first.0,sound.0,999999]}),json!({"clips":[]}),json!({"clip":second.0}),json!({"clips":[999999]}),json!({"item":short.0}),json!({"item":sub.0,"keepSourceIn":true}),json!({"item":unrestricted.0,"keepSourceIn":true}),json!({"item":graphic.0}),json!({"item":u64::MAX}),json!({"keepSourceIn":true}),json!({"item":audio.0,"keepSourceIn":false})];
 let mut cases=vec![];
 for (si,source)in sources.into_iter().enumerate(){for mode in 0..4{for command in ["clip.replaceFromSource","clip.replaceFromSourceMatchFrame","clip.replaceFromBin"]{for params in &choices{
  s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.state.active_sequence=if mode==3{None}else{Some(sequence)};s.state.source_item=source;s.state.selection=if mode==0{vec![]}else if mode==2{vec![ClipId(999999)]}else{vec![first]};s.state.project_selection=if mode==0{vec![]}else if mode==2{vec![graphic,sub]}else{source.into_iter().collect()};s.state.linked_selection=mode!=2;
  s.state.source_playhead=Tick(if si%2==0{0}else{300000000000});s.state.playheads.insert(sequence,Tick(if mode==2{-100000000000}else{100000000000}));
  let context=json!({"activeSequence":s.state.active_sequence,"sourceItem":source,"selection":s.state.selection,"projectSelection":s.state.project_selection,"linkedSelection":s.state.linked_selection,"sourcePlayhead":s.state.source_playhead.0,"playhead":s.playhead().0});
  let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":command,"params":params,"context":context,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undo":s.history.undo.len()}));
 }}}}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"cases":cases}))?)?;Ok(())
}
