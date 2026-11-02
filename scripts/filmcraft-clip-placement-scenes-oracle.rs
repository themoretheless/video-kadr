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

fn main(){let mut cases=vec![];
for start in [0,10*T]{for duration in [T,10*T]{for speed in [-2f64,-0.5,0.5,1.,1.75,2.,16.]{for reverse in [false,true]{for hold in 0..3{
let mut p=Project::new("Native placement scene");p.next_id=1+cases.len() as u64*100;
let generator=Generator::ColorMatte{color:[1.,0.,0.,1.]};let src=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(1000*T));let mut info=src.info().clone();info.video.as_mut().unwrap().has_alpha=true;let src=Arc::new(PatternSource{info:info.clone(),side:0}) as SharedSource;
let media=p.add_item("Media",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info,interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let sid=p.new_sequence("Sequence",SequenceSettings{width:4,height:4,..Default::default()},1,0,None);
let held=20*T+(duration as f64*speed.abs()).round() as i64/2;
let item:TrackItem=serde_json::from_value(json!({"id":100,"item":media,"name":"Native placement","label":"Iris","start":start,"duration":duration,"source_in":20*T,"speed":speed,"reverse":reverse,"frame_hold":if hold>0{Some(held)}else{None},"hold_filters":hold==2,"enabled":true,"effects":[]})).unwrap();
p.sequence_mut(sid).unwrap().video_tracks[0].items.push(item.clone());
for offset in [0,1,T/4,duration-2,duration-1]{let at=start+offset;
let source=|id|if id==media{Some(src.clone())}else{None};let frame=render_sequence(&p,sid,Tick(at),RenderOptions{working_output:true,..Default::default()},&source);
cases.push(json!({"item":item,"at":at,"sourceTime":item.source_time_at(Tick(at)).0,"bits":frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
