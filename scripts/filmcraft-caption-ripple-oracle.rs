// Executes pinned Filmcraft's global extract, including caption track lock filtering.
use filmcraft_project::{Caption,CaptionFormat,CaptionTrack,ClipId,Project,TrackId};
use filmcraft_time::{Tick,TimeRange};
use serde_json::json;
fn main(){
 let mut cases=vec![];
 for (name,locked,sync_lock,holes) in [
  ("boundaries",false,true,vec![(3,5)]),
  ("locked",true,true,vec![(3,5)]),
  ("sync-off",false,false,vec![(3,5)]),
  ("multiple",false,true,vec![(6,8),(2,4)]),
 ] {
  // Each boundary cue has a separate track to retain source non-overlap invariants.
  let mut p=Project::new("ripple");let sid=p.new_sequence("s",Default::default(),1,1,None);
  let mut seq=p.sequence(sid).unwrap().clone();
  for (i,(start,end)) in [(0,3),(0,4),(0,7),(3,5),(4,5),(3,7),(5,7),(0,10)].into_iter().enumerate(){
   let mut tr=CaptionTrack::new(TrackId(900+i as u64),format!("Caption {i}"),CaptionFormat::Subtitle);
   tr.locked=locked;tr.sync_lock=sync_lock;
   tr.captions.push(Caption{id:ClipId(1000+i as u64),start:Tick(start),duration:Tick(end-start),text:format!("cue{i}"),speaker:Some("Voice".into()),cue_id:Some(format!("file{i}")),settings:"line:90%".into()});
   seq.caption_tracks.push(tr);
  }
  let before=serde_json::to_value(&seq.caption_tracks).unwrap();
  let mut next_id=2000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:Tick(1)};
  for &(a,b) in &holes {filmcraft_edit::extract(&mut seq,&[],TimeRange{start:Tick(a),duration:Tick(b-a)},&mut ctx);}
  cases.push(json!({"name":name,"holes":holes,"before":before,"after":seq.caption_tracks}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
