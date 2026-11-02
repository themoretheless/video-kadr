use filmcraft_captions::{Cue,Document};
use filmcraft_time::{FrameRate,Tick,TICKS_PER_SECOND};
use filmcraft_project::{CaptionFormat,TrackId};
use serde_json::json;
fn document(doc:&Document)->serde_json::Value {json!({"cues":doc.cues.iter().map(|c|json!({"start":c.start.0,"end":c.end.0,"text":c.text,"speaker":c.speaker,"id":c.id,"settings":c.settings})).collect::<Vec<_>>(),"blocks":doc.blocks,"warnings":doc.warnings})}
fn main(){
 let mut cases=Vec::new();
 for rate in [FrameRate::FPS_23_976,FrameRate::FPS_29_97,FrameRate::FPS_25] {
  let input=Document{cues:vec![
   Cue{start:Tick(TICKS_PER_SECOND+1),end:Tick(TICKS_PER_SECOND*2+1),text:"fractional".into(),speaker:Some("Ann".into()),id:Some("cue-id".into()),settings:"align:start".into()},
   Cue{start:Tick(TICKS_PER_SECOND*2-1),end:Tick(TICKS_PER_SECOND*2+2),text:"short".into(),..Default::default()},
   Cue{start:Tick(-TICKS_PER_SECOND),end:Tick(TICKS_PER_SECOND/2),text:"negative".into(),..Default::default()},
  ],blocks:vec!["NOTE retained".into()],warnings:vec!["original warning".into()]};
  let mut snapped=input.clone();snapped.snap_to_frames(rate);
  let mut next=10;let track=filmcraft_captions::track_from_document(&snapped,TrackId(1),"Imported",CaptionFormat::Subtitle,&mut ||{next+=1;next});
  for offset in [Tick::ZERO,Tick(TICKS_PER_SECOND*3/2)] {
   let output=filmcraft_captions::document_from_track(&track,offset);
   cases.push(json!({"rate":rate,"offset":offset.0,"input":document(&input),"snapped":document(&snapped),"exported":document(&output)}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
