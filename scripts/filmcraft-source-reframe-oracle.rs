use filmcraft_media::{MediaSource,MediaInfo,MediaError,FrameRequest,SharedSource};use filmcraft_frame::{VideoFrame,AudioBuffer};use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;use filmcraft_render::{render_sequence,RenderOptions};use serde_json::{Value,json};use std::{sync::Arc,collections::BTreeSet};
struct ClockSource{info:MediaInfo,variant:u64}
impl MediaSource for ClockSource{
 fn info(&self)->&MediaInfo{&self.info}
 fn video_frame(&self,request:FrameRequest)->filmcraft_media::Result<Arc<VideoFrame>>{
  const T:i64=254016000000;if request.time.0<0||request.time.0>=100*T{return Err(MediaError::NoStream("video"))}
  let video=self.info.video.as_ref().unwrap();let rate=video.frame_rate.sane();let frame=(request.time.0 as i128*rate.num as i128).div_euclid(T as i128*rate.den as i128)as u64;
  let rgba=(0..video.width as u64*video.height as u64).flat_map(|i|[((i*37+frame*29+11)%256)as u8,((i*73+frame*17+51)%256)as u8,((i*19+frame*43+83)%256)as u8,if self.variant==0{255}else{[0,1,64,128,255][(i+frame)as usize%5]}]).collect();Ok(Arc::new(VideoFrame::rgba8(video.width,video.height,rgba)))
 }
 fn audio(&self,_:i64,_:usize,_:u32)->filmcraft_media::Result<AudioBuffer>{Err(MediaError::NoStream("audio"))}
}
fn main(){let fixture:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut seen=BTreeSet::new();let mut cases=vec![];
for base in fixture["cases"].as_array().unwrap(){let mut doc=base["document"].clone();for item in doc["project"]["items"].as_object_mut().unwrap().values_mut(){if let Some(media)=item.pointer_mut("/kind/Media"){media["media"]=json!({"File":{"path":"/original/clock.mkv"}});}}
 if !seen.insert(serde_json::to_string(&json!([doc,base["sequence"],base["at"],base["scale"]])).unwrap()){continue}
 for profile in 0..15{let mut doc=doc.clone();for item in doc["project"]["items"].as_object_mut().unwrap().values_mut(){if let Some(seq)=item.pointer_mut("/kind/Sequence"){for track in seq["video_tracks"].as_array_mut().unwrap(){for clip in track["items"].as_array_mut().unwrap(){let effects=clip["effects"].as_array_mut().unwrap();if profile>0{effects.push(json!({"effect":"invert","enabled":true,"params":{"channel":{"value":{"Choice":0}},"blend":{"value":{"Float":15.}}}}));}let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="auto_reframe").unwrap().instance();e.params.get_mut("preset").unwrap().value=filmcraft_project::ParamValue::Choice(profile%3);
e.params.get_mut("aspect").unwrap().value=filmcraft_project::ParamValue::Choice(profile/3);
e.params.get_mut("offset").unwrap().value=filmcraft_project::ParamValue::Vec2(filmcraft_geom::Vec2::new(0.3,-0.2));
e.params.get_mut("zoom").unwrap().keyframes=vec![filmcraft_project::Keyframe::new(Tick(0),filmcraft_project::ParamValue::Float(105.)),filmcraft_project::Keyframe::new(Tick(254016000000),filmcraft_project::ParamValue::Float(180.))];
effects.push(serde_json::to_value(e).unwrap());}}}}for variant in 0..2{let mut doc=doc.clone();let salt=cases.len()as i64+1;for item in doc["project"]["items"].as_object_mut().unwrap().values_mut(){if let Some(seq)=item.pointer_mut("/kind/Sequence"){for track in seq["video_tracks"].as_array_mut().unwrap(){for clip in track["items"].as_array_mut().unwrap(){clip["source_in"]=json!(clip["source_in"].as_i64().unwrap()+salt);}}}}let p:Project=serde_json::from_value(doc["project"].clone()).unwrap();let provider=|id:ItemId|{let(_,media,_)=p.resolve_media(id)?;Some(Arc::new(ClockSource{info:media.info.clone(),variant})as SharedSource)};let out=render_sequence(&p,ItemId(base["sequence"].as_u64().unwrap()),Tick(base["at"].as_i64().unwrap()),RenderOptions{scale:base["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&provider);cases.push(json!({"document":doc,"sequence":base["sequence"],"at":base["at"],"scale":base["scale"],"variant":variant,"profile":profile,"width":out.w,"height":out.h,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));}
}
}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
