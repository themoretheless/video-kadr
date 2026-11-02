// Execute the actual pinned engine, including command admission and sequence validation.
use filmcraft_project::*;
use filmcraft_time::Tick;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;
 let sequence=s.state.active_sequence.unwrap();
 let q=std::sync::Arc::make_mut(&mut s.project).sequence_mut(sequence).unwrap();
 let first=q.video_tracks[0].items[0].id;let second=q.video_tracks[1].items[0].id;
 let audio=q.audio_tracks[0].items[0].id;
 q.video_tracks[0].locked=true;q.video_tracks[0].items[0].link=Some(777);q.audio_tracks[0].items[0].link=Some(777);
 let mut effects=vec![];
 for (i,id) in ["motion","opacity","time_remap","volume","channel_volume","panner","brightness_contrast","brightness_contrast","graphic_text","gaussian_blur"].iter().enumerate(){
  let mut e=find_effect(id).unwrap().instance();resolve_auto_points(&mut e,(1920,1080),(640,480));e.enabled=false;e.essential=i==7;e.post_fader=i==6;
  for p in e.params.values_mut(){for time in [-7,13,99]{p.keyframes.push(keyframe::Keyframe::new(Tick(time),p.value.clone()));}}
  let mut mask=mask::Mask::new("Animated",mask::MaskPath::rect(0.,0.,20.,30.));
  for p in [&mut mask.path,&mut mask.feather,&mut mask.opacity,&mut mask.expansion]{for time in [-7,13,99]{p.keyframes.push(keyframe::Keyframe::new(Tick(time),p.value.clone()));}}
  e.masks.push(mask);effects.push(e);
 }
 let mut unknown=effects[6].clone();unknown.effect="future_extension".into();effects.push(unknown);
 let mut video=q.video_tracks[0].items[0].clone();video.source_in=Tick(10);video.duration=Tick(80);video.speed=-1.25;video.effects=effects.clone();
 let mut sound=q.audio_tracks[0].items[0].clone();sound.source_in=Tick(25);sound.duration=Tick(40);sound.speed=0.5;sound.effects=effects.clone();
 for t in q.video_tracks.iter_mut().chain(q.audio_tracks.iter_mut()){for c in &mut t.items{c.effects=effects.clone();c.source_in=Tick(90);c.speed=1.5;c.reverse=true;c.frame_hold=Some(Tick(77));}}
 let base=s.project.clone();let clipboards=vec![vec![],vec![(TrackKind::Video,0,video.clone())],vec![(TrackKind::Audio,0,sound.clone())],vec![(TrackKind::Video,0,video.clone()),(TrackKind::Audio,0,sound),(TrackKind::Video,1,video)]];
 let choices=vec![json!({}),json!({"effects":false}),json!({"motion":false,"opacity":false,"timeRemapping":false,"volume":false,"channelVolume":false,"panner":false,"effects":[]}),json!({"effects":["brightness_contrast",7,"unknown_extension"]}),json!({"scaleTimes":false}),json!({"motion":"ignored","effects":true}),json!({"clips":[first.0,first.0,999999]}),json!({"clips":[second.0,audio.0]}),json!({"clips":[]}),json!({"clip":second.0}),json!({"clips":[999999]}),json!({"clips":[first.0 as f64+0.5]})];
 let mut cases=vec![];
 for active in [Some(sequence),None]{for selection in [vec![],vec![first],vec![ClipId(999999)]]{for linked in [false,true]{for (ci,clipboard)in clipboards.iter().enumerate(){for command in ["edit.pasteAttributes","edit.removeAttributes"]{for params in &choices{
  s.project=base.clone();s.history.undo.clear();s.history.redo.clear();s.state.active_sequence=active;s.state.selection=selection.clone();s.state.linked_selection=linked;s.state.clipboard=clipboard.clone();
  let result=s.execute(command,params.clone());let after=serde_json::to_value(s.project.as_ref())?;
  cases.push(json!({"command":command,"params":params,"context":{"activeSequence":active,"selection":selection,"linkedSelection":linked},"clipboard":ci,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undo":s.history.undo.len()}));
 }}}}}}
 let clipboards:Vec<_>=clipboards.iter().map(|clips|json!({"format":"video-kadr.filmcraft-clipboard","version":1,"clips":clips,"markers":[]})).collect();
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"clipboards":clipboards,"cases":cases}))?)?;Ok(())
}
