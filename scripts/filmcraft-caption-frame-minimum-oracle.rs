// Executes original exact-frame admission; never uses host rounded-us code.
use filmcraft_project::{CaptionFormat,CaptionTrack,Project,TrackId};
use filmcraft_time::{FrameRate,Tick};
use serde_json::json;
fn main(){
 let mut cases=vec![];
 for rate in [FrameRate::FPS_24,FrameRate::FPS_23_976,FrameRate::FPS_29_97]{
  for short in [false,true]{
   let mut p=Project::new("minimum");let sid=p.new_sequence("s",Default::default(),1,1,None);
   let mut seq=p.sequence(sid).unwrap().clone();
   seq.caption_tracks.push(CaptionTrack::new(TrackId(900),"Captions".into(),CaptionFormat::Subtitle));
   let minimum=rate.frame_duration();let duration=Tick(minimum.0-i64::from(short));
   let mut next_id=1000;let mut ctx=filmcraft_edit::EditCtx{next_id:&mut next_id,media_duration:&|_|None,media_start:&|_|Tick::ZERO,min_duration:minimum};
   let result=filmcraft_edit::captions::add_caption(&mut seq,TrackId(900),Tick::ZERO,duration,"Frame",&mut ctx);
   assert_eq!(result.is_ok(),!short);
   if short{assert!(seq.caption_tracks[0].captions.is_empty());}
   let point=Tick(rate.tick_of(13).0+1);
   cases.push(json!({"rate":rate,"minimum":minimum.0.to_string(),"duration":duration.0.to_string(),"success":result.is_ok(),"after":seq.caption_tracks,"point":point.0.to_string(),"floor":rate.snap(point).0.to_string(),"nearest":rate.snap_nearest(point).0.to_string()}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
