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
 let mut count=0;
 for definition in filmcraft_project::effect_defs().iter().filter(|d|d.id=="move") {
 let parameters:Vec<_>=definition.params.iter().filter(|p|p.animatable&&matches!(p.kind,filmcraft_project::ParamKind::Point)).collect();
 count+=parameters.len();
 for parameter in parameters {for auto in 0..3 {for blur in [0.,70.] {
 let effect=definition.id;
 let low=filmcraft_geom::Vec2::new(1.25000000000001,0.50000000000002);
 let high=filmcraft_geom::Vec2::new(3.25000000000003,2.75000000000004);
 let mut instance=definition.instance();
 instance.params.get_mut(parameter.id).unwrap().keyframes=vec![
 filmcraft_project::Keyframe::new(Tick(18*T),filmcraft_project::ParamValue::Vec2(filmcraft_geom::Vec2::new(if auto!=1{f64::NAN}else{1.25},if auto==0{f64::NAN}else{0.5}))),
 filmcraft_project::Keyframe::new(Tick(32*T),filmcraft_project::ParamValue::Vec2(filmcraft_geom::Vec2::new(if auto==1{f64::NAN}else{3.25},if auto!=0{f64::NAN}else{2.75})))];
 instance.params.get_mut("motion_blur").unwrap().value=filmcraft_project::ParamValue::Float(blur);
 let before=instance.params.get(parameter.id).unwrap().clone();
 let source_points:serde_json::Map<String,serde_json::Value>=instance.params.iter().filter(|(_,p)|matches!(p.value,filmcraft_project::ParamValue::Vec2(_))).map(|(k,p)|(k.clone(),serde_json::to_value(p).unwrap())).collect();
 filmcraft_project::resolve_auto_points(&mut instance,(4,4),(4,4));
 for hold in 0..3{for mode in 0..2{for reverse in [false,true]{for at in [9*T,10*T,11*T-1]{
  let mut p=Project::new("transition sequence");p.next_id=1+cases.len()as u64*100;let rate=FrameRate::new(24,1);
  let mut sources=vec![];let mut media=vec![];
  for (side,color) in [[1.,0.,0.,1.],[0.,0.,1.,1.]].into_iter().enumerate(){
   let generator=Generator::ColorMatte{color};let src=GeneratorSource::new(generator.clone(),4,4,rate,Tick(100*T));let mut info=src.info().clone();if let Some(video)=info.video.as_mut(){video.has_alpha=true;}let src=PatternSource{info:info.clone(),side:side as u64};
   let id=p.add_item("matte",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info,interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
   media.push(id);sources.push((id,Arc::new(src)as SharedSource));
  }
  let sid=p.new_sequence("s",SequenceSettings{width:4,height:4,..Default::default()},1,0,None);
  let seq=p.sequence_mut(sid).unwrap();let track=&mut seq.video_tracks[0];
  for (index,id) in [100,101].iter().enumerate(){let item:TrackItem=serde_json::from_value(json!({"id":id,"item":media[index],"name":"matte","label":"Iris","start":index as i64*10*T,"duration":10*T,"source_in":20*T,"speed":1,"enabled":true,"frame_hold":if index==mode&&hold>0{Some(25*T)}else{None},"hold_filters":hold==2,"effects":if index==mode {vec![instance.clone()]}else{vec![]}})).unwrap();track.items.push(item);}
  let tr:Transition=serde_json::from_value(json!({"id":500,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":9*T,"duration":2*T,"from":Some(100),"to":Some(101),"align":"CenterAtCut","reverse":reverse})).unwrap();track.transitions.push(tr);
  let source_time=track.items[mode].source_time_at(Tick(at)).0;let effect_time=track.items[mode].effect_time_at(Tick(at)).0;let local_time=at-track.items[mode].start.0;
  let source=|id|sources.iter().find(|(key,_)|*key==id).map(|(_,source)|source.clone());
  let out=render_sequence(&p,sid,Tick(at),RenderOptions{working_output:true,..Default::default()},&source);
  cases.push(json!({"effect":effect,"motionBlur":blur,"parameter":parameter.id,"sourceParameter":before,"sourcePoints":source_points,"low":low,"high":high,"hold":hold,"sourceTime":source_time.to_string(),"effectTime":effect_time.to_string(),"localTime":local_time.to_string(),"mode":mode,"reverse":reverse,"at":at.to_string(),"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}
 }
 }
 }
 }
 let unsupported:Vec<&str>=vec![];
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","independentSourceIds":true,"unsupportedParameters":unsupported,"sequenceRate":{"numerator":24000,"denominator":1001},"sourcePattern":"spatial-time-alpha-v1","sourceSamples":(0..2).flat_map(|side|[0,19999999,20000000,20500000,29999999,30000000,30500000].map(move|us|json!({"side":side,"timeUs":us,"rgba":pattern(side,us*254016)}))).collect::<Vec<_>>(),"definitions":count,"cases":cases}));
}
