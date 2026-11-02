// Original track audio renderer: transition windows and clipped item audio.
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Transition};
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::audio::track_input;
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
 for phase in [-1i64,0,1,5291999,5292000]{for mode in 0..6{for reverse in [false,true]{for at in [8*T,9*T,9*T+T/2,10*T,10*T+T/2,11*T,12*T]{
  let mut p=Project::new("transition sequence");let rate=FrameRate::new(24,1);
  let mut sources=vec![];let mut media=vec![];
  for hz in [440.,660.]{
   let generator=Generator::Tone{hz,db:-6.};let src=GeneratorSource::new(generator.clone(),4,4,rate,Tick(100*T));let info=src.info().clone();
   let id=p.add_item("matte",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info,interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
   media.push(id);sources.push((id,Arc::new(src)as SharedSource));
  }
  let sid=p.new_sequence("s",SequenceSettings{width:4,height:4,..Default::default()},1,0,None);
  let seq=p.sequence_mut(sid).unwrap();let track=&mut seq.video_tracks[0];
  for (index,id) in [100,101].iter().enumerate(){let item:TrackItem=serde_json::from_value(json!({"id":id,"item":media[index],"name":"matte","label":"Iris","start":index as i64*10*T,"duration":10*T,"source_in":20*T,"speed":1,"enabled":!((mode==3&&index==0)||(mode==4&&index==1)),"effects":[]})).unwrap();track.items.push(item);}
  let tr:Transition=serde_json::from_value(json!({"id":500,"effect":{"effect":"cross_dissolve","enabled":true,"params":{}},"start":9*T+phase,"duration":2*T,"from":if mode==1{None}else{Some(100)},"to":if mode==2{None}else{Some(101)},"align":"CenterAtCut","reverse":reverse})).unwrap();track.transitions.push(tr);
  if mode==5{let mut second=track.transitions[0].clone();second.id=serde_json::from_value(json!(501)).unwrap();second.from=None;second.to=Some(serde_json::from_value(json!(101)).unwrap());track.transitions.push(second);}
  let source=|id|sources.iter().find(|(key,_)|*key==id).map(|(_,source)|source.clone());
  let seq=p.sequence(sid).unwrap();let track=&seq.video_tracks[0];let sample_start=Tick(at).to_units_floor(48000);
  let bits=|buffer:filmcraft_frame::AudioBuffer|buffer.channels.iter().map(|channel|channel.iter().map(|v|v.to_bits()).collect::<Vec<_>>()).collect::<Vec<_>>();
  let out=track_input(&p,track,sample_start,16,48000,&source);
  let mut sides=vec![];
  for index in 0..2{let mut one=track.clone();one.transitions.clear();one.items=vec![track.items[index].clone()];one.items[0].enabled=true;sides.push(bits(track_input(&p,&one,sample_start,16,48000,&source)));}
  cases.push(json!({"phase":phase,"startTick":(9*T+phase).to_string(),"durationTick":(2*T).to_string(),"mode":mode,"reverse":reverse,"startSample":sample_start,"frames":16,"a":sides[0],"b":sides[1],"bits":bits(out)}));
 }}}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
