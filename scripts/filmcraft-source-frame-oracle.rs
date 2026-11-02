// Original full scene: one animated Float control per eligible effect, source/effect/local clocks, and Hold Filters.
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Transition};
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::{render_sequence,RenderOptions};
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
struct PatternSource { info: filmcraft_media::MediaInfo, side:u64 }
fn pattern(side:u64, tick:i64)->Vec<u8>{
 let q=tick.max(0)as u64/(T as u64/10);
 (0..16u64).flat_map(|i|{let(x,y)=(i%4,i/4);[
  ((11+x*37+y*19+q*13+side*71)%256)as u8,
  ((29+x*17+y*43+q*23+side*31)%256)as u8,
  ((47+x*53+y*7+q*41+side*89)%256)as u8,
  [64,128,192,255][((x+y+q+side)%4)as usize],
 ]}).collect()
}
impl MediaSource for PatternSource {
 fn info(&self)->&filmcraft_media::MediaInfo{&self.info}
 fn video_frame(&self,request:filmcraft_media::FrameRequest)->filmcraft_media::Result<Arc<filmcraft_frame::VideoFrame>>{
  Ok(Arc::new(filmcraft_frame::VideoFrame::rgba8(4,4,pattern(self.side,request.time.0)).with_pts(request.time)))
 }
 fn audio(&self,_:i64,_:usize,_:u32)->filmcraft_media::Result<filmcraft_frame::AudioBuffer>{Err(filmcraft_media::MediaError::NoStream("audio"))}
}

fn main(){
 let input:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
 for case in input["cases"].as_array().unwrap(){
  let mut document=case["document"].clone();
  for item in document["project"]["items"].as_object_mut().unwrap().values_mut(){
   if let Some(video)=item.pointer_mut("/kind/Media/info/video"){video["width"]=json!(4);video["height"]=json!(4);video["has_alpha"]=json!(true);}
   if let Some(sequence)=item.pointer_mut("/kind/Sequence"){sequence["settings"]["width"]=json!(8);sequence["settings"]["height"]=json!(4);}
  }
  let p:Project=serde_json::from_value(document["project"].clone()).unwrap();
  let info=p.items.values().find_map(|item|match &item.kind{ItemKind::Media(m)=>Some(m.info.clone()),_=>None}).unwrap();
  let source=|id:filmcraft_project::ItemId|Some(Arc::new(PatternSource{info:info.clone(),side:id.0%2})as SharedSource);
  for scale in [1.,0.5,0.25]{
   let out=render_sequence(&p,filmcraft_project::ItemId(case["sequence"].as_u64().unwrap()),Tick(case["at"].as_i64().unwrap()),RenderOptions{scale,working_output:true,..Default::default()},&source);
   cases.push(json!({"document":document,"sequence":case["sequence"],"at":case["at"],"mode":case["mode"],"scale":scale,"width":out.w,"height":out.h,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sourcePattern":"spatial-time-alpha-v1","cases":cases}));
}
