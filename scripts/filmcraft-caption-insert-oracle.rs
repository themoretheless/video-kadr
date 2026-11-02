// Executes pinned original global insert, including caption sync-lock filtering.
use filmcraft_project::{Caption,CaptionFormat,CaptionTrack,ClipId,Project,TrackId,TrackItem,TrackKind};
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
 for (name,locked,sync_lock,at,duration) in [
  ("boundaries",false,true,3i64,2i64),("locked",true,true,3,2),("sync-off",false,false,3,2),
  ("fractional-frame",false,true,137729592001,10594584000),
 ]{
  let mut p=Project::new("insert");let sid=p.new_sequence("s",Default::default(),1,1,None);let mut seq=p.sequence(sid).unwrap().clone();
  let delta=if name=="fractional-frame"{at-3}else{0};
  for (i,(start,end)) in [(0,3),(0,4),(0,7),(3,5),(4,5),(3,7),(5,7),(0,10)].into_iter().enumerate(){
   let mut tr=CaptionTrack::new(TrackId(900+i as u64),format!("Caption {i}"),CaptionFormat::Subtitle);
   tr.locked=locked;tr.sync_lock=sync_lock;
   tr.captions.push(Caption{id:ClipId(1000+i as u64),start:Tick(start+delta),duration:Tick(end-start),text:format!("cue{i}"),speaker:Some("Voice".into()),cue_id:Some(format!("file{i}")),settings:"line:90%".into()});
   seq.caption_tracks.push(tr);
  }
  let before=serde_json::to_value(&seq.caption_tracks).unwrap();
  let target=seq.tracks(TrackKind::Video)[0].id;
  let item:TrackItem=serde_json::from_value(json!({"id":100000,"item":1,"name":"insert","label":"Iris","start":at,"duration":duration,"source_in":0,"speed":1,"enabled":true,"effects":[]})).unwrap();
  let mut next_id=2000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
  filmcraft_edit::insert(&mut seq,vec![(target,item)],&mut ctx).unwrap();
  cases.push(json!({"name":name,"at":at.to_string(),"duration":duration.to_string(),"before":before,"after":seq.caption_tracks}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
