use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Param};
use filmcraft_project::mixer::AutomationMode;
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::mixer::mix_graph;
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn bits(buffer:filmcraft_frame::AudioBuffer)->Vec<Vec<u32>>{buffer.channels.iter().map(|c|c.iter().map(|v|v.to_bits()).collect()).collect()}
struct AsymmetricSource { base:GeneratorSource }
impl MediaSource for AsymmetricSource {
 fn info(&self)->&filmcraft_media::MediaInfo {self.base.info()}
 fn video_frame(&self,request:filmcraft_media::FrameRequest)->filmcraft_media::Result<Arc<filmcraft_frame::VideoFrame>> {self.base.video_frame(request)}
 fn audio(&self,start:i64,frames:usize,rate:u32)->filmcraft_media::Result<filmcraft_frame::AudioBuffer>{
  let mut out=filmcraft_frame::AudioBuffer::silence(rate,4,frames);
  for i in 0..frames {let n=start+i as i64;if n<0 {continue;}out.channels[0][i]=((n%31)-15)as f32/32.;out.channels[1][i]=((n%23)-7)as f32/64.;out.channels[2][i]=((n%17)-5)as f32/16.;out.channels[3][i]=((n%19)-13)as f32/128.;}
  Ok(out)
 }
}
fn main(){let mut cases=vec![];
for sr in [44100u32,48000,96000]{for speed in [0.,-1.,0.5,1.,1.25,2.]{for reverse in [false,true]{for hold in 0..3 {for channels in [vec![],vec![2],vec![3,1],vec![99,2],vec![0,1,2,3]] {for mode in 0..2 {
let mut p=Project::new("Clip input");let generator=Generator::Tone{hz:440.,db:-6.};let src=GeneratorSource::new(generator.clone(),4,4,FrameRate::new(24,1),Tick(10*T));
let media=p.add_item("Pattern",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:src.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let src=Arc::new(AsymmetricSource{base:src})as SharedSource;let sources=|id|if id==media{Some(src.clone())}else{None};
let sid=p.new_sequence("Sequence",SequenceSettings{sample_rate:sr,..Default::default()},0,1,None);
let duration=Tick::from_units(500,sr as i64);let origin=Tick(7);let mut clip:TrackItem=serde_json::from_value(json!({"id":100,"item":media,"name":"Pattern","label":"Iris","start":origin,"duration":duration,"source_in":Tick::from_units(37,sr as i64),"speed":speed,"reverse":reverse,"frame_hold":if hold==0{None}else{Some(Tick::from_units(123,sr as i64))},"hold_filters":hold==2,"enabled":true,"source_channels":channels,"gain_db":-3.25,"effects":[]})).unwrap();
for id in ["volume","channel_volume","panner"] {let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id==id).unwrap().instance();
for (key,a,b) in match id {"volume"=>vec![("level",-13.25,6.)],"channel_volume"=>vec![("left",-5.5,3.),("right",1.25,-11.)],_=>vec![("balance",-73.25,64.5)]}{
let value=if mode==0{json!({"value":{"Float":a},"keyframes":[]})}else{json!({"value":{"Float":a},"keyframes":[{"time":0,"value":{"Float":a},"interp":"Bezier"},{"time":Tick::from_units(600,sr as i64),"value":{"Float":b},"interp":"EaseOut"}]})};effect.params.insert(key.into(),serde_json::from_value::<Param>(value).unwrap());}
clip.effects.push(effect);}
let track=&mut p.sequence_mut(sid).unwrap().audio_tracks[0];track.items.push(clip.clone());
let mut second=clip.clone();second.id=filmcraft_project::ClipId(101);second.start=Tick::from_units(100,sr as i64);second.duration=Tick::from_units(300,sr as i64);second.source_in=Tick::from_units(1001,sr as i64);second.gain_db=-7.;second.effects.clear();track.items.push(second);
if mode==1 {let kind=["constant_gain","constant_power","exponential_fade"][channels.len()%3];let effect=filmcraft_project::effect_defs().iter().find(|d|d.id==kind).unwrap().instance();track.transitions.push(filmcraft_project::Transition{id:filmcraft_project::TransitionId(9),effect,start:Tick::from_units(100,sr as i64),duration:Tick::from_units(200,sr as i64),from:Some(filmcraft_project::ClipId(100)),to:Some(filmcraft_project::ClipId(101)),align:filmcraft_project::TransitionAlign::CenterAtCut,reverse:false});}
let track=&p.sequence(sid).unwrap().audio_tracks[0];
let requests:Vec<_>=[(-5,97),(0,97),(63,97),(499,97)].into_iter().map(|(start,frames)|json!({"start":start,"frames":frames,"bits":bits(filmcraft_render::audio::track_input(&p,track,start,frames,sr,&sources))})).collect();
cases.push(json!({"track":track,"rate":sr,"requests":requests,"source":media}));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
