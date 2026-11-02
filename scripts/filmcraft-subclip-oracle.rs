// Expected subclip edits/defaults from the actual pinned MIT engine.
use filmcraft_project::*;
use filmcraft_time::{Tick,TimeRange,FrameRate};
use serde_json::{json,Value};use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let sequence=s.state.active_sequence.unwrap();let p=std::sync::Arc::make_mut(&mut s.project);
 let media=p.items.values().find(|i|i.as_media().is_some_and(|m|m.info.has_video())).unwrap().clone();let audio=p.items.values().find(|i|i.as_media().is_some_and(|m|!m.info.has_video())).unwrap().id;
 let marker_ids:Vec<_>=(0..4).map(|_|MarkerId(p.alloc_id())).collect();
 {let m=p.item_mut(media.id).unwrap().as_media_mut().unwrap();m.interpret.frame_rate=Some(FrameRate::FPS_25);m.mark_in=Some(FrameRate::FPS_25.tick_of(10));m.mark_out=Some(FrameRate::FPS_25.tick_of(30));m.offline=true;m.markers=[0,100,125,150].into_iter().zip(marker_ids).map(|(frame,id)|Marker{id,start:FrameRate::FPS_25.tick_of(frame),duration:Tick(0),name:format!("Marker {frame}"),comment:"Inherited".into(),kind:MarkerKind::default(),color:media.label}).collect();}
 let parent=p.add_bin("Media bin",None);let child=p.add_bin("Nested media bin",Some(parent));p.move_to_bin(&[media.id],Some(child));p.root.children.push(BinEntry::Item(media.id));
 let outer=p.add_item("Outer",Label::Mango,ItemKind::Subclip{parent:media.id,range:TimeRange::new(FrameRate::FPS_25.tick_of(100),FrameRate::FPS_25.tick_of(50)),restrict_trims:false},None);
 let nested=p.add_item("Nested",Label::Rose,ItemKind::Subclip{parent:outer,range:TimeRange::new(FrameRate::FPS_25.tick_of(125),FrameRate::FPS_25.tick_of(10)),restrict_trims:true},None);
 let broken=p.add_item("Broken",media.label,ItemKind::Subclip{parent:ItemId(u64::MAX),range:TimeRange::new(Tick(100),Tick(500)),restrict_trims:true},None);
 let cycle=p.add_item("Cycle",media.label,ItemKind::Subclip{parent:ItemId(p.next_id),range:TimeRange::new(Tick(100),Tick(500)),restrict_trims:true},None);
 let graphic=p.add_item("Graphic",media.label,ItemKind::Graphic{width:320,height:240,rate:FrameRate::FPS_25},None);
 let mut still=media.kind.clone();if let ItemKind::Media(m)=&mut still{m.info.kind=filmcraft_media::MediaKind::Still;m.info.duration=Tick(0);m.mark_in=None;m.mark_out=None;}let still=p.add_item("Still",media.label,still,None);
 let mut ntsc=media.kind.clone();if let ItemKind::Media(m)=&mut ntsc{m.interpret.frame_rate=Some(FrameRate::FPS_29_97);}let ntsc=p.add_item("NTSC",media.label,ntsc,None);
 let mut chain=vec![];let mut last=media.id;for i in 0..17{last=p.add_item(&format!("Chain {i}"),media.label,ItemKind::Subclip{parent:last,range:TimeRange::new(Tick(100),Tick(1000)),restrict_trims:true},None);chain.push(last);}
 {let item=p.item_mut(outer).unwrap();item.metadata.insert("Description".into(),"Keep on conversion".into());item.split.video_in=Some(Tick(77));}
 let first=p.sequence(sequence).unwrap().video_tracks[0].items[0].id;let second=p.sequence(sequence).unwrap().video_tracks[1].items[0].id;
 {let q=p.sequence_mut(sequence).unwrap();let clip=&mut q.video_tracks[0].items[0];clip.name="Timeline 🎬".into();clip.label=Label::Yellow;clip.source_in=FrameRate::FPS_25.tick_of(10);clip.duration=FrameRate::FPS_25.tick_of(20);clip.speed=-1.25;clip.reverse=true;clip.frame_hold=Some(Tick(9));q.video_tracks[1].items[0].item=nested;}
 let mut gfx=p.make_track_item(graphic,TrackKind::Video,Tick(100*254016000000),TimeRange::new(Tick(0),Tick(254016000000)),FrameRate::FPS_25).unwrap();let gfx_clip=gfx.id;for e in &mut gfx.effects{resolve_auto_points(e,(1920,1080),(320,240));}p.sequence_mut(sequence).unwrap().video_tracks[0].items.push(gfx);
 let base=s.project.clone();let contexts=vec![
 (Some(sequence),None,vec![],vec![]),(Some(sequence),Some(media.id),vec![outer],vec![gfx_clip]),(Some(sequence),Some(outer),vec![media.id],vec![first]),(Some(sequence),Some(ItemId(u64::MAX)),vec![graphic,ItemId(u64::MAX),media.id],vec![]),
 (Some(sequence),Some(cycle),vec![broken,outer],vec![first]),(Some(sequence),None,vec![graphic],vec![first]),(Some(sequence),None,vec![],vec![ClipId(u64::MAX),first]),(None,None,vec![outer],vec![]),(None,Some(media.id),vec![],vec![]),
 (Some(sequence),Some(nested),vec![nested],vec![]),(Some(sequence),Some(chain[15]),vec![cycle,broken],vec![first]),(Some(sequence),Some(still),vec![media.id],vec![]),(Some(sequence),Some(audio),vec![outer],vec![]),(Some(sequence),Some(ntsc),vec![chain[14]],vec![second]),
 (Some(ItemId(u64::MAX)),None,vec![],vec![first]),(Some(sequence),None,vec![chain[15]],vec![]),
 ];
 let params:Vec<Value>=vec![json!({}),json!({"item":media.id.0}),json!({"item":outer.0}),json!({"item":nested.0}),json!({"item":broken.0}),json!({"item":cycle.0}),json!({"item":chain[14].0}),json!({"item":chain[15].0}),json!({"item":graphic.0}),json!({"item":sequence.0}),json!({"item":u64::MAX}),json!({"clip":first.0}),json!({"clip":second.0}),json!({"clip":gfx_clip.0}),json!({"clip":u64::MAX}),json!({"item":media.id.0,"clip":second.0}),json!({"name":""}),json!({"name":"   "}),json!({"name":"Имя 🎬","restrictTrims":false}),json!({"start":0,"end":FrameRate::FPS_25.tick_of(20).0}),json!({"start":-1,"end":99}),json!({"start":100,"end":100}),json!({"startFrame":3,"endFrame":19}),json!({"start":null,"startFrame":2,"end":false,"endFrame":9}),json!({"start":1.25,"end":"ignored"}),json!({"start":0,"startFrame":20,"end":999,"endFrame":2}),json!({"start":i64::MAX-100,"end":i64::MAX}),json!({"convertToMaster":true}),json!({"item":outer.0,"convertToMaster":true,"startFrame":5,"endFrame":6}),json!({"item":nested.0,"convertToMaster":true}),json!({"item":broken.0,"convertToMaster":false,"restrictTrims":false}),json!({"restrictTrims":"ignored","convertToMaster":"ignored"}),json!({"item":media.id.0 as f64+0.9}),json!({"item":ntsc.0,"startFrame":3,"endFrame":17}),json!({"item":still.0}),json!({"item":audio.0})];
 let mut cases=vec![];
 for (active,source,project,selection)in contexts{for command in ["clip.makeSubclip","clip.editSubclip"]{for params in &params{
  s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.state.active_sequence=active;s.state.source_item=source;s.state.project_selection=project.clone();s.state.selection=selection.clone();s.prefs.timeline.still_image_duration=17.5;s.prefs.timeline.still_image_unit=if source==Some(still){"frames"}else{"seconds"}.into();
  let defaults=filmcraft_engine::clip_ops::subclip_defaults(&s,params);let context=json!({"activeSequence":active,"sourceItem":source,"projectSelection":project,"selection":selection,"stillImageDuration":s.prefs.timeline.still_image_duration,"stillImageUnit":s.prefs.timeline.still_image_unit});
  let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":command,"params":params,"context":context,"defaults":defaults,"ok":result.is_ok(),"result":result.ok(),"afterSelection":s.state.project_selection,"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undo":s.history.undo.len()}));
 }}}
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"cases":cases}))?)?;Ok(())
}
